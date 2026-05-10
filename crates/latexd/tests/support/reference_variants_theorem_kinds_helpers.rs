struct ReferenceVariantsTheoremKindsFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

fn prepare_reference_variants_theorem_kinds_fixture(
    main_source: &str,
) -> ReferenceVariantsTheoremKindsFixture {
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
    ReferenceVariantsTheoremKindsFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_reference_variants_theorem_kinds_fixture(
    fixture: &ReferenceVariantsTheoremKindsFixture,
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

enum ReferenceVariantsTheoremKindsCase {
    ClaimAndExample,
    AxiomFactAndObservation,
    ProblemExerciseQuestionAndNotation,
}

async fn run_reference_variants_theorem_kinds_case(case: ReferenceVariantsTheoremKindsCase) {
    let (main_source, expected_text, compile_success_message, absent_refs) = match case {
        ReferenceVariantsTheoremKindsCase::ClaimAndExample => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{claim}\\label{clm:first}a\\end{claim}\\begin{example}\\label{ex:first}b\\end{example}See \\autoref{clm:first}, \\cref{ex:first}, \\namecref{clm:first}, and \\vref{ex:first}.\\end{document}",
            "See Claim 1, Example 1, claim, and example 1 on page 1.",
            "semantic aux build should succeed",
            ["\\autoref", "\\cref", "\\namecref", "\\vref"],
        ),
        ReferenceVariantsTheoremKindsCase::AxiomFactAndObservation => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{axiom}\\label{ax:first}a\\end{axiom}\\begin{fact}\\label{fact:first}b\\end{fact}\\begin{observation}\\label{obs:first}c\\end{observation}See \\thmref{ax:first}, \\cref{fact:first}, \\namecref{obs:first}, and \\vref{fact:first}.\\end{document}",
            "See Axiom 1, Fact 1, observation, and fact 1 on page 1.",
            "semantic aux build should succeed",
            ["\\thmref", "\\cref", "\\namecref", "\\vref"],
        ),
        ReferenceVariantsTheoremKindsCase::ProblemExerciseQuestionAndNotation => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{problem}\\label{prob:first}a\\end{problem}\\begin{exercise}\\label{ex:first}b\\end{exercise}\\begin{question}\\label{q:first}c\\end{question}\\begin{notation}\\label{not:first}d\\end{notation}See \\thmref{prob:first}, \\cref{ex:first}, \\namecref{not:first}, and \\vref{q:first}.\\end{document}",
            "See Problem 1, Exercise 1, notation, and question 1 on page 1.",
            "internal compile should succeed",
            ["\\thmref", "\\cref", "\\namecref", "\\vref"],
        ),
    };

    let fixture = prepare_reference_variants_theorem_kinds_fixture(main_source);
    compile_reference_variants_theorem_kinds_fixture(&fixture)
        .await
        .expect(compile_success_message);

    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_text));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_text));
    for absent_ref in absent_refs {
        assert!(!executed_main.contains(absent_ref));
    }
}

type RefKinds = ReferenceVariantsTheoremKindsCase;

async fn run_ref_kinds(case: RefKinds) {
    run_reference_variants_theorem_kinds_case(case).await;
}
