#[derive(Clone, Copy)]
enum SamePageRootDirtyOrder {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

type RootBaseCase = SamePageRootDirtyOrder;

async fn run_root_base_case(case: RootBaseCase) {
    run_bibliography_multi_same_page_late_change_root_baseline_rebuild_from_base(case).await;
}

enum SamePageRootSiblingDirtyOrder {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
    Interleaved,
    OtherInterleaved,
    InterleavedUntrackedFollows,
    InterleavedUntrackedPrecedes,
    InterleavedUnreadableFollows,
    InterleavedUnreadablePrecedes,
    OtherInterleavedUntrackedFollows,
    OtherInterleavedUntrackedPrecedes,
    OtherInterleavedUnreadableFollows,
    OtherInterleavedUnreadablePrecedes,
}

type SamePageRootSiblingCase = SamePageRootSiblingDirtyOrder;

async fn run_same_page_root_sibling_case(case: SamePageRootSiblingCase) {
    run_bibliography_multi_same_page_late_change_root_sibling_rebuild_from_base(case).await;
}

type RootSib = SamePageRootSiblingCase;

async fn run_root_sib(case: RootSib) {
    run_same_page_root_sibling_case(case).await;
}

struct BibliographyMultiSamePageLateChangeRootBaselineFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

async fn prepare_bibliography_multi_same_page_late_change_root_baseline_fixture()
-> BibliographyMultiSamePageLateChangeRootBaselineFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let filler = (0..120)
        .map(|index| format!("body{index:04}"))
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
        format!(
            "\\documentclass{{article}}\\begin{{document}}Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}.\\bibliography{{refsa,refsb}}\\input{{sections/tail}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(root.join("sections/tail.tex"), "Tail A.").expect("write tail");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
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
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert_eq!(
        first.page_metadata.len(),
        1,
        "fixture should keep semantic bibliography change and tail on the same page"
    );

    BibliographyMultiSamePageLateChangeRootBaselineFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
    }
}

fn dirty_files_for_bibliography_multi_same_page_late_change_root_baseline(
    dirty_order: SamePageRootDirtyOrder,
) -> Vec<Utf8PathBuf> {
    match dirty_order {
        SamePageRootDirtyOrder::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        SamePageRootDirtyOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
        SamePageRootDirtyOrder::UntrackedFollows | SamePageRootDirtyOrder::UnreadableFollows => {
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("notes.txt"),
            ]
        }
        SamePageRootDirtyOrder::UntrackedPrecedes | SamePageRootDirtyOrder::UnreadablePrecedes => {
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
            ]
        }
    }
}

fn rewrite_bibliography_multi_same_page_late_change_root_baseline(
    fixture: &BibliographyMultiSamePageLateChangeRootBaselineFixture,
    dirty_order: SamePageRootDirtyOrder,
) {
    fs::write(fixture.root.join("sections/tail.tex"), "Tail B.").expect("rewrite tail");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
    match dirty_order {
        SamePageRootDirtyOrder::Baseline | SamePageRootDirtyOrder::Reversed => {}
        SamePageRootDirtyOrder::UntrackedFollows | SamePageRootDirtyOrder::UntrackedPrecedes => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        SamePageRootDirtyOrder::UnreadableFollows | SamePageRootDirtyOrder::UnreadablePrecedes => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_bibliography_multi_same_page_late_change_root_baseline_second_pass(
    fixture: &BibliographyMultiSamePageLateChangeRootBaselineFixture,
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

fn assert_bibliography_multi_same_page_late_change_root_baseline_rebuilds_from_base(
    fixture: &BibliographyMultiSamePageLateChangeRootBaselineFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Late year 2025."),
        "executed main.tex should reflect the semantic bibliography change"
    );
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/tail.tex")].contains("Tail B."),
        "executed tail.tex should reflect the later tracked change"
    );
    assert_eq!(second.reused_checkpoint_id, None);
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_bibliography_multi_same_page_late_change_root_baseline_rebuild_from_base(
    dirty_order: SamePageRootDirtyOrder,
) {
    let fixture = prepare_bibliography_multi_same_page_late_change_root_baseline_fixture().await;
    rewrite_bibliography_multi_same_page_late_change_root_baseline(&fixture, dirty_order);
    let dirty_files =
        dirty_files_for_bibliography_multi_same_page_late_change_root_baseline(dirty_order);
    let second = compile_bibliography_multi_same_page_late_change_root_baseline_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;
    assert_bibliography_multi_same_page_late_change_root_baseline_rebuilds_from_base(
        &fixture,
        &second,
        &dirty_files,
    );
}

async fn run_bibliography_multi_same_page_late_change_root_sibling_rebuild_from_base(
    dirty_order: SamePageRootSiblingDirtyOrder,
) {
    enum DirtyMarker {
        Untracked,
        Unreadable,
    }

    let fixture = prepare_bibliography_multi_same_page_late_change_root_baseline_fixture().await;
    fs::write(fixture.root.join("sections/tail.tex"), "Tail B.").expect("rewrite tail");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");

    let (dirty_marker, dirty_files) = match dirty_order {
        SamePageRootSiblingDirtyOrder::Baseline => (
            None,
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::Reversed => (
            None,
            vec![
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::UntrackedFollows => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::UntrackedPrecedes => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::UnreadableFollows => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::UnreadablePrecedes => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::Interleaved => (
            None,
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::OtherInterleaved => (
            None,
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::InterleavedUntrackedFollows => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::InterleavedUntrackedPrecedes => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::InterleavedUnreadableFollows => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::InterleavedUnreadablePrecedes => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::OtherInterleavedUntrackedFollows => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::OtherInterleavedUntrackedPrecedes => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::OtherInterleavedUnreadableFollows => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        SamePageRootSiblingDirtyOrder::OtherInterleavedUnreadablePrecedes => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        ),
    };

    match dirty_marker {
        Some(DirtyMarker::Untracked) => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        Some(DirtyMarker::Unreadable) => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
        None => {}
    }

    let second = compile_bibliography_multi_same_page_late_change_root_baseline_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;

    assert_bibliography_multi_same_page_late_change_root_baseline_rebuilds_from_base(
        &fixture,
        &second,
        &dirty_files,
    );
}
