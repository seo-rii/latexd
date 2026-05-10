struct BiblatexPrintSimpleRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    output: String,
    executed_main: String,
}

enum BiblatexPrintSimpleCase {
    CoreTextualParentheticalAndPrintbibliography,
    Smartcite,
    FullciteAndBibentry,
    Multicite,
    Supercite,
    PrintbibliographyBibintocHeading,
    PrintbibliographyBibnumberedHeading,
    PrintbibheadingBibnumbered,
}

enum BiblatexPrintSimpleAuxAssertion {
    None,
    TocTitle(&'static str),
    TocNumberAndTitle {
        number: &'static str,
        title: &'static str,
    },
}

async fn compile_biblatex_print_simple_fixture(
    main_source: &str,
    refs_bbl: Option<&str>,
) -> BiblatexPrintSimpleRun {
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

    let mut changed_files = vec![Utf8PathBuf::from("main.tex")];
    if let Some(refs_bbl) = refs_bbl {
        fs::write(root.join("refs.bbl"), refs_bbl).expect("write bbl");
        changed_files.push(Utf8PathBuf::from("refs.bbl"));
    }

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
            changed_files,
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].clone();

    BiblatexPrintSimpleRun {
        _tempdir: tempdir,
        build_root,
        output,
        executed_main,
    }
}

fn assert_biblatex_print_simple_render(
    run: &BiblatexPrintSimpleRun,
    rendered_text: &str,
    removed_commands: &[&str],
) {
    assert!(
        run.output.contains(rendered_text),
        "output should contain rendered text: {}\n{}",
        rendered_text,
        run.output
    );
    assert!(
        run.executed_main.contains(rendered_text),
        "executed main should contain rendered text: {}\n{}",
        rendered_text,
        run.executed_main
    );
    for removed_command in removed_commands {
        assert!(
            !run.executed_main.contains(removed_command),
            "executed main should not contain {}: {}",
            removed_command,
            run.executed_main
        );
    }
}

fn load_biblatex_print_simple_aux(run: &BiblatexPrintSimpleRun) -> tex_aux::SemanticAux {
    load_semantic_aux(&run.build_root.join("rev-1/aux.json")).expect("load aux")
}

async fn run_biblatex_print_simple_case(case: BiblatexPrintSimpleCase) {
    let (
        main_source,
        refs_bbl,
        rendered_text,
        removed_commands,
        extra_output_texts,
        expects_bbl_input,
        aux_assertion,
    ) = match case {
        BiblatexPrintSimpleCase::CoreTextualParentheticalAndPrintbibliography => (
            "\\documentclass{article}\\begin{document}\\addbibresource{refs.bib}\\textcite{alpha} and \\parencite[see][pp.~1--2]{beta}.\\printbibliography\\end{document}",
            Some(
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            ),
            "Alpha (2024) and (see Beta et al., 2023, pp.~1--2).",
            vec![
                "\\textcite",
                "\\parencite",
                "\\printbibliography",
                "\\addbibresource",
            ],
            vec!["[1] Alpha entry.", "[2] Beta entry."],
            true,
            BiblatexPrintSimpleAuxAssertion::None,
        ),
        BiblatexPrintSimpleCase::Smartcite => (
            "\\documentclass{article}\\begin{document}\\smartcite{alpha} and \\smartcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
            Some(
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            ),
            "(Alpha, 2024) and (see Alpha, 2024, chap.~2; cf. Beta et al., 2023, pp.~1--2).",
            vec!["\\smartcite", "\\smartcites"],
            Vec::new(),
            false,
            BiblatexPrintSimpleAuxAssertion::None,
        ),
        BiblatexPrintSimpleCase::FullciteAndBibentry => (
            "\\documentclass{article}\\begin{document}\\fullcite{alpha} and \\bibentry{beta}.\\bibliography{refs}\\end{document}",
            Some(
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
            ),
            "Alpha entry. and Beta entry..",
            vec!["\\fullcite", "\\bibentry"],
            Vec::new(),
            false,
            BiblatexPrintSimpleAuxAssertion::None,
        ),
        BiblatexPrintSimpleCase::Multicite => (
            "\\documentclass{article}\\begin{document}\\textcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta} and \\parencites{alpha}[cf.]{beta}.\\bibliography{refs}\\end{document}",
            Some(
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            ),
            "see Alpha (2024, chap.~2); cf. Beta et al. (2023, pp.~1--2) and (Alpha, 2024; cf. Beta et al., 2023).",
            vec!["\\textcites", "\\parencites"],
            Vec::new(),
            false,
            BiblatexPrintSimpleAuxAssertion::None,
        ),
        BiblatexPrintSimpleCase::Supercite => (
            "\\documentclass{article}\\begin{document}\\supercite{alpha} and \\supercites[see]{alpha}[cf.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
            Some(
                "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
            ),
            "^1 and ^see 1; cf. 2, pp.~1--2.",
            vec!["\\supercite", "\\supercites"],
            Vec::new(),
            false,
            BiblatexPrintSimpleAuxAssertion::None,
        ),
        BiblatexPrintSimpleCase::PrintbibliographyBibintocHeading => (
            "\\documentclass{article}\\begin{document}\\tableofcontents\\addbibresource{refs.bib}\\printbibliography[heading=bibintoc,title={References}]\\end{document}",
            Some("\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}"),
            "References",
            vec!["\\printbibliography", "\\addbibresource"],
            vec!["[1] Alpha entry.", "Contents"],
            true,
            BiblatexPrintSimpleAuxAssertion::TocTitle("References"),
        ),
        BiblatexPrintSimpleCase::PrintbibliographyBibnumberedHeading => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\addbibresource{refs.bib}\\printbibliography[heading=bibnumbered,title={References}]\\end{document}",
            Some("\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}"),
            "2 References",
            vec!["\\printbibliography"],
            vec!["[1] Alpha entry."],
            true,
            BiblatexPrintSimpleAuxAssertion::TocNumberAndTitle {
                number: "2",
                title: "References",
            },
        ),
        BiblatexPrintSimpleCase::PrintbibheadingBibnumbered => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\printbibheading[heading=bibnumbered,title={References}]\\end{document}",
            None,
            "2 References",
            vec!["\\printbibheading"],
            Vec::new(),
            false,
            BiblatexPrintSimpleAuxAssertion::TocNumberAndTitle {
                number: "2",
                title: "References",
            },
        ),
    };

    let run = compile_biblatex_print_simple_fixture(main_source, refs_bbl).await;
    assert_biblatex_print_simple_render(&run, rendered_text, &removed_commands);
    for extra_output_text in extra_output_texts {
        assert!(run.output.contains(extra_output_text));
    }
    if expects_bbl_input {
        assert!(run.executed_main.contains("\\input{refs.bbl}"));
    }

    match aux_assertion {
        BiblatexPrintSimpleAuxAssertion::None => {}
        BiblatexPrintSimpleAuxAssertion::TocTitle(title) => {
            let aux = load_biblatex_print_simple_aux(&run);
            assert!(aux.toc.iter().any(|entry| entry.title == title));
        }
        BiblatexPrintSimpleAuxAssertion::TocNumberAndTitle { number, title } => {
            let aux = load_biblatex_print_simple_aux(&run);
            assert!(
                aux.toc
                    .iter()
                    .any(|entry| entry.number == number && entry.title == title)
            );
        }
    }
}

type BibPrint = BiblatexPrintSimpleCase;

async fn run_bib_print(case: BibPrint) {
    run_biblatex_print_simple_case(case).await;
}
