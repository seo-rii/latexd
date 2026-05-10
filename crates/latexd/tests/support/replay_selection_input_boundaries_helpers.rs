struct EarlierPageInputBoundaryFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first_filler: String,
    second_filler: String,
    appendix_filler: Option<String>,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
}

struct SamePageInputBoundaryFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    filler: String,
    appendix_filler: Option<String>,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
}

async fn prepare_earlier_page_input_boundary_fixture(
    with_appendix: bool,
) -> EarlierPageInputBoundaryFixture {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let first_filler = (0..1500)
        .map(|index| format!("bodya{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let second_filler = (0..1500)
        .map(|index| format!("bodyb{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_suffix = (0..if with_appendix { 800 } else { 900 })
        .map(|index| format!("tail{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let appendix_filler = with_appendix.then(|| {
        (0..1800)
            .map(|index| format!("appendix{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    });
    fs::write(root.join("sections/a.tex"), "alpha-old").expect("write first input");
    fs::write(root.join("sections/b.tex"), "beta-old").expect("write second input");
    if with_appendix {
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");
    }
    let original_main = if let Some(appendix_filler) = &appendix_filler {
        format!(
            "\\documentclass{{article}}\\begin{{document}} {first_filler} \\input{{sections/a}} {second_filler} \\input{{sections/b}} {original_suffix} {appendix_filler} \\input{{sections/appendix}} \\end{{document}}"
        )
    } else {
        format!(
            "\\documentclass{{article}}\\begin{{document}} {first_filler} \\input{{sections/a}} {second_filler} \\input{{sections/b}} {original_suffix} \\end{{document}}"
        )
    };
    fs::write(root.join("main.tex"), &original_main).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let changed_files = if with_appendix {
        vec![Utf8PathBuf::from("main.tex")]
    } else {
        vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/a.tex"),
            Utf8PathBuf::from("sections/b.tex"),
        ]
    };
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files,
        })
        .await
        .expect("first build should succeed");

    let input_page_indexes = ["sections/a.tex", "sections/b.tex"]
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
                .expect("input page")
        })
        .collect::<Vec<_>>();
    assert!(input_page_indexes[0] > 0);
    assert!(input_page_indexes[1] > input_page_indexes[0]);

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                    path == &Utf8PathBuf::from("sections/a.tex")
                        || path == &Utf8PathBuf::from("sections/b.tex")
                })
        })
        .min_by_key(|checkpoint| {
            (
                checkpoint.meta.page_index_after,
                checkpoint.meta.output_start_utf8,
            )
        })
        .expect("earlier input-page checkpoint");

    if with_appendix {
        let appendix_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .expect("appendix checkpoint");
        assert!(
            appendix_checkpoint.meta.page_index_after > expected_checkpoint.meta.page_index_after,
            "appendix should land later than the earlier cross-page input checkpoint"
        );
    }

    EarlierPageInputBoundaryFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first_filler,
        second_filler,
        appendix_filler,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_page_index_after: expected_checkpoint.meta.page_index_after,
    }
}

async fn prepare_same_page_input_boundary_fixture(
    with_appendix: bool,
) -> SamePageInputBoundaryFixture {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let filler = (0..1650)
        .map(|index| format!("body{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_suffix = (0..600)
        .map(|index| format!("tail{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let appendix_filler = with_appendix.then(|| {
        (0..1800)
            .map(|index| format!("appendix{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    });
    fs::write(root.join("sections/a.tex"), "alpha-old").expect("write first input");
    fs::write(root.join("sections/b.tex"), "beta-old").expect("write second input");
    if with_appendix {
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");
    }
    let original_main = if let Some(appendix_filler) = &appendix_filler {
        format!(
            "\\documentclass{{article}}\\begin{{document}} {filler} \\input{{sections/a}} \\input{{sections/b}} {original_suffix} {appendix_filler} \\input{{sections/appendix}} \\end{{document}}"
        )
    } else {
        format!(
            "\\documentclass{{article}}\\begin{{document}} {filler} \\input{{sections/a}} \\input{{sections/b}} {original_suffix} \\end{{document}}"
        )
    };
    fs::write(root.join("main.tex"), &original_main).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let changed_files = if with_appendix {
        vec![Utf8PathBuf::from("main.tex")]
    } else {
        vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/a.tex"),
            Utf8PathBuf::from("sections/b.tex"),
        ]
    };
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files,
        })
        .await
        .expect("first build should succeed");

    let input_page_indexes = ["sections/a.tex", "sections/b.tex"]
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
                .expect("input page")
        })
        .collect::<Vec<_>>();
    assert_eq!(input_page_indexes[0], input_page_indexes[1]);
    assert!(input_page_indexes[0] > 0);

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                    path == &Utf8PathBuf::from("sections/a.tex")
                        || path == &Utf8PathBuf::from("sections/b.tex")
                })
        })
        .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
        .expect("earlier same-page input checkpoint");

    if with_appendix {
        let appendix_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .expect("appendix checkpoint");
        assert!(
            appendix_checkpoint.meta.page_index_after > expected_checkpoint.meta.page_index_after,
            "appendix should land later than the earlier same-page input checkpoint"
        );
    }

    SamePageInputBoundaryFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        filler,
        appendix_filler,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_page_index_after: expected_checkpoint.meta.page_index_after,
    }
}

fn assert_input_boundary_replay_selection(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert_eq!(build_meta.start_page_index, expected_page_index_after);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

fn input_boundary_edited_suffix(
    tail_start: usize,
    tail_end: usize,
    edit_start: usize,
    edit_end: usize,
) -> String {
    format!(
        "{} {}",
        (tail_start..tail_end)
            .map(|index| format!("tail{index:04}"))
            .collect::<Vec<_>>()
            .join(" "),
        (edit_start..edit_end)
            .map(|index| format!("edit{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn input_boundary_changed_files(paths: &[&str]) -> Vec<Utf8PathBuf> {
    paths.iter().map(|path| Utf8PathBuf::from(*path)).collect()
}

async fn compile_input_boundary_rev2(
    root: &Utf8Path,
    world: &ProjectWorld,
    driver: &CompilerDriver,
    build_root: &Utf8Path,
    changed_files: &[&str],
) -> (CompileOutcome, Vec<Utf8PathBuf>) {
    let dirty_files = input_boundary_changed_files(changed_files);
    let second = driver
        .compile(CompileRequest {
            root: root.to_path_buf(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.to_path_buf(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("second build should succeed");
    (second, dirty_files)
}

async fn compile_same_page_input_boundary_rev2(
    fixture: &SamePageInputBoundaryFixture,
    changed_files: &[&str],
) -> (CompileOutcome, Vec<Utf8PathBuf>) {
    compile_input_boundary_rev2(
        &fixture.root,
        &fixture.world,
        &fixture.driver,
        &fixture.build_root,
        changed_files,
    )
    .await
}

async fn compile_earlier_page_input_boundary_rev2(
    fixture: &EarlierPageInputBoundaryFixture,
    changed_files: &[&str],
) -> (CompileOutcome, Vec<Utf8PathBuf>) {
    compile_input_boundary_rev2(
        &fixture.root,
        &fixture.world,
        &fixture.driver,
        &fixture.build_root,
        changed_files,
    )
    .await
}

enum ReplaySelectionInputBoundaryToplevelCase {
    SamePage,
    SamePageAppendix,
    SamePageInputOnly,
    SamePageCp0,
    EarlierPage,
    EarlierPageAppendix,
    EarlierPageInputOnly,
}

type BoundaryTopCase = ReplaySelectionInputBoundaryToplevelCase;

async fn run_boundary_top_case(case: BoundaryTopCase) {
    run_replay_selection_input_boundary_toplevel(case).await;
}

async fn run_replay_selection_input_boundary_toplevel(
    case: ReplaySelectionInputBoundaryToplevelCase,
) {
    match case {
        ReplaySelectionInputBoundaryToplevelCase::SamePageInputOnly => {
            let fixture = prepare_same_page_input_boundary_fixture(false).await;
            fs::write(fixture.root.join("sections/a.tex"), "alpha-new")
                .expect("rewrite first input");
            fs::write(fixture.root.join("sections/b.tex"), "beta-new")
                .expect("rewrite second input");

            let (second, dirty_files) = compile_same_page_input_boundary_rev2(
                &fixture,
                &["sections/b.tex", "sections/a.tex"],
            )
            .await;

            assert_input_boundary_replay_selection(
                &fixture.build_root,
                &second,
                dirty_files,
                fixture.expected_checkpoint_id.clone(),
                fixture.expected_page_index_after,
            );
        }
        ReplaySelectionInputBoundaryToplevelCase::SamePageCp0 => {
            let fixture = prepare_same_page_input_boundary_fixture(false).await;
            let first_checkpoints =
                load_checkpoint_bundle(&fixture.build_root.join("rev-1/checkpoints.json"))
                    .expect("load rev1 checkpoints");
            let cp0_id = first_checkpoints.checkpoints[0].meta.checkpoint_id.clone();

            let second_main = fs::read_to_string(fixture.root.join("main.tex"))
                .expect("read main")
                .replacen("\\begin{document}", "\\begin{document} Intro", 1);
            fs::write(fixture.root.join("main.tex"), second_main).expect("rewrite main");
            fs::write(fixture.root.join("sections/a.tex"), "alpha-new")
                .expect("rewrite first input");
            fs::write(fixture.root.join("sections/b.tex"), "beta-new")
                .expect("rewrite second input");

            let (second, dirty_files) = compile_same_page_input_boundary_rev2(
                &fixture,
                &["main.tex", "sections/b.tex", "sections/a.tex"],
            )
            .await;

            assert_input_boundary_replay_selection(
                &fixture.build_root,
                &second,
                dirty_files,
                cp0_id,
                0,
            );
        }
        ReplaySelectionInputBoundaryToplevelCase::SamePage
        | ReplaySelectionInputBoundaryToplevelCase::SamePageAppendix => {
            let with_appendix = matches!(
                case,
                ReplaySelectionInputBoundaryToplevelCase::SamePageAppendix
            );
            let fixture = prepare_same_page_input_boundary_fixture(with_appendix).await;
            let edited_suffix = input_boundary_edited_suffix(0, 300, 300, 600);
            let second_main = if let Some(appendix_filler) = &fixture.appendix_filler {
                format!(
                    "\\documentclass{{article}}\\begin{{document}} {} \\input{{sections/a}} \\input{{sections/b}} {edited_suffix} {appendix_filler} \\input{{sections/appendix}} \\end{{document}}",
                    fixture.filler,
                )
            } else {
                format!(
                    "\\documentclass{{article}}\\begin{{document}} {} \\input{{sections/a}} \\input{{sections/b}} {edited_suffix} \\end{{document}}",
                    fixture.filler,
                )
            };
            fs::write(fixture.root.join("main.tex"), &second_main).expect("rewrite main");
            fs::write(fixture.root.join("sections/a.tex"), "alpha-new")
                .expect("rewrite first input");
            fs::write(fixture.root.join("sections/b.tex"), "beta-new")
                .expect("rewrite second input");
            let changed_files = if with_appendix {
                fs::write(fixture.root.join("sections/appendix.tex"), "appendix-new")
                    .expect("rewrite appendix");
                vec![
                    "main.tex",
                    "sections/appendix.tex",
                    "sections/b.tex",
                    "sections/a.tex",
                ]
            } else {
                vec!["main.tex", "sections/b.tex", "sections/a.tex"]
            };

            let (second, dirty_files) =
                compile_same_page_input_boundary_rev2(&fixture, &changed_files).await;

            assert_input_boundary_replay_selection(
                &fixture.build_root,
                &second,
                dirty_files,
                fixture.expected_checkpoint_id.clone(),
                fixture.expected_page_index_after,
            );
        }
        ReplaySelectionInputBoundaryToplevelCase::EarlierPageInputOnly => {
            let fixture = prepare_earlier_page_input_boundary_fixture(false).await;
            fs::write(fixture.root.join("sections/a.tex"), "alpha-new")
                .expect("rewrite first input");
            fs::write(fixture.root.join("sections/b.tex"), "beta-new")
                .expect("rewrite second input");

            let (second, dirty_files) = compile_earlier_page_input_boundary_rev2(
                &fixture,
                &["sections/b.tex", "sections/a.tex"],
            )
            .await;

            assert_input_boundary_replay_selection(
                &fixture.build_root,
                &second,
                dirty_files,
                fixture.expected_checkpoint_id.clone(),
                fixture.expected_page_index_after,
            );
        }
        ReplaySelectionInputBoundaryToplevelCase::EarlierPage
        | ReplaySelectionInputBoundaryToplevelCase::EarlierPageAppendix => {
            let with_appendix = matches!(
                case,
                ReplaySelectionInputBoundaryToplevelCase::EarlierPageAppendix
            );
            let fixture = prepare_earlier_page_input_boundary_fixture(with_appendix).await;
            let edited_suffix = if with_appendix {
                input_boundary_edited_suffix(0, 400, 400, 800)
            } else {
                input_boundary_edited_suffix(0, 450, 450, 900)
            };
            let second_main = if let Some(appendix_filler) = &fixture.appendix_filler {
                format!(
                    "\\documentclass{{article}}\\begin{{document}} {} \\input{{sections/a}} {} \\input{{sections/b}} {edited_suffix} {appendix_filler} \\input{{sections/appendix}} \\end{{document}}",
                    fixture.first_filler, fixture.second_filler,
                )
            } else {
                format!(
                    "\\documentclass{{article}}\\begin{{document}} {} \\input{{sections/a}} {} \\input{{sections/b}} {edited_suffix} \\end{{document}}",
                    fixture.first_filler, fixture.second_filler,
                )
            };
            fs::write(fixture.root.join("main.tex"), &second_main).expect("rewrite main");
            fs::write(fixture.root.join("sections/a.tex"), "alpha-new")
                .expect("rewrite first input");
            fs::write(fixture.root.join("sections/b.tex"), "beta-new")
                .expect("rewrite second input");
            let changed_files = if with_appendix {
                fs::write(fixture.root.join("sections/appendix.tex"), "appendix-new")
                    .expect("rewrite appendix");
                vec![
                    "main.tex",
                    "sections/appendix.tex",
                    "sections/b.tex",
                    "sections/a.tex",
                ]
            } else {
                vec!["main.tex", "sections/b.tex", "sections/a.tex"]
            };

            let (second, dirty_files) =
                compile_earlier_page_input_boundary_rev2(&fixture, &changed_files).await;

            assert_input_boundary_replay_selection(
                &fixture.build_root,
                &second,
                dirty_files,
                fixture.expected_checkpoint_id.clone(),
                fixture.expected_page_index_after,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum ReplaySelectionLateInputNoiseDirtyKind {
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum ReplaySelectionLateInputNoiseDirtyOrder {
    FollowsLateInput,
    PrecedesLateInput,
}

#[derive(Clone, Copy)]
enum ReplaySelectionLateInputNoiseCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

type BoundaryLateInputNoiseCase = ReplaySelectionLateInputNoiseCase;

async fn run_boundary_late_input_noise_case(case: BoundaryLateInputNoiseCase) {
    run_replay_selection_input_boundaries_late_input_noise_case(case).await;
}

async fn run_replay_selection_input_boundaries_late_input_noise_case(
    case: ReplaySelectionLateInputNoiseCase,
) {
    let (dirty_kind, dirty_order) = match case {
        ReplaySelectionLateInputNoiseCase::UntrackedFollows => (
            ReplaySelectionLateInputNoiseDirtyKind::Untracked,
            ReplaySelectionLateInputNoiseDirtyOrder::FollowsLateInput,
        ),
        ReplaySelectionLateInputNoiseCase::UntrackedPrecedes => (
            ReplaySelectionLateInputNoiseDirtyKind::Untracked,
            ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput,
        ),
        ReplaySelectionLateInputNoiseCase::UnreadableFollows => (
            ReplaySelectionLateInputNoiseDirtyKind::Unreadable,
            ReplaySelectionLateInputNoiseDirtyOrder::FollowsLateInput,
        ),
        ReplaySelectionLateInputNoiseCase::UnreadablePrecedes => (
            ReplaySelectionLateInputNoiseDirtyKind::Unreadable,
            ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput,
        ),
    };

    run_replay_selection_input_boundaries_late_input_noise(dirty_kind, dirty_order).await;
}

async fn run_replay_selection_input_boundaries_late_input_noise(
    dirty_kind: ReplaySelectionLateInputNoiseDirtyKind,
    dirty_order: ReplaySelectionLateInputNoiseDirtyOrder,
) {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let mut words = (0..1800)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    words.insert(1500, "\\input{sections/tail}".to_string());
    fs::write(root.join("sections/tail.tex"), "tail-A").expect("write tail input");
    fs::write(root.join("main.tex"), words.join(" ")).expect("write main tex");

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
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let input_page_index = first
        .page_metadata
        .iter()
        .find(|page| {
            page.source_spans
                .iter()
                .any(|span| span.file == Utf8PathBuf::from("sections/tail.tex"))
        })
        .map(|page| page.index)
        .expect("input page");
    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expects_input_boundary_replay = matches!(
        (dirty_kind, dirty_order),
        (
            ReplaySelectionLateInputNoiseDirtyKind::Untracked,
            ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput
        )
    );
    let (expected_checkpoint_id, expected_page_index_after) = if expects_input_boundary_replay {
        assert!(input_page_index > 0);
        let expected_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .expect("input boundary checkpoint");

        (
            expected_checkpoint.meta.checkpoint_id.clone(),
            expected_checkpoint.meta.page_index_after,
        )
    } else {
        (
            first_checkpoints.checkpoints[0].meta.checkpoint_id.clone(),
            0,
        )
    };

    fs::write(root.join("sections/tail.tex"), "tail-B").expect("rewrite tail input");
    match dirty_kind {
        ReplaySelectionLateInputNoiseDirtyKind::Untracked => {
            fs::write(root.join("notes.txt"), "fresh scratch notes").expect("write notes");
        }
        ReplaySelectionLateInputNoiseDirtyKind::Unreadable => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
    }
    let dirty_files = match dirty_order {
        ReplaySelectionLateInputNoiseDirtyOrder::FollowsLateInput => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("notes.txt"),
        ],
        ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput => vec![
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
    };
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("second build should succeed");

    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert_eq!(build_meta.start_page_index, expected_page_index_after);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    if expects_input_boundary_replay {
        assert!(build_meta.rebuilt_page_count >= 1);
    }
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}
