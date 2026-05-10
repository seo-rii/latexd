enum ExternalOracleMissingArtifactsPs2pdfStageCase {
    Quiet,
    Stdout,
    Stderr,
    Spawn,
}

type Ps2pdfStage = ExternalOracleMissingArtifactsPs2pdfStageCase;

async fn run_ps2pdf_stage(case: Ps2pdfStage) {
    run_external_oracle_missing_artifacts_ps2pdf_stage_case(case).await;
}

async fn run_external_oracle_missing_artifacts_ps2pdf_stage_failure(
    main_source: &str,
    ps2pdf_script: &str,
    expected_failure: &str,
) -> latexd::compiler::CompileFailure {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: latex_dvips_ps2_pdf
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("main.tex"), main_source).expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    write_executable_script(&tool_dir.join("latex"), fake_latex_dvi_script());
    write_executable_script(&tool_dir.join("dvips"), fake_dvips_script());
    write_executable_script(&tool_dir.join("ps2pdf"), ps2pdf_script);

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&tool_dir, false);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect_err(expected_failure)
}

fn assert_external_oracle_missing_artifacts_ps2pdf_failure(
    failure: &latexd::compiler::CompileFailure,
    message_parts: &[&str],
    diagnostic_part: &str,
) {
    for message_part in message_parts {
        assert!(failure.message.contains(message_part));
    }
    assert_eq!(failure.diagnostics.len(), 1);
    assert!(failure.diagnostics[0].message.contains(diagnostic_part));
}

async fn run_external_oracle_missing_artifacts_ps2pdf_stage_case(
    case: ExternalOracleMissingArtifactsPs2pdfStageCase,
) {
    let (main_source, ps2pdf_script, expected_failure, message_parts, diagnostic_part) = match case
    {
        ExternalOracleMissingArtifactsPs2pdfStageCase::Quiet => (
            "\\documentclass{article}\\begin{document}ps2pdf quiet lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
exit 11
"#,
            "ps2pdf build should fail on quiet non-zero exit",
            vec!["ps2pdf", "status"],
            "exited with status",
        ),
        ExternalOracleMissingArtifactsPs2pdfStageCase::Stdout => (
            "\\documentclass{article}\\begin{document}ps2pdf stdout lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "ps2pdf reported only stdout detail"
exit 11
"#,
            "ps2pdf build should fail on stdout-only non-zero exit",
            vec!["ps2pdf", "status"],
            "ps2pdf reported only stdout detail",
        ),
        ExternalOracleMissingArtifactsPs2pdfStageCase::Stderr => (
            "\\documentclass{article}\\begin{document}ps2pdf exit lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "ps2pdf failed to initialize ghostscript" >&2
exit 11
"#,
            "ps2pdf build should fail on non-zero exit",
            vec!["ps2pdf", "status"],
            "ps2pdf failed to initialize ghostscript",
        ),
        ExternalOracleMissingArtifactsPs2pdfStageCase::Spawn => (
            "\\documentclass{article}\\begin{document}ps2pdf spawn lane\\end{document}",
            "#!/definitely/missing/ps2pdf-interpreter\n",
            "ps2pdf stage should fail when the binary cannot be spawned",
            vec!["failed to spawn compiler", "ps2pdf"],
            "failed to spawn compiler",
        ),
    };

    let failure = run_external_oracle_missing_artifacts_ps2pdf_stage_failure(
        main_source,
        ps2pdf_script,
        expected_failure,
    )
    .await;
    assert_external_oracle_missing_artifacts_ps2pdf_failure(
        &failure,
        &message_parts,
        diagnostic_part,
    );
}
