struct ToplevelCheckpointPreferenceFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    original_main: String,
    intro_filler: String,
    first_bibliography_body: String,
    preamble_checkpoint_id: String,
    bibliography_checkpoint_id: String,
}

struct IncludedBodyCheckpointPreferenceFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    appendix_filler: String,
    first_bibliography_body: String,
    preamble_checkpoint_id: String,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
}

async fn prepare_toplevel_checkpoint_preference_fixture() -> ToplevelCheckpointPreferenceFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
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
    let intro_filler = "mixed bibliography page ordering filler ".repeat(220);
    let first_bibliography_body = (0..1800)
        .map(|index| format!("alpha{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_suffix = (0..900)
        .map(|index| format!("tail{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_main = format!(
        "\\documentclass{{article}}\\begin{{document}}Intro. {intro_filler} \\cite{{alpha}} and \\cite{{beta}}. {original_suffix}\\bibliography{{refsa,refsb}}\\end{{document}}"
    );
    fs::write(root.join("main.tex"), &original_main).expect("write main");
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
    let first = driver
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
            ],
        })
        .await
        .expect("first semantic aux build should succeed");

    let bibliography_page_indexes = ["refsa.bbl", "refsb.bbl"]
        .into_iter()
        .map(|path| {
            first
                .page_metadata
                .iter()
                .find(|page| {
                    page.source_spans
                        .iter()
                        .any(|span| span.file == Utf8PathBuf::from(path))
                })
                .map(|page| page.index)
                .expect("bibliography page")
        })
        .collect::<Vec<_>>();
    assert!(
        bibliography_page_indexes[0] < bibliography_page_indexes[1],
        "fixture should place refsa before refsb across pages, saw {:?}",
        bibliography_page_indexes
    );
    assert!(bibliography_page_indexes[0] > 0);

    let first_checkpoints =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let preamble_checkpoint_id = first_checkpoints.checkpoints[0].meta.checkpoint_id.clone();
    let bibliography_checkpoint_id = first_checkpoints
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
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("earlier bibliography-page checkpoint");

    ToplevelCheckpointPreferenceFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        original_main,
        intro_filler,
        first_bibliography_body,
        preamble_checkpoint_id,
        bibliography_checkpoint_id,
    }
}

fn rewrite_semantically_equal_toplevel_multi_bibliography_replay(
    root: &Utf8Path,
    intro_filler: &str,
    first_bibliography_body: &str,
) -> String {
    let edited_suffix = format!(
        "{} {}",
        (0..450)
            .map(|index| format!("tail{index:04}"))
            .collect::<Vec<_>>()
            .join(" "),
        (450..900)
            .map(|index| format!("edit{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let second_main = format!(
        "\\documentclass{{article}}\\begin{{document}}Intro. {intro_filler} \\cite{{alpha}} and \\cite{{beta}}. {edited_suffix}\\bibliography{{refsa,refsb}}\\end{{document}}"
    );
    fs::write(root.join("main.tex"), &second_main).expect("rewrite main");
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
    second_main
}

fn expected_toplevel_checkpoint_id(
    first: &CompileOutcome,
    build_root: &Utf8Path,
    original_main: &str,
    second_main: &str,
) -> String {
    let first_checkpoints =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let diff_offset = original_main
        .bytes()
        .zip(second_main.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.meta.page_index_after > 0)
        .take_while(|checkpoint| checkpoint.meta.source_offset_utf8 <= diff_offset as u32)
        .last()
        .and_then(|offset_checkpoint| {
            let span_start_page = first.page_metadata.iter().find_map(|page| {
                page.source_spans
                    .iter()
                    .find(|span| span.file == Utf8PathBuf::from("main.tex"))
                    .and_then(|span| {
                        if (diff_offset as u32) < span.end_utf8 {
                            Some(page.index)
                        } else {
                            None
                        }
                    })
            })?;
            let expected_page_index_after =
                offset_checkpoint.meta.page_index_after.min(span_start_page);
            first_checkpoints
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.meta.page_index_after == expected_page_index_after)
                .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        })
        .expect("expected main checkpoint")
}

fn assert_toplevel_checkpoint_preference_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    expected_checkpoint_id: String,
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert!(build_meta.start_page_index >= 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_aux_backdated);
}

async fn compile_toplevel_checkpoint_preference_second_pass(
    fixture: &ToplevelCheckpointPreferenceFixture,
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

#[derive(Clone, Copy)]
enum ToplevelExtraDirty {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum ToplevelExpectedReplay {
    MainCheckpoint { require_earlier: bool },
    Preamble,
}

enum ToplevelCheckpointPreferenceDirtyCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

enum ToplevelCheckpointPreferenceReplayCase {
    Baseline,
    Reversed,
}

async fn run_toplevel_checkpoint_preference_case(
    extra_dirty: ToplevelExtraDirty,
    dirty_files: &[&str],
    expected_replay: ToplevelExpectedReplay,
) {
    let fixture = prepare_toplevel_checkpoint_preference_fixture().await;
    let second_main = rewrite_semantically_equal_toplevel_multi_bibliography_replay(
        &fixture.root,
        &fixture.intro_filler,
        &fixture.first_bibliography_body,
    );
    match extra_dirty {
        ToplevelExtraDirty::NoExtraDirty => {}
        ToplevelExtraDirty::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        ToplevelExtraDirty::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second = compile_toplevel_checkpoint_preference_second_pass(&fixture, &dirty_files).await;

    match expected_replay {
        ToplevelExpectedReplay::MainCheckpoint { require_earlier } => {
            let expected_checkpoint_id = expected_toplevel_checkpoint_id(
                &fixture.first,
                &fixture.build_root,
                &fixture.original_main,
                &second_main,
            );
            if require_earlier {
                assert_ne!(
                    expected_checkpoint_id, fixture.bibliography_checkpoint_id,
                    "the main.tex replay checkpoint should be earlier than the bibliography replay candidate"
                );
            }
            assert_toplevel_checkpoint_preference_replay(
                &fixture.build_root,
                &second,
                dirty_files,
                expected_checkpoint_id,
            );
        }
        ToplevelExpectedReplay::Preamble => {
            assert_toplevel_checkpoint_preference_falls_back_to_preamble(
                &fixture,
                &second,
                &dirty_files,
            );
        }
    }
}

async fn run_toplevel_checkpoint_preference_dirty_case(
    case: ToplevelCheckpointPreferenceDirtyCase,
) {
    let (extra_dirty, dirty_files, expected_replay) = match case {
        ToplevelCheckpointPreferenceDirtyCase::UntrackedFollows => (
            ToplevelExtraDirty::Untracked,
            ["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"],
            ToplevelExpectedReplay::Preamble,
        ),
        ToplevelCheckpointPreferenceDirtyCase::UntrackedPrecedes => (
            ToplevelExtraDirty::Untracked,
            ["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"],
            ToplevelExpectedReplay::MainCheckpoint {
                require_earlier: true,
            },
        ),
        ToplevelCheckpointPreferenceDirtyCase::UnreadableFollows => (
            ToplevelExtraDirty::Unreadable,
            ["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"],
            ToplevelExpectedReplay::Preamble,
        ),
        ToplevelCheckpointPreferenceDirtyCase::UnreadablePrecedes => (
            ToplevelExtraDirty::Unreadable,
            ["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"],
            ToplevelExpectedReplay::Preamble,
        ),
    };
    run_toplevel_checkpoint_preference_case(extra_dirty, &dirty_files, expected_replay).await;
}

async fn run_toplevel_checkpoint_preference_replay_case(
    dirty_files: &[&str],
    require_earlier: bool,
) {
    run_toplevel_checkpoint_preference_case(
        ToplevelExtraDirty::NoExtraDirty,
        dirty_files,
        ToplevelExpectedReplay::MainCheckpoint { require_earlier },
    )
    .await;
}

async fn run_toplevel_checkpoint_preference_replay_case_variant(
    case: ToplevelCheckpointPreferenceReplayCase,
) {
    let (dirty_files, require_earlier) = match case {
        ToplevelCheckpointPreferenceReplayCase::Baseline => {
            (&["main.tex", "refsb.bbl", "refsa.bbl"][..], true)
        }
        ToplevelCheckpointPreferenceReplayCase::Reversed => {
            (&["refsb.bbl", "refsa.bbl", "main.tex"][..], false)
        }
    };

    run_toplevel_checkpoint_preference_replay_case(dirty_files, require_earlier).await;
}

type TopCpDirty = ToplevelCheckpointPreferenceDirtyCase;
type TopCpReplay = ToplevelCheckpointPreferenceReplayCase;

async fn run_top_cp_dirty(case: TopCpDirty) {
    run_toplevel_checkpoint_preference_dirty_case(case).await;
}

async fn run_top_cp_replay(case: TopCpReplay) {
    run_toplevel_checkpoint_preference_replay_case_variant(case).await;
}

fn assert_toplevel_checkpoint_preference_falls_back_to_preamble(
    fixture: &ToplevelCheckpointPreferenceFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(fixture.preamble_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id,
        Some(fixture.preamble_checkpoint_id.clone())
    );
    assert_eq!(build_meta.start_page_index, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_aux_backdated);
}

async fn prepare_included_body_checkpoint_preference_fixture()
-> IncludedBodyCheckpointPreferenceFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "cross page bibliography replay filler ".repeat(220);
    let appendix_filler = "appendix trailing filler text ".repeat(180);
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\input{sections/appendix}\\bibliography{refsa,refsb}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Cite \\cite{{alpha}} and \\cite{{beta}}. {body_filler}"),
    )
    .expect("write body");
    fs::write(
        root.join("sections/appendix.tex"),
        format!("Appendix A. {appendix_filler}"),
    )
    .expect("write appendix");
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
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let preamble_checkpoint_id = first_bundle.checkpoints[0].meta.checkpoint_id.clone();
    let expected_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/appendix.tex"))
        })
        .expect("appendix input boundary");
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
    assert!(
        expected_checkpoint.meta.page_index_after < bibliography_checkpoint.meta.page_index_after,
        "appendix input boundary should stay earlier than cross-page bibliography replay boundary"
    );

    IncludedBodyCheckpointPreferenceFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        appendix_filler,
        first_bibliography_body,
        preamble_checkpoint_id,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_page_index_after: expected_checkpoint.meta.page_index_after,
    }
}

fn rewrite_semantically_equal_included_body_multi_bibliography_replay(
    root: &Utf8Path,
    appendix_filler: &str,
    first_bibliography_body: &str,
) {
    fs::write(
        root.join("sections/appendix.tex"),
        format!("Appendix B. {appendix_filler}"),
    )
    .expect("rewrite appendix");
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
}

#[derive(Clone, Copy)]
enum IncludedBodyCheckpointPreferenceExtraDirtyKind {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum IncludedBodyCheckpointPreferenceExpectedReplay {
    EarlierInput,
    Preamble,
}

enum IncludedBodyCheckpointPreferenceDirtyNoiseCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

enum IncludedBodyCheckpointPreferenceBaselineCase {
    Baseline,
    Reversed,
}

async fn run_included_body_checkpoint_preference_case(
    extra_dirty: IncludedBodyCheckpointPreferenceExtraDirtyKind,
    dirty_files: &[&str],
    expected_replay: IncludedBodyCheckpointPreferenceExpectedReplay,
) {
    let fixture = prepare_included_body_checkpoint_preference_fixture().await;
    rewrite_semantically_equal_included_body_multi_bibliography_replay(
        &fixture.root,
        &fixture.appendix_filler,
        &fixture.first_bibliography_body,
    );
    match extra_dirty {
        IncludedBodyCheckpointPreferenceExtraDirtyKind::NoExtraDirty => {}
        IncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        IncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second = fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("second semantic aux build should succeed");
    let (expected_checkpoint_id, expected_page_index_after) = match expected_replay {
        IncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput => (
            fixture.expected_checkpoint_id.clone(),
            fixture.expected_page_index_after,
        ),
        IncludedBodyCheckpointPreferenceExpectedReplay::Preamble => {
            (fixture.preamble_checkpoint_id.clone(), 0)
        }
    };

    assert_included_body_checkpoint_preference_replay(
        &fixture.build_root,
        &second,
        dirty_files,
        expected_checkpoint_id,
        expected_page_index_after,
    );
}

async fn run_included_body_checkpoint_preference_dirty_noise_case(
    case: IncludedBodyCheckpointPreferenceDirtyNoiseCase,
) {
    let (extra_dirty, dirty_files, expected_replay) = match case {
        IncludedBodyCheckpointPreferenceDirtyNoiseCase::UntrackedFollows => (
            IncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked,
            vec![
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
                "notes.txt",
            ],
            IncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
        IncludedBodyCheckpointPreferenceDirtyNoiseCase::UntrackedPrecedes => (
            IncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked,
            vec![
                "notes.txt",
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
            ],
            IncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput,
        ),
        IncludedBodyCheckpointPreferenceDirtyNoiseCase::UnreadableFollows => (
            IncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable,
            vec![
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
                "notes.txt",
            ],
            IncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
        IncludedBodyCheckpointPreferenceDirtyNoiseCase::UnreadablePrecedes => (
            IncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable,
            vec![
                "notes.txt",
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
            ],
            IncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
    };
    run_included_body_checkpoint_preference_case(extra_dirty, &dirty_files, expected_replay).await;
}

async fn run_included_body_checkpoint_preference_baseline_case(
    case: IncludedBodyCheckpointPreferenceBaselineCase,
) {
    let dirty_files = match case {
        IncludedBodyCheckpointPreferenceBaselineCase::Baseline => {
            ["sections/appendix.tex", "refsb.bbl", "refsa.bbl"]
        }
        IncludedBodyCheckpointPreferenceBaselineCase::Reversed => {
            ["refsb.bbl", "refsa.bbl", "sections/appendix.tex"]
        }
    };
    run_included_body_checkpoint_preference_case(
        IncludedBodyCheckpointPreferenceExtraDirtyKind::NoExtraDirty,
        &dirty_files,
        IncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput,
    )
    .await;
}

type ReplayBodyCpBase = IncludedBodyCheckpointPreferenceBaselineCase;
type ReplayBodyCpNoise = IncludedBodyCheckpointPreferenceDirtyNoiseCase;

async fn run_replay_body_cp_base(case: ReplayBodyCpBase) {
    run_included_body_checkpoint_preference_baseline_case(case).await;
}

async fn run_replay_body_cp_noise(case: ReplayBodyCpNoise) {
    run_included_body_checkpoint_preference_dirty_noise_case(case).await;
}

fn assert_included_body_checkpoint_preference_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/appendix.tex")]
            .contains("Appendix B."),
        "executed appendix.tex should reflect the earlier tracked change"
    );
    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert_eq!(build_meta.start_page_index, expected_page_index_after);
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
