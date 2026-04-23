#[derive(Clone, Copy)]
enum ExternalOracleCustomCompilerWarningStream {
    Stdout,
    Stderr,
}

async fn run_external_oracle_custom_compiler_warning_stream(
    warning_stream: ExternalOracleCustomCompilerWarningStream,
) {
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
    let body = match warning_stream {
        ExternalOracleCustomCompilerWarningStream::Stdout => "warning lane",
        ExternalOracleCustomCompilerWarningStream::Stderr => "stderr warning lane",
    };
    fs::write(
        root.join("main.tex"),
        format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}"),
    )
    .expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    let compiler_script = tool_dir.join("oracle-compiler");
    let warning_command = match warning_stream {
        ExternalOracleCustomCompilerWarningStream::Stdout => {
            r#"echo "LaTeX Warning: rerun to get cross-references right.""#
        }
        ExternalOracleCustomCompilerWarningStream::Stderr => {
            r#"echo "LaTeX Warning: label(s) may have changed." >&2"#
        }
    };
    write_executable_script(
        &compiler_script,
        &format!(
            r#"#!/bin/bash
set -euo pipefail
output="$1"
fls="$2"
{warning_command}
cat > "$output" <<'EOF'
%PDF-1.4
1 0 obj
<<>>
endobj
trailer
<<>>
%%EOF
EOF
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
"#
        ),
    );

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(
        Some(compiler_script.to_string()),
        vec!["{out_pdf}".to_string(), "{fls}".to_string()],
    );
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("external oracle build should succeed");

    assert!(outcome.pdf_path.exists());
    assert_eq!(
        outcome.dep_trace.inputs,
        vec![Utf8PathBuf::from("main.tex")]
    );
    let expected_warning = match warning_stream {
        ExternalOracleCustomCompilerWarningStream::Stdout => "LaTeX Warning",
        ExternalOracleCustomCompilerWarningStream::Stderr => "label(s) may have changed",
    };
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(expected_warning)),
        "expected {expected_warning} in diagnostics, saw {:?}",
        outcome.diagnostics
    );
}
