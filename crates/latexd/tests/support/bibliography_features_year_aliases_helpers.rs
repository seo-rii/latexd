struct BibliographyFeaturesYearAliasesRender {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    output: String,
    executed_main: String,
}

enum BibliographyFeaturesYearAliasesCase {
    Citeyearpar,
    Natexlab,
    Citefullauthor,
    AliasVariants,
}

async fn compile_bibliography_features_year_aliases_case(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesYearAliasesRender {
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

    BibliographyFeaturesYearAliasesRender {
        _tempdir: tempdir,
        build_root,
        output,
        executed_main,
    }
}

fn assert_bibliography_features_year_aliases_render(
    render: &BibliographyFeaturesYearAliasesRender,
    expected: &str,
    removed_macros: &[&str],
) {
    assert!(render.output.contains(expected));
    assert!(render.executed_main.contains(expected));
    for removed_macro in removed_macros {
        assert!(!render.executed_main.contains(removed_macro));
    }
}

async fn run_bibliography_features_year_aliases_case(case: BibliographyFeaturesYearAliasesCase) {
    match case {
        BibliographyFeaturesYearAliasesCase::Citeyearpar => {
            let render = compile_bibliography_features_year_aliases_case(
                "\\documentclass{article}\\begin{document}See \\citeyear{alpha} and \\citeyearpar*{beta}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024a]{alpha} Alpha entry.\\bibitem[Beta et al., 2023b]{beta} Beta entry.\\end{thebibliography}",
            )
            .await;

            assert_bibliography_features_year_aliases_render(
                &render,
                "See 2024a and (2023b).",
                &["\\citeyear", "\\citeyearpar"],
            );
        }
        BibliographyFeaturesYearAliasesCase::Natexlab => {
            let render = compile_bibliography_features_year_aliases_case(
                "\\documentclass{article}\\begin{document}See \\citeyear{alpha} and \\citeyearpar{beta}.\\bibliography{refs}\\end{document}",
                r"\begin{thebibliography}{2}\bibitem[Alpha 2024\natexlab{a}]{alpha} Alpha entry.\bibitem[Beta et al., 2023\NAT@exlab{b}]{beta} Beta entry.\end{thebibliography}",
            )
            .await;

            assert_bibliography_features_year_aliases_render(
                &render,
                "See 2024a and (2023b).",
                &[],
            );
            assert!(!render.output.contains("\\natexlab"));
            assert!(!render.output.contains("\\NAT@exlab"));
        }
        BibliographyFeaturesYearAliasesCase::Citefullauthor => {
            let render = compile_bibliography_features_year_aliases_case(
                "\\documentclass{article}\\begin{document}See \\citefullauthor{alpha} and \\Citefullauthor*{beta} in \\Citeyearpar{alpha}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{2}\\bibitem{alpha}\\bibinfo{author}{Alpha and Beta}. \\bibinfo{year}{2024}.\\bibitem[beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            )
            .await;

            assert_bibliography_features_year_aliases_render(
                &render,
                "See Alpha and Beta and Beta and Gamma in (2024).",
                &["\\citefullauthor", "\\Citefullauthor", "\\Citeyearpar"],
            );
        }
        BibliographyFeaturesYearAliasesCase::AliasVariants => {
            let render = compile_bibliography_features_year_aliases_case(
                "\\documentclass{article}\\begin{document}\\defcitealias{alpha}{Paper I}See \\citetalias{alpha}, \\citepalias{alpha}, and \\Citetalias{alpha}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
            )
            .await;

            assert_bibliography_features_year_aliases_render(
                &render,
                "See Paper I, (Paper I), and Paper I.",
                &["\\citetalias", "\\citepalias", "\\Citetalias"],
            );

            let aux =
                load_semantic_aux(&render.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.citation_alias_text("alpha"), Some("Paper I"));
        }
    }
}

type YearAlias = BibliographyFeaturesYearAliasesCase;

async fn run_year_alias(case: YearAlias) {
    run_bibliography_features_year_aliases_case(case).await;
}
