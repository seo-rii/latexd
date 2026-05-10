struct TrailingInputIncludedTailReplayFixture {
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

enum TrailingInputIncludedTailReplayNoise {
    Untracked,
    Unreadable,
}

enum TrailingInputIncludedTailReplayStart {
    Preamble,
    Bibliography,
}

enum TrailingInputIncludedTailReplayCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

#[derive(Clone, Copy)]
enum TrailingInputIncludedTailReplayBaselineOrder {
    Baseline,
    Reversed,
}

async fn prepare_trailing_input_included_tail_replay_fixture()
-> TrailingInputIncludedTailReplayFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let intro_filler = "multi bibliography trailing replay filler ".repeat(220);
    let first_bibliography_body = (0..1800)
        .map(|index| format!("alpha{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
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
            "\\documentclass{{article}}\\begin{{document}}Intro. {intro_filler} \\cite{{alpha}} and \\cite{{beta}}.\\bibliography{{refsa,refsb}}\\input{{sections/tail}}\\end{{document}}"
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
        .expect("earlier bibliography checkpoint");

    TrailingInputIncludedTailReplayFixture {
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

fn rewrite_trailing_input_included_tail_replay_fixture(
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

async fn compile_trailing_input_included_tail_replay_second_pass(
    fixture: &TrailingInputIncludedTailReplayFixture,
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

async fn run_trailing_input_included_tail_baseline_replay(
    order: TrailingInputIncludedTailReplayBaselineOrder,
) {
    let fixture = prepare_trailing_input_included_tail_replay_fixture().await;
    rewrite_trailing_input_included_tail_replay_fixture(
        &fixture.root,
        &fixture.first_bibliography_body,
        &fixture.tail_filler,
    );

    let dirty_files = match order {
        TrailingInputIncludedTailReplayBaselineOrder::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("refsa.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        TrailingInputIncludedTailReplayBaselineOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("refsa.bbl"),
        ],
    };
    let second =
        compile_trailing_input_included_tail_replay_second_pass(&fixture, &dirty_files).await;

    assert_trailing_input_included_tail_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        &fixture.bibliography_checkpoint_id,
        fixture.bibliography_checkpoint_page_index,
    );
}

async fn run_trailing_input_included_tail_dirty_replay(
    noise: TrailingInputIncludedTailReplayNoise,
    dirty_files: &[&str],
    replay_start: TrailingInputIncludedTailReplayStart,
) {
    let fixture = prepare_trailing_input_included_tail_replay_fixture().await;
    rewrite_trailing_input_included_tail_replay_fixture(
        &fixture.root,
        &fixture.first_bibliography_body,
        &fixture.tail_filler,
    );
    match noise {
        TrailingInputIncludedTailReplayNoise::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        TrailingInputIncludedTailReplayNoise::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second =
        compile_trailing_input_included_tail_replay_second_pass(&fixture, &dirty_files).await;

    let (expected_checkpoint_id, expected_start_page_index) = match replay_start {
        TrailingInputIncludedTailReplayStart::Preamble => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
        TrailingInputIncludedTailReplayStart::Bibliography => (
            fixture.bibliography_checkpoint_id.as_str(),
            fixture.bibliography_checkpoint_page_index,
        ),
    };
    assert_trailing_input_included_tail_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

async fn run_trailing_input_included_tail_replay_case(case: TrailingInputIncludedTailReplayCase) {
    match case {
        TrailingInputIncludedTailReplayCase::Baseline => {
            run_trailing_input_included_tail_baseline_replay(
                TrailingInputIncludedTailReplayBaselineOrder::Baseline,
            )
            .await;
        }
        TrailingInputIncludedTailReplayCase::Reversed => {
            run_trailing_input_included_tail_baseline_replay(
                TrailingInputIncludedTailReplayBaselineOrder::Reversed,
            )
            .await;
        }
        TrailingInputIncludedTailReplayCase::UntrackedFollows => {
            run_trailing_input_included_tail_dirty_replay(
                TrailingInputIncludedTailReplayNoise::Untracked,
                &["refsb.bbl", "refsa.bbl", "sections/tail.tex", "notes.txt"],
                TrailingInputIncludedTailReplayStart::Preamble,
            )
            .await;
        }
        TrailingInputIncludedTailReplayCase::UntrackedPrecedes => {
            run_trailing_input_included_tail_dirty_replay(
                TrailingInputIncludedTailReplayNoise::Untracked,
                &["notes.txt", "refsb.bbl", "refsa.bbl", "sections/tail.tex"],
                TrailingInputIncludedTailReplayStart::Bibliography,
            )
            .await;
        }
        TrailingInputIncludedTailReplayCase::UnreadableFollows => {
            run_trailing_input_included_tail_dirty_replay(
                TrailingInputIncludedTailReplayNoise::Unreadable,
                &["refsb.bbl", "refsa.bbl", "sections/tail.tex", "notes.txt"],
                TrailingInputIncludedTailReplayStart::Preamble,
            )
            .await;
        }
        TrailingInputIncludedTailReplayCase::UnreadablePrecedes => {
            run_trailing_input_included_tail_dirty_replay(
                TrailingInputIncludedTailReplayNoise::Unreadable,
                &["notes.txt", "refsb.bbl", "refsa.bbl", "sections/tail.tex"],
                TrailingInputIncludedTailReplayStart::Preamble,
            )
            .await;
        }
    }
}

type TailReplayCase = TrailingInputIncludedTailReplayCase;

async fn run_tail_replay_case(case: TailReplayCase) {
    run_trailing_input_included_tail_replay_case(case).await;
}

fn assert_trailing_input_included_tail_replay(
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
