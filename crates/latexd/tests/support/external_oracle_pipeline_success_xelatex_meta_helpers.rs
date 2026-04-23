#[derive(Clone, Copy)]
enum ExternalOracleXelatexMetaSuccessCase {
    Pipeline,
    BuildMeta,
    ChangedFileOrder,
}

async fn run_external_oracle_xelatex_meta_success(case: ExternalOracleXelatexMetaSuccessCase) {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        match case {
            ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => {
                r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#
            }
            ExternalOracleXelatexMetaSuccessCase::Pipeline
            | ExternalOracleXelatexMetaSuccessCase::BuildMeta => {
                r#"
compiler: xe_latex
toplevel:
  - main.tex
"#
            }
        },
    )
    .expect("write manifest");
    if matches!(case, ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder) {
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\begin{document}\\input{sections/intro}\\end{document}",
        )
        .expect("write main");
        fs::write(root.join("sections/intro.tex"), "Intro").expect("write intro");
    } else {
        let body = match case {
            ExternalOracleXelatexMetaSuccessCase::Pipeline => "xelatex pipeline",
            ExternalOracleXelatexMetaSuccessCase::BuildMeta => "xelatex build meta lane",
            ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => unreachable!(),
        };
        fs::write(
            root.join("main.tex"),
            format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}"),
        )
        .expect("write main");
    }

    let mut _path_lock = None;
    let mut _path_guard = None;
    let driver = if matches!(case, ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder) {
        let compiler_script = root.join("fake-compiler.sh");
        write_executable_script(
            &compiler_script,
            r#"#!/bin/bash
set -euo pipefail
output="$1"
fls="$2"
depfile="${3:-}"
cat > "$output" <<'EOF'
%PDF-1.4
1 0 obj
<<>>
endobj
trailer
<<>>
%%EOF
EOF
printf '%s: %s %s\n' "$output" "main.tex" "sections/intro.tex" > "$depfile"
printf 'INPUT %s\n' "$(pwd)/main.tex" > "$fls"
printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" >> "$fls"
"#,
        );
        CompilerDriver::new(
            Some(compiler_script.to_string()),
            vec![
                "{out_pdf}".to_string(),
                "{fls}".to_string(),
                "{depfile}".to_string(),
            ],
        )
    } else {
        let tool_dir = root.join("fake-tools");
        fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
        write_executable_script(
            &tool_dir.join("xelatex"),
            match case {
                ExternalOracleXelatexMetaSuccessCase::Pipeline => {
                    r#"#!/usr/bin/env bash
set -euo pipefail
out_dir=""
main=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -output-directory)
      out_dir="$2"
      shift 2
      ;;
    *)
      main="$1"
      shift
      ;;
  esac
done
stem="$(basename "$main" .tex)"
mkdir -p "$out_dir"
cat > "$out_dir/$stem.pdf" <<'EOF'
%PDF-1.4
1 0 obj
<<>>
endobj
trailer
<<>>
%%EOF
EOF
printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
"#
                }
                ExternalOracleXelatexMetaSuccessCase::BuildMeta => {
                    r#"#!/bin/bash
set -euo pipefail
out_dir=""
main=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -output-directory)
      out_dir="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      main="$1"
      shift
      ;;
  esac
done
stem="${main##*/}"
stem="${stem%.tex}"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$out_dir/$stem.pdf"
printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
"#
                }
                ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => unreachable!(),
            },
        );

        _path_lock = Some(lock_path_env());
        _path_guard = Some(set_path(
            &tool_dir,
            matches!(case, ExternalOracleXelatexMetaSuccessCase::Pipeline),
        ));
        CompilerDriver::new(None, Vec::new())
    };

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    let changed_files = match case {
        ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => vec![
            Utf8PathBuf::from("sections/intro.tex"),
            Utf8PathBuf::from("main.tex"),
        ],
        ExternalOracleXelatexMetaSuccessCase::Pipeline
        | ExternalOracleXelatexMetaSuccessCase::BuildMeta => vec![Utf8PathBuf::from("main.tex")],
    };
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: changed_files.clone(),
        })
        .await
        .expect(match case {
            ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => {
                "configured external oracle should succeed"
            }
            ExternalOracleXelatexMetaSuccessCase::Pipeline
            | ExternalOracleXelatexMetaSuccessCase::BuildMeta => "xelatex build should succeed",
        });

    assert!(outcome.pdf_path.exists());
    if !matches!(case, ExternalOracleXelatexMetaSuccessCase::BuildMeta) {
        assert_eq!(
            outcome.dep_trace.inputs,
            match case {
                ExternalOracleXelatexMetaSuccessCase::Pipeline => {
                    vec![Utf8PathBuf::from("main.tex")]
                }
                ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder => vec![
                    Utf8PathBuf::from("main.tex"),
                    Utf8PathBuf::from("sections/intro.tex"),
                ],
                ExternalOracleXelatexMetaSuccessCase::BuildMeta => unreachable!(),
            }
        );
    }
    assert_nonreplay_build_meta(&build_root, 1, &changed_files);
}
