enum ReplaySelectionLateInputRemovalTailMode {
    Deleted,
    FileRemains,
    PageReplay,
    RetainedDirty,
}

enum ReplaySelectionLateInputRemovalNoise {
    Untracked,
    Unreadable,
}

enum ReplaySelectionLateInputRemovalCase {
    DeletedBaseline,
    DeletedUntrackedFollows,
    DeletedUntrackedPrecedes,
    DeletedUnreadableFollows,
    DeletedUnreadablePrecedes,
    FileRemainsBaseline,
    FileRemainsUntrackedFollows,
    FileRemainsUntrackedPrecedes,
    FileRemainsUnreadableFollows,
    FileRemainsUnreadablePrecedes,
    PageReplay,
    RetainedDirtyBaseline,
    RetainedDirtyUntrackedFollows,
    RetainedDirtyUntrackedPrecedes,
    RetainedDirtyUnreadableFollows,
    RetainedDirtyUnreadablePrecedes,
}

type LateInputRemovalCase = ReplaySelectionLateInputRemovalCase;

async fn run_late_input_removal_case(case: LateInputRemovalCase) {
    run_replay_selection_late_input_removal_case(case).await;
}

struct ReplaySelectionLateInputFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    main_without_tail_input: String,
    input_page_index: usize,
    first_page_count: usize,
}

async fn prepare_replay_selection_late_input_fixture() -> ReplaySelectionLateInputFixture {
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
    let words = (0..1800)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    let mut original_words = words.clone();
    original_words.insert(1500, "\\input{sections/tail}".to_string());
    fs::write(root.join("sections/tail.tex"), "tail-A").expect("write tail input");
    fs::write(root.join("main.tex"), original_words.join(" ")).expect("write main tex");

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

    let input_page = first
        .page_metadata
        .iter()
        .find(|page| {
            page.source_spans
                .iter()
                .any(|span| span.file == Utf8PathBuf::from("sections/tail.tex"))
        })
        .expect("input page");
    assert!(input_page.index > 0);

    ReplaySelectionLateInputFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        main_without_tail_input: words.join(" "),
        input_page_index: input_page.index,
        first_page_count: first.page_metadata.len(),
    }
}

async fn run_replay_selection_late_input_removal(
    tail_mode: ReplaySelectionLateInputRemovalTailMode,
    dirty_file_paths: &[&str],
    noise: Option<ReplaySelectionLateInputRemovalNoise>,
) {
    let fixture = prepare_replay_selection_late_input_fixture().await;
    if !matches!(
        tail_mode,
        ReplaySelectionLateInputRemovalTailMode::PageReplay
    ) {
        fs::write(
            fixture.root.join("main.tex"),
            &fixture.main_without_tail_input,
        )
        .expect("rewrite main tex");
    }
    match tail_mode {
        ReplaySelectionLateInputRemovalTailMode::Deleted => {
            fs::remove_file(fixture.root.join("sections/tail.tex")).expect("delete tail input");
        }
        ReplaySelectionLateInputRemovalTailMode::FileRemains => {}
        ReplaySelectionLateInputRemovalTailMode::PageReplay => {
            fs::write(fixture.root.join("sections/tail.tex"), "tail-B")
                .expect("rewrite tail input");
        }
        ReplaySelectionLateInputRemovalTailMode::RetainedDirty => {
            fs::write(fixture.root.join("sections/tail.tex"), "tail-B")
                .expect("rewrite retained tail input");
        }
    }
    match noise {
        Some(ReplaySelectionLateInputRemovalNoise::Untracked) => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        Some(ReplaySelectionLateInputRemovalNoise::Unreadable) => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
        None => {}
    }

    let dirty_files = dirty_file_paths
        .iter()
        .map(|path| Utf8PathBuf::from(*path))
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
        .expect("second build should succeed");

    if matches!(
        tail_mode,
        ReplaySelectionLateInputRemovalTailMode::PageReplay
    ) {
        let first_checkpoints =
            load_checkpoint_bundle(&fixture.build_root.join("rev-1/checkpoints.json"))
                .expect("load rev1 checkpoints");
        let expected_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .expect("input boundary checkpoint");
        assert_late_input_replay_build_meta(
            &fixture.build_root,
            &second,
            &dirty_files,
            Some(expected_checkpoint.meta.checkpoint_id.clone()),
            expected_checkpoint.meta.page_index_after,
        );
        return;
    }

    assert!(second.page_metadata.len() <= fixture.first_page_count);
    assert!(second.page_metadata.iter().all(|page| {
        page.source_spans
            .iter()
            .all(|span| span.file != Utf8PathBuf::from("sections/tail.tex"))
    }));
    let replace_indexes = second
        .page_patches
        .iter()
        .filter_map(|patch| match patch {
            PagePatchOp::ReplacePage { index, .. } => Some(*index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        replace_indexes,
        (fixture.input_page_index..second.page_metadata.len()).collect::<Vec<_>>()
    );
    assert!(!second.page_patches.iter().any(|patch| matches!(
        patch,
        PagePatchOp::InsertPage { .. } | PagePatchOp::DeletePage { .. }
    )));

    assert_late_input_replay_build_meta(&fixture.build_root, &second, &dirty_files, None, 0);
}

async fn run_replay_selection_late_input_removal_case(case: ReplaySelectionLateInputRemovalCase) {
    let (tail_mode, dirty_file_paths, noise) = match case {
        ReplaySelectionLateInputRemovalCase::DeletedBaseline => (
            ReplaySelectionLateInputRemovalTailMode::Deleted,
            &["main.tex", "sections/tail.tex"][..],
            None,
        ),
        ReplaySelectionLateInputRemovalCase::DeletedUntrackedFollows => (
            ReplaySelectionLateInputRemovalTailMode::Deleted,
            &["main.tex", "sections/tail.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::DeletedUntrackedPrecedes => (
            ReplaySelectionLateInputRemovalTailMode::Deleted,
            &["notes.txt", "main.tex", "sections/tail.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::DeletedUnreadableFollows => (
            ReplaySelectionLateInputRemovalTailMode::Deleted,
            &["main.tex", "sections/tail.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
        ReplaySelectionLateInputRemovalCase::DeletedUnreadablePrecedes => (
            ReplaySelectionLateInputRemovalTailMode::Deleted,
            &["notes.txt", "main.tex", "sections/tail.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
        ReplaySelectionLateInputRemovalCase::FileRemainsBaseline => (
            ReplaySelectionLateInputRemovalTailMode::FileRemains,
            &["main.tex"][..],
            None,
        ),
        ReplaySelectionLateInputRemovalCase::FileRemainsUntrackedFollows => (
            ReplaySelectionLateInputRemovalTailMode::FileRemains,
            &["main.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::FileRemainsUntrackedPrecedes => (
            ReplaySelectionLateInputRemovalTailMode::FileRemains,
            &["notes.txt", "main.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::FileRemainsUnreadableFollows => (
            ReplaySelectionLateInputRemovalTailMode::FileRemains,
            &["main.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
        ReplaySelectionLateInputRemovalCase::FileRemainsUnreadablePrecedes => (
            ReplaySelectionLateInputRemovalTailMode::FileRemains,
            &["notes.txt", "main.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
        ReplaySelectionLateInputRemovalCase::PageReplay => (
            ReplaySelectionLateInputRemovalTailMode::PageReplay,
            &["sections/tail.tex"][..],
            None,
        ),
        ReplaySelectionLateInputRemovalCase::RetainedDirtyBaseline => (
            ReplaySelectionLateInputRemovalTailMode::RetainedDirty,
            &["sections/tail.tex", "main.tex"][..],
            None,
        ),
        ReplaySelectionLateInputRemovalCase::RetainedDirtyUntrackedFollows => (
            ReplaySelectionLateInputRemovalTailMode::RetainedDirty,
            &["sections/tail.tex", "main.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::RetainedDirtyUntrackedPrecedes => (
            ReplaySelectionLateInputRemovalTailMode::RetainedDirty,
            &["notes.txt", "sections/tail.tex", "main.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Untracked),
        ),
        ReplaySelectionLateInputRemovalCase::RetainedDirtyUnreadableFollows => (
            ReplaySelectionLateInputRemovalTailMode::RetainedDirty,
            &["sections/tail.tex", "main.tex", "notes.txt"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
        ReplaySelectionLateInputRemovalCase::RetainedDirtyUnreadablePrecedes => (
            ReplaySelectionLateInputRemovalTailMode::RetainedDirty,
            &["notes.txt", "sections/tail.tex", "main.tex"][..],
            Some(ReplaySelectionLateInputRemovalNoise::Unreadable),
        ),
    };
    run_replay_selection_late_input_removal(tail_mode, dirty_file_paths, noise).await;
}

fn assert_late_input_replay_build_meta(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: Option<String>,
    expected_start_page_index: usize,
) {
    assert_eq!(second.reused_checkpoint_id, expected_checkpoint_id.clone());
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, expected_checkpoint_id);
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
