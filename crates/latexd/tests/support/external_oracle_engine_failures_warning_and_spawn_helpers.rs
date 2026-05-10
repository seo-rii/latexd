struct ExternalOracleEngineFailuresWarningAndSpawnFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    tool_dir: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

enum ExternalOracleWarningAndSpawnFailureCase {
    PdfLatexFallback,
    Tectonic,
}

type SpawnFail = ExternalOracleWarningAndSpawnFailureCase;

async fn run_spawn_fail(case: SpawnFail) {
    run_external_oracle_warning_and_spawn_spawn_failure_case(case).await;
}

enum ExternalOracleWarningAndSpawnWarningCase {
    PdfLatex,
    Tectonic,
}

type EngineWarn = ExternalOracleWarningAndSpawnWarningCase;

async fn run_engine_warn(case: EngineWarn) {
    run_external_oracle_warning_and_spawn_warning_case(case).await;
}

fn prepare_external_oracle_engine_failures_warning_and_spawn_fixture(
    main_source: &str,
) -> ExternalOracleEngineFailuresWarningAndSpawnFixture {
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
    fs::write(root.join("main.tex"), main_source).expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
    ExternalOracleEngineFailuresWarningAndSpawnFixture {
        _tempdir: tempdir,
        root,
        tool_dir,
        world,
        driver,
    }
}

async fn compile_external_oracle_engine_failures_warning_and_spawn_fixture(
    fixture: &ExternalOracleEngineFailuresWarningAndSpawnFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
}

fn assert_external_oracle_warning_outcome(outcome: &CompileOutcome, warning_message: &str) {
    assert!(outcome.pdf_path.exists());
    assert_eq!(
        outcome.dep_trace.inputs,
        vec![Utf8PathBuf::from("main.tex")]
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(warning_message))
    );
}

async fn run_external_oracle_warning_and_spawn_spawn_failure_case(
    case: ExternalOracleWarningAndSpawnFailureCase,
) {
    let (main_source, tool_name, script, expect_err) = match case {
        ExternalOracleWarningAndSpawnFailureCase::PdfLatexFallback => (
            "\\documentclass{article}\\begin{document}pdflatex spawn lane\\end{document}",
            "pdflatex",
            "#!/definitely/missing/pdflatex-interpreter\n",
            "pdf_latex build should fail when pdflatex fallback cannot be spawned",
        ),
        ExternalOracleWarningAndSpawnFailureCase::Tectonic => (
            "\\documentclass{article}\\begin{document}tectonic spawn lane\\end{document}",
            "tectonic",
            "#!/definitely/missing/tectonic-interpreter\n",
            "pdf_latex build should fail when tectonic cannot be spawned",
        ),
    };
    let fixture = prepare_external_oracle_engine_failures_warning_and_spawn_fixture(main_source);
    write_executable_script(&fixture.tool_dir.join(tool_name), script);

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&fixture.tool_dir, false);

    let failure = compile_external_oracle_engine_failures_warning_and_spawn_fixture(&fixture)
        .await
        .expect_err(expect_err);

    assert!(failure.message.contains("failed to spawn compiler"));
    assert!(failure.message.contains(tool_name));
    assert_eq!(failure.diagnostics.len(), 1);
    assert!(
        failure.diagnostics[0]
            .message
            .contains("failed to spawn compiler")
    );
}

async fn run_external_oracle_warning_and_spawn_warning_case(
    case: ExternalOracleWarningAndSpawnWarningCase,
) {
    let (main_source, tool_name, script_kind, warning_message, expect_msg) = match case {
        ExternalOracleWarningAndSpawnWarningCase::PdfLatex => (
            "\\documentclass{article}\\begin{document}pdflatex warning lane\\end{document}",
            "pdflatex",
            FakeWarningPdfScript::Latex,
            "pdfLaTeX warning: labels may have changed.",
            "pdflatex build should succeed",
        ),
        ExternalOracleWarningAndSpawnWarningCase::Tectonic => (
            "\\documentclass{article}\\begin{document}tectonic warning lane\\end{document}",
            "tectonic",
            FakeWarningPdfScript::Tectonic,
            "Tectonic warning: package index reused from cache.",
            "tectonic build should succeed",
        ),
    };
    let fixture = prepare_external_oracle_engine_failures_warning_and_spawn_fixture(main_source);
    write_executable_script(
        &fixture.tool_dir.join(tool_name),
        &fake_warning_pdf_script(script_kind, warning_message),
    );

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&fixture.tool_dir, false);

    let outcome = compile_external_oracle_engine_failures_warning_and_spawn_fixture(&fixture)
        .await
        .expect(expect_msg);

    assert_external_oracle_warning_outcome(&outcome, warning_message);
}
