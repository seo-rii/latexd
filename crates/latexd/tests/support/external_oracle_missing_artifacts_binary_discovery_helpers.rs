struct ExternalOracleMissingArtifactsBinaryDiscoveryFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    tool_dir: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

enum ExternalOracleMissingArtifactsBinaryDiscoveryCase {
    XeLatex,
    PdfLatex,
    LatexDvipsPs2Pdf,
}

type BinDiscovery = ExternalOracleMissingArtifactsBinaryDiscoveryCase;

async fn run_bin_discovery(case: BinDiscovery) {
    run_external_oracle_missing_artifacts_binary_discovery_case(case).await;
}

fn prepare_external_oracle_missing_artifacts_binary_discovery_fixture(
    compiler: &str,
) -> ExternalOracleMissingArtifactsBinaryDiscoveryFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        format!(
            r#"
compiler: {compiler}
toplevel:
  - main.tex
"#
        ),
    )
    .expect("write manifest");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}x\\end{document}",
    )
    .expect("write main");
    let tool_dir = root.join("empty-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
    ExternalOracleMissingArtifactsBinaryDiscoveryFixture {
        _tempdir: tempdir,
        root,
        tool_dir,
        world,
        driver,
    }
}

async fn compile_external_oracle_missing_artifacts_binary_discovery_fixture(
    fixture: &ExternalOracleMissingArtifactsBinaryDiscoveryFixture,
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

async fn run_external_oracle_missing_artifacts_binary_discovery_case(
    case: ExternalOracleMissingArtifactsBinaryDiscoveryCase,
) {
    let (compiler, expected_failure, expected_message, diagnostic_part) = match case {
        ExternalOracleMissingArtifactsBinaryDiscoveryCase::XeLatex => (
            "xe_latex",
            "xelatex build should fail without xelatex",
            "xelatex is not installed",
            "xelatex is not installed",
        ),
        ExternalOracleMissingArtifactsBinaryDiscoveryCase::PdfLatex => (
            "pdf_latex",
            "pdf_latex build should fail without tectonic or pdflatex",
            "no TeX compiler found on PATH",
            "no TeX compiler found",
        ),
        ExternalOracleMissingArtifactsBinaryDiscoveryCase::LatexDvipsPs2Pdf => (
            "latex_dvips_ps2_pdf",
            "latex -> dvips -> ps2pdf build should fail without latex",
            "latex is not installed",
            "latex is not installed",
        ),
    };
    let fixture = prepare_external_oracle_missing_artifacts_binary_discovery_fixture(compiler);

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&fixture.tool_dir, false);

    let failure = compile_external_oracle_missing_artifacts_binary_discovery_fixture(&fixture)
        .await
        .expect_err(expected_failure);

    assert_eq!(failure.message, expected_message);
    assert_eq!(failure.diagnostics.len(), 1);
    assert!(failure.diagnostics[0].message.contains(diagnostic_part));
}
