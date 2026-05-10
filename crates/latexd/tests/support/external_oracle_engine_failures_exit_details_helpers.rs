async fn assert_external_oracle_engine_nonzero_exit(
    engine_bin: &str,
    body_label: &str,
    script: &str,
    expected_detail: &str,
) {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let manifest_compiler = match engine_bin {
        "xelatex" => "xe_latex",
        _ => "pdf_latex",
    };
    fs::write(
        root.join("00README.yaml"),
        format!(
            r#"
compiler: {manifest_compiler}
toplevel:
  - main.tex
"#
        ),
    )
    .expect("write manifest");
    fs::write(
        root.join("main.tex"),
        format!("\\documentclass{{article}}\\begin{{document}}{body_label}\\end{{document}}"),
    )
    .expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    write_executable_script(&tool_dir.join(engine_bin), script);
    if engine_bin == "tectonic" {
        write_executable_script(&tool_dir.join("pdflatex"), fake_pdflatex_pdf_script());
    }

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&tool_dir, false);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
    let failure = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .unwrap_err();

    assert!(failure.message.contains(engine_bin));
    assert!(failure.message.contains("status"));
    assert_eq!(failure.diagnostics.len(), 1);
    assert!(failure.diagnostics[0].message.contains(expected_detail));
}

enum ExternalOracleEngineNonzeroExitCase {
    PdfLatexStdout,
    PdfLatexStderr,
    PdfLatexQuiet,
    TectonicStdout,
    TectonicStderr,
    TectonicQuiet,
    XeLatexStdout,
    XeLatexStderr,
    XeLatexQuiet,
}

type EngineExit = ExternalOracleEngineNonzeroExitCase;

async fn run_engine_exit(case: EngineExit) {
    run_external_oracle_engine_nonzero_exit(case).await;
}

async fn run_external_oracle_engine_nonzero_exit(case: ExternalOracleEngineNonzeroExitCase) {
    let (engine_bin, body_label, script, expected_detail) = match case {
        ExternalOracleEngineNonzeroExitCase::PdfLatexStdout => (
            "pdflatex",
            "pdflatex stdout lane",
            r#"#!/bin/bash
set -euo pipefail
echo "pdflatex reported only stdout detail"
exit 23
"#,
            "pdflatex reported only stdout detail",
        ),
        ExternalOracleEngineNonzeroExitCase::PdfLatexStderr => (
            "pdflatex",
            "pdflatex exit lane",
            r#"#!/bin/bash
set -euo pipefail
echo "pdflatex emergency stop" >&2
exit 23
"#,
            "pdflatex emergency stop",
        ),
        ExternalOracleEngineNonzeroExitCase::PdfLatexQuiet => (
            "pdflatex",
            "pdflatex quiet lane",
            r#"#!/bin/bash
set -euo pipefail
exit 23
"#,
            "exited with status",
        ),
        ExternalOracleEngineNonzeroExitCase::TectonicStdout => (
            "tectonic",
            "tectonic stdout lane",
            r#"#!/bin/bash
set -euo pipefail
echo "tectonic reported only stdout detail"
exit 29
"#,
            "tectonic reported only stdout detail",
        ),
        ExternalOracleEngineNonzeroExitCase::TectonicStderr => (
            "tectonic",
            "tectonic exit lane",
            r#"#!/bin/bash
set -euo pipefail
echo "tectonic could not parse engine configuration" >&2
exit 19
"#,
            "tectonic could not parse engine configuration",
        ),
        ExternalOracleEngineNonzeroExitCase::TectonicQuiet => (
            "tectonic",
            "tectonic quiet lane",
            r#"#!/bin/bash
set -euo pipefail
exit 29
"#,
            "exited with status",
        ),
        ExternalOracleEngineNonzeroExitCase::XeLatexStdout => (
            "xelatex",
            "xelatex stdout lane",
            r#"#!/bin/bash
set -euo pipefail
echo "xelatex reported only stdout detail"
exit 42
"#,
            "xelatex reported only stdout detail",
        ),
        ExternalOracleEngineNonzeroExitCase::XeLatexStderr => (
            "xelatex",
            "xelatex exit lane",
            r#"#!/bin/bash
set -euo pipefail
echo "fatal xelatex configuration error" >&2
exit 42
"#,
            "fatal xelatex configuration error",
        ),
        ExternalOracleEngineNonzeroExitCase::XeLatexQuiet => (
            "xelatex",
            "xelatex quiet lane",
            r#"#!/bin/bash
set -euo pipefail
exit 42
"#,
            "exited with status",
        ),
    };

    assert_external_oracle_engine_nonzero_exit(engine_bin, body_label, script, expected_detail)
        .await;
}
