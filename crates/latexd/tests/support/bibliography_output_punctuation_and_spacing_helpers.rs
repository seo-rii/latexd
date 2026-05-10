enum BibliographyOutputPunctuationAndSpacingCase {
    LowLevelHelpers,
    SuperSubAndBraces,
    DashAndSlash,
    ParentextAndSpacing,
    UrlPathDetokenize,
}

async fn assert_bibliography_output_punctuation_and_spacing_case(
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

async fn run_bibliography_output_punctuation_and_spacing_case(
    case: BibliographyOutputPunctuationAndSpacingCase,
) {
    let (refs, expected_output_fragments, forbidden_output_fragments, expected_executed_refs) =
        match case {
            BibliographyOutputPunctuationAndSpacingCase::LowLevelHelpers => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\adddotspace Beta\\unspace\\isdot\\nopunct Gamma\\isdot \\bibopenparen Delta\\bibcloseparen \\bibopenbracket Epsilon\\bibclosebracket \\bibopenbrace Zeta\\bibclosebrace\\end{thebibliography}",
                vec!["Alpha. Beta. Gamma. (Delta) [Epsilon] Zeta"],
                vec![
                    "\\adddotspace",
                    "\\unspace",
                    "\\isdot",
                    "\\nopunct",
                    "\\bibopenparen",
                    "\\bibcloseparen",
                    "\\bibopenbracket",
                    "\\bibclosebracket",
                    "\\bibopenbrace",
                    "\\bibclosebrace",
                ],
                "[1] Alpha. Beta. Gamma. (Delta) [Epsilon] {Zeta}",
            ),
            BibliographyOutputPunctuationAndSpacingCase::SuperSubAndBraces => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}Edition\\mkbibsuperscript{2}\\mkbibsubscript{a} \\mkbibbraces{Supplement}.\\end{thebibliography}",
                vec!["Edition2a Supplement."],
                vec!["\\mkbibsuperscript", "\\mkbibsubscript", "\\mkbibbraces"],
                "[1] Edition2a {Supplement}.",
            ),
            BibliographyOutputPunctuationAndSpacingCase::DashAndSlash => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}Pages 10\\bibrangedash20\\addcomma\\addspace Vol\\adddot 2\\addslash Issue 3\\addhyphen4\\textendash5\\textemdash appendix.\\end{thebibliography}",
                vec!["Pages 10-20, Vol. 2/Issue 3-4-5--- appendix."],
                vec![
                    "\\bibrangedash",
                    "\\addslash",
                    "\\addhyphen",
                    "\\textendash",
                    "\\textemdash",
                ],
                "[1] Pages 10-20, Vol. 2/Issue 3-4-5--- appendix.",
            ),
            BibliographyOutputPunctuationAndSpacingCase::ParentextAndSpacing => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addabbrvspace Beta\\addnbspace Gamma\\addthinspace Delta\\addlowpenspace Epsilon\\addhighpenspace Zeta\\parentext{Supplement}.\\end{thebibliography}",
                vec!["Alpha Beta Gamma Delta Epsilon Zeta (Supplement)."],
                vec![
                    "\\addabbrvspace",
                    "\\addnbspace",
                    "\\addthinspace",
                    "\\addlowpenspace",
                    "\\addhighpenspace",
                    "\\parentext",
                ],
                "[1] Alpha Beta Gamma Delta Epsilon Zeta (Supplement).",
            ),
            BibliographyOutputPunctuationAndSpacingCase::UrlPathDetokenize => (
                "\\begin{thebibliography}{1}\\bibitem{alpha}Source: \\nolinkurl{https://example.test/paper} at \\path{/tmp/archive} via \\detokenize{arXiv:2401.01234}.\\end{thebibliography}",
                vec!["Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234."],
                vec!["\\nolinkurl", "\\path", "\\detokenize"],
                "[1] Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234.",
            ),
        };
    assert_bibliography_output_punctuation_and_spacing_case(
        refs,
        &expected_output_fragments,
        &forbidden_output_fragments,
        expected_executed_refs,
    )
    .await;
}

type BibOutPunc = BibliographyOutputPunctuationAndSpacingCase;

async fn run_bib_out_punc(case: BibOutPunc) {
    run_bibliography_output_punctuation_and_spacing_case(case).await;
}
