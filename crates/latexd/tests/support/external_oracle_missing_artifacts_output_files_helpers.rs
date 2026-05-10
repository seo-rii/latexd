struct ExternalOracleMissingArtifactsOutputFilesFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    tool_dir: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum ExternalOracleMissingArtifactsOutputFileCase {
    MissingDvipsBinary,
    MissingDviArtifact,
    MissingPs2pdfBinary,
    MissingPostscriptArtifact,
    MissingPdfFromPs2pdf,
    MissingPdfFromXelatex,
}

type MissingOutput = ExternalOracleMissingArtifactsOutputFileCase;

async fn run_missing_output(case: MissingOutput) {
    run_external_oracle_missing_artifacts_output_file_case(case).await;
}

enum ExternalOracleMissingArtifactsPdfLatexToolchainCase {
    PrefersTectonic,
    FallbackPdflatex,
    MissingToolchain,
    MissingPdfFromTectonic,
    MissingPdfFromPdflatex,
}

type PdfToolchain = ExternalOracleMissingArtifactsPdfLatexToolchainCase;

async fn run_pdf_toolchain(case: PdfToolchain) {
    run_external_oracle_missing_artifacts_pdf_latex_toolchain_case(case).await;
}

fn prepare_external_oracle_missing_artifacts_output_files_fixture(
    compiler: &str,
) -> ExternalOracleMissingArtifactsOutputFilesFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        format!("\ncompiler: {compiler}\ntoplevel:\n  - main.tex\n"),
    )
    .expect("write manifest");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}x\\end{document}",
    )
    .expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    ExternalOracleMissingArtifactsOutputFilesFixture {
        _tempdir: tempdir,
        root,
        tool_dir,
        build_root,
        world,
    }
}

async fn compile_external_oracle_missing_artifacts_output_files_fixture(
    fixture: &ExternalOracleMissingArtifactsOutputFilesFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let _path_lock = lock_path_env();
    let _path_guard = set_path(&fixture.tool_dir, false);
    let driver = CompilerDriver::new(None, Vec::new());
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

fn assert_external_oracle_missing_main_pdf_failure(
    fixture: &ExternalOracleMissingArtifactsOutputFilesFixture,
    failure: &latexd::compiler::CompileFailure,
) {
    let expected_pdf = fixture.build_root.join("rev-1/main.pdf");
    assert_eq!(
        failure.message,
        format!("expected PDF {expected_pdf} was not created")
    );
    assert_eq!(failure.diagnostics.len(), 1);
    assert!(
        failure.diagnostics[0]
            .message
            .contains("did not produce expected PDF")
    );
}

async fn run_external_oracle_missing_artifacts_output_file_case(
    case: ExternalOracleMissingArtifactsOutputFileCase,
) {
    match case {
        ExternalOracleMissingArtifactsOutputFileCase::MissingDvipsBinary => {
            let fixture = prepare_external_oracle_missing_artifacts_output_files_fixture(
                "latex_dvips_ps2_pdf",
            );
            write_executable_script(&fixture.tool_dir.join("latex"), fake_latex_dvi_script());

            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("dvips build should fail without dvips");

            assert_eq!(failure.message, "dvips is not installed");
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("dvips is not installed")
            );
        }
        ExternalOracleMissingArtifactsOutputFileCase::MissingDviArtifact => {
            let fixture = prepare_external_oracle_missing_artifacts_output_files_fixture(
                "latex_dvips_ps2_pdf",
            );
            write_executable_script(
                &fixture.tool_dir.join("latex"),
                fake_success_without_output_script(),
            );
            write_executable_script(&fixture.tool_dir.join("dvips"), fake_dvips_script());
            write_executable_script(&fixture.tool_dir.join("ps2pdf"), fake_ps2pdf_script());

            let expected_dvi = fixture.build_root.join("rev-1/main.dvi");
            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("latex build should fail when it does not emit a DVI");

            assert_eq!(
                failure.message,
                format!("expected DVI {expected_dvi} was not created")
            );
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("did not produce expected DVI")
            );
        }
        ExternalOracleMissingArtifactsOutputFileCase::MissingPs2pdfBinary => {
            let fixture = prepare_external_oracle_missing_artifacts_output_files_fixture(
                "latex_dvips_ps2_pdf",
            );
            write_executable_script(&fixture.tool_dir.join("latex"), fake_latex_dvi_script());
            write_executable_script(&fixture.tool_dir.join("dvips"), fake_dvips_script());

            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("dvips build should fail without ps2pdf");

            assert_eq!(failure.message, "ps2pdf is not installed");
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("ps2pdf is not installed")
            );
        }
        ExternalOracleMissingArtifactsOutputFileCase::MissingPostscriptArtifact => {
            let fixture = prepare_external_oracle_missing_artifacts_output_files_fixture(
                "latex_dvips_ps2_pdf",
            );
            write_executable_script(&fixture.tool_dir.join("latex"), fake_latex_dvi_script());
            write_executable_script(
                &fixture.tool_dir.join("dvips"),
                fake_success_without_output_script(),
            );
            write_executable_script(&fixture.tool_dir.join("ps2pdf"), fake_ps2pdf_script());

            let expected_ps = fixture.build_root.join("rev-1/main.ps");
            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("dvips build should fail when it does not emit PostScript");

            assert_eq!(
                failure.message,
                format!("expected PostScript {expected_ps} was not created")
            );
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("did not produce expected PostScript")
            );
        }
        ExternalOracleMissingArtifactsOutputFileCase::MissingPdfFromPs2pdf => {
            let fixture = prepare_external_oracle_missing_artifacts_output_files_fixture(
                "latex_dvips_ps2_pdf",
            );
            write_executable_script(&fixture.tool_dir.join("latex"), fake_latex_dvi_script());
            write_executable_script(&fixture.tool_dir.join("dvips"), fake_dvips_script());
            write_executable_script(
                &fixture.tool_dir.join("ps2pdf"),
                fake_success_without_output_script(),
            );

            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("ps2pdf build should fail when it does not emit a PDF");
            assert_external_oracle_missing_main_pdf_failure(&fixture, &failure);
        }
        ExternalOracleMissingArtifactsOutputFileCase::MissingPdfFromXelatex => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("xe_latex");
            write_executable_script(
                &fixture.tool_dir.join("xelatex"),
                fake_success_without_output_script(),
            );

            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("xelatex build should fail when it does not emit a PDF");
            assert_external_oracle_missing_main_pdf_failure(&fixture, &failure);
        }
    }
}

async fn run_external_oracle_missing_artifacts_pdf_latex_toolchain_case(
    case: ExternalOracleMissingArtifactsPdfLatexToolchainCase,
) {
    match case {
        ExternalOracleMissingArtifactsPdfLatexToolchainCase::PrefersTectonic => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("pdf_latex");
            write_executable_script(
                &fixture.tool_dir.join("tectonic"),
                fake_tectonic_pdf_script(),
            );
            write_executable_script(
                &fixture.tool_dir.join("pdflatex"),
                "#!/bin/bash\nset -euo pipefail\nexit 99\n",
            );
            let outcome = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect("pdf_latex should prefer tectonic when both compilers exist");

            assert!(outcome.pdf_path.exists());
            assert_eq!(outcome.pdf_path, fixture.build_root.join("rev-1/main.pdf"));
        }
        ExternalOracleMissingArtifactsPdfLatexToolchainCase::FallbackPdflatex => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("pdf_latex");
            write_executable_script(
                &fixture.tool_dir.join("pdflatex"),
                fake_pdflatex_pdf_script(),
            );
            let outcome = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect("pdf_latex should fall back to pdflatex when tectonic is absent");

            assert!(outcome.pdf_path.exists());
            assert_eq!(outcome.pdf_path, fixture.build_root.join("rev-1/main.pdf"));
        }
        ExternalOracleMissingArtifactsPdfLatexToolchainCase::MissingToolchain => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("pdf_latex");
            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("pdf_latex should fail when no TeX compiler is present");

            assert_eq!(failure.message, "no TeX compiler found on PATH");
            assert_eq!(failure.diagnostics.len(), 1);
            assert!(
                failure.diagnostics[0]
                    .message
                    .contains("no TeX compiler found")
            );
        }
        ExternalOracleMissingArtifactsPdfLatexToolchainCase::MissingPdfFromTectonic => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("pdf_latex");
            write_executable_script(
                &fixture.tool_dir.join("tectonic"),
                fake_success_without_output_script(),
            );
            write_executable_script(
                &fixture.tool_dir.join("pdflatex"),
                fake_pdflatex_pdf_script(),
            );
            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("tectonic path should fail when it does not emit a PDF");

            assert_external_oracle_missing_main_pdf_failure(&fixture, &failure);
        }
        ExternalOracleMissingArtifactsPdfLatexToolchainCase::MissingPdfFromPdflatex => {
            let fixture =
                prepare_external_oracle_missing_artifacts_output_files_fixture("pdf_latex");
            write_executable_script(
                &fixture.tool_dir.join("pdflatex"),
                fake_success_without_output_script(),
            );
            let failure = compile_external_oracle_missing_artifacts_output_files_fixture(&fixture)
                .await
                .expect_err("pdflatex path should fail when it does not emit a PDF");

            assert_external_oracle_missing_main_pdf_failure(&fixture, &failure);
        }
    }
}
