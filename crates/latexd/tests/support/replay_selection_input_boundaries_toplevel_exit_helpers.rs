#[derive(Clone, Copy)]
enum ReplaySelectionToplevelExitCase {
    Baseline,
    ClampLastPage,
}

async fn run_replay_selection_input_boundaries_toplevel_exit(
    exit_case: ReplaySelectionToplevelExitCase,
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
    fs::write(root.join("article.cls"), "").expect("write class");
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    let main_source = match exit_case {
        ReplaySelectionToplevelExitCase::Baseline => {
            "\\documentclass{article}\\begin{document} intro \\input{sections/tail} after-old \\end{document}".to_string()
        }
        ReplaySelectionToplevelExitCase::ClampLastPage => {
            let mut words = (0..3200)
                .map(|index| format!("word{index:04}"))
                .collect::<Vec<_>>();
            words.insert(3190, "\\input{sections/tail}".to_string());
            words.push("after-old".to_string());
            format!(
                "\\documentclass{{article}}\\begin{{document}} {} \\end{{document}}",
                words.join(" ")
            )
        }
    };
    fs::write(root.join("main.tex"), main_source).expect("write main");

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
    let last_page_index = if matches!(exit_case, ReplaySelectionToplevelExitCase::ClampLastPage) {
        assert!(first.page_metadata.len() > 1);
        Some(
            first
                .page_metadata
                .last()
                .expect("last page metadata")
                .index,
        )
    } else {
        None
    };

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .max_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
        .expect("toplevel input exit checkpoint");
    if let Some(last_page_index) = last_page_index {
        assert_eq!(expected_checkpoint.meta.page_index_after, last_page_index);
    }

    let rewritten_source = fs::read_to_string(root.join("main.tex").as_std_path())
        .expect("read main")
        .replace("after-old", "after-new");
    fs::write(root.join("main.tex"), rewritten_source).expect("rewrite main");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second build should succeed");

    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint.meta.checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("main.tex")]);
    assert_eq!(
        build_meta.start_checkpoint_id,
        Some(expected_checkpoint.meta.checkpoint_id.clone())
    );
    assert_eq!(
        build_meta.start_page_index,
        expected_checkpoint.meta.page_index_after
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
