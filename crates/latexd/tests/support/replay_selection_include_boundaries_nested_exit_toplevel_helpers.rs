struct ReplaySelectionNestedExitToplevelFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    second_main: String,
    second_parent: String,
    has_appendix: bool,
    expected_checkpoint_id: String,
    expected_start_page_index: usize,
}

enum ReplaySelectionNestedExitToplevelCase {
    Plain,
    Appendix,
}

type NestedExitTopCase = ReplaySelectionNestedExitToplevelCase;

async fn run_nested_exit_top_case(case: NestedExitTopCase) {
    run_replay_selection_nested_exit_toplevel(case).await;
}

async fn prepare_replay_selection_nested_exit_toplevel_fixture(
    with_appendix: bool,
) -> ReplaySelectionNestedExitToplevelFixture {
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
    let mut words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    words.insert(900, "\\input{sections/tail}".to_string());
    words.push("after-old".to_string());
    let original_suffix = (0..1800)
        .map(|index| format!("late{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let appendix_filler = (0..1800)
        .map(|index| format!("appendix{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    fs::write(root.join("sections/parent.tex"), words.join(" ")).expect("write parent");
    if with_appendix {
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");
    }
    let original_main = if with_appendix {
        format!(
            "\\documentclass{{article}}\\begin{{document}} alpha \\input{{sections/parent}} {original_suffix} {appendix_filler} \\input{{sections/appendix}} omega \\end{{document}}"
        )
    } else {
        format!(
            "\\documentclass{{article}}\\begin{{document}} alpha \\input{{sections/parent}} {original_suffix} omega \\end{{document}}"
        )
    };
    fs::write(root.join("main.tex"), &original_main).expect("write main");

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
                && checkpoint.meta.resume_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/parent.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .expect("nested exit checkpoint");
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
    } else {
        let later_toplevel_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .filter(|checkpoint| {
                checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind
                        == Some(tex_vm::VmModuleCheckpointKind::Exit)
                    && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
            })
            .max_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
            .expect("later toplevel input exit checkpoint");
        assert!(
            later_toplevel_checkpoint.meta.output_start_utf8
                > expected_checkpoint.meta.output_start_utf8,
            "late toplevel edit candidate should land after the nested input exit boundary"
        );
    }

    let mut edited_words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    edited_words.insert(900, "\\input{sections/tail}".to_string());
    edited_words.push("after-new".to_string());
    let edited_suffix = format!(
        "{} {}",
        (0..900)
            .map(|index| format!("late{index:04}"))
            .collect::<Vec<_>>()
            .join(" "),
        (900..1800)
            .map(|index| format!("edit{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let second_main = if with_appendix {
        format!(
            "\\documentclass{{article}}\\begin{{document}} alpha \\input{{sections/parent}} {edited_suffix} {appendix_filler} \\input{{sections/appendix}} omega \\end{{document}}"
        )
    } else {
        format!(
            "\\documentclass{{article}}\\begin{{document}} alpha \\input{{sections/parent}} {edited_suffix} omega \\end{{document}}"
        )
    };

    ReplaySelectionNestedExitToplevelFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        second_main,
        second_parent: edited_words.join(" "),
        has_appendix: with_appendix,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_start_page_index: expected_checkpoint.meta.page_index_after,
    }
}

fn rewrite_replay_selection_nested_exit_toplevel(
    fixture: &ReplaySelectionNestedExitToplevelFixture,
) {
    fs::write(fixture.root.join("main.tex"), &fixture.second_main).expect("rewrite main");
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

async fn compile_replay_selection_nested_exit_toplevel_second_pass(
    fixture: &ReplaySelectionNestedExitToplevelFixture,
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

fn assert_replay_selection_nested_exit_toplevel(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
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
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_replay_selection_nested_exit_toplevel(case: ReplaySelectionNestedExitToplevelCase) {
    let fixture = prepare_replay_selection_nested_exit_toplevel_fixture(matches!(
        case,
        ReplaySelectionNestedExitToplevelCase::Appendix
    ))
    .await;
    rewrite_replay_selection_nested_exit_toplevel(&fixture);
    let dirty_files = match case {
        ReplaySelectionNestedExitToplevelCase::Plain => vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/parent.tex"),
        ],
        ReplaySelectionNestedExitToplevelCase::Appendix => vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/parent.tex"),
            Utf8PathBuf::from("sections/appendix.tex"),
        ],
    };
    let second =
        compile_replay_selection_nested_exit_toplevel_second_pass(&fixture, &dirty_files).await;
    assert_replay_selection_nested_exit_toplevel(
        &fixture.build_root,
        &second,
        &dirty_files,
        &fixture.expected_checkpoint_id,
        fixture.expected_start_page_index,
    );
}
