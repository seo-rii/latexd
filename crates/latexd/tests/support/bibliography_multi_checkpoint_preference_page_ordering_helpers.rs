struct BibliographyCheckpointPageOrderingFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    second_refsa: String,
    second_refsb: String,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
}

async fn prepare_bibliography_checkpoint_page_ordering_fixture(
    same_page: bool,
) -> BibliographyCheckpointPageOrderingFixture {
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

    let (main_contents, refsa_contents, refsb_contents, second_refsa, second_refsb) = if same_page {
        (
            "\\documentclass{article}\\begin{document}Order check. \\cite{alpha} and \\cite{beta}.\\bibliography{refsa,refsb}\\end{document}".to_string(),
            "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n"
                .to_string(),
            "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n"
                .to_string(),
            "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha}  Alpha entry.\n\\end{thebibliography}\n"
                .to_string(),
            "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n"
                .to_string(),
        )
    } else {
        let intro_filler = "bibliography page ordering filler ".repeat(220);
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
            "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n"
                .to_string(),
            format!(
                "\\begin{{thebibliography}}{{1}}\n\\bibitem[A 2024]{{alpha}} Alpha  entry. {first_bibliography_body}\n\\end{{thebibliography}}\n"
            ),
            "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n"
                .to_string(),
        )
    };

    fs::write(root.join("main.tex"), main_contents).expect("write main");
    fs::write(root.join("refsa.bbl"), &refsa_contents).expect("write first bibliography");
    fs::write(root.join("refsb.bbl"), &refsb_contents).expect("write second bibliography");

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
    let expected_checkpoint = if same_page {
        assert_eq!(
            first.page_metadata.len(),
            1,
            "fixture should keep both bibliography files on the same page"
        );
        let same_page_bibliography_checkpoints = first_bundle
            .checkpoints
            .iter()
            .filter(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                        path == &Utf8PathBuf::from("refsa.bbl")
                            || path == &Utf8PathBuf::from("refsb.bbl")
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(same_page_bibliography_checkpoints.len(), 2);
        assert_eq!(
            same_page_bibliography_checkpoints[0].meta.page_index_after,
            same_page_bibliography_checkpoints[1].meta.page_index_after
        );
        same_page_bibliography_checkpoints
            .into_iter()
            .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
            .expect("earlier same-page bibliography checkpoint")
    } else {
        let bibliography_page_indexes = ["refsa.bbl", "refsb.bbl"]
            .into_iter()
            .map(|path| {
                first
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
        first_bundle
            .checkpoints
            .iter()
            .filter(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                        path == &Utf8PathBuf::from("refsa.bbl")
                            || path == &Utf8PathBuf::from("refsb.bbl")
                    })
            })
            .min_by_key(|checkpoint| {
                (
                    checkpoint.meta.page_index_after,
                    checkpoint.meta.output_start_utf8,
                )
            })
            .expect("earlier bibliography-page checkpoint")
    };

    BibliographyCheckpointPageOrderingFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        second_refsa,
        second_refsb,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_page_index_after: expected_checkpoint.meta.page_index_after,
    }
}

fn rewrite_bibliography_checkpoint_page_ordering(
    root: &Utf8Path,
    second_refsa: &str,
    second_refsb: &str,
) {
    fs::write(root.join("refsa.bbl"), second_refsa).expect("rewrite first bibliography");
    fs::write(root.join("refsb.bbl"), second_refsb).expect("rewrite second bibliography");
}

async fn compile_bibliography_checkpoint_page_ordering_second(
    fixture: &BibliographyCheckpointPageOrderingFixture,
) -> (CompileOutcome, Vec<Utf8PathBuf>) {
    let dirty_files = vec![
        Utf8PathBuf::from("refsb.bbl"),
        Utf8PathBuf::from("refsa.bbl"),
    ];
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
        .expect("second semantic aux build should succeed");
    (second, dirty_files)
}

fn assert_bibliography_checkpoint_page_ordering_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    expected_checkpoint_id: String,
    expected_page_index_after: usize,
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    assert!(second.page_patches.is_empty());
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert_eq!(build_meta.start_page_index, expected_page_index_after);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

#[derive(Clone, Copy)]
enum BibliographyCheckpointPageOrderingCase {
    SamePage,
    MultiPage,
}

fn assert_bibliography_checkpoint_page_ordering_case(
    fixture: &BibliographyCheckpointPageOrderingFixture,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    case: BibliographyCheckpointPageOrderingCase,
) {
    assert_bibliography_checkpoint_page_ordering_replay(
        fixture.build_root.as_path(),
        second,
        dirty_files,
        fixture.expected_checkpoint_id.clone(),
        fixture.expected_page_index_after,
    );

    match case {
        BibliographyCheckpointPageOrderingCase::SamePage => {
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
            assert!(
                second
                    .page_artifacts
                    .iter()
                    .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
            );
        }
        BibliographyCheckpointPageOrderingCase::MultiPage => {
            assert!(fixture.expected_page_index_after > 0);
        }
    }
}

async fn run_bibliography_checkpoint_page_ordering_case(
    case: BibliographyCheckpointPageOrderingCase,
) {
    let fixture = prepare_bibliography_checkpoint_page_ordering_fixture(matches!(
        case,
        BibliographyCheckpointPageOrderingCase::SamePage
    ))
    .await;
    rewrite_bibliography_checkpoint_page_ordering(
        fixture.root.as_path(),
        &fixture.second_refsa,
        &fixture.second_refsb,
    );

    let (second, dirty_files) =
        compile_bibliography_checkpoint_page_ordering_second(&fixture).await;
    assert_bibliography_checkpoint_page_ordering_case(&fixture, &second, dirty_files, case);
}

type PageOrderingCase = BibliographyCheckpointPageOrderingCase;

async fn run_page_ordering_case(case: PageOrderingCase) {
    run_bibliography_checkpoint_page_ordering_case(case).await;
}
