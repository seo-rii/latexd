#[derive(Clone, Copy)]
enum ExternalOraclePipelineWarningCase {
    Dvips,
    MultiStage,
}

async fn run_external_oracle_custom_compiler_pipeline_warning(
    warning_case: ExternalOraclePipelineWarningCase,
) {
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
    let document_body = match warning_case {
        ExternalOraclePipelineWarningCase::Dvips => "dvips warning lane",
        ExternalOraclePipelineWarningCase::MultiStage => "multi-stage warning lane",
    };
    fs::write(
        root.join("main.tex"),
        format!("\\documentclass{{article}}\\begin{{document}}{document_body}\\end{{document}}"),
    )
    .expect("write main");

    let tool_dir = root.join("fake-tools");
    fs::create_dir_all(tool_dir.as_std_path()).expect("create tool dir");
    match warning_case {
        ExternalOraclePipelineWarningCase::Dvips => {
            write_executable_script(&tool_dir.join("latex"), fake_latex_dvi_script());
            write_executable_script(
                &tool_dir.join("dvips"),
                r#"#!/bin/bash
set -euo pipefail
out=""
input=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    *)
      input="$1"
      shift
      ;;
  esac
done
test -f "$input"
echo "dvips warning: paper size fallback applied." >&2
printf 'fake-ps' > "$out"
"#,
            );
            write_executable_script(&tool_dir.join("ps2pdf"), fake_ps2pdf_script());
        }
        ExternalOraclePipelineWarningCase::MultiStage => {
            write_executable_script(
                &tool_dir.join("latex"),
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
echo "latex warning: rerun for updated labels."
printf 'fake-dvi' > "$out_dir/$stem.dvi"
"#,
            );
            write_executable_script(&tool_dir.join("dvips"), fake_dvips_script());
            write_executable_script(
                &tool_dir.join("ps2pdf"),
                r#"#!/bin/bash
set -euo pipefail
input="$1"
output="$2"
test -f "$input"
echo "ps2pdf warning: compatibility mode used." >&2
printf '%s\n' \
  '%PDF-1.4' \
  '1 0 obj' \
  '<<>>' \
  'endobj' \
  'trailer' \
  '<<>>' \
  '%%EOF' > "$output"
"#,
            );
        }
    }

    let _path_lock = PATH_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("lock path env");
    let _path_guard = set_path(
        &tool_dir,
        matches!(warning_case, ExternalOraclePipelineWarningCase::Dvips),
    );

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(None, Vec::new());
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
        .expect("latex_dvips_ps2pdf build should succeed");

    assert!(outcome.pdf_path.exists());
    let warning_messages = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    let expected_warning_messages = match warning_case {
        ExternalOraclePipelineWarningCase::Dvips => vec!["dvips warning"],
        ExternalOraclePipelineWarningCase::MultiStage => vec!["latex warning", "ps2pdf warning"],
    };
    for expected_warning in expected_warning_messages {
        assert!(
            warning_messages
                .iter()
                .any(|message| message.contains(expected_warning)),
            "expected {expected_warning} in diagnostics, saw {warning_messages:?}"
        );
    }
}
