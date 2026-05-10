#[derive(Clone, Copy)]
enum SamePageIncludedBodyDirtyOrder {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

struct BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

async fn prepare_bibliography_multi_same_page_late_change_included_body_baseline_fixture()
-> BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let filler = (0..80)
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\bibliography{refsa,refsb}\\input{sections/tail}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}."),
    )
    .expect("write body");
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
                Utf8PathBuf::from("sections/body.tex"),
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

    BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
    }
}

fn dirty_files_for_bibliography_multi_same_page_late_change_included_body_baseline(
    dirty_order: SamePageIncludedBodyDirtyOrder,
) -> Vec<Utf8PathBuf> {
    match dirty_order {
        SamePageIncludedBodyDirtyOrder::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        SamePageIncludedBodyDirtyOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
        SamePageIncludedBodyDirtyOrder::UntrackedFollows
        | SamePageIncludedBodyDirtyOrder::UnreadableFollows => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("notes.txt"),
        ],
        SamePageIncludedBodyDirtyOrder::UntrackedPrecedes
        | SamePageIncludedBodyDirtyOrder::UnreadablePrecedes => vec![
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
    }
}

fn rewrite_bibliography_multi_same_page_late_change_included_body_baseline(
    fixture: &BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture,
    dirty_order: SamePageIncludedBodyDirtyOrder,
) {
    fs::write(fixture.root.join("sections/tail.tex"), "Tail B.").expect("rewrite tail");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
    match dirty_order {
        SamePageIncludedBodyDirtyOrder::Baseline | SamePageIncludedBodyDirtyOrder::Reversed => {}
        SamePageIncludedBodyDirtyOrder::UntrackedFollows
        | SamePageIncludedBodyDirtyOrder::UntrackedPrecedes => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        SamePageIncludedBodyDirtyOrder::UnreadableFollows
        | SamePageIncludedBodyDirtyOrder::UnreadablePrecedes => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_bibliography_multi_same_page_late_change_included_body_baseline_second_pass(
    fixture: &BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture,
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

fn assert_bibliography_multi_same_page_late_change_included_body_baseline_rebuilds_from_base(
    fixture: &BibliographyMultiSamePageLateChangeIncludedBodyBaselineFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/body.tex")]
            .contains("Late year 2025."),
        "executed body.tex should reflect the semantic bibliography change"
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

async fn run_bibliography_multi_same_page_late_change_included_body_baseline_rebuild_from_base(
    dirty_order: SamePageIncludedBodyDirtyOrder,
) {
    let fixture =
        prepare_bibliography_multi_same_page_late_change_included_body_baseline_fixture().await;
    rewrite_bibliography_multi_same_page_late_change_included_body_baseline(&fixture, dirty_order);
    let dirty_files =
        dirty_files_for_bibliography_multi_same_page_late_change_included_body_baseline(
            dirty_order,
        );
    let second =
        compile_bibliography_multi_same_page_late_change_included_body_baseline_second_pass(
            &fixture,
            &dirty_files,
        )
        .await;
    assert_bibliography_multi_same_page_late_change_included_body_baseline_rebuilds_from_base(
        &fixture,
        &second,
        &dirty_files,
    );
}

type BodyLateBaseCase = SamePageIncludedBodyDirtyOrder;

async fn run_body_late_base_case(dirty_order: BodyLateBaseCase) {
    run_bibliography_multi_same_page_late_change_included_body_baseline_rebuild_from_base(
        dirty_order,
    )
    .await;
}
