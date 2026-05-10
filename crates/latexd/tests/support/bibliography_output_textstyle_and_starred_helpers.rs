async fn assert_bibliography_output_textstyle_and_starred_case(
    refs: &str,
    expected_output_fragment: &str,
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
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
    assert!(
        output.contains(expected_output_fragment),
        "output should contain expected fragment: {expected_output_fragment}\nactual: {output}"
    );
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

enum BibliographyOutputTextstyleAndStarredCase {
    CaseTextstyleAndTextsuper,
    Urlstyle,
    NameAffix,
    StarredCaseWrappers,
    StarredFormattingWrappers,
}

async fn run_bibliography_output_textstyle_and_starred_case(
    case: BibliographyOutputTextstyleAndStarredCase,
) {
    let (refs, expected_output_fragment, forbidden_output_fragments, expected_executed_refs) =
        match case {
            BibliographyOutputTextstyleAndStarredCase::CaseTextstyleAndTextsuper => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\NoCaseChange{NASA}. \\MakeSentenceCase{alpha title}. \\MakeTitleCase{beta title}. \\protect\\relax\\leavevmode\\ignorespaces   \\emph{Emph}. Trimmed \\unskip. \\phantom{Ghost}\\hphantom{Wide}\\vphantom{Tall}Visible. Tight\\!Join. Soft\\,Gap. Wide\\;Gap. Colon\\:Gap. Named\\space Gap. Backslash\\ Gap. Quote\\textquotesingle s. Double\\textquotedbl q. Angles\\textless x\\textgreater. Pipe\\textbar join. Path\\slash name. \\mbox{Stable}. \\hbox{Fixed}. \\fbox{Framed}. \\framebox[2em][c]{Wide}. \\raisebox{0.5ex}[1ex][0ex]{Raised}. \\parbox[t]{4em}{Paragraph}. \\makebox[3em][l]{Inline}. \\texttt{Code}. \\textsf{Sans}. \\textsc{Caps}. \\textbf{Bold}. \\textit{Italic}. \\textrm{Roman}. \\textup{Upright}. \\textmd{Medium}. \\textnormal{Normal}. Edition\\textsuperscript{2}\\textsubscript{a}.\\end{thebibliography}",
                "NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a.",
                vec![
                    "\\NoCaseChange",
                    "\\MakeSentenceCase",
                    "\\MakeTitleCase",
                    "\\protect",
                    "\\relax",
                    "\\leavevmode",
                    "\\ignorespaces",
                    "\\unskip",
                    "\\emph",
                    "\\mbox",
                    "\\hbox",
                    "\\fbox",
                    "\\framebox",
                    "\\raisebox",
                    "\\parbox",
                    "\\makebox",
                    "\\phantom",
                    "\\hphantom",
                    "\\vphantom",
                    "\\!",
                    "\\,",
                    "\\;",
                    "\\:",
                    "\\space",
                    "\\ Gap",
                    "\\textquotesingle",
                    "\\textquotedbl",
                    "\\textless",
                    "\\textgreater",
                    "\\textbar",
                    "\\slash",
                    "\\texttt",
                    "\\textsf",
                    "\\textsc",
                    "\\textbf",
                    "\\textit",
                    "\\textrm",
                    "\\textup",
                    "\\textmd",
                    "\\textnormal",
                    "\\textsuperscript",
                    "\\textsubscript",
                ],
                "[1] NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a.",
            ),
            BibliographyOutputTextstyleAndStarredCase::Urlstyle => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\urlstyle{same}\\url{https://example.test/paper}.\\end{thebibliography}",
                "https://example.test/paper.",
                vec!["\\urlstyle"],
                "[1] https://example.test/paper.",
            ),
            BibliographyOutputTextstyleAndStarredCase::NameAffix => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Doe}, \\mkbibnameaffix{Jr.}.\\end{thebibliography}",
                "Doe, Jr..",
                vec!["\\mkbibnamefamily", "\\mkbibnameaffix"],
                "[1] Doe, Jr..",
            ),
            BibliographyOutputTextstyleAndStarredCase::StarredCaseWrappers => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\MakeSentenceCase*{alpha title}. \\MakeTitleCase*{beta title}.\\end{thebibliography}",
                "alpha title. beta title.",
                vec!["\\MakeSentenceCase*", "\\MakeTitleCase*"],
                "[1] alpha title. beta title.",
            ),
            BibliographyOutputTextstyleAndStarredCase::StarredFormattingWrappers => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote*{Alpha Title}. \\mkbibparens*{2024}. \\mkbibbrackets*{note}. \\mkbibbraces*{Supplement}. \\mkbibemph*{Emph}. \\mkbibbold*{Bold}. \\mkbibitalic*{Italic}.\\end{thebibliography}",
                "\"Alpha Title\". (2024). [note]. Supplement. Emph. Bold. Italic.",
                vec![
                    "\\mkbibquote*",
                    "\\mkbibparens*",
                    "\\mkbibbrackets*",
                    "\\mkbibbraces*",
                    "\\mkbibemph*",
                    "\\mkbibbold*",
                    "\\mkbibitalic*",
                ],
                "[1] \"Alpha Title\". (2024). [note]. {Supplement}. Emph. Bold. Italic.",
            ),
        };

    assert_bibliography_output_textstyle_and_starred_case(
        refs,
        expected_output_fragment,
        &forbidden_output_fragments,
        expected_executed_refs,
    )
    .await;
}

type BibOutStyle = BibliographyOutputTextstyleAndStarredCase;

async fn run_bib_out_style(case: BibOutStyle) {
    run_bibliography_output_textstyle_and_starred_case(case).await;
}
