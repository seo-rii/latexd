struct BibliographyFeaturesCitationFieldsIdentifiersFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum BibliographyFeaturesCitationFieldsIdentifierCase {
    DirectIdentifiers,
    DateVariants,
    DoiAndEprintCiteField,
    CiteUrl,
    BibFieldUrl,
    CiteNum,
}

fn prepare_bibliography_features_citation_fields_identifiers_fixture(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesCitationFieldsIdentifiersFixture {
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
    BibliographyFeaturesCitationFieldsIdentifiersFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_bibliography_features_citation_fields_identifiers_fixture(
    fixture: &BibliographyFeaturesCitationFieldsIdentifiersFixture,
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

async fn assert_bibliography_features_citation_fields_identifier_output_and_sources(
    fixture: &BibliographyFeaturesCitationFieldsIdentifiersFixture,
    expected_output: &str,
    removed_commands: &[&str],
) {
    compile_bibliography_features_citation_fields_identifiers_fixture(fixture)
        .await
        .expect("semantic aux build should succeed");

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

async fn run_bibliography_features_citation_fields_identifier(
    case: BibliographyFeaturesCitationFieldsIdentifierCase,
) {
    match case {
        BibliographyFeaturesCitationFieldsIdentifierCase::DirectIdentifiers => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citedoi{alpha}, \\citeeprint{alpha}, \\citeisbn{alpha}, and \\citeissn{alpha}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{doi}{10.1000/example}. \\bibinfo{eprint}{arXiv:2401.00001}. \\bibfield{isbn}{978-1-4028-9462-6}. \\bibfield{issn}{2049-3630}.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See 10.1000/example, arXiv:2401.00001, 978-1-4028-9462-6, and 2049-3630.",
                &["\\citedoi", "\\citeeprint", "\\citeisbn", "\\citeissn"],
            )
            .await;

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
            assert_eq!(
                stored_aux.citation_eprint("alpha"),
                Some("arXiv:2401.00001")
            );
            assert_eq!(
                stored_aux.citation_field("alpha", "isbn"),
                Some("978-1-4028-9462-6")
            );
            assert_eq!(
                stored_aux.citation_field("alpha", "issn"),
                Some("2049-3630")
            );
        }
        BibliographyFeaturesCitationFieldsIdentifierCase::DateVariants => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citedate{alpha}, \\Citedate{beta}, \\citeurldate{alpha}, and \\Citeurldate{beta}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem{alpha}\\bibinfo{date}{March 2024}. \\bibfield{urldate}{2024-03-01}.\\bibitem{beta}\\bibinfo{year}{2023}. \\bibfield{urldate}{2023-08-15}.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See March 2024, 2023, 2024-03-01, and 2023-08-15.",
                &["\\citedate", "\\Citedate", "\\citeurldate", "\\Citeurldate"],
            )
            .await;

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_field("alpha", "date"),
                Some("March 2024")
            );
            assert_eq!(stored_aux.citation_year("beta"), Some("2023".to_string()));
            assert_eq!(
                stored_aux.citation_field("alpha", "urldate"),
                Some("2024-03-01")
            );
            assert_eq!(
                stored_aux.citation_field("beta", "urldate"),
                Some("2023-08-15")
            );
        }
        BibliographyFeaturesCitationFieldsIdentifierCase::DoiAndEprintCiteField => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citefield{alpha}{doi} and \\citefield{alpha}{eprint}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha entry. \\doi{10.1000/example}. \\eprint{arXiv:2401.00001}.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See 10.1000/example and arXiv:2401.00001.",
                &["\\citefield"],
            )
            .await;

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
            assert_eq!(
                stored_aux.citation_eprint("alpha"),
                Some("arXiv:2401.00001")
            );
        }
        BibliographyFeaturesCitationFieldsIdentifierCase::CiteUrl => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha \\href{https://example.test/paper}{Paper Link}.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See https://example.test/paper and https://example.test/paper.",
                &["\\citeurl", "\\citefield"],
            )
            .await;

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_url("alpha"),
                Some("https://example.test/paper")
            );
        }
        BibliographyFeaturesCitationFieldsIdentifierCase::BibFieldUrl => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{url}{https://example.test/bibfield}.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See https://example.test/bibfield and https://example.test/bibfield.",
                &["\\citeurl", "\\citefield"],
            )
            .await;

            let stored_aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(
                stored_aux.citation_url("alpha"),
                Some("https://example.test/bibfield")
            );
        }
        BibliographyFeaturesCitationFieldsIdentifierCase::CiteNum => {
            let fixture = prepare_bibliography_features_citation_fields_identifiers_fixture(
                "\\documentclass{article}\\begin{document}See \\citenum{alpha} and \\citenum{alpha,beta}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
            );
            assert_bibliography_features_citation_fields_identifier_output_and_sources(
                &fixture,
                "See 1 and 1, 2.",
                &["\\citenum"],
            )
            .await;
        }
    }
}

type CiteIdent = BibliographyFeaturesCitationFieldsIdentifierCase;

async fn run_cite_ident(case: CiteIdent) {
    run_bibliography_features_citation_fields_identifier(case).await;
}
