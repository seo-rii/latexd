struct ExternalOracleCustomCompilerFailuresFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    tool_dir: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum ExternalOracleCustomCompilerArtifactAndSpawnFailureCase {
    MissingPdf,
    Spawn,
}

type CustomArtifact = ExternalOracleCustomCompilerArtifactAndSpawnFailureCase;

async fn run_custom_artifact(case: CustomArtifact) {
    run_external_oracle_custom_compiler_artifact_and_spawn_failure(case).await;
}

enum ExternalOracleCustomCompilerNonzeroExitCase {
    Stdout,
    Stderr,
    Quiet,
}

type CustomExit = ExternalOracleCustomCompilerNonzeroExitCase;

async fn run_custom_exit(case: CustomExit) {
    run_external_oracle_custom_compiler_nonzero_exit(case).await;
}

fn prepare_external_oracle_custom_compiler_failures_fixture(
    main_source: &str,
) -> ExternalOracleCustomCompilerFailuresFixture {
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
    let build_root = root.join(".latexd/build");
    ExternalOracleCustomCompilerFailuresFixture {
        _tempdir: tempdir,
        root,
        tool_dir,
        build_root,
        world,
    }
}

async fn compile_external_oracle_custom_compiler_failures_fixture(
    fixture: &ExternalOracleCustomCompilerFailuresFixture,
    compiler_bin: String,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(
        Some(compiler_bin),
        vec!["{out_pdf}".to_string(), "{fls}".to_string()],
    );
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

async fn expect_custom_compiler_failure(
    fixture: &ExternalOracleCustomCompilerFailuresFixture,
    compiler_bin: String,
    expect_err: &str,
) -> latexd::compiler::CompileFailure {
    compile_external_oracle_custom_compiler_failures_fixture(fixture, compiler_bin)
        .await
        .expect_err(expect_err)
}

fn assert_custom_compiler_failure_diagnostic<'a>(
    failure: &'a latexd::compiler::CompileFailure,
    expected: &str,
) -> &'a str {
    assert_eq!(failure.diagnostics.len(), 1);
    let diagnostic = &failure.diagnostics[0].message;
    assert!(diagnostic.contains(expected));
    diagnostic
}

async fn assert_external_oracle_custom_compiler_nonzero_exit(
    main_source: &str,
    compiler_script: &str,
    expect_err: &str,
    expected_detail: Option<&str>,
) {
    let fixture = prepare_external_oracle_custom_compiler_failures_fixture(main_source);
    let compiler_path = fixture.tool_dir.join("oracle-compiler");
    write_executable_script(&compiler_path, compiler_script);
    let failure =
        expect_custom_compiler_failure(&fixture, compiler_path.to_string(), expect_err).await;

    assert!(failure.message.contains("oracle-compiler"));
    assert!(failure.message.contains("status"));
    let diagnostic = assert_custom_compiler_failure_diagnostic(
        &failure,
        expected_detail.unwrap_or("oracle-compiler"),
    );
    if expected_detail.is_none() {
        assert!(diagnostic.contains("status"));
    }
}

async fn run_external_oracle_custom_compiler_nonzero_exit(
    case: ExternalOracleCustomCompilerNonzeroExitCase,
) {
    let (main_source, compiler_script, expect_err, expected_detail) = match case {
        ExternalOracleCustomCompilerNonzeroExitCase::Stdout => (
            "\\documentclass{article}\\begin{document}custom compiler stdout lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "custom oracle printed only stdout detail"
exit 17
"#,
            "configured external oracle should fail on non-zero exit",
            Some("custom oracle printed only stdout detail"),
        ),
        ExternalOracleCustomCompilerNonzeroExitCase::Stderr => (
            "\\documentclass{article}\\begin{document}custom compiler stderr lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
echo "custom oracle backend rejected flags" >&2
exit 31
"#,
            "configured external oracle should fail on non-zero exit",
            Some("custom oracle backend rejected flags"),
        ),
        ExternalOracleCustomCompilerNonzeroExitCase::Quiet => (
            "\\documentclass{article}\\begin{document}custom compiler empty output lane\\end{document}",
            r#"#!/bin/bash
set -euo pipefail
exit 19
"#,
            "configured external oracle should fail on quiet non-zero exit",
            None,
        ),
    };

    assert_external_oracle_custom_compiler_nonzero_exit(
        main_source,
        compiler_script,
        expect_err,
        expected_detail,
    )
    .await;
}

async fn run_external_oracle_custom_compiler_artifact_and_spawn_failure(
    case: ExternalOracleCustomCompilerArtifactAndSpawnFailureCase,
) {
    match case {
        ExternalOracleCustomCompilerArtifactAndSpawnFailureCase::MissingPdf => {
            let fixture = prepare_external_oracle_custom_compiler_failures_fixture(
                "\\documentclass{article}\\begin{document}custom compiler missing pdf lane\\end{document}",
            );
            let compiler_script = fixture.tool_dir.join("oracle-compiler");
            write_executable_script(
                &compiler_script,
                r#"#!/bin/bash
set -euo pipefail
fls="$2"
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
exit 0
"#,
            );

            let expected_pdf = fixture.build_root.join("rev-1/main.pdf");
            let failure = expect_custom_compiler_failure(
                &fixture,
                compiler_script.to_string(),
                "compiler without PDF should fail",
            )
            .await;

            assert_eq!(
                failure.message,
                format!("expected PDF {} was not created", expected_pdf)
            );
            assert_custom_compiler_failure_diagnostic(&failure, "did not produce expected PDF");
        }
        ExternalOracleCustomCompilerArtifactAndSpawnFailureCase::Spawn => {
            let fixture = prepare_external_oracle_custom_compiler_failures_fixture(
                "\\documentclass{article}\\begin{document}custom compiler spawn lane\\end{document}",
            );
            let missing_compiler = fixture.root.join("missing-compiler");
            let failure = expect_custom_compiler_failure(
                &fixture,
                missing_compiler.to_string(),
                "missing compiler should fail",
            )
            .await;

            assert!(failure.message.contains("failed to spawn compiler"));
            assert!(failure.message.contains("missing-compiler"));
            assert_custom_compiler_failure_diagnostic(&failure, "failed to spawn compiler");
        }
    }
}
