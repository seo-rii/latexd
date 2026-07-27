#[derive(Clone, Copy)]
enum CrossLayerReplayCase {
    Plain,
    PlainMutation,
    SemanticAux,
}

async fn run_cross_layer_replay_case(case: CrossLayerReplayCase) {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
    )
    .expect("write manifest");
    fs::write(root.join("article.cls"), "").expect("write article class");
    fs::create_dir_all(root.join("sections")).expect("create sections");
    let body = (0..1_600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/body.tex"), &body).expect("write body");
    let semantic_prefix = match case {
        CrossLayerReplayCase::Plain | CrossLayerReplayCase::PlainMutation => {
            "\\section{Intro}\n".to_string()
        }
        CrossLayerReplayCase::SemanticAux => {
            "\\section{Intro}\\label{sec:intro}\nSee Section~\\ref{sec:intro}.\n".to_string()
        }
    };
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{semantic_prefix}\\input{{sections/body}}\n\\end{{document}}\n"
        ),
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let replay_driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let replay_build_root = root.join(".latexd/replay-build");
    let first = replay_driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: replay_build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
            ],
        })
        .await
        .expect("clean build");
    let first_checkpoints =
        load_checkpoint_bundle(&replay_build_root.join("rev-1/checkpoints.json"))
            .expect("load first checkpoint bundle");

    let updated_body = match case {
        CrossLayerReplayCase::PlainMutation => body.replacen("word0800", "changed0800", 1),
        CrossLayerReplayCase::Plain | CrossLayerReplayCase::SemanticAux => {
            format!("{body}\n% replay-only trailing comment\n")
        }
    };
    fs::write(root.join("sections/body.tex"), updated_body).expect("rewrite body");
    let replayed = replay_driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: replay_build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("sections/body.tex")],
        })
        .await
        .expect("checkpoint replay build");
    let clean_build_root = root.join(".latexd/clean-build");
    let clean = CompilerDriver::new(Some("internal".to_string()), Vec::new())
        .compile(CompileRequest {
            root,
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: clean_build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
            ],
        })
        .await
        .expect("clean comparison build");
    let clean_rev = clean_build_root.join("rev-2");
    let replay_rev = replay_build_root.join("rev-2");

    assert!(first.reused_checkpoint_id.is_none());
    assert!(replayed.reused_checkpoint_id.is_some());
    assert!(clean.reused_checkpoint_id.is_none());
    let replayed_checkpoint_id = replayed
        .reused_checkpoint_id
        .as_deref()
        .expect("replayed checkpoint id");
    let replayed_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.checkpoint_id == replayed_checkpoint_id)
        .expect("selected replay checkpoint");
    assert_eq!(
        replayed_checkpoint.meta.input_boundary_kind,
        Some(VmModuleCheckpointKind::Enter)
    );
    assert_eq!(clean.diagnostics, replayed.diagnostics);
    assert_eq!(clean.dep_trace, replayed.dep_trace);
    let module_checkpoint_shape = |path: Utf8PathBuf| {
        let sources: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read source snapshot"))
                .expect("decode source snapshot");
        sources["module_checkpoints"]
            .as_array()
            .expect("module checkpoint array")
            .iter()
            .map(|checkpoint| {
                (
                    checkpoint["kind"].clone(),
                    checkpoint["module_path"].clone(),
                    checkpoint["resume_path"].clone(),
                    checkpoint["source_offset_utf8"].clone(),
                    checkpoint["continuation_stack"].clone(),
                    checkpoint["output_start_utf8"].clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        module_checkpoint_shape(clean_build_root.join("rev-2/sources.json")),
        module_checkpoint_shape(replay_build_root.join("rev-2/sources.json"))
    );
    assert_eq!(clean.page_metadata, replayed.page_metadata);
    match case {
        CrossLayerReplayCase::PlainMutation => assert!(
            !replayed.page_patches.is_empty(),
            "visible input edit did not change renderer pages"
        ),
        CrossLayerReplayCase::Plain | CrossLayerReplayCase::SemanticAux => assert!(
            replayed.page_patches.is_empty(),
            "trailing comment changed renderer pages: {:?}",
            replayed.page_patches
        ),
    }
    let normalize_renderer_metadata =
        |pages: &[latexd::compiler::PageArtifactMeta]| -> Vec<latexd::compiler::PageArtifactMeta> {
            pages
                .iter()
                .cloned()
                .map(|mut page| {
                    page.pdf_artifact_path = Utf8PathBuf::new();
                    page
                })
                .collect()
        };
    let clean_renderer_metadata = normalize_renderer_metadata(&clean.renderer_page_metadata);
    let replay_renderer_metadata = normalize_renderer_metadata(&replayed.renderer_page_metadata);
    if clean_renderer_metadata != replay_renderer_metadata {
        let differing_page = clean_renderer_metadata
            .iter()
            .zip(&replay_renderer_metadata)
            .position(|(clean, replay)| clean != replay)
            .unwrap_or(
                clean_renderer_metadata
                    .len()
                    .min(replay_renderer_metadata.len()),
            );
        let clean_page = clean_renderer_metadata.get(differing_page);
        let replay_page = replay_renderer_metadata.get(differing_page);
        let differing_span = clean_page
            .zip(replay_page)
            .and_then(|(clean, replay)| {
                clean
                    .source_spans
                    .iter()
                    .zip(&replay.source_spans)
                    .position(|(clean, replay)| clean != replay)
                    .or_else(|| {
                        (clean.source_spans.len() != replay.source_spans.len())
                            .then_some(clean.source_spans.len().min(replay.source_spans.len()))
                    })
            });
        panic!(
            "renderer metadata differs after replay: page={differing_page}; clean_page={:?}; replay_page={:?}; span={differing_span:?}; clean_span={:?}; replay_span={:?}",
            clean_page.map(|page| (
                &page.page_id,
                &page.content_hash,
                page.text_start_utf8,
                page.text_end_utf8,
                page.source_spans.len(),
            )),
            replay_page.map(|page| (
                &page.page_id,
                &page.content_hash,
                page.text_start_utf8,
                page.text_end_utf8,
                page.source_spans.len(),
            )),
            differing_span.and_then(|index| clean_page?.source_spans.get(index)),
            differing_span.and_then(|index| replay_page?.source_spans.get(index)),
        );
    }
    assert_eq!(
        clean
            .page_artifacts
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        replayed
            .page_artifacts
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );

    for relative_path in [
        "output.txt",
        "page-syncmap.json",
        "render-ir/legacy-output.txt",
        "render-ir/events.json",
        "render-ir/document-ir.json",
        "render-ir/page-display-list.json",
    ] {
        let clean_artifact =
            fs::read(clean_rev.join(relative_path)).expect("read clean artifact");
        let replay_artifact =
            fs::read(replay_rev.join(relative_path)).expect("read replay artifact");
        if clean_artifact != replay_artifact {
            let first_difference = clean_artifact
                .iter()
                .zip(&replay_artifact)
                .position(|(clean, replay)| clean != replay)
                .unwrap_or(clean_artifact.len().min(replay_artifact.len()));
            let context_start = first_difference.saturating_sub(32);
            let clean_context_end = (first_difference + 32).min(clean_artifact.len());
            let replay_context_end = (first_difference + 32).min(replay_artifact.len());
            panic!(
                "artifact differs after checkpoint replay: {relative_path}; offset={first_difference}; clean_len={}; replay_len={}; clean={:?}; replay={:?}",
                clean_artifact.len(),
                replay_artifact.len(),
                String::from_utf8_lossy(&clean_artifact[context_start..clean_context_end]),
                String::from_utf8_lossy(&replay_artifact[context_start..replay_context_end]),
            );
        }
    }

    let clean_aux = clean_rev.join("aux.json");
    let replay_aux = replay_rev.join("aux.json");
    assert_eq!(clean_aux.exists(), replay_aux.exists());
    match case {
        CrossLayerReplayCase::Plain | CrossLayerReplayCase::PlainMutation => {
            assert!(!clean_aux.exists())
        }
        CrossLayerReplayCase::SemanticAux => assert_eq!(
            load_semantic_aux(&clean_aux).expect("load clean semantic aux"),
            load_semantic_aux(&replay_aux).expect("load replay semantic aux")
        ),
    }
}
