async fn prepare_optioned_bibliography_order_stack_fixture_with_tracked_tail_input(
    fixture_root: &Utf8Path,
    root: &Utf8Path,
    driver: &CompilerDriver,
    build_root: &Utf8Path,
) -> ProjectWorld {
    build_optioned_bibliography_order_stack_to_rev4(fixture_root, root, driver, build_root).await;
    fs::create_dir_all(root.join("sections")).expect("create sections dir");
    let main =
        fs::read_to_string(root.join("main.tex")).expect("read tracked toplevel after rev4 build");
    fs::write(
        root.join("main.tex"),
        main.replace("\\end{document}", "\\input{sections/tail}\n\\end{document}"),
    )
    .expect("rewrite tracked toplevel");
    fs::write(root.join("sections/tail.tex"), "Tail A.").expect("write tracked tail input");
    let world = ProjectWorld::load(root.to_owned()).expect("world");
    driver
        .compile(CompileRequest {
            root: root.to_owned(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 5,
            build_root: build_root.to_owned(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("tracked tail baseline build should succeed");
    let rev5_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-5/sources.json")).expect("read fifth sources"),
    )
    .expect("parse fifth sources");
    assert!(
        rev5_sources.executed_files[&Utf8PathBuf::from("sections/tail.tex")].contains("Tail A."),
        "tracked tail input baseline should execute before the semantic bibliography edit"
    );
    assert!(
        fs::read_to_string(build_root.join("rev-5/output.txt"))
            .expect("read fifth output")
            .contains("Tail A."),
        "baseline output should include the tracked tail input"
    );
    world
}

enum TrackedTailInputExtraDirtyKind {
    Untracked,
    Unreadable,
}

enum TrackedTailInputExtraDirtyOrder {
    Follows,
    Precedes,
}

enum TrackedTailInputSiblingDirtyOrder {
    Baseline,
    Reversed,
    Interleaved,
    OtherInterleaved,
}

enum TrackedTailInputBaselineDirtyOrder {
    Baseline,
    Reversed,
}

enum TrackedTailInputBaselineExtraDirtyCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

enum TrackedTailInputSiblingOrderingExtraDirtyCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
    OtherInterleavedUntrackedFollows,
    OtherInterleavedUntrackedPrecedes,
    OtherInterleavedUnreadableFollows,
    OtherInterleavedUnreadablePrecedes,
}

enum TrackedTailInputSiblingExtraDirtyInterleavedCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

async fn run_tracked_tail_input_sibling_ordering_extra_dirty_case(
    kind: TrackedTailInputExtraDirtyKind,
    order: TrackedTailInputExtraDirtyOrder,
    bibliography_dirty_files: [&str; 2],
) {
    let dirty_files = match order {
        TrackedTailInputExtraDirtyOrder::Follows => [
            "sections/tail.tex",
            bibliography_dirty_files[0],
            bibliography_dirty_files[1],
            "notes.txt",
        ],
        TrackedTailInputExtraDirtyOrder::Precedes => [
            "notes.txt",
            "sections/tail.tex",
            bibliography_dirty_files[0],
            bibliography_dirty_files[1],
        ],
    };
    run_tracked_tail_input_sibling_extra_dirty_case_with_dirty_files(kind, dirty_files).await;
}

async fn run_tracked_tail_input_sibling_ordering_extra_dirty_case_variant(
    case: TrackedTailInputSiblingOrderingExtraDirtyCase,
) {
    let (kind, order, bibliography_dirty_files) = match case {
        TrackedTailInputSiblingOrderingExtraDirtyCase::UntrackedFollows => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Follows,
            ["refsb.bbl", "refsa.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::UntrackedPrecedes => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Precedes,
            ["refsb.bbl", "refsa.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::UnreadableFollows => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Follows,
            ["refsb.bbl", "refsa.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::UnreadablePrecedes => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Precedes,
            ["refsb.bbl", "refsa.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::OtherInterleavedUntrackedFollows => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Follows,
            ["refsa.bbl", "refsb.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::OtherInterleavedUntrackedPrecedes => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Precedes,
            ["refsa.bbl", "refsb.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::OtherInterleavedUnreadableFollows => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Follows,
            ["refsa.bbl", "refsb.bbl"],
        ),
        TrackedTailInputSiblingOrderingExtraDirtyCase::OtherInterleavedUnreadablePrecedes => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Precedes,
            ["refsa.bbl", "refsb.bbl"],
        ),
    };
    run_tracked_tail_input_sibling_ordering_extra_dirty_case(kind, order, bibliography_dirty_files)
        .await;
}

async fn run_tracked_tail_input_sibling_extra_dirty_interleaved_case(
    case: TrackedTailInputSiblingExtraDirtyInterleavedCase,
) {
    let (kind, dirty_files) = match case {
        TrackedTailInputSiblingExtraDirtyInterleavedCase::UntrackedFollows => (
            TrackedTailInputExtraDirtyKind::Untracked,
            ["refsb.bbl", "sections/tail.tex", "refsa.bbl", "notes.txt"],
        ),
        TrackedTailInputSiblingExtraDirtyInterleavedCase::UntrackedPrecedes => (
            TrackedTailInputExtraDirtyKind::Untracked,
            ["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"],
        ),
        TrackedTailInputSiblingExtraDirtyInterleavedCase::UnreadableFollows => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            ["refsb.bbl", "sections/tail.tex", "refsa.bbl", "notes.txt"],
        ),
        TrackedTailInputSiblingExtraDirtyInterleavedCase::UnreadablePrecedes => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            ["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"],
        ),
    };
    run_tracked_tail_input_sibling_extra_dirty_case_with_dirty_files(kind, dirty_files).await;
}

async fn run_tracked_tail_input_baseline_extra_dirty_case(
    kind: TrackedTailInputExtraDirtyKind,
    order: TrackedTailInputExtraDirtyOrder,
) {
    let dirty_files = match order {
        TrackedTailInputExtraDirtyOrder::Follows => ["refsb.bbl", "sections/tail.tex", "notes.txt"],
        TrackedTailInputExtraDirtyOrder::Precedes => {
            ["notes.txt", "sections/tail.tex", "refsb.bbl"]
        }
    };
    run_tracked_tail_input_extra_dirty_case_with_dirty_files(kind, dirty_files).await;
}

async fn run_tracked_tail_input_baseline_extra_dirty_case_variant(
    case: TrackedTailInputBaselineExtraDirtyCase,
) {
    let (kind, order) = match case {
        TrackedTailInputBaselineExtraDirtyCase::UntrackedFollows => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Follows,
        ),
        TrackedTailInputBaselineExtraDirtyCase::UntrackedPrecedes => (
            TrackedTailInputExtraDirtyKind::Untracked,
            TrackedTailInputExtraDirtyOrder::Precedes,
        ),
        TrackedTailInputBaselineExtraDirtyCase::UnreadableFollows => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Follows,
        ),
        TrackedTailInputBaselineExtraDirtyCase::UnreadablePrecedes => (
            TrackedTailInputExtraDirtyKind::Unreadable,
            TrackedTailInputExtraDirtyOrder::Precedes,
        ),
    };

    run_tracked_tail_input_baseline_extra_dirty_case(kind, order).await;
}

type TtBase = TrackedTailInputBaselineExtraDirtyCase;
type TtBaseOrd = TrackedTailInputBaselineDirtyOrder;
type TtOrd = TrackedTailInputSiblingOrderingExtraDirtyCase;
type TtSib = TrackedTailInputSiblingExtraDirtyInterleavedCase;
type TtSibOrder = TrackedTailInputSiblingDirtyOrder;

async fn run_tt_base(case: TtBase) {
    run_tracked_tail_input_baseline_extra_dirty_case_variant(case).await;
}

async fn run_tt_base_ord(order: TtBaseOrd) {
    run_tracked_tail_input_baseline_dirty_order_case(order).await;
}

async fn run_tt_ord(case: TtOrd) {
    run_tracked_tail_input_sibling_ordering_extra_dirty_case_variant(case).await;
}

async fn run_tt_sib(case: TtSib) {
    run_tracked_tail_input_sibling_extra_dirty_interleaved_case(case).await;
}

async fn run_tt_sib_order(case: TtSibOrder) {
    run_tracked_tail_input_sibling_dirty_order_case(case).await;
}

async fn run_tracked_tail_input_baseline_dirty_order_case(
    order: TrackedTailInputBaselineDirtyOrder,
) {
    let dirty_files = match order {
        TrackedTailInputBaselineDirtyOrder::Baseline => ["refsb.bbl", "sections/tail.tex"],
        TrackedTailInputBaselineDirtyOrder::Reversed => ["sections/tail.tex", "refsb.bbl"],
    };
    run_tracked_tail_input_baseline_dirty_case_with_dirty_files(dirty_files).await;
}

async fn run_tracked_tail_input_baseline_dirty_case_with_dirty_files(dirty_files: [&str; 2]) {
    run_tracked_tail_input_sibling_dirty_case_with_dirty_files(dirty_files).await;
}

async fn run_tracked_tail_input_sibling_dirty_order_case(order: TrackedTailInputSiblingDirtyOrder) {
    let dirty_files = match order {
        TrackedTailInputSiblingDirtyOrder::Baseline => {
            ["refsb.bbl", "sections/tail.tex", "refsa.bbl"]
        }
        TrackedTailInputSiblingDirtyOrder::Reversed => {
            ["refsa.bbl", "sections/tail.tex", "refsb.bbl"]
        }
        TrackedTailInputSiblingDirtyOrder::Interleaved => {
            ["sections/tail.tex", "refsb.bbl", "refsa.bbl"]
        }
        TrackedTailInputSiblingDirtyOrder::OtherInterleaved => {
            ["sections/tail.tex", "refsa.bbl", "refsb.bbl"]
        }
    };
    run_tracked_tail_input_sibling_dirty_case_with_dirty_files(dirty_files).await;
}

async fn run_tracked_tail_input_sibling_dirty_case_with_dirty_files<const N: usize>(
    dirty_files: [&str; N],
) {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let world = prepare_optioned_bibliography_order_stack_fixture_with_tracked_tail_input(
        &fixture_root,
        &root,
        &driver,
        &build_root,
    )
    .await;

    apply_fixture_overlay(&fixture_root.join("rev5"), &root);
    fs::write(root.join("sections/tail.tex"), "Tail B.").expect("rewrite tracked tail input");
    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let sixth = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 6,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("sixth build should succeed");

    assert_fixture_semantic_multi_bibliography_rebuild_with_later_tracked_input(
        &build_root,
        &sixth,
        dirty_files,
    );
}

async fn run_tracked_tail_input_sibling_extra_dirty_case_with_dirty_files(
    kind: TrackedTailInputExtraDirtyKind,
    dirty_files: [&str; 4],
) {
    run_tracked_tail_input_extra_dirty_case_with_dirty_files(kind, dirty_files).await;
}

async fn run_tracked_tail_input_extra_dirty_case_with_dirty_files<const N: usize>(
    kind: TrackedTailInputExtraDirtyKind,
    dirty_files: [&str; N],
) {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let world = prepare_optioned_bibliography_order_stack_fixture_with_tracked_tail_input(
        &fixture_root,
        &root,
        &driver,
        &build_root,
    )
    .await;

    apply_fixture_overlay(&fixture_root.join("rev5"), &root);
    fs::write(root.join("sections/tail.tex"), "Tail B.").expect("rewrite tracked tail input");
    match kind {
        TrackedTailInputExtraDirtyKind::Untracked => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        TrackedTailInputExtraDirtyKind::Unreadable => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
    }
    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let sixth = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 6,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("sixth build should succeed");

    assert_fixture_semantic_multi_bibliography_rebuild_with_later_tracked_input(
        &build_root,
        &sixth,
        dirty_files,
    );
}

fn assert_fixture_semantic_multi_bibliography_rebuild_with_later_tracked_input(
    build_root: &Utf8Path,
    sixth: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
) {
    let sixth_output =
        fs::read_to_string(build_root.join("rev-6/output.txt")).expect("read sixth output");
    assert!(sixth_output.contains("Order check."));
    assert!(sixth_output.contains("Beta revised entry."));
    assert!(sixth_output.contains("Tail B."));
    assert_eq!(
        sixth.reused_checkpoint_id, None,
        "semantic-changing bibliography edit should still rebuild from the base snapshot when a later tracked input changes"
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-6/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, sixth.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
    let sixth_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-6/sources.json")).expect("read sixth sources"),
    )
    .expect("parse sixth sources");
    assert!(
        sixth_sources.executed_files[&Utf8PathBuf::from("sections/tail.tex")].contains("Tail B."),
        "executed tracked input should reflect the later tracked change"
    );
}
