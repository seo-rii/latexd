struct TrailingInputToplevelReplayFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first_bibliography_body: String,
    tail_filler: String,
    bibliography_checkpoint_id: String,
    bibliography_checkpoint_page_index: usize,
    preamble_checkpoint_id: String,
}

enum TrailingInputToplevelReplayNoise {
    Untracked,
    Unreadable,
}

enum TrailingInputToplevelReplayStart {
    Preamble,
    Bibliography,
}

enum TrailingInputToplevelReplayCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

type TopReplay = TrailingInputToplevelReplayCase;

async fn run_top_replay(case: TopReplay) {
    run_trailing_input_toplevel_replay_case(case).await;
}

#[derive(Clone, Copy)]
enum TrailingInputToplevelReplayBaselineOrder {
    Baseline,
    Reversed,
}

async fn prepare_trailing_input_toplevel_replay_fixture() -> TrailingInputToplevelReplayFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "multi bibliography replay filler text ".repeat(220);
    let tail_filler = "tail replay filler text ".repeat(180);
    let first_bibliography_body = (0..1800)
        .map(|index| format!("alpha{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("article.cls"), "").expect("write class");
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}} Cite \\cite{{alpha}} and \\cite{{beta}}.\\section{{Intro}} {body_filler}\\bibliography{{refsa,refsb}}\\input{{sections/tail}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("sections/tail.tex"),
        format!("Tail A. {tail_filler}"),
    )
    .expect("write tail");
    fs::write(
        root.join("refsa.bbl"),
        format!(
            "\\begin{{thebibliography}}{{1}}\n\\bibitem[A 2024]{{alpha}} Alpha entry. {first_bibliography_body}\n\\end{{thebibliography}}\n"
        ),
    )
    .expect("write first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write second bibliography");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let _first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let preamble_checkpoint_id = first_bundle.checkpoints[0].meta.checkpoint_id.clone();
    let bibliography_checkpoint = first_bundle
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                    path == &Utf8PathBuf::from("refsa.bbl")
                        || path == &Utf8PathBuf::from("refsb.bbl")
                })
        })
        .min_by_key(|checkpoint| {
            (
                checkpoint.meta.page_index_after,
                checkpoint.meta.output_start_utf8,
            )
        })
        .expect("earlier bibliography input boundary");

    TrailingInputToplevelReplayFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first_bibliography_body,
        tail_filler,
        bibliography_checkpoint_id: bibliography_checkpoint.meta.checkpoint_id.clone(),
        bibliography_checkpoint_page_index: bibliography_checkpoint.meta.page_index_after,
        preamble_checkpoint_id,
    }
}

fn rewrite_trailing_input_toplevel_replay_fixture(
    root: &Utf8Path,
    first_bibliography_body: &str,
    tail_filler: &str,
) {
    fs::write(
        root.join("refsa.bbl"),
        format!(
            "\\begin{{thebibliography}}{{1}}\n\\bibitem[A 2024]{{alpha}} Alpha  entry. {first_bibliography_body}\n\\end{{thebibliography}}\n"
        ),
    )
    .expect("rewrite first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
    fs::write(
        root.join("sections/tail.tex"),
        format!("Tail B. {tail_filler}"),
    )
    .expect("rewrite tail");
}

async fn compile_trailing_input_toplevel_replay_second_pass(
    fixture: &TrailingInputToplevelReplayFixture,
    dirty_files: &[Utf8PathBuf],
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: dirty_files.to_vec(),
        })
        .await
        .expect("second semantic aux build should succeed")
}

async fn run_trailing_input_toplevel_baseline_replay(
    order: TrailingInputToplevelReplayBaselineOrder,
) {
    let fixture = prepare_trailing_input_toplevel_replay_fixture().await;
    rewrite_trailing_input_toplevel_replay_fixture(
        &fixture.root,
        &fixture.first_bibliography_body,
        &fixture.tail_filler,
    );

    let dirty_files = match order {
        TrailingInputToplevelReplayBaselineOrder::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("refsa.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        TrailingInputToplevelReplayBaselineOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("refsa.bbl"),
        ],
    };
    let second = compile_trailing_input_toplevel_replay_second_pass(&fixture, &dirty_files).await;

    assert_trailing_input_toplevel_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        &fixture.bibliography_checkpoint_id,
        fixture.bibliography_checkpoint_page_index,
    );
}

async fn run_trailing_input_toplevel_dirty_replay(
    noise: TrailingInputToplevelReplayNoise,
    dirty_files: &[&str],
    replay_start: TrailingInputToplevelReplayStart,
) {
    let fixture = prepare_trailing_input_toplevel_replay_fixture().await;
    rewrite_trailing_input_toplevel_replay_fixture(
        &fixture.root,
        &fixture.first_bibliography_body,
        &fixture.tail_filler,
    );
    match noise {
        TrailingInputToplevelReplayNoise::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        TrailingInputToplevelReplayNoise::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second = compile_trailing_input_toplevel_replay_second_pass(&fixture, &dirty_files).await;

    let (expected_checkpoint_id, expected_start_page_index) = match replay_start {
        TrailingInputToplevelReplayStart::Preamble => (fixture.preamble_checkpoint_id.as_str(), 0),
        TrailingInputToplevelReplayStart::Bibliography => (
            fixture.bibliography_checkpoint_id.as_str(),
            fixture.bibliography_checkpoint_page_index,
        ),
    };
    assert_trailing_input_toplevel_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

async fn run_trailing_input_toplevel_replay_case(case: TrailingInputToplevelReplayCase) {
    match case {
        TrailingInputToplevelReplayCase::Baseline => {
            run_trailing_input_toplevel_baseline_replay(
                TrailingInputToplevelReplayBaselineOrder::Baseline,
            )
            .await;
        }
        TrailingInputToplevelReplayCase::Reversed => {
            run_trailing_input_toplevel_baseline_replay(
                TrailingInputToplevelReplayBaselineOrder::Reversed,
            )
            .await;
        }
        TrailingInputToplevelReplayCase::UntrackedFollows => {
            run_trailing_input_toplevel_dirty_replay(
                TrailingInputToplevelReplayNoise::Untracked,
                &["refsb.bbl", "refsa.bbl", "sections/tail.tex", "notes.txt"],
                TrailingInputToplevelReplayStart::Preamble,
            )
            .await;
        }
        TrailingInputToplevelReplayCase::UntrackedPrecedes => {
            run_trailing_input_toplevel_dirty_replay(
                TrailingInputToplevelReplayNoise::Untracked,
                &["notes.txt", "refsb.bbl", "refsa.bbl", "sections/tail.tex"],
                TrailingInputToplevelReplayStart::Bibliography,
            )
            .await;
        }
        TrailingInputToplevelReplayCase::UnreadableFollows => {
            run_trailing_input_toplevel_dirty_replay(
                TrailingInputToplevelReplayNoise::Unreadable,
                &["refsb.bbl", "refsa.bbl", "sections/tail.tex", "notes.txt"],
                TrailingInputToplevelReplayStart::Preamble,
            )
            .await;
        }
        TrailingInputToplevelReplayCase::UnreadablePrecedes => {
            run_trailing_input_toplevel_dirty_replay(
                TrailingInputToplevelReplayNoise::Unreadable,
                &["notes.txt", "refsb.bbl", "refsa.bbl", "sections/tail.tex"],
                TrailingInputToplevelReplayStart::Preamble,
            )
            .await;
        }
    }
}

fn assert_trailing_input_toplevel_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/tail.tex")].contains("Tail B."),
        "executed tail.tex should reflect the later tracked change"
    );
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    assert_eq!(build_meta.start_page_index, expected_start_page_index);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}
