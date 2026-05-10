struct LaterTrailingInputToplevelSemanticallyEqualReplayFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    tail_filler: String,
    preamble_checkpoint_id: String,
    bibliography_checkpoint_id: String,
    bibliography_checkpoint_page_index: usize,
}

enum LaterTrailingInputToplevelSemanticallyEqualReplayNoise {
    Untracked,
    Unreadable,
}

enum LaterTrailingInputToplevelSemanticallyEqualReplayStart {
    Preamble,
    Bibliography,
}

enum LaterTrailingInputToplevelSemanticallyEqualReplayOrder {
    Baseline,
    Reversed,
}

enum LaterTrailingInputToplevelSemanticallyEqualReplayCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

type TrailTopSemEqCase = LaterTrailingInputToplevelSemanticallyEqualReplayCase;

async fn run_trail_top_sem_eq_case(case: TrailTopSemEqCase) {
    run_later_trailing_input_toplevel_semantically_equal_replay_case(case).await;
}

async fn prepare_later_trailing_input_toplevel_semantically_equal_replay_fixture()
-> LaterTrailingInputToplevelSemanticallyEqualReplayFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "bibliography replay filler text ".repeat(220);
    let tail_filler = "tail replay filler text ".repeat(180);
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
            "\\documentclass{{article}}\\begin{{document}} Cite \\cite{{alpha}}.\\section{{Intro}} {body_filler}\\bibliography{{refs}}\\input{{sections/tail}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("sections/tail.tex"),
        format!("Tail A. {tail_filler}"),
    )
    .expect("write tail");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

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
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let bibliography_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .expect("refs.bbl input boundary");

    LaterTrailingInputToplevelSemanticallyEqualReplayFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        tail_filler,
        preamble_checkpoint_id: first_bundle.checkpoints[0].meta.checkpoint_id.clone(),
        bibliography_checkpoint_id: bibliography_checkpoint.meta.checkpoint_id.clone(),
        bibliography_checkpoint_page_index: bibliography_checkpoint.meta.page_index_after,
    }
}

fn rewrite_later_trailing_input_toplevel_semantically_equal_replay(
    fixture: &LaterTrailingInputToplevelSemanticallyEqualReplayFixture,
) {
    fs::write(
        fixture.root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha   entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");
    fs::write(
        fixture.root.join("sections/tail.tex"),
        format!("Tail B. {}", fixture.tail_filler),
    )
    .expect("rewrite tail");
}

async fn compile_later_trailing_input_toplevel_semantically_equal_replay_second_pass(
    fixture: &LaterTrailingInputToplevelSemanticallyEqualReplayFixture,
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

async fn run_later_trailing_input_toplevel_semantically_equal_dirty_replay(
    noise: LaterTrailingInputToplevelSemanticallyEqualReplayNoise,
    dirty_files: &[&str],
    replay_start: LaterTrailingInputToplevelSemanticallyEqualReplayStart,
) {
    let fixture = prepare_later_trailing_input_toplevel_semantically_equal_replay_fixture().await;
    rewrite_later_trailing_input_toplevel_semantically_equal_replay(&fixture);
    match noise {
        LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second = compile_later_trailing_input_toplevel_semantically_equal_replay_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;

    let (expected_checkpoint_id, expected_start_page_index) = match replay_start {
        LaterTrailingInputToplevelSemanticallyEqualReplayStart::Preamble => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayStart::Bibliography => (
            fixture.bibliography_checkpoint_id.as_str(),
            fixture.bibliography_checkpoint_page_index,
        ),
    };
    assert_later_trailing_input_toplevel_semantically_equal_replay(
        &fixture,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

async fn run_later_trailing_input_toplevel_semantically_equal_baseline_replay(
    order: LaterTrailingInputToplevelSemanticallyEqualReplayOrder,
) {
    let fixture = prepare_later_trailing_input_toplevel_semantically_equal_replay_fixture().await;
    rewrite_later_trailing_input_toplevel_semantically_equal_replay(&fixture);
    let dirty_files = match order {
        LaterTrailingInputToplevelSemanticallyEqualReplayOrder::Baseline => vec![
            Utf8PathBuf::from("refs.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        LaterTrailingInputToplevelSemanticallyEqualReplayOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refs.bbl"),
        ],
    };
    let second = compile_later_trailing_input_toplevel_semantically_equal_replay_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;
    assert_later_trailing_input_toplevel_semantically_equal_replay(
        &fixture,
        &second,
        &dirty_files,
        &fixture.bibliography_checkpoint_id,
        fixture.bibliography_checkpoint_page_index,
    );
}

async fn run_later_trailing_input_toplevel_semantically_equal_replay_case(
    case: LaterTrailingInputToplevelSemanticallyEqualReplayCase,
) {
    match case {
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::Baseline => {
            run_later_trailing_input_toplevel_semantically_equal_baseline_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayOrder::Baseline,
            )
            .await;
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::Reversed => {
            run_later_trailing_input_toplevel_semantically_equal_baseline_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayOrder::Reversed,
            )
            .await;
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::UntrackedFollows => {
            run_later_trailing_input_toplevel_semantically_equal_dirty_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Untracked,
                &["refs.bbl", "sections/tail.tex", "notes.txt"],
                LaterTrailingInputToplevelSemanticallyEqualReplayStart::Preamble,
            )
            .await;
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::UntrackedPrecedes => {
            run_later_trailing_input_toplevel_semantically_equal_dirty_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Untracked,
                &["notes.txt", "refs.bbl", "sections/tail.tex"],
                LaterTrailingInputToplevelSemanticallyEqualReplayStart::Bibliography,
            )
            .await;
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::UnreadableFollows => {
            run_later_trailing_input_toplevel_semantically_equal_dirty_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Unreadable,
                &["refs.bbl", "sections/tail.tex", "notes.txt"],
                LaterTrailingInputToplevelSemanticallyEqualReplayStart::Preamble,
            )
            .await;
        }
        LaterTrailingInputToplevelSemanticallyEqualReplayCase::UnreadablePrecedes => {
            run_later_trailing_input_toplevel_semantically_equal_dirty_replay(
                LaterTrailingInputToplevelSemanticallyEqualReplayNoise::Unreadable,
                &["notes.txt", "refs.bbl", "sections/tail.tex"],
                LaterTrailingInputToplevelSemanticallyEqualReplayStart::Preamble,
            )
            .await;
        }
    }
}

fn assert_later_trailing_input_toplevel_semantically_equal_replay(
    fixture: &LaterTrailingInputToplevelSemanticallyEqualReplayFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-2/sources.json")).expect("read second sources"),
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
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
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
