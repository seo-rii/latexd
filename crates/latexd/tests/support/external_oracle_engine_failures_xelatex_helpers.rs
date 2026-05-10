struct ExternalOracleEngineFailuresXelatexFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    tool_dir: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

enum ExternalOracleEngineFailuresXelatexCase {
    SpawnFailure,
    Warning,
}

type XEngine = ExternalOracleEngineFailuresXelatexCase;

async fn run_xengine(case: XEngine) {
    run_external_oracle_engine_failures_xelatex_case(case).await;
}

fn prepare_external_oracle_engine_failures_xelatex_fixture(
    main_source: &str,
) -> ExternalOracleEngineFailuresXelatexFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: xe_latex
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
    ExternalOracleEngineFailuresXelatexFixture {
        _tempdir: tempdir,
        root,
        tool_dir,
        world,
        driver,
    }
}

async fn compile_external_oracle_engine_failures_xelatex_fixture(
    fixture: &ExternalOracleEngineFailuresXelatexFixture,
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

async fn run_external_oracle_engine_failures_xelatex_case(
    case: ExternalOracleEngineFailuresXelatexCase,
) {
    match case {
        ExternalOracleEngineFailuresXelatexCase::SpawnFailure => {
            let fixture = prepare_external_oracle_engine_failures_xelatex_fixture(
                "\\documentclass{article}\\begin{document}xelatex spawn lane\\end{document}",
            );
            write_executable_script(
                &fixture.tool_dir.join("xelatex"),
                "#!/definitely/missing/xelatex-interpreter\n",
            );

            let _path_lock = lock_path_env();
            let _path_guard = set_path(&fixture.tool_dir, false);

            let failure = compile_external_oracle_engine_failures_xelatex_fixture(&fixture)
                .await
                .expect_err("xelatex build should fail when the binary cannot be spawned");

            assert!(failure.message.contains("failed to spawn compiler"));
            assert!(failure.message.contains("xelatex"));
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("failed to spawn compiler")
            );
        }
        ExternalOracleEngineFailuresXelatexCase::Warning => {
            let fixture = prepare_external_oracle_engine_failures_xelatex_fixture(
                "\\documentclass{article}\\begin{document}xelatex warning lane\\end{document}",
            );
            write_executable_script(
                &fixture.tool_dir.join("xelatex"),
                &fake_warning_pdf_script(
                    FakeWarningPdfScript::Latex,
                    "XeLaTeX Warning: font cache refreshed.",
                ),
            );

            let _path_lock = lock_path_env();
            let _path_guard = set_path(&fixture.tool_dir, false);

            let outcome = compile_external_oracle_engine_failures_xelatex_fixture(&fixture)
                .await
                .expect("xelatex build should succeed");

            assert_external_oracle_warning_outcome(&outcome, "XeLaTeX Warning");
        }
    }
}
