struct BibliographyFeaturesNatbibVariantsRender {
    output: String,
    executed_main: String,
}

async fn compile_bibliography_features_natbib_variants_case(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesNatbibVariantsRender {
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
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root,
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].clone();

    BibliographyFeaturesNatbibVariantsRender {
        output,
        executed_main,
    }
}

fn assert_bibliography_features_natbib_variants_render(
    render: &BibliographyFeaturesNatbibVariantsRender,
    expected: &str,
    removed_macros: &[&str],
) {
    assert!(render.output.contains(expected));
    assert!(render.executed_main.contains(expected));
    for removed_macro in removed_macros {
        assert!(!render.executed_main.contains(removed_macro));
    }
}

enum BibliographyFeaturesNatbibVariantCase {
    TextualBasic,
    TextualCapitalized,
    TextualNotes,
    CitetextNested,
    StarredTextual,
    Parenthetical,
}

async fn run_bibliography_features_natbib_variant(case: BibliographyFeaturesNatbibVariantCase) {
    let (main_source, refs_source, expected, removed_macros) = match case {
        BibliographyFeaturesNatbibVariantCase::TextualBasic => (
            "\\documentclass{article}\\begin{document}See \\citet{alpha} and \\citealt{beta} / \\citealp{beta} / \\onlinecite{beta}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023 / Beta et al. 2023.",
            &["\\citet", "\\citealt", "\\citealp", "\\onlinecite"][..],
        ),
        BibliographyFeaturesNatbibVariantCase::TextualCapitalized => (
            "\\documentclass{article}\\begin{document}See \\Citet{alpha} and \\Citealt{beta} / \\Citealp{beta}. \\Textcite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{2}\\bibitem[alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al. 2023]{beta} Beta entry.\\end{thebibliography}",
            "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023. Alpha (2024).",
            &["\\Citet", "\\Citealt", "\\Citealp", "\\Textcite"][..],
        ),
        BibliographyFeaturesNatbibVariantCase::TextualNotes => (
            "\\documentclass{article}\\begin{document}\\citet[see][chap.~2]{alpha} and \\citealt[e.g.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            "see Alpha (2024, chap.~2) and e.g. Beta et al. 2023, pp.~1--2.",
            &["\\citet", "\\citealt"][..],
        ),
        BibliographyFeaturesNatbibVariantCase::CitetextNested => (
            "\\documentclass{article}\\begin{document}See \\citetext{compare \\citealp{beta} with \\citeyearpar{alpha}}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            "See (compare Beta et al. 2023 with (2024)).",
            &["\\citetext", "\\citealp", "\\citeyearpar"][..],
        ),
        BibliographyFeaturesNatbibVariantCase::StarredTextual => (
            "\\documentclass{article}\\begin{document}See \\citet*{beta}, \\citep*{beta}, \\citealt*{beta} / \\citealp*{beta}, and \\Textcite*{beta}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            "See Beta and Gamma (2023), (Beta and Gamma, 2023), Beta and Gamma 2023 / Beta and Gamma 2023, and Beta and Gamma (2023).",
            &[
                "\\citet*",
                "\\citep*",
                "\\citealt*",
                "\\citealp*",
                "\\Textcite*",
            ][..],
        ),
        BibliographyFeaturesNatbibVariantCase::Parenthetical => (
            "\\documentclass{article}\\begin{document}See \\citep[see][chap.~2]{alpha} and \\Citep{beta}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al. 2023]{beta} Beta entry.\\end{thebibliography}",
            "See (see Alpha, 2024, chap.~2) and (Beta et al., 2023).",
            &["\\citep", "\\Citep"][..],
        ),
    };

    let render = compile_bibliography_features_natbib_variants_case(main_source, refs_source).await;
    assert_bibliography_features_natbib_variants_render(&render, expected, removed_macros);
}

type NatbibCase = BibliographyFeaturesNatbibVariantCase;

async fn run_natbib(case: NatbibCase) {
    run_bibliography_features_natbib_variant(case).await;
}
