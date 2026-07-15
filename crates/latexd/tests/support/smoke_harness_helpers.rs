#[derive(Debug, serde::Deserialize)]
struct BuildMeta {
    aux_sensitive: bool,
    dirty_files: Vec<Utf8PathBuf>,
    start_checkpoint_id: Option<String>,
    start_page_index: usize,
    page_count: usize,
    rebuilt_page_count: usize,
    reused_page_count: usize,
    semantic_pass_count: usize,
    semantic_rerun_count: usize,
    semantic_fixpoint_reached: bool,
    semantic_aux_backdated: bool,
}

#[derive(Debug, serde::Deserialize)]
struct StoredSources {
    files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    executed_files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    rewrite_spans: BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
}

macro_rules! smoke_case_test {
    ($name:ident, $future:expr) => {
        #[tokio::test]
        async fn $name() {
            ($future).await;
        }
    };
}

macro_rules! smoke {
    ($name:ident => $future:expr) => {
        smoke_case_test!($name, $future);
    };
}

fn assert_page_patches_transform(
    previous_page_ids: &[String],
    patches: &[PagePatchOp],
    expected_page_ids: &[String],
) {
    let mut actual_page_ids = previous_page_ids.to_vec();
    for patch in patches {
        match patch {
            PagePatchOp::ReplacePage { index, page_id, .. } => {
                actual_page_ids[*index] = page_id.clone();
            }
            PagePatchOp::InsertPage { index, page_id, .. } => {
                actual_page_ids.insert(*index, page_id.clone());
            }
            PagePatchOp::DeletePage { index } => {
                actual_page_ids.remove(*index);
            }
        }
    }
    assert_eq!(actual_page_ids, expected_page_ids);
}

fn assert_renderer_page_artifact_reuse(
    previous: &CompileOutcome,
    current: &CompileOutcome,
    previous_rev: u64,
    current_rev: u64,
) {
    assert_eq!(
        current.page_artifacts.len(),
        current.renderer_page_metadata.len()
    );
    for (artifact, page) in current
        .page_artifacts
        .iter()
        .zip(&current.renderer_page_metadata)
    {
        assert_eq!(artifact.page_id, page.page_id);
        let rev = if previous
            .renderer_page_metadata
            .iter()
            .any(|previous_page| previous_page.page_id == page.page_id)
        {
            previous_rev
        } else {
            current_rev
        };
        assert!(
            artifact
                .pdf_url
                .starts_with(&format!("/artifacts/rev/{rev}/pages/")),
            "renderer page {} should use revision {rev}, got {}",
            page.page_id,
            artifact.pdf_url
        );
    }
}

fn renderer_page_index_covering_source_offset(
    outcome: &CompileOutcome,
    source_path: &Utf8Path,
    source_offset: usize,
) -> usize {
    outcome
        .renderer_page_metadata
        .iter()
        .find(|page| {
            page.source_spans.iter().any(|span| {
                span.file == source_path
                    && span.start_utf8 as usize <= source_offset
                    && source_offset < span.end_utf8 as usize
            })
        })
        .map(|page| page.index)
        .unwrap_or_else(|| {
            panic!(
                "no page covers source offset {source_offset} in {source_path}; pages={:?}",
                outcome.renderer_page_metadata
            )
        })
}

static PATH_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct PathOverrideGuard {
    original_path: Option<OsString>,
}

impl Drop for PathOverrideGuard {
    fn drop(&mut self) {
        match self.original_path.take() {
            Some(path) => unsafe {
                std::env::set_var("PATH", path);
            },
            None => unsafe {
                std::env::remove_var("PATH");
            },
        }
    }
}

fn set_path(tool_dir: &Utf8Path, include_original_path: bool) -> PathOverrideGuard {
    let original_path = std::env::var_os("PATH");
    let mut path_entries = vec![tool_dir.as_std_path().to_path_buf()];
    if include_original_path {
        path_entries.extend(std::env::split_paths(
            original_path.as_deref().unwrap_or_default(),
        ));
    }
    let joined_path = std::env::join_paths(path_entries).expect("join path");
    unsafe {
        std::env::set_var("PATH", &joined_path);
    }
    PathOverrideGuard { original_path }
}

fn write_executable_script(path: &Utf8Path, body: &str) {
    use std::io::Write as _;

    let temp_path = path.with_extension("tmp");
    {
        let mut file = fs::File::create(temp_path.as_std_path()).expect("write executable script");
        file.write_all(body.as_bytes())
            .expect("write executable script body");
        file.sync_all().expect("sync executable script");
    }
    fs::set_permissions(temp_path.as_std_path(), fs::Permissions::from_mode(0o755))
        .expect("chmod executable script");
    fs::rename(temp_path.as_std_path(), path.as_std_path()).expect("install executable script");
    if let Some(parent) = path.parent() {
        let parent_dir = fs::File::open(parent.as_std_path()).expect("open executable script dir");
        parent_dir.sync_all().expect("sync executable script dir");
    }
}

fn copy_test_fixture_tree(source_root: &Utf8Path, target_root: &Utf8Path) {
    let mut copy_dirs = vec![(source_root.to_owned(), target_root.to_owned())];
    while let Some((source_dir, target_dir)) = copy_dirs.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 source path");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                copy_dirs.push((source_path, target_path));
                continue;
            }
            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                .expect("copy fixture file");
        }
    }
}

fn lock_path_env() -> MutexGuard<'static, ()> {
    PATH_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn assert_nonreplay_build_meta(build_root: &Utf8Path, rev: usize, dirty_files: &[Utf8PathBuf]) {
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join(format!("rev-{rev}/build-meta.json"))).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, 0);
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, 0);
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(!build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

fn fake_latex_dvi_script() -> &'static str {
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
    *)
      main="$1"
      shift
      ;;
  esac
done
stem="${main##*/}"
stem="${stem%.tex}"
printf 'fake-dvi' > "$out_dir/$stem.dvi"
printf 'INPUT %s\n' "$(pwd)/$main" > "$out_dir/$stem.fls"
"#
}

fn fake_dvips_script() -> &'static str {
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
printf 'fake-ps' > "$out"
"#
}

fn fake_ps2pdf_script() -> &'static str {
    r#"#!/bin/bash
set -euo pipefail
input="$1"
output="$2"
test -f "$input"
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

fn fake_pdflatex_pdf_script() -> &'static str {
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

fn fake_tectonic_pdf_script() -> &'static str {
    r#"#!/bin/bash
set -euo pipefail
depfile=""
out_dir=""
main=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --makefile-rules)
      depfile="$2"
      shift 2
      ;;
    --outdir)
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
if [ -n "$depfile" ]; then
  printf '%s: %s\n' "$out_dir/$stem.pdf" "$main" > "$depfile"
fi
"#
}

enum FakeWarningPdfScript {
    Latex,
    Tectonic,
}

fn fake_warning_pdf_script(kind: FakeWarningPdfScript, warning: &str) -> String {
    let script = match kind {
        FakeWarningPdfScript::Latex => fake_pdflatex_pdf_script(),
        FakeWarningPdfScript::Tectonic => fake_tectonic_pdf_script(),
    };
    script.replacen(
        "stem=\"${stem%.tex}\"\n",
        &format!("stem=\"${{stem%.tex}}\"\necho \"{warning}\" >&2\n"),
        1,
    )
}

fn fake_success_without_output_script() -> &'static str {
    "#!/bin/bash\nset -euo pipefail\nexit 0\n"
}
