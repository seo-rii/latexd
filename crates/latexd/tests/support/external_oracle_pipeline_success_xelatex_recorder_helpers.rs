struct ExternalOracleXelatexRecorderFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

fn prepare_external_oracle_xelatex_recorder_fixture(
    compiler: &str,
    main_source: &str,
) -> ExternalOracleXelatexRecorderFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        format!(
            r#"
compiler: {compiler}
toplevel:
  - main.tex
"#
        ),
    )
    .expect("write manifest");
    fs::write(root.join("main.tex"), main_source).expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    ExternalOracleXelatexRecorderFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

#[derive(Clone, Copy)]
enum ExternalOracleXelatexRecorderCase {
    MainAndIntro,
    IntroOnly,
    UnreadableCustomCompilerRecorder,
}

type XRec = ExternalOracleXelatexRecorderCase;

async fn run_xrec(case: XRec) {
    run_external_oracle_xelatex_recorder(case).await;
}

async fn run_external_oracle_xelatex_recorder(case: ExternalOracleXelatexRecorderCase) {
    if matches!(
        case,
        ExternalOracleXelatexRecorderCase::UnreadableCustomCompilerRecorder
    ) {
        let fixture = prepare_external_oracle_xelatex_recorder_fixture(
            "pdf_latex",
            "\\documentclass{article}\\begin{document}recorder read error\\end{document}",
        );

        let tool_dir = fixture.root.join("fake-tools");
        fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
        let compiler_script = tool_dir.join("oracle-compiler");
        write_executable_script(
            &compiler_script,
            r#"#!/bin/bash
set -euo pipefail
touch "$1"
mkdir -p "$2"
"#,
        );

        let driver = CompilerDriver::new(
            Some(compiler_script.to_string()),
            vec!["{out_pdf}".to_string(), "{fls}".to_string()],
        );
        let failure = driver
            .compile(CompileRequest {
                root: fixture.root.clone(),
                manifest: fixture.world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev: 1,
                build_root: fixture.build_root.clone(),
                changed_files: vec![Utf8PathBuf::from("main.tex")],
            })
            .await
            .expect_err("configured external oracle should fail when recorder file is unreadable");

        let fls_path = fixture.build_root.join("rev-1/main.fls");
        assert_eq!(failure.diagnostics.len(), 1);
        for expected in [
            "failed to read recorder file",
            fls_path.as_str(),
            "Is a directory",
        ] {
            assert!(failure.message.contains(expected));
            assert!(failure.diagnostics[0].message.contains(expected));
        }
        assert!(
            !fixture.build_root.join("rev-1/build-meta.json").exists(),
            "failed recorder reads should not emit build metadata"
        );
        return;
    }

    let fixture = prepare_external_oracle_xelatex_recorder_fixture(
        "xe_latex",
        "\\documentclass{article}\\begin{document}\\input{sections/intro}\\end{document}",
    );
    let root = &fixture.root;
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections/intro.tex"), "Intro").expect("write intro");

    let fls_rule = match case {
        ExternalOracleXelatexRecorderCase::MainAndIntro => {
            r#"printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" >> "$out_dir/$stem.fls""#
        }
        ExternalOracleXelatexRecorderCase::IntroOnly => {
            r#"printf 'INPUT %s\n' "$(pwd)/sections/intro.tex" > "$out_dir/$stem.fls""#
        }
        ExternalOracleXelatexRecorderCase::UnreadableCustomCompilerRecorder => unreachable!(),
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

    let driver = CompilerDriver::new(None, Vec::new());
    let outcome = driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
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
    assert_nonreplay_build_meta(&fixture.build_root, 1, &[Utf8PathBuf::from("main.tex")]);
}
