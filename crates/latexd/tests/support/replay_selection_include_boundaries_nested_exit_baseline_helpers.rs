struct ReplaySelectionNestedExitBaselineFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    has_appendix: bool,
    second_parent: String,
    expected_checkpoint_id: String,
    expected_start_page_index: usize,
}

enum ReplaySelectionNestedExitBaselineCase {
    Plain,
    MultiFileAppendix,
}

type NestedExitBaseCase = ReplaySelectionNestedExitBaselineCase;

async fn run_nested_exit_base_case(case: NestedExitBaseCase) {
    run_replay_selection_nested_exit_baseline(case).await;
}

async fn prepare_replay_selection_nested_exit_baseline_fixture(
    with_appendix: bool,
) -> ReplaySelectionNestedExitBaselineFixture {
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
    let mut words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    words.insert(900, "\\input{sections/tail}".to_string());
    words.push("after-old".to_string());
    let appendix_filler = (0..1800)
        .map(|index| format!("late{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    if with_appendix {
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");
    }
    fs::write(root.join("sections/parent.tex"), words.join(" ")).expect("write parent");
    let main = if with_appendix {
        format!(
            "alpha \\input{{sections/parent}} {appendix_filler} \\input{{sections/appendix}} omega"
        )
    } else {
        "alpha \\input{sections/parent} omega".to_string()
    };
    fs::write(root.join("main.tex"), main).expect("write main");

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
    if with_appendix {
        assert!(first.page_metadata.len() > 1);
    }

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(tex_vm::VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/parent.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .expect("nested exit checkpoint");
    assert!(first_checkpoints.checkpoints.iter().any(|checkpoint| {
        checkpoint.meta.kind == tex_checkpoint::CheckpointKind::Shipout
            && checkpoint.meta.resume_path.is_some()
    }));
    if with_appendix {
        let appendix_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind
                        == Some(tex_vm::VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .expect("appendix checkpoint");
        assert!(
            appendix_checkpoint.meta.page_index_after > expected_checkpoint.meta.page_index_after,
            "appendix should land later than the nested input exit boundary"
        );
    }

    let mut edited_words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    edited_words.insert(900, "\\input{sections/tail}".to_string());
    edited_words.push("after-new".to_string());

    ReplaySelectionNestedExitBaselineFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        has_appendix: with_appendix,
        second_parent: edited_words.join(" "),
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_start_page_index: expected_checkpoint.meta.page_index_after,
    }
}

fn rewrite_replay_selection_nested_exit_baseline(
    fixture: &ReplaySelectionNestedExitBaselineFixture,
) {
    fs::write(
        fixture.root.join("sections/parent.tex"),
        &fixture.second_parent,
    )
    .expect("rewrite parent");
    if fixture.has_appendix {
        fs::write(fixture.root.join("sections/appendix.tex"), "appendix-new")
            .expect("rewrite appendix");
    }
}

async fn compile_replay_selection_nested_exit_baseline_second_pass(
    fixture: &ReplaySelectionNestedExitBaselineFixture,
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
        .expect("second build should succeed")
}

fn assert_replay_selection_nested_exit_baseline(
    fixture: &ReplaySelectionNestedExitBaselineFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
) {
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(fixture.expected_checkpoint_id.as_str())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id.as_deref(),
        Some(fixture.expected_checkpoint_id.as_str())
    );
    assert_eq!(
        build_meta.start_page_index,
        fixture.expected_start_page_index
    );
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

async fn run_replay_selection_nested_exit_baseline(case: ReplaySelectionNestedExitBaselineCase) {
    let with_appendix = matches!(
        case,
        ReplaySelectionNestedExitBaselineCase::MultiFileAppendix
    );
    let fixture = prepare_replay_selection_nested_exit_baseline_fixture(with_appendix).await;
    rewrite_replay_selection_nested_exit_baseline(&fixture);

    let dirty_files = match case {
        ReplaySelectionNestedExitBaselineCase::Plain => {
            vec![Utf8PathBuf::from("sections/parent.tex")]
        }
        ReplaySelectionNestedExitBaselineCase::MultiFileAppendix => vec![
            Utf8PathBuf::from("sections/appendix.tex"),
            Utf8PathBuf::from("sections/parent.tex"),
        ],
    };
    let second =
        compile_replay_selection_nested_exit_baseline_second_pass(&fixture, &dirty_files).await;

    assert_replay_selection_nested_exit_baseline(&fixture, &second, &dirty_files);
}
