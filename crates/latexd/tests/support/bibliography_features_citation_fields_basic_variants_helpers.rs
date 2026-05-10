type BibliographyFeaturesCitationFieldsBasicVariantsFixture =
    BibliographyFeaturesSemanticCaseFixture;

enum BibliographyFeaturesCitationFieldsBasicVariantsCase {
    StarredRefs,
    CiteauthorYear,
    CapitalizedAuthorYear,
    Title,
    Citefield,
}

fn prepare_bibliography_features_citation_fields_basic_variants_fixture(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesCitationFieldsBasicVariantsFixture {
    prepare_bibliography_features_semantic_case_fixture(main_source, refs_source)
}

async fn compile_bibliography_features_citation_fields_basic_variants_fixture(
    fixture: &BibliographyFeaturesCitationFieldsBasicVariantsFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    compile_bibliography_features_semantic_case_fixture(fixture).await
}

async fn assert_basic_citation_variant_output_and_sources(
    fixture: &BibliographyFeaturesCitationFieldsBasicVariantsFixture,
    expected_output_texts: &[&str],
    expected_source_text: &str,
    removed_commands: &[&str],
) -> StoredSources {
    compile_bibliography_features_citation_fields_basic_variants_fixture(fixture)
        .await
        .expect("semantic aux build should succeed");

    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    for expected_output in expected_output_texts {
        assert!(output.contains(expected_output));
    }

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_source_text));
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
    stored_sources
}

async fn run_bibliography_features_citation_fields_basic_variants_case(
    case: BibliographyFeaturesCitationFieldsBasicVariantsCase,
) {
    match case {
        BibliographyFeaturesCitationFieldsBasicVariantsCase::StarredRefs => {
            let fixture = prepare_bibliography_features_citation_fields_basic_variants_fixture(
                "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\ref*{sec:intro} on page \\pageref*{sec:intro}. Cite \\cite[see][chap.~2]{alpha}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha {Entry} \\emph{Title}.\\end{thebibliography}",
            );
            let stored_sources = assert_basic_citation_variant_output_and_sources(
                &fixture,
                &[
                    "See 1 on page 1.",
                    "Cite [see 1, chap.~2].",
                    "Alpha Entry Title.",
                ],
                "See 1 on page 1. Cite [see 1, chap.~2].",
                &["\\ref*", "\\pageref*", "\\cite[see][chap.~2]"],
            )
            .await;
            assert_eq!(
                stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
                "[1] Alpha Entry Title."
            );
        }
        BibliographyFeaturesCitationFieldsBasicVariantsCase::CiteauthorYear => {
            let fixture = prepare_bibliography_features_citation_fields_basic_variants_fixture(
                "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}) and \\citeauthor*{beta} (\\citeyear*{beta}).\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            );
            assert_basic_citation_variant_output_and_sources(
                &fixture,
                &["See Alpha (2024) and Beta and Gamma (2023)."],
                "See Alpha (2024) and Beta and Gamma (2023).",
                &["\\citeauthor", "\\citeyear"],
            )
            .await;
        }
        BibliographyFeaturesCitationFieldsBasicVariantsCase::CapitalizedAuthorYear => {
            let fixture = prepare_bibliography_features_citation_fields_basic_variants_fixture(
                "\\documentclass{article}\\begin{document}See \\Citeauthor{alpha} (\\Citeyear{alpha}) and \\Citeauthor*{beta} (\\Citeyear*{beta}).\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem[alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            );
            assert_basic_citation_variant_output_and_sources(
                &fixture,
                &["See Alpha (2024) and Beta and Gamma (2023)."],
                "See Alpha (2024) and Beta and Gamma (2023).",
                &["\\Citeauthor", "\\Citeyear"],
            )
            .await;
        }
        BibliographyFeaturesCitationFieldsBasicVariantsCase::Title => {
            let fixture = prepare_bibliography_features_citation_fields_basic_variants_fixture(
                "\\documentclass{article}\\begin{document}See \\citetitle{alpha} and \\Citetitle{beta}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} alpha entry title.\\bibitem[Beta 2023]{beta} beta study heading.\\end{thebibliography}",
            );
            assert_basic_citation_variant_output_and_sources(
                &fixture,
                &["See alpha entry title and Beta study heading."],
                "See alpha entry title and Beta study heading.",
                &["\\citetitle", "\\Citetitle"],
            )
            .await;
        }
        BibliographyFeaturesCitationFieldsBasicVariantsCase::Citefield => {
            let fixture = prepare_bibliography_features_citation_fields_basic_variants_fixture(
                "\\documentclass{article}\\begin{document}See \\citefield{alpha}{author}, \\citefield{alpha}{year}, \\citefield{alpha}{title}, and \\citefield{alpha}{label}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} alpha entry title.\\end{thebibliography}",
            );
            assert_basic_citation_variant_output_and_sources(
                &fixture,
                &["See Alpha, 2024, alpha entry title, and Alpha 2024."],
                "See Alpha, 2024, alpha entry title, and Alpha 2024.",
                &["\\citefield"],
            )
            .await;
        }
    }
}

type CiteBasic = BibliographyFeaturesCitationFieldsBasicVariantsCase;

async fn run_cite_basic(case: CiteBasic) {
    run_bibliography_features_citation_fields_basic_variants_case(case).await;
}
