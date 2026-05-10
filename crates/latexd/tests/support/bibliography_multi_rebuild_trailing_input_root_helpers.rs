#[derive(Clone, Copy)]
enum RootTrailingInputDirtyOrder {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

struct BibliographyMultiRebuildTrailingInputRootFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    tail_filler: String,
}

async fn prepare_bibliography_multi_rebuild_trailing_input_root_fixture()
-> BibliographyMultiRebuildTrailingInputRootFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let intro_filler = "late multi bibliography replay filler ".repeat(220);
    let tail_filler = "tail replay filler text ".repeat(180);
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
            "\\documentclass{{article}}\\begin{{document}}Early cite \\cite{{alpha}}. {intro_filler} Late year \\citeyear{{beta}}.\\bibliography{{refsa,refsb}}\\input{{sections/tail}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("sections/tail.tex"),
        format!("Tail A. {tail_filler}"),
    )
    .expect("write tail");
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
    assert!(
        first.page_metadata.len() >= 2,
        "fixture should push late bibliography-dependent content onto a later page"
    );

    BibliographyMultiRebuildTrailingInputRootFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        tail_filler,
    }
}

fn dirty_files_for_bibliography_multi_rebuild_trailing_input_root(
    dirty_order: RootTrailingInputDirtyOrder,
) -> Vec<Utf8PathBuf> {
    match dirty_order {
        RootTrailingInputDirtyOrder::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
        ],
        RootTrailingInputDirtyOrder::Reversed => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
        RootTrailingInputDirtyOrder::UntrackedFollows
        | RootTrailingInputDirtyOrder::UnreadableFollows => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("notes.txt"),
        ],
        RootTrailingInputDirtyOrder::UntrackedPrecedes
        | RootTrailingInputDirtyOrder::UnreadablePrecedes => vec![
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
    }
}

fn rewrite_bibliography_multi_rebuild_trailing_input_root(
    fixture: &BibliographyMultiRebuildTrailingInputRootFixture,
    dirty_order: RootTrailingInputDirtyOrder,
) {
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
    fs::write(
        fixture.root.join("sections/tail.tex"),
        format!("Tail B. {}", fixture.tail_filler),
    )
    .expect("rewrite tail");
    match dirty_order {
        RootTrailingInputDirtyOrder::Baseline | RootTrailingInputDirtyOrder::Reversed => {}
        RootTrailingInputDirtyOrder::UntrackedFollows
        | RootTrailingInputDirtyOrder::UntrackedPrecedes => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        RootTrailingInputDirtyOrder::UnreadableFollows
        | RootTrailingInputDirtyOrder::UnreadablePrecedes => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_bibliography_multi_rebuild_trailing_input_root_second_pass(
    fixture: &BibliographyMultiRebuildTrailingInputRootFixture,
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

fn assert_bibliography_multi_rebuild_trailing_input_root_rebuild_from_base(
    fixture: &BibliographyMultiRebuildTrailingInputRootFixture,
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

async fn run_bibliography_multi_rebuild_trailing_input_root_rebuild_from_base(
    dirty_order: RootTrailingInputDirtyOrder,
) {
    let fixture = prepare_bibliography_multi_rebuild_trailing_input_root_fixture().await;
    rewrite_bibliography_multi_rebuild_trailing_input_root(&fixture, dirty_order);
    let dirty_files = dirty_files_for_bibliography_multi_rebuild_trailing_input_root(dirty_order);
    let second =
        compile_bibliography_multi_rebuild_trailing_input_root_second_pass(&fixture, &dirty_files)
            .await;
    assert_bibliography_multi_rebuild_trailing_input_root_rebuild_from_base(
        &fixture,
        &second,
        &dirty_files,
    );
}

type RootTrailCase = RootTrailingInputDirtyOrder;

async fn run_root_trail_case(case: RootTrailCase) {
    run_bibliography_multi_rebuild_trailing_input_root_rebuild_from_base(case).await;
}
