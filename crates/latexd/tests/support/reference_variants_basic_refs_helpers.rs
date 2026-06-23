struct ReferenceVariantsBasicRefsFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum ReferenceVariantsTocNumberingCase {
    Subsubsection,
    OptionalTitle,
    Chapter,
}

enum ReferenceVariantsPageRangeCase {
    PageOriented,
    Varioref,
    CPageRefRange,
    PageRefRange,
}

enum ReferenceVariantsAppendixNumberingCase {
    Appendix,
    Appendices,
    Chapter,
}

type RefAppendix = ReferenceVariantsAppendixNumberingCase;

async fn run_ref_appendix(case: RefAppendix) {
    run_reference_variants_appendix_numbering(case).await;
}

enum ReferenceVariantsCommandFormsCase {
    FullRef,
    ThmRef,
    LabelCref,
    PluralNameCref,
    Cref,
    NameCref,
    CrefRange,
    Wrappers,
}

enum ReferenceVariantsAutorefCase {
    Base,
    ChapterAppendix,
    SubsectionDepth,
    ParagraphDepth,
}

enum ReferenceVariantsBasicRefsCase {
    EqRef,
    SubRef,
    NameRef,
    TitleRef,
    PreferredSectionTitle,
    PreferredFloatCaption,
}

type RefBasic = ReferenceVariantsBasicRefsCase;

async fn run_ref_basic(case: RefBasic) {
    run_reference_variants_basic_refs_case(case).await;
}

fn prepare_reference_variants_basic_refs_fixture(
    main_source: &str,
) -> ReferenceVariantsBasicRefsFixture {
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
    fs::write(root.join("book.cls"), "").expect("write book class");
    fs::write(root.join("main.tex"), main_source).expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    ReferenceVariantsBasicRefsFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_reference_variants_basic_refs_fixture(
    fixture: &ReferenceVariantsBasicRefsFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
}

async fn run_reference_variants_toc_numbering(case: ReferenceVariantsTocNumberingCase) {
    match case {
        ReferenceVariantsTocNumberingCase::Subsubsection => {
            let fixture = prepare_reference_variants_basic_refs_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\subsection{Scope}\\subsubsection{Detail}\\label{sec:detail}See \\ref{sec:detail}.\\end{document}",
            );
            compile_reference_variants_basic_refs_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("1 Intro"));
            assert!(output.contains("1.1 Scope"));
            assert!(output.contains("1.1.1 Detail"));
            assert!(output.contains("See 1.1.1."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 3);
            assert_eq!(aux.toc[2].title, "Detail");
            assert_eq!(aux.toc[2].level, 3);
            assert_eq!(aux.toc[2].number, "1.1.1");
            assert_eq!(aux.labels[0].number, "1.1.1");

            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            assert!(
                stored_sources.executed_files[&Utf8PathBuf::from("main.tex")]
                    .contains("See 1.1.1.")
            );
        }
        ReferenceVariantsTocNumberingCase::OptionalTitle => {
            let fixture = prepare_reference_variants_basic_refs_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\section[Short Intro]{Long Introduction}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
            );
            compile_reference_variants_basic_refs_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("1 Short Intro"));
            assert!(output.contains("Long Introduction"));
            assert!(output.contains("See 1."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 1);
            assert_eq!(aux.toc[0].title, "Short Intro");
            assert_eq!(aux.labels[0].number, "1");

            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
            assert!(executed_main.contains("Long Introduction"));
            assert!(!executed_main.contains("\\section[Short Intro]"));
        }
        ReferenceVariantsTocNumberingCase::Chapter => {
            let fixture = prepare_reference_variants_basic_refs_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\chapter{Intro}\\section{Scope}\\label{sec:scope}See \\ref{sec:scope}.\\end{document}",
            );
            compile_reference_variants_basic_refs_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("1 Intro"));
            assert!(output.contains("1.1 Scope"));
            assert!(output.contains("See 1.1."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 2);
            assert_eq!(aux.toc[0].level, 0);
            assert_eq!(aux.toc[0].number, "1");
            assert_eq!(aux.toc[1].number, "1.1");
            assert_eq!(aux.labels[0].number, "1.1");
        }
    }
}

async fn run_reference_variants_page_range(case: ReferenceVariantsPageRangeCase) {
    let (main_source, expected_output, removed_commands): (&str, &str, &[&str]) = match case {
        ReferenceVariantsPageRangeCase::PageOriented => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\subsection{Scope}\\label{sub:scope}See \\cpageref{sec:intro}, \\Cpageref{sub:scope}, \\vpageref{sub:scope}, \\autopageref{sec:intro}, \\vref{sec:intro}, and \\Vref{sub:scope}.\\end{document}",
            "See page 1, Page 1, page 1, page 1, section 1 on page 1, and Subsection 1.1 on page 1.",
            &[
                "\\cpageref",
                "\\Cpageref",
                "\\vpageref",
                "\\autopageref",
                "\\vref",
                "\\Vref",
            ],
        ),
        ReferenceVariantsPageRangeCase::Varioref => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\vpagerefrange{sec:intro}{sub:scope} and \\vrefrange{sec:intro}{sub:scope}.\\end{document}",
            "See pages 1 to 1 and section 1 on page 1 to section 2 on page 1.",
            &["\\vpagerefrange", "\\vrefrange"],
        ),
        ReferenceVariantsPageRangeCase::CPageRefRange => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\cpagerefrange{sec:intro}{sub:scope} and \\Cpagerefrange{sec:intro}{sub:scope}.\\end{document}",
            "See pages 1 to 1 and Pages 1 to 1.",
            &["\\cpagerefrange", "\\Cpagerefrange"],
        ),
        ReferenceVariantsPageRangeCase::PageRefRange => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\pagerefrange{sec:intro}{sub:scope}.\\end{document}",
            "See pages 1 to 1.",
            &["\\pagerefrange"],
        ),
    };
    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let build_root = &fixture.build_root;

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_output));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_output));
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
}

async fn run_reference_variants_appendix_numbering(case: ReferenceVariantsAppendixNumberingCase) {
    let (main_source, expected_output_texts, assert_executed_source) = match case {
        ReferenceVariantsAppendixNumberingCase::Appendix => (
            "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\appendix\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
            vec!["1 Intro", "A Proofs", "A.1 Lemma", "See A.1."],
            true,
        ),
        ReferenceVariantsAppendixNumberingCase::Appendices => (
            "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\appendices\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
            vec!["A Proofs", "A.1 Lemma", "See A.1."],
            false,
        ),
        ReferenceVariantsAppendixNumberingCase::Chapter => (
            "\\documentclass{article}\\begin{document}\\tableofcontents\\chapter{Intro}\\appendix\\chapter{Proofs}\\section{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
            vec!["1 Intro", "A Proofs", "A.1 Lemma", "See A.1."],
            false,
        ),
    };

    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let build_root = &fixture.build_root;

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    for expected_output_text in expected_output_texts {
        assert!(output.contains(expected_output_text));
    }

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 3);
    assert_eq!(aux.toc[1].number, "A");
    assert_eq!(aux.toc[2].number, "A.1");
    assert_eq!(aux.labels[0].number, "A.1");

    if assert_executed_source {
        let stored_sources = serde_json::from_slice::<StoredSources>(
            &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
        )
        .expect("parse sources");
        assert!(stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("See A.1."));
    }
}

async fn run_reference_variants_command_forms(case: ReferenceVariantsCommandFormsCase) {
    let (main_source, expected_output, removed_commands): (&str, &str, &[&str]) = match case {
        ReferenceVariantsCommandFormsCase::FullRef => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\fullref{sec:intro} and \\Fullref{thm:first}.\\end{document}",
            "See Section 1 (Intro) and Theorem 1 (Pythagoras).",
            &["\\fullref", "\\Fullref"],
        ),
        ReferenceVariantsCommandFormsCase::ThmRef => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{claim}\\label{clm:first}b\\end{claim}See \\thmref{thm:first} and \\Thmref{clm:first}.\\end{document}",
            "See Theorem 1 and Claim 1.",
            &["\\thmref", "\\Thmref"],
        ),
        ReferenceVariantsCommandFormsCase::LabelCref => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\subsection{Detail}\\label{sub:detail}See \\labelcref{sec:intro,eq:first} and \\labelcpageref{sec:intro,sub:detail}.\\end{document}",
            "See 1, (1) and 1, 1.",
            &["\\labelcref", "\\labelcpageref"],
        ),
        ReferenceVariantsCommandFormsCase::PluralNameCref => (
            "\\documentclass{article}\\begin{document}\\section{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\paragraph{Claim}\\label{par:claim}\\paragraph{Case}\\label{par:case}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\namecrefs{sub:scope,sub:detail}, \\nameCrefs{par:claim,par:case}, and \\lcnamecrefs{thm:first,lem:first}.\\end{document}",
            "See section; subsection, Paragraphs, and theorem; lemma.",
            &["\\namecrefs", "\\nameCrefs", "\\lcnamecrefs"],
        ),
        ReferenceVariantsCommandFormsCase::Cref => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\cref{sec:intro} and \\Cref*{sec:intro}.\\end{document}",
            "See Section 1 and Section 1.",
            &["\\cref", "\\Cref"],
        ),
        ReferenceVariantsCommandFormsCase::NameCref => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\subsection{Scope}\\label{sub:scope}\\subsubsection{Detail}\\label{subsub:detail}See \\namecref{sec:intro}, \\nameCref{sub:scope}, and \\lcnamecref{subsub:detail}.\\end{document}",
            "See section, Subsection, and subsubsection.",
            &["\\namecref", "\\nameCref", "\\lcnamecref"],
        ),
        ReferenceVariantsCommandFormsCase::CrefRange => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\paragraph{Claim}\\label{par:claim}\\paragraph{Case}\\label{par:case}See \\crefrange{sub:scope}{sub:detail} and \\Crefrange{par:claim}{par:case}.\\end{document}",
            "See Subsections 1.1 to 1.2 and Paragraphs 1.2.1 to 1.2.2.",
            &["\\crefrange", "\\Crefrange"],
        ),
        ReferenceVariantsCommandFormsCase::Wrappers => (
            "\\documentclass{article}\\begin{document}\\begin{figure}\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{equation}\\label{eq:first}b\\end{equation}\\end{document}",
            "Figure 1: Long Figure Titlea$b$",
            &[
                "\\begin{figure}",
                "\\end{figure}",
                "\\begin{equation}",
                "\\end{equation}",
            ],
        ),
    };

    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let build_root = &fixture.build_root;

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_output));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_output));
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
}

async fn run_reference_variants_autoref(case: ReferenceVariantsAutorefCase) {
    let (main_source, expected_output) = match case {
        ReferenceVariantsAutorefCase::Base => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\autoref{sec:intro} and \\autoref*{sec:intro}.\\end{document}",
            "See Section 1 and Section 1.",
        ),
        ReferenceVariantsAutorefCase::ChapterAppendix => (
            "\\documentclass{book}\\begin{document}\\chapter{Intro}\\label{chap:intro}\\appendix\\chapter{Proofs}\\section{Lemma}\\label{sec:lemma}See \\autoref{chap:intro}, \\autoref*{sec:lemma}, and \\autoref{chap:proof}.\\label{chap:proof}\\end{document}",
            "See Chapter 1, Appendix A.1, and Appendix A.",
        ),
        ReferenceVariantsAutorefCase::SubsectionDepth => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsubsection{Detail}\\label{subsub:detail}See \\autoref{sub:scope} and \\autoref{subsub:detail}.\\end{document}",
            "See Subsection 1.1 and Subsubsection 1.1.1.",
        ),
        ReferenceVariantsAutorefCase::ParagraphDepth => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\subsubsection{Detail}\\paragraph{Claim}\\label{par:claim}\\subparagraph{Case}\\label{subpar:case}See \\autoref{par:claim} and \\autoref{subpar:case}.\\end{document}",
            "See Paragraph 1.1.1.1 and Subparagraph 1.1.1.1.1.",
        ),
    };

    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let build_root = &fixture.build_root;

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_output));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_output));
    assert!(!executed_main.contains("\\autoref"));
}

async fn run_reference_variants_basic_refs_case(case: ReferenceVariantsBasicRefsCase) {
    let (
        main_source,
        expected_output_texts,
        absent_output_texts,
        expected_source_texts,
        absent_source_texts,
    ) = match case {
        ReferenceVariantsBasicRefsCase::EqRef => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}See \\eqref{eq:first} and \\eqref*{eq:second}.\\end{document}",
            vec!["See (1) and (2)."],
            vec![],
            vec!["See (1) and (2)."],
            vec!["\\eqref"],
        ),
        ReferenceVariantsBasicRefsCase::SubRef => (
            "\\documentclass{article}\\begin{document}\\begin{figure}\\label{fig:panel}a\\end{figure}\\begin{equation}\\label{eq:panel}b\\end{equation}See \\subref{fig:panel} and \\subeqref{eq:panel}.\\end{document}",
            vec!["See 1 and (1)."],
            vec![],
            vec!["See 1 and (1)."],
            vec!["\\subref", "\\subeqref"],
        ),
        ReferenceVariantsBasicRefsCase::NameRef => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\nameref{sec:intro} and \\nameref*{sec:intro}.\\end{document}",
            vec!["See Intro and Intro."],
            vec![],
            vec!["See Intro and Intro."],
            vec!["\\nameref"],
        ),
        ReferenceVariantsBasicRefsCase::TitleRef => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\titleref{sec:intro} and \\Titleref{thm:first}.\\end{document}",
            vec!["See Intro and Pythagoras."],
            vec![],
            vec!["See Intro and Pythagoras."],
            vec!["\\titleref", "\\Titleref"],
        ),
        ReferenceVariantsBasicRefsCase::PreferredSectionTitle => (
            "\\documentclass{article}\\begin{document}\\tableofcontents\\section[Short Intro]{Long Introduction}\\label{sec:intro}See \\nameref{sec:intro}.\\end{document}",
            vec![
                "1 Short Intro",
                "Long Introduction",
                "See Long Introduction.",
            ],
            vec![],
            vec!["See Long Introduction."],
            vec!["\\nameref"],
        ),
        ReferenceVariantsBasicRefsCase::PreferredFloatCaption => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{figure}\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}See \\nameref{fig:first}.\\end{document}",
            vec!["See Long Figure Title."],
            vec!["See Intro."],
            vec!["See Long Figure Title."],
            vec!["See Intro.", "\\nameref"],
        ),
    };

    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let build_root = &fixture.build_root;

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    for expected_output_text in expected_output_texts {
        assert!(output.contains(expected_output_text));
    }
    for absent_output_text in absent_output_texts {
        assert!(!output.contains(absent_output_text));
    }

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    for expected_source_text in expected_source_texts {
        assert!(executed_main.contains(expected_source_text));
    }
    for absent_source_text in absent_source_texts {
        assert!(!executed_main.contains(absent_source_text));
    }
}
