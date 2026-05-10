fn copy_fixture_tree(source_root: &Utf8Path, target_root: &Utf8Path) {
    let mut copy_dirs = vec![(source_root.to_owned(), target_root.to_owned())];
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
}

fn apply_fixture_overlay(overlay_root: &Utf8Path, target_root: &Utf8Path) {
    let mut overlay_dirs = vec![overlay_root.to_owned()];
    while let Some(source_dir) = overlay_dirs.pop() {
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read overlay dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 overlay path");
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                overlay_dirs.push(source_path);
                continue;
            }
            let relative_path = source_path
                .strip_prefix(overlay_root)
                .expect("overlay path should be relative to overlay root");
            let target_path = target_root.join(relative_path);
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent.as_std_path()).expect("create parent dir");
            }
            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                .expect("copy overlay file");
        }
    }
}

async fn build_optioned_bibliography_order_stack_to_rev4(
    fixture_root: &Utf8Path,
    root: &Utf8Path,
    driver: &CompilerDriver,
    build_root: &Utf8Path,
) {
    copy_fixture_tree(fixture_root, root);
    for rev in 1..=4u64 {
        let changed_files = if rev > 1 {
            let overlay_root = fixture_root.join(format!("rev{rev}"));
            if overlay_root.exists() {
                let mut changed_files = Vec::new();
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
                changed_files
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let world = ProjectWorld::load(root.to_owned()).expect("world");
        driver
            .compile(CompileRequest {
                root: root.to_owned(),
                manifest: world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev,
                build_root: build_root.to_owned(),
                changed_files,
            })
            .await
            .expect("fixture build should succeed");
    }
}

struct Rev5SemanticMultiBibliographyBaseRebuildNoiseRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    fifth: CompileOutcome,
}

enum Rev5SemanticMultiBibliographyBaseRebuildNoiseCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

async fn compile_rev5_semantic_multi_bibliography_base_rebuild_with_noise(
    dirty_files: Vec<Utf8PathBuf>,
    unreadable_noise: bool,
) -> Rev5SemanticMultiBibliographyBaseRebuildNoiseRun {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    build_optioned_bibliography_order_stack_to_rev4(&fixture_root, &root, &driver, &build_root)
        .await;

    if unreadable_noise {
        fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
    } else {
        fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
    }
    apply_fixture_overlay(&fixture_root.join("rev5"), &root);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let fifth = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 5,
            build_root: build_root.clone(),
            changed_files: dirty_files,
        })
        .await
        .expect("fifth build should succeed");

    Rev5SemanticMultiBibliographyBaseRebuildNoiseRun {
        _tempdir: tempdir,
        build_root,
        fifth,
    }
}

fn assert_rev5_semantic_multi_bibliography_base_rebuild(
    build_root: &Utf8Path,
    fifth: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
) {
    let fifth_output =
        fs::read_to_string(build_root.join("rev-5/output.txt")).expect("read fifth output");
    assert!(fifth_output.contains("Order check. [2] and [1]"));
    assert!(fifth_output.contains("Alpha entry."));
    assert!(fifth_output.contains("Beta revised entry."));
    assert_eq!(
        fifth.reused_checkpoint_id, None,
        "semantic-changing earlier bibliography edits should rebuild from the base snapshot"
    );

    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-5/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, fifth.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, fifth.page_metadata.len());
    assert_eq!(build_meta.reused_page_count, 0);
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_rev5_semantic_multi_bibliography_base_rebuild_noise_case(
    case: Rev5SemanticMultiBibliographyBaseRebuildNoiseCase,
) {
    let (dirty_files, unreadable_noise) = match case {
        Rev5SemanticMultiBibliographyBaseRebuildNoiseCase::UntrackedFollows => (
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            false,
        ),
        Rev5SemanticMultiBibliographyBaseRebuildNoiseCase::UntrackedPrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
            false,
        ),
        Rev5SemanticMultiBibliographyBaseRebuildNoiseCase::UnreadableFollows => (
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            true,
        ),
        Rev5SemanticMultiBibliographyBaseRebuildNoiseCase::UnreadablePrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
            true,
        ),
    };
    let run = compile_rev5_semantic_multi_bibliography_base_rebuild_with_noise(
        dirty_files.clone(),
        unreadable_noise,
    )
    .await;
    assert_rev5_semantic_multi_bibliography_base_rebuild(&run.build_root, &run.fifth, dirty_files);
}

type Rev5BaseNoise = Rev5SemanticMultiBibliographyBaseRebuildNoiseCase;

async fn run_rev5_base_noise(case: Rev5BaseNoise) {
    run_rev5_semantic_multi_bibliography_base_rebuild_noise_case(case).await;
}
