#[derive(Clone, Copy)]
enum ExternalOracleCustomCompilerDependencyTrackingCase {
    FallbackWithoutFls,
    DepfilePreferred,
    FlsInputs,
    UnreadableDepfile,
}

type CustomDep = ExternalOracleCustomCompilerDependencyTrackingCase;

async fn run_custom_dep(case: CustomDep) {
    run_external_oracle_custom_compiler_dependency_tracking(case).await;
}

async fn run_external_oracle_custom_compiler_dependency_tracking(
    case: ExternalOracleCustomCompilerDependencyTrackingCase,
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
    if matches!(
        case,
        ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred
            | ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs
    ) {
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\begin{document}\\input{sections/intro}\\end{document}",
        )
        .expect("write main");
        fs::write(root.join("sections/intro.tex"), "Intro").expect("write intro");
    } else {
        let body = match case {
            ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls => {
                "custom compiler dep trace lane"
            }
            ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile => {
                "depfile read error"
            }
            ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred
            | ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs => unreachable!(),
        };
        fs::write(
            root.join("main.tex"),
            format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}"),
        )
        .expect("write main");
    }

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    let compiler_script = tool_dir.join("oracle-compiler");
    let compiler_body = match case {
        ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls => {
            r#"#!/bin/bash
set -euo pipefail
output="$1"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
"#
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred => {
            r#"#!/bin/bash
set -euo pipefail
output="$1"
fls="$2"
depfile="${3:-}"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
printf '%s: %s %s\n' "$output" "main.tex" "sections/intro.tex" > "$depfile"
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
"#
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs => {
            r#"#!/bin/bash
set -euo pipefail
output="$1"
fls="$2"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" >> "$fls"
"#
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile => {
            r#"#!/bin/bash
set -euo pipefail
output="$1"
depfile="$2"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
/bin/mkdir -p "$depfile"
"#
        }
    };
    write_executable_script(&compiler_script, compiler_body);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver_args = match case {
        ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls
        | ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs => {
            vec!["{out_pdf}".to_string(), "{fls}".to_string()]
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred => vec![
            "{out_pdf}".to_string(),
            "{fls}".to_string(),
            "{depfile}".to_string(),
        ],
        ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile => {
            vec!["{out_pdf}".to_string(), "{depfile}".to_string()]
        }
    };
    let driver = CompilerDriver::new(Some(compiler_script.to_string()), driver_args);
    let build_root = root.join(".latexd/build");
    let request = CompileRequest {
        root: root.clone(),
        manifest: world.manifest.clone(),
        toplevel: Utf8PathBuf::from("main.tex"),
        rev: 1,
        build_root: build_root.clone(),
        changed_files: vec![Utf8PathBuf::from("main.tex")],
    };

    if matches!(
        case,
        ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile
    ) {
        let failure = driver
            .compile(request)
            .await
            .expect_err("configured external oracle should fail when depfile is unreadable");

        let depfile_path = build_root.join("rev-1/deps.mk");
        assert!(failure.message.contains("failed to read depfile"));
        assert!(failure.message.contains(depfile_path.as_str()));
        assert!(failure.message.contains("Is a directory"));
        assert_eq!(failure.diagnostics.len(), 1);
        assert!(
            failure.diagnostics[0]
                .message
                .contains("failed to read depfile")
        );
        assert!(
            failure.diagnostics[0]
                .message
                .contains(depfile_path.as_str())
        );
        assert!(failure.diagnostics[0].message.contains("Is a directory"));
        assert!(
            !build_root.join("rev-1/build-meta.json").exists(),
            "failed depfile reads should not emit build metadata"
        );
        return;
    }

    let outcome = driver.compile(request).await.expect(match case {
        ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls => {
            "configured external oracle should succeed without recorder output"
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred
        | ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs => {
            "configured external oracle should succeed"
        }
        ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile => unreachable!(),
    });

    assert!(outcome.pdf_path.exists());
    assert_eq!(
        outcome.dep_trace.inputs,
        match case {
            ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls => {
                vec![Utf8PathBuf::from("main.tex")]
            }
            ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred
            | ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs => vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/intro.tex"),
            ],
            ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile => unreachable!(),
        }
    );
    if !matches!(
        case,
        ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls
    ) {
        assert_nonreplay_build_meta(&build_root, 1, &[Utf8PathBuf::from("main.tex")]);
    }
}
