enum BibliographyOutputBasicWrappersCase {
    BibstringAndAcro,
    CommonWrappers,
    Href,
    NewunitAndPunctuation,
    UrlprefixAndNamedash,
}

async fn assert_bibliography_output_basic_wrappers_case(
    main_source: &str,
    refs: &str,
    expected_output_fragments: &[&str],
    forbidden_output_fragments: &[&str],
    expected_executed_refs: &str,
) {
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
    fs::write(root.join("refs.bbl"), refs).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    for expected in expected_output_fragments {
        assert!(
            output.contains(expected),
            "output should contain expected fragment: {expected}\nactual: {output}"
        );
    }
    for forbidden in forbidden_output_fragments {
        assert!(
            !output.contains(forbidden),
            "output should not contain forbidden fragment: {forbidden}\nactual: {output}"
        );
    }

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        expected_executed_refs
    );
}

async fn run_bibliography_output_basic_wrappers_case(case: BibliographyOutputBasicWrappersCase) {
    let (
        main_source,
        refs,
        expected_output_fragments,
        forbidden_output_fragments,
        expected_executed_refs,
    ) = match case {
        BibliographyOutputBasicWrappersCase::BibstringAndAcro => (
            "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Alpha} \\bibstring{andothers}. \\mkbibacro{URL}: \\url{https://example.test/paper}.\\end{thebibliography}",
            vec!["Alpha et al. URL: https://example.test/paper."],
            vec!["\\bibstring", "\\mkbibacro", "\\mkbibnamefamily"],
            "[1] Alpha et al. URL: https://example.test/paper.",
        ),
        BibliographyOutputBasicWrappersCase::CommonWrappers => (
            "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote{Alpha Title}. \\mkbibparens{2024}. \\mkbibbrackets{note}. \\mkbibemph{Emph}. \\mkbibbold{Bold}. \\mkbibitalic{Italic}. \\enquote{Nested}.\\end{thebibliography}",
            vec!["\"Alpha Title\". (2024). [note]. Emph. Bold. Italic. \"Nested\"."],
            vec![
                "\\mkbibquote",
                "\\mkbibparens",
                "\\mkbibbrackets",
                "\\mkbibemph",
                "\\mkbibbold",
                "\\mkbibitalic",
                "\\enquote",
            ],
            "[1] \"Alpha Title\". (2024). [note]. Emph. Bold. Italic. \"Nested\".",
        ),
        BibliographyOutputBasicWrappersCase::Href => (
            "\\documentclass{article}\\begin{document}See \\cite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha \\href{https://example.test/paper}{Paper Link}.\\end{thebibliography}",
            vec!["See [1].", "Alpha Paper Link."],
            vec!["https://example.test/paper"],
            "[1] Alpha Paper Link.",
        ),
        BibliographyOutputBasicWrappersCase::NewunitAndPunctuation => (
            "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addcomma\\addspace Beta\\newunit Gamma\\addcolon\\addspace Delta\\addsemicolon\\addspace Epsilon\\adddot\\finentry\\end{thebibliography}",
            vec!["Alpha, Beta Gamma: Delta; Epsilon."],
            vec![
                "\\newunit",
                "\\finentry",
                "\\addcomma",
                "\\addspace",
                "\\addcolon",
                "\\addsemicolon",
                "\\adddot",
            ],
            "[1] Alpha, Beta Gamma: Delta; Epsilon.",
        ),
        BibliographyOutputBasicWrappersCase::UrlprefixAndNamedash => (
            "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibnamedash. \\urlprefix\\url{https://example.test/paper}.\\end{thebibliography}",
            vec!["---. https://example.test/paper."],
            vec!["\\urlprefix", "\\bibnamedash"],
            "[1] ---. https://example.test/paper.",
        ),
    };
    assert_bibliography_output_basic_wrappers_case(
        main_source,
        refs,
        &expected_output_fragments,
        &forbidden_output_fragments,
        expected_executed_refs,
    )
    .await;
}

type BibOutBasic = BibliographyOutputBasicWrappersCase;

async fn run_bib_out_basic(case: BibOutBasic) {
    run_bibliography_output_basic_wrappers_case(case).await;
}
