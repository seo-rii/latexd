struct LateMultiBibliographySkipCaseFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    preamble_checkpoint_id: String,
}

enum LateMultiBibliographySkipCase {
    SinglePage,
    MultiPageBibliography,
}

type LateSkip = LateMultiBibliographySkipCase;

async fn run_late_skip(case: LateSkip) {
    run_late_multi_bibliography_skip_case(case).await;
}

async fn prepare_late_multi_bibliography_skip_case_fixture(
    main_source: String,
    first_bibliography: String,
) -> LateMultiBibliographySkipCaseFixture {
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
    fs::write(root.join("main.tex"), main_source).expect("write main");
    fs::write(root.join("refsa.bbl"), first_bibliography).expect("write first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
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
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let preamble_checkpoint_id = first_bundle
        .checkpoints
        .first()
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("preamble checkpoint");

    LateMultiBibliographySkipCaseFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        preamble_checkpoint_id,
    }
}

async fn compile_late_multi_bibliography_skip_case_second_pass(
    fixture: &LateMultiBibliographySkipCaseFixture,
) -> CompileOutcome {
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");

    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("refsb.bbl")],
        })
        .await
        .expect("second semantic aux build should succeed")
}

#[derive(Clone, Copy)]
enum LateMultiBibliographySkipCaseShape {
    SinglePage,
    MultiPageBibliography,
}

fn assert_late_multi_bibliography_skip_case_shape(
    fixture: &LateMultiBibliographySkipCaseFixture,
    shape: LateMultiBibliographySkipCaseShape,
) {
    match shape {
        LateMultiBibliographySkipCaseShape::SinglePage => {
            assert_eq!(fixture.first.page_metadata.len(), 1);
        }
        LateMultiBibliographySkipCaseShape::MultiPageBibliography => {
            let bibliography_page_indexes = ["refsa.bbl", "refsb.bbl"]
                .into_iter()
                .map(|path| {
                    fixture
                        .first
                        .page_metadata
                        .iter()
                        .find(|page| {
                            page.source_spans
                                .iter()
                                .any(|span| span.file == Utf8PathBuf::from(path))
                        })
                        .map(|page| page.index)
                        .expect("bibliography page")
                })
                .collect::<Vec<_>>();
            assert!(
                bibliography_page_indexes[0] < bibliography_page_indexes[1],
                "fixture should place refsa before refsb across pages, saw {:?}",
                bibliography_page_indexes
            );
            assert!(bibliography_page_indexes[0] > 0);
        }
    }
}

fn assert_late_multi_bibliography_skip_case_reuses_preamble(
    fixture: &LateMultiBibliographySkipCaseFixture,
    second: &CompileOutcome,
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(fixture.preamble_checkpoint_id.clone())
    );
    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, fixture.first.page_metadata.len());
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert_eq!(
        second
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        fixture
            .first
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(second.page_patches.is_empty());
    assert!(
        second
            .page_artifacts
            .iter()
            .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
    );

    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refsb.bbl")]);
    assert_eq!(
        build_meta.start_checkpoint_id,
        Some(fixture.preamble_checkpoint_id.clone())
    );
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

async fn run_late_multi_bibliography_skip_case(case: LateMultiBibliographySkipCase) {
    let (main_source, first_bibliography, shape) = match case {
        LateMultiBibliographySkipCase::SinglePage => (
            "\\documentclass{article}\\begin{document}Order check. \\cite{alpha} and \\cite{beta}.\\bibliography{refsa,refsb}\\end{document}".to_string(),
            "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n".to_string(),
            LateMultiBibliographySkipCaseShape::SinglePage,
        ),
        LateMultiBibliographySkipCase::MultiPageBibliography => {
            let intro_filler = "late multi bibliography skip filler ".repeat(220);
            let first_bibliography_body = (0..1800)
                .map(|index| format!("alpha{index:04}"))
                .collect::<Vec<_>>()
                .join(" ");
            (
                format!(
                    "\\documentclass{{article}}\\begin{{document}}Intro. {intro_filler} \\cite{{alpha}} and \\cite{{beta}}.\\bibliography{{refsa,refsb}}\\end{{document}}"
                ),
                format!(
                    "\\begin{{thebibliography}}{{1}}\n\\bibitem[A 2024]{{alpha}} Alpha entry. {first_bibliography_body}\n\\end{{thebibliography}}\n"
                ),
                LateMultiBibliographySkipCaseShape::MultiPageBibliography,
            )
        }
    };

    let fixture =
        prepare_late_multi_bibliography_skip_case_fixture(main_source, first_bibliography).await;
    assert_late_multi_bibliography_skip_case_shape(&fixture, shape);

    let second = compile_late_multi_bibliography_skip_case_second_pass(&fixture).await;

    assert_late_multi_bibliography_skip_case_reuses_preamble(&fixture, &second);
}
