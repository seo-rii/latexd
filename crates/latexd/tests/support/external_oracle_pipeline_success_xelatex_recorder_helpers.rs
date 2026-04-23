#[derive(Clone, Copy)]
enum ExternalOracleXelatexRecorderFlsCase {
    MainAndIntro,
    IntroOnly,
}

async fn run_external_oracle_xelatex_recorder_success(
    fls_case: ExternalOracleXelatexRecorderFlsCase,
) {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: xe_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\input{sections/intro}\\end{document}",
    )
    .expect("write main");
    fs::write(root.join("sections/intro.tex"), "Intro").expect("write intro");

    let fls_rule = match fls_case {
        ExternalOracleXelatexRecorderFlsCase::MainAndIntro => {
            r#"printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" >> "$out_dir/$stem.fls""#
        }
        ExternalOracleXelatexRecorderFlsCase::IntroOnly => {
            r#"printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" > "$out_dir/$stem.fls""#
        }
    };
    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    write_executable_script(
        &tool_dir.join("xelatex"),
        &format!(
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
stem="${{main##*/}}"
stem="${{stem%.tex}}"
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$out_dir/$stem.pdf"
{fls_rule}
"#
        ),
    );

    let _path_lock = lock_path_env();
    let _path_guard = set_path(&tool_dir, false);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
    let build_root = root.join(".latexd/build");
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("xelatex build should succeed");

    assert!(outcome.pdf_path.exists());
    assert_eq!(
        outcome.dep_trace.inputs,
        vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/intro.tex"),
        ]
    );
    assert_nonreplay_build_meta(&build_root, 1, &[Utf8PathBuf::from("main.tex")]);
}
