struct ReferenceVariantsTheoremSetupFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

fn prepare_reference_variants_theorem_setup_fixture(
    main_source: &str,
) -> ReferenceVariantsTheoremSetupFixture {
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

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    ReferenceVariantsTheoremSetupFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_reference_variants_theorem_setup_fixture(
    fixture: &ReferenceVariantsTheoremSetupFixture,
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

fn assert_reference_variants_theorem_setup_output_and_sources(
    fixture: &ReferenceVariantsTheoremSetupFixture,
    expected_texts: &[&str],
    expected_source_texts: &[&str],
    expected_labels: &[(&str, &str)],
    removed_commands: &[&str],
) {
    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    for expected_text in expected_texts {
        assert!(
            output.contains(expected_text),
            "missing {expected_text:?} in output {output:?}"
        );
    }

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    for expected_text in expected_source_texts {
        assert!(
            executed_main.contains(expected_text),
            "missing {expected_text:?} in materialized source {executed_main:?}"
        );
    }
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
    assert_reference_label_shipouts(&fixture.build_root, expected_labels);

    // These fixtures contain no authored hyphens. The presentation layout may
    // add a standalone hyphen run when wrapping a word such as Observation.
    // Keep the exact output.txt checks above; only ignore wrapping here.
    let rendered: String = load_page_display_list_text_runs(&fixture.build_root, 1)
        .into_iter()
        .filter(|run| run != "-")
        .collect::<String>()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    for expected_text in expected_texts {
        assert!(
            !expected_text.contains('-'),
            "fixture must not contain authored hyphens"
        );
        let expected_visible: String = expected_text
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(
            rendered.contains(&expected_visible),
            "missing {expected_text:?} in rendered text {rendered:?}"
        );
    }
}

enum ReferenceVariantsTheoremSetupCase {
    AlgorithmKinds,
    TheoremKinds,
    TheoremAndProofHeaders,
    TheoremstyleDeclarations,
    NewtheoremDefinedEnvironments,
    NewtheoremSharedCounters,
    NewtheoremSectionScopedCounters,
    NewtheoremBuiltinOverrideSharedScope,
}

async fn run_reference_variants_theorem_setup_case(case: ReferenceVariantsTheoremSetupCase) {
    let expected_labels: &[(&str, &str)] = match &case {
        ReferenceVariantsTheoremSetupCase::AlgorithmKinds => &[("alg:first", "1")],
        ReferenceVariantsTheoremSetupCase::TheoremKinds => {
            &[("thm:first", "1"), ("lem:first", "1")]
        }
        ReferenceVariantsTheoremSetupCase::TheoremAndProofHeaders => &[("thm:first", "1")],
        ReferenceVariantsTheoremSetupCase::TheoremstyleDeclarations => &[("obs:first", "1")],
        ReferenceVariantsTheoremSetupCase::NewtheoremDefinedEnvironments
        | ReferenceVariantsTheoremSetupCase::NewtheoremSharedCounters => {
            &[("obs:first", "1"), ("obs:second", "2")]
        }
        ReferenceVariantsTheoremSetupCase::NewtheoremSectionScopedCounters => {
            &[("obs:first", "1.1"), ("obs:second", "2.1")]
        }
        ReferenceVariantsTheoremSetupCase::NewtheoremBuiltinOverrideSharedScope => &[
            ("thm:first", "1.1"),
            ("cor:first", "1.2"),
            ("thm:second", "2.1"),
            ("cor:second", "2.2"),
        ],
    };
    // Labels remain executable so the real page builder can emit destinations.
    // They are intentionally present between the generated header and its body.
    let expected_source_texts: Option<&[&str]> = match &case {
        ReferenceVariantsTheoremSetupCase::AlgorithmKinds
        | ReferenceVariantsTheoremSetupCase::TheoremKinds => None,
        ReferenceVariantsTheoremSetupCase::TheoremAndProofHeaders => {
            Some(&[r"Theorem 1 (Pythagoras). \label{thm:first}aProof (Sketch). See Pythagoras."])
        }
        ReferenceVariantsTheoremSetupCase::TheoremstyleDeclarations => Some(&[
            r"Observation Lemma 1. \label{obs:first}a",
            "See Observation Lemma 1.",
        ]),
        ReferenceVariantsTheoremSetupCase::NewtheoremDefinedEnvironments => Some(&[
            r"Observation Lemma 1. \label{obs:first}a",
            r"Observation Lemma 2 (Second). \label{obs:second}b",
            "See Observation Lemma 1, Observation Lemmas 1, 2, observation lemmas, and Second.",
        ]),
        ReferenceVariantsTheoremSetupCase::NewtheoremSharedCounters => Some(&[
            r"Observation Lemma 1. \label{obs:first}a",
            r"Observation Corollary 2. \label{obs:second}b",
            "See Observation Lemma 1 and Observation Corollary 2.",
        ]),
        ReferenceVariantsTheoremSetupCase::NewtheoremSectionScopedCounters => Some(&[
            r"Observation Lemma 1.1. \label{obs:first}a",
            r"Observation Lemma 2.1. \label{obs:second}b",
            "See Observation Lemma 1.1 and Observation Lemma 2.1.",
        ]),
        ReferenceVariantsTheoremSetupCase::NewtheoremBuiltinOverrideSharedScope => Some(&[
            r"Theorem 1.1. \label{thm:first}a",
            r"Corollary 1.2. \label{cor:first}b",
            r"Theorem 2.1. \label{thm:second}c",
            r"Corollary 2.2. \label{cor:second}d",
            "See Theorem 1.1, Corollary 1.2, Theorem 2.1, and Corollary 2.2.",
        ]),
    };
    let (main_source, expected_texts, removed_commands) = match case {
        ReferenceVariantsTheoremSetupCase::AlgorithmKinds => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{algorithm}\\label{alg:first}a\\end{algorithm}See \\autoref{alg:first}, \\namecref{alg:first}, and \\vref{alg:first}.\\end{document}",
            vec!["See Algorithm 1, algorithm, and algorithm 1 on page 1."],
            vec!["\\autoref", "\\namecref", "\\vref"],
        ),
        ReferenceVariantsTheoremSetupCase::TheoremKinds => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\autoref{thm:first}, \\cref{lem:first}, \\namecref{thm:first}, and \\vref{lem:first}.\\end{document}",
            vec!["See Theorem 1, Lemma 1, theorem, and lemma 1 on page 1."],
            vec!["\\autoref", "\\cref", "\\namecref", "\\vref"],
        ),
        ReferenceVariantsTheoremSetupCase::TheoremAndProofHeaders => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}\\begin{proof}[Sketch]See \\nameref{thm:first}.\\end{proof}\\end{document}",
            vec!["Theorem 1 (Pythagoras). aProof (Sketch). See Pythagoras."],
            vec![
                "\\begin{theorem}",
                "\\end{theorem}",
                "\\begin{proof}",
                "\\end{proof}",
                "\\nameref",
            ],
        ),
        ReferenceVariantsTheoremSetupCase::TheoremstyleDeclarations => (
            "\\documentclass{article}\\begin{document}\\theoremstyle{definition}\\newtheoremstyle{tight}{}{}{}{}{}{}{ }{}\\swapnumbers\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}See \\autoref{obs:first}.\\end{document}",
            vec!["Observation Lemma 1. a", "See Observation Lemma 1."],
            vec!["\\theoremstyle", "\\newtheoremstyle", "\\swapnumbers"],
        ),
        ReferenceVariantsTheoremSetupCase::NewtheoremDefinedEnvironments => (
            "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{oblemma}[Second]\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first}, \\cref{obs:first,obs:second}, \\namecrefs{obs:first,obs:second}, and \\nameref{obs:second}.\\end{document}",
            vec![
                "Observation Lemma 1. a",
                "Observation Lemma 2 (Second). b",
                "See Observation Lemma 1, Observation Lemmas 1, 2, observation lemmas, and Second.",
            ],
            vec![
                "\\newtheorem",
                "\\begin{oblemma}",
                "\\end{oblemma}",
                "\\autoref",
                "\\cref",
                "\\namecrefs",
                "\\nameref",
            ],
        ),
        ReferenceVariantsTheoremSetupCase::NewtheoremSharedCounters => (
            "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}\\newtheorem{obcor}[oblemma]{Observation Corollary}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{obcor}\\label{obs:second}b\\end{obcor}See \\autoref{obs:first} and \\autoref{obs:second}.\\end{document}",
            vec![
                "Observation Lemma 1. a",
                "Observation Corollary 2. b",
                "See Observation Lemma 1 and Observation Corollary 2.",
            ],
            vec![
                "\\newtheorem",
                "\\begin{oblemma}",
                "\\begin{obcor}",
                "\\autoref",
            ],
        ),
        ReferenceVariantsTheoremSetupCase::NewtheoremSectionScopedCounters => (
            "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}[section]\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\section{Next}\\begin{oblemma}\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first} and \\autoref{obs:second}.\\end{document}",
            vec![
                "Observation Lemma 1.1. a",
                "Observation Lemma 2.1. b",
                "See Observation Lemma 1.1 and Observation Lemma 2.1.",
            ],
            vec!["\\newtheorem", "\\autoref"],
        ),
        ReferenceVariantsTheoremSetupCase::NewtheoremBuiltinOverrideSharedScope => (
            "\\documentclass{article}\\begin{document}\\newtheorem{theorem}{Theorem}[section]\\newtheorem{cor}[theorem]{Corollary}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{cor}\\label{cor:first}b\\end{cor}\\section{Next}\\begin{theorem}\\label{thm:second}c\\end{theorem}\\begin{cor}\\label{cor:second}d\\end{cor}See \\autoref{thm:first}, \\autoref{cor:first}, \\autoref{thm:second}, and \\autoref{cor:second}.\\end{document}",
            vec![
                "Theorem 1.1. a",
                "Corollary 1.2. b",
                "Theorem 2.1. c",
                "Corollary 2.2. d",
                "See Theorem 1.1, Corollary 1.2, Theorem 2.1, and Corollary 2.2.",
            ],
            vec!["\\newtheorem", "\\autoref"],
        ),
    };

    let fixture = prepare_reference_variants_theorem_setup_fixture(main_source);
    compile_reference_variants_theorem_setup_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    assert_reference_variants_theorem_setup_output_and_sources(
        &fixture,
        &expected_texts,
        expected_source_texts.unwrap_or(&expected_texts),
        expected_labels,
        &removed_commands,
    );
}

type RefSetup = ReferenceVariantsTheoremSetupCase;

async fn run_ref_setup(case: RefSetup) {
    run_reference_variants_theorem_setup_case(case).await;
}
