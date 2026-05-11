#[derive(Clone, Copy)]
enum ExternalOracleCustomCompilerWarningStream {
    Stdout,
    Stderr,
}

type WarnStream = ExternalOracleCustomCompilerWarningStream;

async fn run_warn_stream(case: WarnStream) {
    run_external_oracle_custom_compiler_warning_stream(case).await;
}

enum ExternalOracleCustomCompilerPreviewAndPlaceholderCase {
    PreviewRetainsLastGood,
    PlaceholderExpansion,
}

type PreviewCase = ExternalOracleCustomCompilerPreviewAndPlaceholderCase;

async fn run_preview_case(case: PreviewCase) {
    run_external_oracle_custom_compiler_preview_and_placeholder(case).await;
}

struct ExternalOracleCustomCompilerFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
}

fn prepare_external_oracle_custom_compiler_fixture(
    main_source: impl AsRef<str>,
) -> ExternalOracleCustomCompilerFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    write_external_oracle_custom_compiler_project(&root, main_source);
    let build_root = root.join(".latexd/build");
    ExternalOracleCustomCompilerFixture {
        _tempdir: tempdir,
        root,
        build_root,
    }
}

fn write_external_oracle_custom_compiler_project(root: &Utf8Path, main_source: impl AsRef<str>) {
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("main.tex"), main_source.as_ref()).expect("write main");
}

async fn compile_external_oracle_custom_compiler_main(
    fixture: &ExternalOracleCustomCompilerFixture,
    driver: &CompilerDriver,
    rev: u64,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let world = ProjectWorld::load(fixture.root.clone()).expect("world");
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
}

fn assert_external_oracle_dep_trace(outcome: &CompileOutcome, expected_inputs: &[&str]) {
    assert_eq!(
        outcome.dep_trace.inputs,
        expected_inputs
            .iter()
            .map(|input| Utf8PathBuf::from(*input))
            .collect::<Vec<_>>()
    );
}

async fn run_external_oracle_custom_compiler_preview_and_placeholder(
    case: ExternalOracleCustomCompilerPreviewAndPlaceholderCase,
) {
    match case {
        ExternalOracleCustomCompilerPreviewAndPlaceholderCase::PreviewRetainsLastGood => {
            let fixture = prepare_external_oracle_custom_compiler_fixture(
                "\\documentclass{article}\n\\begin{document}\n\\input{sections/intro}\nHello latexd\n\\end{document}\n",
            );
            fs::create_dir_all(fixture.root.join("sections")).expect("sections dir");
            fs::write(fixture.root.join("sections/intro.tex"), "Intro section")
                .expect("write intro tex");

            let driver = CompilerDriver::new(
                Some(env!("CARGO_BIN_EXE_latexd").to_string()),
                "mock-compiler --input {main} --output {out_pdf} --depfile {depfile} --fail-if-contains \\broken"
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect(),
            );

            let first = compile_external_oracle_custom_compiler_main(&fixture, &driver, 1)
                .await
                .expect("first build should succeed");

            assert!(first.pdf_path.exists());
            assert_external_oracle_dep_trace(&first, &["main.tex", "sections/intro.tex"]);

            let mut snapshot = PreviewSnapshot::default();
            snapshot.apply_started(1, vec!["main.tex".to_string()]);
            snapshot.apply_success(
                1,
                first.diagnostics.clone(),
                "/artifacts/rev/1/main.pdf".to_string(),
                first.page_metadata.len(),
                first
                    .page_metadata
                    .iter()
                    .map(|page| page.page_id.clone())
                    .collect(),
                first.page_artifacts.clone(),
            );
            let last_good = snapshot.pdf_url.clone();

            fs::write(fixture.root.join("main.tex"), "\\broken").expect("write broken source");
            let failure = compile_external_oracle_custom_compiler_main(&fixture, &driver, 2)
                .await
                .expect_err("second build should fail");

            snapshot.apply_started(2, vec!["main.tex".to_string()]);
            snapshot.apply_failure(2, failure.diagnostics.clone());

            assert_eq!(snapshot.pdf_url, last_good);
            assert_eq!(snapshot.last_build_succeeded, Some(false));
            assert!(!failure.diagnostics.is_empty());
        }
        ExternalOracleCustomCompilerPreviewAndPlaceholderCase::PlaceholderExpansion => {
            let fixture = prepare_external_oracle_custom_compiler_fixture(
                "\\documentclass{article}\\begin{document}placeholder lane\\end{document}",
            );

            let args_log = fixture.root.join("compiler-args.txt");
            let compiler_script = write_external_oracle_custom_compiler_script(
                &fixture.root,
                r#"#!/bin/bash
set -euo pipefail
root="$1"
main="$2"
out_dir="$3"
out_pdf="$4"
fls="$5"
depfile="$6"
rev="$7"
args_log="$8"
printf 'root=%s\nmain=%s\nout_dir=%s\nout_pdf=%s\nfls=%s\ndepfile=%s\nrev=%s\n' \
  "$root" "$main" "$out_dir" "$out_pdf" "$fls" "$depfile" "$rev" > "$args_log"
/bin/mkdir -p "$out_dir"
: > "$out_pdf"
printf 'INPUT %s\n' "$root/$main" > "$fls"
printf '%s: %s\n' "$out_pdf" "$main" > "$depfile"
"#,
            );

            let mut driver_args = "{root} {main} {out_dir} {out_pdf} {fls} {depfile} {rev}"
                .split_whitespace()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            driver_args.push(args_log.to_string());
            let driver = CompilerDriver::new(Some(compiler_script.to_string()), driver_args);
            let outcome = compile_external_oracle_custom_compiler_main(&fixture, &driver, 7)
                .await
                .expect("configured external oracle should succeed");

            assert!(outcome.pdf_path.exists());
            assert_external_oracle_main_only_dep_trace(&outcome);
            let args = fs::read_to_string(args_log.as_std_path()).expect("read compiler args");
            let rev_root = fixture.build_root.join("rev-7");
            assert!(args.contains(&format!("root={}", fixture.root)));
            assert!(args.contains("main=main.tex"));
            assert!(args.contains(&format!("out_dir={rev_root}")));
            assert!(args.contains(&format!("out_pdf={}", rev_root.join("main.pdf"))));
            assert!(args.contains(&format!("fls={}", rev_root.join("main.fls"))));
            assert!(args.contains(&format!("depfile={}", rev_root.join("deps.mk"))));
            assert!(args.contains("rev=7"));
        }
    }
}

fn assert_external_oracle_main_only_dep_trace(outcome: &CompileOutcome) {
    assert_external_oracle_dep_trace(outcome, &["main.tex"]);
}

fn write_external_oracle_custom_compiler_script(root: &Utf8Path, body: &str) -> Utf8PathBuf {
    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    let compiler_script = tool_dir.join("oracle-compiler");
    write_executable_script(&compiler_script, body);
    compiler_script
}

async fn run_external_oracle_custom_compiler_warning_stream(
    warning_stream: ExternalOracleCustomCompilerWarningStream,
) {
    let body = match warning_stream {
        ExternalOracleCustomCompilerWarningStream::Stdout => "warning lane",
        ExternalOracleCustomCompilerWarningStream::Stderr => "stderr warning lane",
    };
    let fixture = prepare_external_oracle_custom_compiler_fixture(format!(
        "\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}"
    ));

    let warning_command = match warning_stream {
        ExternalOracleCustomCompilerWarningStream::Stdout => {
            r#"echo "LaTeX Warning: rerun to get cross-references right.""#
        }
        ExternalOracleCustomCompilerWarningStream::Stderr => {
            r#"echo "LaTeX Warning: label(s) may have changed." >&2"#
        }
    };
    let compiler_script = write_external_oracle_custom_compiler_script(
        &fixture.root,
        &format!(
            r#"#!/bin/bash
set -euo pipefail
output="$1"
fls="$2"
{warning_command}
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
"#
        ),
    );

    let driver = CompilerDriver::new(
        Some(compiler_script.to_string()),
        vec!["{out_pdf}".to_string(), "{fls}".to_string()],
    );
    let outcome = compile_external_oracle_custom_compiler_main(&fixture, &driver, 1)
        .await
        .expect("external oracle build should succeed");

    assert!(outcome.pdf_path.exists());
    assert_external_oracle_main_only_dep_trace(&outcome);
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
