struct OptionedBibliographyOrderStackRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    outcomes: BTreeMap<u64, CompileOutcome>,
}

async fn run_optioned_bibliography_order_stack_revisions(
    max_rev: u64,
) -> OptionedBibliographyOrderStackRun {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let mut copy_dirs = vec![(fixture_root.clone(), root.clone())];
    while let Some((source_dir, target_dir)) = copy_dirs.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 source path");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                copy_dirs.push((source_path, target_path));
                continue;
            }
            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                .expect("copy fixture file");
        }
    }

    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let mut outcomes = BTreeMap::new();
    for rev in 1..=max_rev {
        let mut changed_files = Vec::new();
        if rev > 1 {
            let overlay_root = fixture_root.join(format!("rev{rev}"));
            if overlay_root.exists() {
                let mut overlay_dirs = vec![overlay_root.clone()];
                while let Some(source_dir) = overlay_dirs.pop() {
                    for entry in fs::read_dir(source_dir.as_std_path())
                        .expect("read overlay dir")
                        .filter_map(|entry| entry.ok())
                    {
                        let source_path =
                            Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 overlay path");
                        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                            overlay_dirs.push(source_path);
                            continue;
                        }
                        let relative_path = source_path
                            .strip_prefix(&overlay_root)
                            .expect("overlay path should be relative to overlay root");
                        let target_path = root.join(relative_path);
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent.as_std_path()).expect("create parent dir");
                        }
                        fs::copy(source_path.as_std_path(), target_path.as_std_path())
                            .expect("copy overlay file");
                        changed_files.push(relative_path.to_owned());
                    }
                }
            }
        }
        let world = ProjectWorld::load(root.clone()).expect("world");
        let outcome = driver
            .compile(CompileRequest {
                root: root.clone(),
                manifest: world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev,
                build_root: build_root.clone(),
                changed_files,
            })
            .await
            .expect("build should succeed");
        outcomes.insert(rev, outcome);
    }

    OptionedBibliographyOrderStackRun {
        _tempdir: tempdir,
        build_root,
        outcomes,
    }
}

async fn run_rev5_baseline_core_rebuild_from_base() {
    let run = run_optioned_bibliography_order_stack_revisions(5).await;
    let fifth = run.outcomes.get(&5).expect("fifth outcome");
    assert_rev5_semantic_multi_bibliography_base_rebuild(
        &run.build_root,
        fifth,
        vec![Utf8PathBuf::from("refsb.bbl")],
    );
}
