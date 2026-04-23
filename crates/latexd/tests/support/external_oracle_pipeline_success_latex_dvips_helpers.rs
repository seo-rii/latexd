#[derive(Clone, Copy)]
enum ExternalOracleLatexDvipsSuccessCase {
    MainOnlyPipeline,
    MainAndIntroFls,
    IntroOnlyFls,
}

async fn run_external_oracle_latex_dvips_success(case: ExternalOracleLatexDvipsSuccessCase) {
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
    if matches!(case, ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline) {
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\begin{document}dvips pipeline\\end{document}",
        )
        .expect("write main");
    } else {
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\begin{document}\\input{sections/intro}\\end{document}",
        )
        .expect("write main");
        fs::write(root.join("sections/intro.tex"), "Intro").expect("write intro");
    }

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    if matches!(case, ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline) {
        write_executable_script(&tool_dir.join("latex"), fake_latex_dvi_script());
    } else {
        let fls_rule = match case {
            ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline => unreachable!(),
            ExternalOracleLatexDvipsSuccessCase::MainAndIntroFls => {
                r#"printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" >> "$out_dir/$stem.fls""#
            }
            ExternalOracleLatexDvipsSuccessCase::IntroOnlyFls => {
                r#"printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" > "$out_dir/$stem.fls""#
            }
        };
        write_executable_script(
            &tool_dir.join("latex"),
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
printf 'fake-dvi' > "$out_dir/$stem.dvi"
{fls_rule}
"#
            ),
        );
    }
    write_executable_script(&tool_dir.join("dvips"), fake_dvips_script());
    write_executable_script(&tool_dir.join("ps2pdf"), fake_ps2pdf_script());

    let _path_lock = lock_path_env();
    let _path_guard = set_path(
        &tool_dir,
        matches!(case, ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline),
    );

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
        .expect("latex -> dvips -> ps2pdf build should succeed");

    assert!(outcome.pdf_path.exists());
    assert_eq!(
        outcome.dep_trace.inputs,
        match case {
            ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline => {
                vec![Utf8PathBuf::from("main.tex")]
            }
            ExternalOracleLatexDvipsSuccessCase::MainAndIntroFls
            | ExternalOracleLatexDvipsSuccessCase::IntroOnlyFls => vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/intro.tex"),
            ],
        }
    );
    if matches!(case, ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline) {
        assert!(build_root.join("rev-1/main.dvi").exists());
        assert!(build_root.join("rev-1/main.ps").exists());
    } else {
        assert_nonreplay_build_meta(&build_root, 1, &[Utf8PathBuf::from("main.tex")]);
    }
}
