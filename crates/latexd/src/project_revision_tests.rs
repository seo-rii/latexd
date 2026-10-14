use super::*;
use std::fs;
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn live_preview_daemon_skips_noop_saves_and_reuses_owners_after_cold_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let source = r"\documentclass{article}\begin{document}First stable page.\newpage Second stable page.\end{document}";
    fs::write(root.join("main.tex"), source).unwrap();
    let build_root = root.join(".latexd/build");
    let artifacts_root = root.join(".latexd/artifacts");
    fs::create_dir_all(&build_root).unwrap();
    fs::create_dir_all(&artifacts_root).unwrap();
    let mut first_metadata = None;
    let mut first_artifacts = None;

    for expected_rev in 1..=2 {
        let recovered = if expected_rev == 1 {
            LivePreviewState::default()
        } else {
            let recovered = preview_persistence::recover(&build_root, false)
                .await
                .unwrap();
            assert_eq!(recovered.snapshot.current_rev, 1);
            assert_eq!(recovered.snapshot.last_applied_rev, 1);
            assert_eq!(recovered.page_metadata, *first_metadata.as_ref().unwrap());
            assert!(!recovered.latest_pdf_path.as_ref().unwrap().exists());
            recovered
        };
        let (events, _) = broadcast::channel(64);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: artifacts_root.clone(),
            world: tex_world::ProjectWorld::load(root.clone()).unwrap(),
            compiler: CompilerDriver::new(Some("internal".into()), Vec::new())
                .with_preview_artifacts(),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: StdMutex::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(recovered),
            events,
        });
        let mut received = state.events.subscribe();
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(state.clone().run_build_loop(rx));
        // Startup intentionally performs one cold build to establish current
        // input hashes. A recovered preview remains visible until that commit.
        assert!(state.build_cache.read().await.tracked_inputs.is_empty());
        tx.send(vec!["main.tex".into()]).unwrap();
        timeout(Duration::from_secs(30), async {
            loop {
                match received.recv().await.unwrap() {
                    ServerMsg::PreviewCommitted {
                        base_preview_rev,
                        new_preview_rev,
                        ..
                    } => {
                        assert_eq!(base_preview_rev, expected_rev - 1);
                        assert_eq!(new_preview_rev, expected_rev);
                        break;
                    }
                    ServerMsg::BuildFinished { success: false, .. } => {
                        panic!(
                            "live preview build failed: {:?}",
                            state.live.read().await.snapshot.diagnostics
                        );
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("cold preview published");
        let live = state.live.read().await.clone();
        assert_eq!(live.snapshot.current_rev, expected_rev);
        assert_eq!(live.snapshot.last_applied_rev, expected_rev);
        assert_eq!(live.page_metadata.len(), 2);
        assert_eq!(
            state.build_cache.read().await.input_hashes[Utf8Path::new("main.tex")],
            Some(blake3::hash(source.as_bytes()).to_hex().to_string())
        );
        let directory = build_root.join(format!("rev-{expected_rev}"));
        assert!(
            !directory.join("main.pdf").exists(),
            "whole PDF remains deferred"
        );
        assert!(directory.join("pdf-export.json").is_file());
        assert!(!directory.join("checkpoints.json").exists());
        assert!(!directory.join("checkpoint-write-outcome.json").exists());
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("build-meta.json")).unwrap()).unwrap();
        assert_eq!(
            metadata["checkpoint_write_outcome"]["status"],
            "not_attempted"
        );
        assert_eq!(metadata["trace"]["restored_prefix_source_bytes"], 0);
        assert!(metadata["trace"]["vm_executions"].as_u64().unwrap() > 0);
        if expected_rev == 1 {
            first_metadata = Some(live.page_metadata.clone());
            first_artifacts = Some(live.snapshot.page_artifacts.clone());
        } else {
            assert_eq!(live.page_metadata, *first_metadata.as_ref().unwrap());
            assert_eq!(
                live.snapshot.page_artifacts,
                *first_artifacts.as_ref().unwrap()
            );
            assert_eq!(metadata["trace"]["reused_output_pages"], 2);
        }
        for page in &live.page_metadata {
            assert!(page.pdf_artifact_path.starts_with("rev-1"));
            assert!(build_root.join(&page.pdf_artifact_path).is_file());
            assert!(
                build_root
                    .join(page.pdf_artifact_path.with_extension("svg"))
                    .is_file()
            );
        }
        fs::write(root.join("main.tex"), source).unwrap();
        tx.send(vec!["main.tex".into()]).unwrap();
        drop(tx);
        // Closing the queue lets the loop finish after consuming the no-op.
        // This proves save-event elision without a timer-based absence check.
        timeout(Duration::from_secs(30), task)
            .await
            .unwrap()
            .unwrap();
        let after_noop = state.live.read().await;
        assert_eq!(after_noop.snapshot.current_rev, expected_rev);
        assert_eq!(after_noop.snapshot.last_applied_rev, expected_rev);
        assert!(
            !build_root
                .join(format!("rev-{}", expected_rev + 1))
                .exists()
        );
        drop(after_noop);
        drop_render_session_handle(&state);
    }
}

#[tokio::test]
async fn input_revision_keeps_native_continuations_and_matches_clean_build_after_edit() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    fs::write(
        root.join("main.tex"),
        r"\begin{document}Before.\input{child}\bibliography{refs}After.\end{document}",
    )
    .unwrap();
    fs::write(root.join("child.tex"), "First child.").unwrap();
    fs::write(
        root.join("main.bbl"),
        r"\begin{thebibliography}{1}\bibitem{alpha}Entry.\end{thebibliography}",
    )
    .unwrap();
    let build_root = root.join(".latexd/build");
    let artifacts_root = root.join(".latexd/artifacts");
    fs::create_dir_all(&build_root).unwrap();
    fs::create_dir_all(&artifacts_root).unwrap();
    let (events, _) = broadcast::channel(64);
    let state = Arc::new(AppState {
        root: root.clone(),
        build_root: build_root.clone(),
        artifacts_root,
        world: tex_world::ProjectWorld::load(root.clone()).unwrap(),
        compiler: CompilerDriver::new(Some("internal".into()), Vec::new()),
        tile_renderer: TileRendererConfig::Mock,
        editor_bridge: None,
        raster_cache: RwLock::new(BTreeMap::new()),
        inflight_rasters: StdMutex::new(BTreeMap::new()),
        build_cache: RwLock::new(BuildCache::default()),
        live: RwLock::new(LivePreviewState::default()),
        events,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(state.clone().run_build_loop(rx));
    for (rev, child) in [(1, "First child."), (2, "Other child.")] {
        fs::write(root.join("child.tex"), child).unwrap();
        tx.send(vec!["child.tex".into()]).unwrap();
        timeout(Duration::from_secs(20), async {
            while state.live.read().await.snapshot.last_applied_rev != rev {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("native preview applied");
        let stored: serde_json::Value = serde_json::from_slice(
            &fs::read(build_root.join(format!("rev-{rev}/sources.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(stored["files"]["child.tex"], child);
        assert!(!stored["executed_files"].as_object().unwrap().is_empty());
        assert!(stored["rewrite_spans"].is_object());
        assert!(!stored["module_checkpoints"].as_array().unwrap().is_empty());
        assert!(!stored["module_traces"].as_array().unwrap().is_empty());
    }
    let clean = tempfile::tempdir().unwrap();
    let clean_root = Utf8PathBuf::from_path_buf(clean.path().to_owned()).unwrap();
    CompilerDriver::new(Some("internal".into()), Vec::new())
        .compile(CompileRequest {
            root: root.clone(),
            manifest: tex_world::ProjectManifest::discover(&root).unwrap(),
            toplevel: "main.tex".into(),
            rev: 2,
            build_root: clean_root.clone(),
            changed_files: vec!["main.tex".into(), "child.tex".into()],
        })
        .await
        .unwrap();
    for name in [
        "render-ir/document-ir.json",
        "render-ir/page-display-list.json",
        "page-syncmap.json",
        "aux.json",
    ] {
        let preview: serde_json::Value =
            serde_json::from_slice(&fs::read(build_root.join(format!("rev-2/{name}"))).unwrap())
                .unwrap();
        let clean: serde_json::Value =
            serde_json::from_slice(&fs::read(clean_root.join(format!("rev-2/{name}"))).unwrap())
                .unwrap();
        assert_eq!(preview, clean, "native daemon incremental and clean {name}");
    }
    drop(tx);
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn input_revision_records_consumed_hash_and_detects_negative_dependency_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(root.join("main.tex"), "A").unwrap();
    fs::write(root.join("notes.txt"), "unrelated").unwrap();
    let revision = ProjectRevision::capture(root, &root.join(".latexd/build"))
        .await
        .unwrap();
    fs::write(root.join("main.tex"), "B").unwrap();
    let mut cache = BuildCache::default();
    cache.record_success(&revision, BTreeSet::from([Utf8PathBuf::from("main.tex")]));
    assert_eq!(
        cache.input_hashes[Utf8Path::new("main.tex")],
        Some(blake3::hash(b"A").to_hex().to_string())
    );
    assert!(
        BuildCache::plan_rebuild(root, &cache, &["main.tex".into()])
            .await
            .unwrap()
            .needs_rebuild
    );
    fs::write(root.join("notes.txt"), "unrelated change").unwrap();
    assert!(
        !BuildCache::plan_rebuild(root, &cache, &["notes.txt".into()])
            .await
            .unwrap()
            .needs_rebuild
    );
    fs::write(root.join("optional.sty"), "new package").unwrap();
    assert!(
        BuildCache::plan_rebuild(root, &cache, &["optional.sty".into()])
            .await
            .unwrap()
            .needs_rebuild
    );
    fs::create_dir(root.join("figures")).unwrap();
    assert!(
        BuildCache::plan_rebuild(root, &cache, &["figures".into()])
            .await
            .unwrap()
            .needs_rebuild
    );
    fs::create_dir(root.join(".git")).unwrap();
    assert!(
        !BuildCache::plan_rebuild(root, &cache, &[".git".into()])
            .await
            .unwrap()
            .needs_rebuild
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_revision_preserves_latest_save_in_100_controlled_compile_races() {
    use axum::body::to_bytes;

    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    fs::write(root.join("main.tex"), "initial").unwrap();
    // The fake external compiler captures its input, pauses on odd revisions,
    // then reads it again. A live-root compile fails cmp after the concurrent save.
    fs::write(
        root.join("compiler.sh"),
        r#"set -eu
cat "$1" > "$2"
if [ $(($4 % 2)) -eq 1 ]; then
    : > "$3/ready"
    while [ ! -f "$3/release" ]; do sleep 0.005; done
    cmp "$1" "$2"
fi
"#,
    )
    .unwrap();
    let build_root = root.join(".latexd/build");
    let artifacts_root = root.join(".latexd/artifacts");
    fs::create_dir_all(&build_root).unwrap();
    fs::create_dir_all(&artifacts_root).unwrap();
    let (events, mut received) = broadcast::channel(2048);
    let state = Arc::new(AppState {
        root: root.clone(),
        build_root: build_root.clone(),
        artifacts_root: artifacts_root.clone(),
        world: tex_world::ProjectWorld::load(root.clone()).unwrap(),
        compiler: CompilerDriver::new(
            Some("/bin/sh".into()),
            vec![
                "{root}/compiler.sh".into(),
                "{root}/{main}".into(),
                "{out_pdf}".into(),
                "{out_dir}".into(),
                "{rev}".into(),
            ],
        ),
        tile_renderer: TileRendererConfig::Mock,
        editor_bridge: None,
        raster_cache: RwLock::new(BTreeMap::new()),
        inflight_rasters: StdMutex::new(BTreeMap::new()),
        build_cache: RwLock::new(BuildCache::default()),
        live: RwLock::new(LivePreviewState::default()),
        events,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(state.clone().run_build_loop(rx));
    for iteration in 0..100u64 {
        let old_rev = iteration * 2 + 1;
        let final_rev = old_rev + 1;
        fs::write(root.join("main.tex"), format!("before-{iteration}")).unwrap();
        tx.send(vec!["main.tex".into()]).unwrap();
        let waiting = build_root.join(format!("rev-{old_rev}/ready"));
        timeout(Duration::from_secs(10), async {
            while !waiting.exists() {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("compiler read its isolated input");
        let final_source = format!("after-{iteration}");
        fs::write(root.join("main.tex"), &final_source).unwrap();
        tx.send(vec!["main.tex".into()]).unwrap();
        // Hold the cache owner through manifest preparation. Publication must
        // wait for its input hash transaction instead of exposing a new live
        // revision first and updating the cache at a later await point.
        let cache_guard = state.build_cache.read().await;
        fs::write(build_root.join(format!("rev-{old_rev}/release")), "").unwrap();
        let prepared = build_root.join(format!("rev-{final_rev}/preview.pending"));
        timeout(Duration::from_secs(10), async {
            while !prepared.exists() {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("latest preview prepared before cache publication");
        assert_eq!(
            state.live.read().await.snapshot.last_applied_rev,
            iteration * 2,
            "preview publication must include the consumed-input cache"
        );
        drop(cache_guard);
        timeout(Duration::from_secs(10), async {
            while state.live.read().await.snapshot.last_applied_rev != final_rev {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("latest saved input published");
        assert_eq!(
            fs::read_to_string(artifacts_root.join("latest.pdf")).unwrap(),
            final_source
        );
        assert_eq!(
            state.build_cache.read().await.input_hashes[Utf8Path::new("main.tex")],
            Some(blake3::hash(final_source.as_bytes()).to_hex().to_string())
        );
        assert_eq!(
            load_revision_source_texts(&build_root, final_rev).await[Utf8Path::new("main.tex")],
            final_source
        );
        while let Ok(event) = received.try_recv() {
            if let ServerMsg::PreviewCommitted {
                new_preview_rev: rev,
                ..
            } = event
            {
                assert_eq!(rev, final_rev);
            }
        }
    }
    // Old preview source reads remain bound to their revision after another save.
    fs::write(root.join("main.tex"), "not yet compiled").unwrap();
    let response = source_file(
        Path(200),
        Query(SourceFileQuery {
            file: "main.tex".into(),
        }),
        State(state.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"], "after-99");
    fs::write(root.join("new.tex"), "not in preview").unwrap();
    let missing = source_file(
        Path(200),
        Query(SourceFileQuery {
            file: "new.tex".into(),
        }),
        State(state.clone()),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    drop(tx);
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
}
