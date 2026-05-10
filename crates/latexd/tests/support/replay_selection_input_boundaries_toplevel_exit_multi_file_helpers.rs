struct ReplaySelectionToplevelExitMultiFileFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
}

enum ReplaySelectionToplevelExitMultiFileCase {
    AppendixFirst,
    MainFirst,
}

type BoundaryExitMultiCase = ReplaySelectionToplevelExitMultiFileCase;

async fn run_boundary_exit_multi_case(case: BoundaryExitMultiCase) {
    run_replay_selection_toplevel_exit_multi_file_case(case).await;
}

async fn prepare_replay_selection_toplevel_exit_multi_file_fixture()
-> ReplaySelectionToplevelExitMultiFileFixture {
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
    let appendix_filler = (0..1800)
        .map(|index| format!("late{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}} intro \\input{{sections/tail}} after-old {appendix_filler} \\input{{sections/appendix}} \\end{{document}}"
        ),
    )
    .expect("write main");

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
    assert!(first.page_metadata.len() > 1);

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(tex_vm::VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .expect("toplevel input exit checkpoint");
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
        "appendix should land later than the toplevel input exit boundary"
    );

    ReplaySelectionToplevelExitMultiFileFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_page_index_after: expected_checkpoint.meta.page_index_after,
    }
}

fn rewrite_replay_selection_toplevel_exit_multi_file(
    fixture: &ReplaySelectionToplevelExitMultiFileFixture,
) {
    let rewritten_main = fs::read_to_string(fixture.root.join("main.tex").as_std_path())
        .expect("read main")
        .replace("after-old", "after-new");
    fs::write(fixture.root.join("main.tex"), rewritten_main).expect("rewrite main");
    fs::write(fixture.root.join("sections/appendix.tex"), "appendix-new")
        .expect("rewrite appendix");
}

async fn compile_replay_selection_toplevel_exit_multi_file_second_pass(
    fixture: &ReplaySelectionToplevelExitMultiFileFixture,
    changed_files: &[Utf8PathBuf],
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: changed_files.to_vec(),
        })
        .await
        .expect("second build should succeed")
}

fn assert_replay_selection_toplevel_exit_multi_file_prefers_exit_boundary(
    fixture: &ReplaySelectionToplevelExitMultiFileFixture,
    second: &CompileOutcome,
    changed_files: &[Utf8PathBuf],
) {
    assert_eq!(
        second.page_metadata.len(),
        fixture.first.page_metadata.len()
    );
    assert_eq!(
        second.reused_checkpoint_id,
        Some(fixture.expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, changed_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id,
        Some(fixture.expected_checkpoint_id.clone())
    );
    assert_eq!(
        build_meta.start_page_index,
        fixture.expected_page_index_after
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

async fn run_replay_selection_toplevel_exit_multi_file_case(
    case: ReplaySelectionToplevelExitMultiFileCase,
) {
    let fixture = prepare_replay_selection_toplevel_exit_multi_file_fixture().await;
    rewrite_replay_selection_toplevel_exit_multi_file(&fixture);
    let changed_files = match case {
        ReplaySelectionToplevelExitMultiFileCase::AppendixFirst => vec![
            Utf8PathBuf::from("sections/appendix.tex"),
            Utf8PathBuf::from("main.tex"),
        ],
        ReplaySelectionToplevelExitMultiFileCase::MainFirst => vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/appendix.tex"),
        ],
    };
    let second =
        compile_replay_selection_toplevel_exit_multi_file_second_pass(&fixture, &changed_files)
            .await;
    assert_replay_selection_toplevel_exit_multi_file_prefers_exit_boundary(
        &fixture,
        &second,
        &changed_files,
    );
}
