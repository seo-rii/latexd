struct BibliographyFeaturesCitationFieldsMetadataFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum BibliographyFeaturesCitationFieldsMetadataCase {
    BibInfo,
    BibField,
    GenericBibInfo,
}

fn prepare_bibliography_features_citation_fields_metadata_fixture(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesCitationFieldsMetadataFixture {
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
    fs::write(root.join("refs.bbl"), refs_source).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    BibliographyFeaturesCitationFieldsMetadataFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_bibliography_features_citation_fields_metadata_fixture(
    fixture: &BibliographyFeaturesCitationFieldsMetadataFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
}

fn assert_bibliography_features_citation_fields_metadata_output_and_sources(
    fixture: &BibliographyFeaturesCitationFieldsMetadataFixture,
    expected_output: &str,
    removed_commands: &[&str],
) {
    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_output));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_output));
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
}

async fn run_bibliography_features_citation_fields_metadata(
    case: BibliographyFeaturesCitationFieldsMetadataCase,
) {
    match case {
        BibliographyFeaturesCitationFieldsMetadataCase::BibInfo => {
            let fixture = prepare_bibliography_features_citation_fields_metadata_fixture(
                "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}), \\citetitle{alpha}, \\citefield{alpha}{doi}, and \\citefield{alpha}{eprint}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{author}{Alpha and Beta}. \\bibinfo{year}{2024}. \\bibinfo{title}{Exact Title}. \\bibinfo{doi}{10.1000/example}. \\bibinfo{eprint}{arXiv:2401.00001}.\\end{thebibliography}",
            );
            compile_bibliography_features_citation_fields_metadata_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");

            assert_bibliography_features_citation_fields_metadata_output_and_sources(
                &fixture,
                "See Alpha and Beta (2024), Exact Title, 10.1000/example, and arXiv:2401.00001.",
                &["\\citeauthor", "\\citeyear", "\\citetitle", "\\citefield"],
            );

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_author("alpha"),
                Some("Alpha and Beta".to_string())
            );
            assert_eq!(stored_aux.citation_year("alpha"), Some("2024".to_string()));
            assert_eq!(stored_aux.citation_title("alpha"), Some("Exact Title"));
            assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
            assert_eq!(
                stored_aux.citation_eprint("alpha"),
                Some("arXiv:2401.00001")
            );
        }
        BibliographyFeaturesCitationFieldsMetadataCase::BibField => {
            let fixture = prepare_bibliography_features_citation_fields_metadata_fixture(
                "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}), \\citetitle{alpha}, and \\citefield{alpha}{journal}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{author}{Alpha and Beta}. \\bibfield{year}{2024}. \\bibfield{title}{Field Title}. \\bibfield{journal}{Journal of Fields}.\\end{thebibliography}",
            );
            compile_bibliography_features_citation_fields_metadata_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");

            assert_bibliography_features_citation_fields_metadata_output_and_sources(
                &fixture,
                "See Alpha and Beta (2024), Field Title, and Journal of Fields.",
                &["\\citeauthor", "\\citeyear", "\\citetitle", "\\citefield"],
            );

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_author("alpha"),
                Some("Alpha and Beta".to_string())
            );
            assert_eq!(stored_aux.citation_year("alpha"), Some("2024".to_string()));
            assert_eq!(stored_aux.citation_title("alpha"), Some("Field Title"));
            assert_eq!(
                stored_aux.citation_field("alpha", "journal"),
                Some("Journal of Fields")
            );
        }
        BibliographyFeaturesCitationFieldsMetadataCase::GenericBibInfo => {
            let fixture = prepare_bibliography_features_citation_fields_metadata_fixture(
                "\\documentclass{article}\\begin{document}See \\citefield{alpha}{journal} and \\citefield{alpha}{pages}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{journal}{Journal of Testing}. \\bibinfo{pages}{10--20}.\\end{thebibliography}",
            );
            compile_bibliography_features_citation_fields_metadata_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");

            assert_bibliography_features_citation_fields_metadata_output_and_sources(
                &fixture,
                "See Journal of Testing and 10--20.",
                &["\\citefield"],
            );

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_field("alpha", "journal"),
                Some("Journal of Testing")
            );
            assert_eq!(stored_aux.citation_field("alpha", "pages"), Some("10--20"));
        }
    }
}

type CiteMeta = BibliographyFeaturesCitationFieldsMetadataCase;

async fn run_cite_meta(case: CiteMeta) {
    run_bibliography_features_citation_fields_metadata(case).await;
}
