enum ExternalOracleMissingArtifactsDvipsStageCase {
    Quiet,
    Stdout,
    Stderr,
    Spawn,
}

type DvipsStage = ExternalOracleMissingArtifactsDvipsStageCase;

async fn run_dvips_stage(case: DvipsStage) {
    run_external_oracle_missing_artifacts_dvips_stage_case(case).await;
}

async fn run_external_oracle_missing_artifacts_dvips_stage_failure(
    main_source: &str,
    dvips_script: &str,
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
    write_executable_script(&tool_dir.join("dvips"), dvips_script);
    write_executable_script(&tool_dir.join("ps2pdf"), fake_ps2pdf_script());

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

fn assert_external_oracle_missing_artifacts_dvips_failure(
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

async fn run_external_oracle_missing_artifacts_dvips_stage_case(
    case: ExternalOracleMissingArtifactsDvipsStageCase,
) {
    let (main_source, dvips_script, expected_failure, message_parts, diagnostic_part) = match case {
        ExternalOracleMissingArtifactsDvipsStageCase::Quiet => (
            "\\documentclass{article}\\begin{document}dvips exit lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
exit 7
"#,
            "dvips build should fail on non-zero exit",
            vec!["dvips", "status"],
            "exited with status",
        ),
        ExternalOracleMissingArtifactsDvipsStageCase::Stdout => (
            "\\documentclass{article}\\begin{document}dvips stdout lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "dvips reported only stdout detail"
exit 7
"#,
            "dvips build should fail on stdout-only non-zero exit",
            vec!["dvips", "status"],
            "dvips reported only stdout detail",
        ),
        ExternalOracleMissingArtifactsDvipsStageCase::Stderr => (
            "\\documentclass{article}\\begin{document}dvips stderr lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "dvips could not open font map" >&2
exit 7
"#,
            "dvips build should fail on non-zero exit",
            vec!["dvips", "status"],
            "dvips could not open font map",
        ),
        ExternalOracleMissingArtifactsDvipsStageCase::Spawn => (
            "\\documentclass{article}\\begin{document}dvips spawn lane\\end{document}",
            "#!/definitely/missing/dvips-interpreter\n",
            "dvips stage should fail when the binary cannot be spawned",
            vec!["failed to spawn compiler", "dvips"],
            "failed to spawn compiler",
        ),
    };

    let failure = run_external_oracle_missing_artifacts_dvips_stage_failure(
        main_source,
        dvips_script,
        expected_failure,
    )
    .await;
    assert_external_oracle_missing_artifacts_dvips_failure(
        &failure,
        &message_parts,
        diagnostic_part,
    );
}
