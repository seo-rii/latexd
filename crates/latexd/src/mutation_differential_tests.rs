//! Synthetic edit equivalence, never an arXiv compatibility score.
//!
//! The clean side owns an independent project and build directory on every step.
//! The preview side retains its real checkpoint/artifact chain and advances its
//! applied revision only after successful compilation, including error recovery.

use std::{collections::BTreeMap, fs, time::Instant};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use serde_json::{Value, json};

use crate::compiler::{CompileFailure, CompileOutcome, CompileRequest, CompilerDriver};

const DEFAULT_SEED: u64 = 0x4c41_5445_5844;
const CYCLE_LENGTH: usize = 16;
type ProjectFiles = BTreeMap<String, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComparisonMode {
    EagerDebug,
    LivePreview,
}

impl ComparisonMode {
    fn layers(self) -> &'static [&'static str] {
        match self {
            Self::EagerDebug => &["semantic", "paint", "aux", "interaction"],
            Self::LivePreview => &[
                "execution_output_and_readset",
                "paint",
                "aux",
                "interaction",
            ],
        }
    }

    fn report_filename(self) -> &'static str {
        match self {
            Self::EagerDebug => "report.json",
            Self::LivePreview => "live-preview-report.json",
        }
    }

    fn evidence_prefix(self) -> &'static str {
        match self {
            Self::EagerDebug => "latexd-mutations-",
            Self::LivePreview => "latexd-live-mutations-",
        }
    }

    fn output_environment(self) -> &'static str {
        match self {
            Self::EagerDebug => "LATEXD_MUTATION_OUTPUT",
            Self::LivePreview => "LATEXD_LIVE_MUTATION_OUTPUT",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
struct FixtureState {
    middle_word: usize,
    tail_word: usize,
    macro_word: usize,
    included_word: usize,
    comment_lines: usize,
    included_comment_lines: usize,
    sentence: bool,
    math_digit: usize,
    renamed_label: bool,
    inserted_page: bool,
    error: bool,
}

impl FixtureState {
    fn files(&self) -> ProjectFiles {
        const WORDS: [&str; 8] = [
            "alpha", "bravo", "charm", "delta", "ember", "fable", "globe", "hotel",
        ];
        let label = if self.renamed_label { "last" } else { "tail" };
        let comments = "% Same-output source movement.\n".repeat(self.comment_lines);
        let input_comments = "% Included-source movement.\n".repeat(self.included_comment_lines);
        let sentence = if self.sentence {
            " An added sentence changes paragraph width."
        } else {
            ""
        };
        let inserted_page = if self.inserted_page {
            // Exercise actual renderer pagination even when the mini-kernel
            // does not emit a forced break for a source-level \newpage.
            format!(
                "{}\nInserted content before the stable tail.\n",
                r"\[\frac{x_1+x_2}{y_1+y_2}=z^2\]".repeat(32)
            )
        } else {
            String::new()
        };
        let error = if self.error {
            "\\errmessage{intentional mutation failure}\n"
        } else {
            ""
        };
        let source = format!(
            "\\documentclass{{article}}\n\\input{{defs.sty}}\n{comments}\\begin{{document}}\n\
             \\section{{Opening}}\\label{{opening}}\n\\input{{body.tex}}\n\
             \\newpage\n\\section{{Middle}}\\label{{middle}}\n\
             Middle {}.{} Inline math $x^{}+y^2$.\n\
             Tail page \\pageref{{{label}}}; section \\ref{{{label}}}.\n\
             {error}{inserted_page}\\newpage\n\\section{{Tail}}\\label{{{label}}}\n\
             Tail {}. Macro \\word.\n\\end{{document}}\n",
            WORDS[self.middle_word],
            sentence,
            self.math_digit + 1,
            WORDS[self.tail_word]
        );
        BTreeMap::from([
            ("main.tex".into(), source),
            (
                "defs.sty".into(),
                format!("\\def\\word{{{}}}\n", WORDS[self.macro_word]),
            ),
            (
                "body.tex".into(),
                format!(
                    "{input_comments}Included {}. Opening body remains visible.\n",
                    WORDS[self.included_word]
                ),
            ),
        ])
    }
}

struct Generator {
    state: u64,
}

impl Generator {
    fn next(&mut self) -> usize {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state as usize
    }

    fn mutate(
        &mut self,
        step: usize,
        state: &mut FixtureState,
        history: &[FixtureState],
    ) -> &'static str {
        let advance = 1 + self.next() % 7;
        match (step - 1) % CYCLE_LENGTH {
            0 => {
                state.middle_word = (state.middle_word + advance) % 8;
                "local_middle_word"
            }
            1 => {
                state.tail_word = (state.tail_word + advance) % 8;
                "local_tail_word"
            }
            2 => {
                state.comment_lines = 1 + self.next() % 3;
                "same_output_source_movement"
            }
            3 => {
                state.macro_word = (state.macro_word + advance) % 8;
                "preamble_macro"
            }
            4 => {
                state.included_word = (state.included_word + advance) % 8;
                "included_file"
            }
            5 => {
                state.sentence = !state.sentence;
                "paragraph_insert_or_delete"
            }
            6 => {
                state.math_digit = (state.math_digit + advance) % 8;
                "inline_math"
            }
            7 => {
                state.error = true;
                "intentional_error"
            }
            8 => {
                state.error = false;
                "repair_after_error"
            }
            9 => {
                *state = history[history.len().saturating_sub(3)].clone();
                "undo_to_previous_input"
            }
            10 => {
                state.renamed_label = !state.renamed_label;
                "rename_label_and_reference"
            }
            11 => {
                state.inserted_page = true;
                "page_insertion"
            }
            12 => {
                state.inserted_page = false;
                "page_deletion"
            }
            13 => {
                state.included_comment_lines = 1 + self.next() % 3;
                "included_source_movement"
            }
            14 => "no_op_compile_request",
            _ => {
                *state = FixtureState::default();
                "undo_to_baseline"
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct Observation {
    success: bool,
    error: Option<String>,
    wall_us: u64,
    work_trace: Option<Value>,
    diagnostics: Value,
    layers: BTreeMap<String, Value>,
    active_artifact_hashes: BTreeMap<Utf8PathBuf, String>,
    // Publication ownership is not serialized into either output oracle.
    #[serde(skip)]
    publishable_preview: Option<crate::LivePreviewState>,
}

/// Array order, source offsets, glyph positions and semantic values remain
/// exact. Only the independently allocated absolute project mount is rebased.
fn normalize_project_mount(value: &mut Value, root: &Utf8Path) {
    match value {
        Value::String(text) => *text = text.replace(root.as_str(), "<project>"),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| normalize_project_mount(value, root)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| normalize_project_mount(value, root)),
        _ => {}
    }
}

fn read_json(path: &Utf8Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {path}"))?)
        .with_context(|| format!("parse {path}"))
}

fn remove_sources(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("source");
            fields.remove("source_spans");
            fields.values_mut().for_each(remove_sources);
        }
        Value::Array(values) => values.iter_mut().for_each(remove_sources),
        _ => {}
    }
}

fn observe(
    root: &Utf8Path,
    revision_dir: &Utf8Path,
    outcome: Result<CompileOutcome, CompileFailure>,
    wall_us: u64,
    comparison: ComparisonMode,
    live_producer: bool,
    revision: u64,
) -> Result<Observation> {
    let work_trace = revision_dir
        .join("build-meta.json")
        .exists()
        .then(|| read_json(&revision_dir.join("build-meta.json")))
        .transpose()?
        .and_then(|metadata| metadata.get("trace").cloned());
    let mut layers = BTreeMap::new();
    let mut active_artifact_hashes = BTreeMap::new();
    let mut publishable_preview = None;
    let (success, error, mut diagnostics) = match outcome {
        Ok(outcome) => {
            if live_producer {
                for path in [
                    "render-ir/events.json",
                    "render-ir/document-ir.json",
                    "render-ir/legacy-output.txt",
                    "render-ir/display-list.pdf",
                    "checkpoints.json",
                ] {
                    if revision_dir.join(path).exists() {
                        bail!("live preview unexpectedly emitted deferred/debug artifact {path}");
                    }
                }
                if outcome.pdf_path.exists() {
                    bail!("live preview eagerly emitted its whole-document PDF");
                }
                if !crate::pdf_export::validated_recipe(
                    revision_dir.parent().context("build root")?,
                    &outcome.pdf_path,
                )? {
                    bail!("live preview lacks a valid deferred PDF recipe");
                }
                let trace = work_trace.as_ref().context("live work trace")?;
                if trace["full_pdf_exports"] != 0
                    || trace["debug_artifact_exports"] != 0
                    || trace["checkpoint_fallback"] != "live_preview_cold_execution_output_reuse"
                    || trace["restored_prefix_source_bytes"] != 0
                {
                    bail!("oracle did not exercise the production live execution policy: {trace}");
                }
                let mut live = crate::LivePreviewState::default();
                live.latest_pdf_path = Some(outcome.pdf_path.clone());
                live.page_metadata = outcome.renderer_page_metadata.clone();
                live.snapshot.apply_success(
                    revision,
                    outcome.diagnostics.clone(),
                    crate::viewer_prefixed_path(&format!("/artifacts/rev/{revision}/main.pdf")),
                    outcome.renderer_page_metadata.len(),
                    outcome
                        .renderer_page_metadata
                        .iter()
                        .map(|page| page.page_id.clone())
                        .collect(),
                    outcome.page_artifacts.clone(),
                );
                publishable_preview = Some(live);
            }
            let display_list = read_json(&revision_dir.join("render-ir/page-display-list.json"))?;
            let mut paint = display_list.clone();
            remove_sources(&mut paint);
            // The eager oracle retains its full semantic contract. Live
            // does not manufacture equality for intentionally absent debug
            // JSON; its separately named layer compares real output/readset.
            match comparison {
                ComparisonMode::EagerDebug => {
                    let events = read_json(&revision_dir.join("render-ir/events.json"))?;
                    let document = read_json(&revision_dir.join("render-ir/document-ir.json"))?;
                    layers.insert(
                        "semantic".into(),
                        json!({
                            "output": fs::read_to_string(revision_dir.join("output.txt"))?,
                            "events": events, "document_ir": document,
                            "dependencies": outcome.dep_trace.inputs,
                        }),
                    );
                }
                ComparisonMode::LivePreview => {
                    layers.insert(
                        "execution_output_and_readset".into(),
                        json!({
                            "output": fs::read_to_string(revision_dir.join("output.txt"))?,
                            "dependencies": outcome.dep_trace.inputs,
                        }),
                    );
                }
            }
            layers.insert("paint".into(), paint);
            let sources = read_json(&revision_dir.join("sources.json"))?;
            let mut interaction = json!({
                "syncmap": read_json(&revision_dir.join("page-syncmap.json"))?,
                "display_list_with_sources": display_list,
                "consumed_sources": sources.get("files"),
            });
            if comparison == ComparisonMode::LivePreview {
                interaction["saved_sources_and_execution_origins"] = sources;
            }
            layers.insert("interaction".into(), interaction);
            let mut aux = serde_json::Map::new();
            for name in ["aux.json", "semantic-index.json"] {
                let path = revision_dir.join(name);
                aux.insert(
                    name.into(),
                    if path.exists() {
                        read_json(&path)?
                    } else {
                        Value::Null
                    },
                );
            }
            let concrete_aux = revision_dir.join("semantic.aux");
            aux.insert(
                "semantic.aux".into(),
                if concrete_aux.exists() {
                    Value::String(fs::read_to_string(concrete_aux)?)
                } else {
                    Value::Null
                },
            );
            layers.insert("aux".into(), Value::Object(aux));

            // A reused artifact may legitimately belong to an older revision;
            // validate ownership and bytes rather than comparing owner URLs.
            for page in &outcome.renderer_page_metadata {
                let path = revision_dir
                    .parent()
                    .context("build root")?
                    .join(&page.pdf_artifact_path);
                let bytes =
                    fs::read(&path).with_context(|| format!("active PDF artifact {path}"))?;
                if !bytes.starts_with(b"%PDF-") {
                    bail!("invalid active PDF artifact {path}");
                }
                active_artifact_hashes.insert(path, blake3::hash(&bytes).to_hex().to_string());
            }
            (true, None, serde_json::to_value(outcome.diagnostics)?)
        }
        Err(failure) => (
            false,
            Some(failure.message),
            serde_json::to_value(failure.diagnostics)?,
        ),
    };
    layers
        .values_mut()
        .for_each(|value| normalize_project_mount(value, root));
    normalize_project_mount(&mut diagnostics, root);
    Ok(Observation {
        success,
        error: error.map(|error| error.replace(root.as_str(), "<project>")),
        wall_us,
        work_trace,
        diagnostics,
        layers,
        active_artifact_hashes,
        publishable_preview,
    })
}

fn assert_artifacts_unchanged(artifacts: &BTreeMap<Utf8PathBuf, String>) -> Result<()> {
    for (path, expected_hash) in artifacts {
        let bytes = fs::read(path).with_context(|| format!("last-good artifact lost: {path}"))?;
        if blake3::hash(&bytes).to_hex().as_str() != expected_hash {
            bail!("immutable last-good artifact changed in place: {path}");
        }
    }
    Ok(())
}

fn directory_bytes(path: &Utf8Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += directory_bytes(
                Utf8Path::from_path(&entry.path()).context("UTF-8 artifact path")?,
            )?;
        } else {
            total += metadata.len();
        }
    }
    Ok(total)
}

fn summarize_work(steps: &[Value], side: &str, edits_only: bool) -> Value {
    let builds = steps
        .iter()
        .filter(|step| (!edits_only || step["step"] != 0) && step[side]["success"] == true)
        .map(|step| &step[side])
        .collect::<Vec<_>>();
    let mut wall = builds
        .iter()
        .filter_map(|build| build["wall_us"].as_u64())
        .collect::<Vec<_>>();
    wall.sort_unstable();
    let percentile = |percent: usize| {
        wall.get((wall.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
    };
    let mut counters = BTreeMap::new();
    for counter in [
        "vm_executions",
        "dispatched_tokens",
        "skipped_execution_tokens",
        "reused_output_pages",
    ] {
        let measured = builds
            .iter()
            .filter_map(|build| build["work_trace"][counter].as_u64())
            .collect::<Vec<_>>();
        counters.insert(counter, json!({
            "measured_builds": measured.len(), "unmeasured_builds": builds.len() - measured.len(),
            "sum": if measured.is_empty() { None } else { Some(measured.iter().sum::<u64>()) },
        }));
    }
    json!({"successful_builds": builds.len(), "compiler_wall_us": {"p50": percentile(50), "p95": percentile(95)}, "work_counters": counters})
}

async fn compile(
    root: &Utf8Path,
    revision: u64,
    applied: u64,
    changed_files: Vec<Utf8PathBuf>,
    comparison: ComparisonMode,
    live_producer: bool,
) -> Result<Observation> {
    let build_root = root.join(".latexd/build");
    let start = Instant::now();
    let driver = CompilerDriver::new(Some("internal".into()), Vec::new());
    let driver = if live_producer {
        driver.with_preview_artifacts()
    } else {
        driver
    };
    let outcome = driver
        .compile_for_preview(
            CompileRequest {
                root: root.to_owned(),
                manifest: tex_world::ProjectManifest::discover(root)?,
                toplevel: "main.tex".into(),
                rev: revision,
                build_root: build_root.clone(),
                changed_files,
            },
            applied,
        )
        .await;
    observe(
        root,
        &build_root.join(format!("rev-{revision}")),
        outcome,
        start.elapsed().as_micros().min(u64::MAX as u128) as u64,
        comparison,
        live_producer,
        revision,
    )
}

fn write_project(root: &Utf8Path, files: &ProjectFiles) -> Result<()> {
    fs::create_dir_all(root)?;
    for (path, source) in files {
        fs::write(root.join(path), source)?;
    }
    Ok(())
}

fn hash(value: &Value) -> String {
    blake3::hash(&serde_json::to_vec(value).expect("serializable JSON"))
        .to_hex()
        .to_string()
}

fn first_difference(left: &Value, right: &Value, path: &str) -> Option<String> {
    if left == right {
        return None;
    }
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys()) {
                let next = format!("{path}/{key}");
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        if let Some(path) = first_difference(left, right, &next) {
                            return Some(path);
                        }
                    }
                    _ => return Some(next),
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                if let Some(path) = first_difference(left, right, &format!("{path}/{index}")) {
                    return Some(path);
                }
            }
        }
        _ => {}
    }
    Some(path.to_owned())
}

async fn run_mutations(
    root: &Utf8Path,
    count: usize,
    seed: u64,
    comparison: ComparisonMode,
) -> Result<()> {
    let incremental_root = root.join("incremental");
    let clean_root = root.join("clean");
    let mut generator = Generator { state: seed };
    let mut state = FixtureState::default();
    let mut history = vec![state.clone()];
    let mut applied = 0;
    let mut applied_files = ProjectFiles::new();
    let mut steps = Vec::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut previous_clean_layers = None::<BTreeMap<String, Value>>;
    let incremental_build_root = incremental_root.join(".latexd/build");
    let preview_lease = crate::preview_leases::ClientLease::new(&incremental_build_root).await;
    let mut active_artifacts = BTreeMap::new();
    let mut peak_workspace_bytes = 0;
    let mut gc_runs = 0;
    let mut gc_removed_revisions = 0;
    let mut max_retained_revisions = 0;
    let mut published_live = None::<crate::LivePreviewState>;
    let mut published_manifest = None::<Vec<u8>>;
    let mut publication_commits = 0;
    let mut failed_publication_checks = 0;
    for step in 0..=count {
        let mutation = if step == 0 {
            "initial_cold"
        } else {
            generator.mutate(step, &mut state, &history)
        };
        let files = state.files();
        fs::write(
            root.join("current-input.json"),
            serde_json::to_vec_pretty(&json!({
                "seed": seed, "step": step, "requested_mutations": count,
                "mutation": mutation, "fixture": state, "files": files,
            }))?,
        )?;
        write_project(&incremental_root, &files)?;
        write_project(&clean_root, &files)?;
        let mut changed_files = files
            .iter()
            .filter(|(path, source)| applied_files.get(*path) != Some(*source))
            .map(|(path, _)| Utf8PathBuf::from(path))
            .collect::<Vec<_>>();
        if changed_files.is_empty() {
            changed_files.push("main.tex".into());
        }
        let revision = step as u64 + 1;
        if comparison == ComparisonMode::LivePreview && applied > 0 {
            let (published_revision, _) =
                crate::preview_persistence::published_page_metadata(&incremental_build_root)
                    .await?
                    .context("missing published live baseline before compilation")?;
            if published_revision != applied {
                bail!("published live baseline differs from applied revision before compile");
            }
        }
        let incremental = compile(
            &incremental_root,
            revision,
            applied,
            changed_files.clone(),
            comparison,
            comparison == ComparisonMode::LivePreview,
        )
        .await?;
        let clean = compile(
            &clean_root,
            1,
            0,
            files.keys().map(Utf8PathBuf::from).collect(),
            comparison,
            false,
        )
        .await?;
        assert_artifacts_unchanged(&active_artifacts)?;
        if let Some(expected_manifest) = &published_manifest {
            if fs::read(incremental_build_root.join("published-preview.json"))?
                != *expected_manifest
            {
                bail!("compiler changed the published manifest before the preview commit");
            }
            if !incremental.success {
                failed_publication_checks += 1;
            }
        }
        let mut differences = BTreeMap::new();
        if incremental.success != clean.success {
            differences.insert("build".into(), "success differs".to_owned());
        }
        if let Some(path) =
            first_difference(&incremental.diagnostics, &clean.diagnostics, "diagnostics")
        {
            differences.insert("diagnostics".into(), path);
        }
        if incremental.error != clean.error {
            differences.insert("error".into(), "failure message differs".into());
        }
        for &layer in comparison.layers() {
            if let Some(path) = first_difference(
                incremental.layers.get(layer).unwrap_or(&Value::Null),
                clean.layers.get(layer).unwrap_or(&Value::Null),
                layer,
            ) {
                differences.insert(layer.to_owned(), path);
            }
        }
        if clean.success == state.error {
            differences.insert(
                "fixture_expectation".into(),
                "clean result disagrees with deliberate error state".into(),
            );
        }
        if clean.success {
            if let Some(previous) = &previous_clean_layers {
                if matches!(
                    mutation,
                    "same_output_source_movement" | "included_source_movement"
                ) && clean.layers.get("paint") != previous.get("paint")
                {
                    differences.insert(
                        "fixture_paint".into(),
                        "comment-only edit changed clean paint".into(),
                    );
                }
                if mutation == "no_op_compile_request" && clean.layers != *previous {
                    differences.insert(
                        "fixture_noop".into(),
                        "identical source changed independent clean output".into(),
                    );
                }
                let page_count = clean.layers["paint"]
                    .as_array()
                    .context("clean paint pages")?
                    .len();
                let previous_count = previous["paint"]
                    .as_array()
                    .context("previous paint pages")?
                    .len();
                if (mutation == "page_insertion" && page_count <= previous_count)
                    || (mutation == "page_deletion" && page_count >= previous_count)
                {
                    differences.insert("fixture_page_count".into(), format!("{mutation} did not change actual page count: {previous_count} -> {page_count}"));
                }
            }
        }
        let status = if !clean.success && !state.error {
            "reference_unbuildable"
        } else if !differences.is_empty() {
            "new_failure"
        } else if state.error {
            "expected_failure"
        } else {
            "supported_approximate"
        };
        *statuses.entry(status.into()).or_default() += 1;
        steps.push(json!({
            "step": step, "revision": revision, "base_preview_revision": applied,
            "mutation": mutation, "fixture": state, "support_status": status,
            "changed_files": changed_files, "source_hashes": files.iter().map(|(path, source)|
                (path.clone(), blake3::hash(source.as_bytes()).to_hex().to_string())).collect::<BTreeMap<_, _>>(),
            "differences": differences,
            "incremental": {
                "success": incremental.success, "wall_us": incremental.wall_us,
                "work_trace": incremental.work_trace,
                "layer_hashes": incremental.layers.iter().map(|(name, value)| (name, hash(value))).collect::<BTreeMap<_, _>>()
            },
            "clean": {
                "success": clean.success, "wall_us": clean.wall_us,
                "work_trace": clean.work_trace,
                "layer_hashes": clean.layers.iter().map(|(name, value)| (name, hash(value))).collect::<BTreeMap<_, _>>()
            }
        }));
        peak_workspace_bytes = peak_workspace_bytes.max(directory_bytes(root)?);
        let mut report = json!({
            "schema_version": 1, "seed": seed, "requested_mutations": count,
            "completed_mutations": step, "coverage_kind": "synthetic_incremental_equivalence",
            "engine_profile": "internal_mini_kernel_approximate",
            "artifact_mode": "eager_debug_and_pdf_for_oracle",
            "machine": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
            "normalization": "JSON object key order and independent absolute project mounts only; paint excludes sources also compared in interaction",
            "pdf_comparison": "structured paint and valid artifact refs; PDF bytes are not compared",
            "no_op_scope": "CompilerDriver request, not daemon save-event elision",
            "retention": "production recent-revision GC plus last-good preview lease and referenced artifact owners; clean build deleted after each successful comparison",
            "peak_workspace_bytes_before_gc": peak_workspace_bytes,
            "compiler_measurements": {
                "incremental_edits": summarize_work(&steps, "incremental", true),
                "independent_clean": summarize_work(&steps, "clean", false),
            },
            "status_counts": statuses, "steps": steps,
        });
        if comparison == ComparisonMode::LivePreview {
            report["coverage_kind"] = json!("synthetic_live_preview_equivalence");
            report["artifact_mode"] = json!("production_live_preview_vs_independent_eager_clean");
            report["execution_policy"] = json!("live_preview_cold_execution_output_reuse");
            report["comparison_contract"] = json!({
                "exact": ["page_display_lists_and_provenance", "page_syncmap", "full_saved_sources_and_execution_origins", "aux", "legacy_output_text", "dependency_readset", "diagnostics_and_errors"],
                "not_compared": ["RenderEvent_debug_JSON_absent_in_live", "DocumentIR_debug_JSON_absent_in_live", "whole_PDF_bytes_and_checkpoint_files"],
                "live_policy_assertions": ["debug_and_whole_PDF_exports_absent", "checkpoints_absent", "valid_deferred_export_recipe", "cold_execution_policy_trace"],
                "clean_driver": "eager_and_independent_on_every_step",
                "publication": "prepare_and_commit_every_success; failures_preserve_manifest; recover_after_every_GC",
            });
            report["gc"] = json!({"completed_checks": gc_runs, "removed_revisions": gc_removed_revisions, "max_retained_revisions": max_retained_revisions});
        }
        fs::write(
            root.join(comparison.report_filename()),
            serde_json::to_vec_pretty(&report)?,
        )?;
        if !differences.is_empty() {
            fs::write(
                root.join("incremental-observation.json"),
                serde_json::to_vec_pretty(&incremental)?,
            )?;
            fs::write(
                root.join("clean-observation.json"),
                serde_json::to_vec_pretty(&clean)?,
            )?;
            bail!(
                "seed={seed:#x} step={step} mutation={mutation}: {differences:?}; evidence={root}"
            );
        }
        if incremental.success {
            if comparison == ComparisonMode::LivePreview {
                let live = incremental
                    .publishable_preview
                    .as_ref()
                    .context("successful live build lacks publication state")?;
                crate::preview_persistence::prepare(&incremental_build_root, live)
                    .await?
                    .commit()?;
                let (published_revision, published_pages) =
                    crate::preview_persistence::published_page_metadata(&incremental_build_root)
                        .await?
                        .context("published preview unavailable after commit")?;
                if published_revision != revision
                    || serde_json::to_value(published_pages)?
                        != serde_json::to_value(&live.page_metadata)?
                {
                    bail!("published preview differs from accepted live output");
                }
                published_manifest = Some(fs::read(
                    incremental_build_root.join("published-preview.json"),
                )?);
                published_live = Some(live.clone());
                publication_commits += 1;
            }
            applied = revision;
            applied_files = files;
            history.push(state.clone());
            previous_clean_layers = Some(clean.layers);
            active_artifacts = incremental.active_artifact_hashes;
            if !preview_lease.set_revision(applied, applied).await {
                bail!("failed to retain applied preview revision {applied}");
            }
        }
        // No clean checkpoint/artifact/source cache survives to the next step.
        fs::remove_dir_all(&clean_root)?;
        let before_gc = if comparison == ComparisonMode::LivePreview {
            Some(revision_directories(&incremental_build_root)?)
        } else {
            None
        };
        crate::prune_old_build_revisions(&incremental_build_root, revision).await;
        assert_artifacts_unchanged(&active_artifacts)?;
        if let Some(before) = before_gc {
            let after = revision_directories(&incremental_build_root)?;
            gc_runs += 1;
            gc_removed_revisions += before.difference(&after).count();
            max_retained_revisions = max_retained_revisions.max(after.len());
            let recovered =
                crate::preview_persistence::recover(&incremental_build_root, false).await?;
            let expected = published_live
                .as_ref()
                .context("missing last-good live publication")?;
            if recovered.snapshot.last_applied_rev != applied
                || recovered.latest_pdf_path != expected.latest_pdf_path
                || serde_json::to_value(recovered.page_metadata)?
                    != serde_json::to_value(&expected.page_metadata)?
                || serde_json::to_value(recovered.snapshot.page_artifacts)?
                    != serde_json::to_value(&expected.snapshot.page_artifacts)?
            {
                bail!("published last-good preview did not recover after GC");
            }
        }
    }
    if comparison == ComparisonMode::LivePreview {
        if gc_runs < 20 || gc_removed_revisions == 0 {
            bail!("live preview oracle requires at least 20 real GC checks and removed revisions");
        }
        let path = root.join(comparison.report_filename());
        let mut report = read_json(&path)?;
        report["gc"] = json!({"completed_checks": gc_runs, "removed_revisions": gc_removed_revisions, "max_retained_revisions": max_retained_revisions});
        report["publication"] = json!({"successful_commits": publication_commits, "failed_build_manifest_preservation_checks": failed_publication_checks, "recovery_after_gc_checks": gc_runs});
        fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!(
        "mutation differential: seed={seed:#x} mutations={count} status_counts={statuses:?} report={}",
        root.join(comparison.report_filename())
    );
    Ok(())
}

fn revision_directories(root: &Utf8Path) -> Result<std::collections::BTreeSet<u64>> {
    let mut revisions = std::collections::BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(revision) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("rev-"))
                .and_then(|number| number.parse().ok())
            {
                revisions.insert(revision);
            }
        }
    }
    Ok(revisions)
}

async fn run_with_evidence(count: usize, seed: u64, destination: Option<std::path::PathBuf>) {
    run_with_evidence_mode(count, seed, destination, ComparisonMode::EagerDebug).await;
}

async fn run_with_evidence_mode(
    count: usize,
    seed: u64,
    destination: Option<std::path::PathBuf>,
    comparison: ComparisonMode,
) {
    assert!(
        count > 0 && count <= 100_000,
        "mutation count must be in 1..=100000"
    );
    assert_ne!(seed, 0, "xorshift seed must be nonzero");
    let directory = if let Some(destination) = destination {
        fs::create_dir_all(&destination).expect("create report parent");
        tempfile::Builder::new()
            .prefix(comparison.evidence_prefix())
            .tempdir_in(destination)
    } else {
        tempfile::Builder::new()
            .prefix(comparison.evidence_prefix())
            .tempdir()
    }
    .expect("create isolated mutation workspace");
    let retain = std::env::var_os(comparison.output_environment()).is_some();
    let root = Utf8Path::from_path(directory.path()).expect("UTF-8 mutation workspace");
    let result = run_mutations(root, count, seed, comparison).await;
    if result.is_err() || retain {
        let evidence = directory.keep();
        println!("mutation evidence retained at {}", evidence.display());
    }
    if let Err(error) = result {
        panic!("{error:#}");
    }
}

#[tokio::test]
async fn mutation_differential_supported_edits_match_independent_clean() {
    run_with_evidence(2 * CYCLE_LENGTH, DEFAULT_SEED, None).await;
}

#[tokio::test]
#[ignore = "explicit configurable 1000-edit native equivalence run"]
async fn mutation_differential_thousand_edits_match_independent_clean() {
    let count = std::env::var("LATEXD_MUTATIONS")
        .map(|count| count.parse().expect("integer LATEXD_MUTATIONS"))
        .unwrap_or(1_000);
    let seed = std::env::var("LATEXD_MUTATION_SEED")
        .map(|seed| {
            if let Some(hex) = seed.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).expect("hexadecimal LATEXD_MUTATION_SEED")
            } else {
                seed.parse().expect("integer LATEXD_MUTATION_SEED")
            }
        })
        .unwrap_or(DEFAULT_SEED);
    run_with_evidence(
        count,
        seed,
        std::env::var_os("LATEXD_MUTATION_OUTPUT").map(Into::into),
    )
    .await;
}

#[tokio::test]
async fn mutation_differential_live_supported_edits_match_eager_clean() {
    run_with_evidence_mode(
        2 * CYCLE_LENGTH,
        DEFAULT_SEED,
        None,
        ComparisonMode::LivePreview,
    )
    .await;
}

#[tokio::test]
#[ignore = "explicit production live preview vs eager clean 1000-edit equivalence run"]
async fn mutation_differential_live_thousand_edits_match_eager_clean() {
    let count = std::env::var("LATEXD_LIVE_MUTATIONS")
        .map(|count| count.parse().expect("integer LATEXD_LIVE_MUTATIONS"))
        .unwrap_or(1_000);
    assert!(
        count >= 2 * CYCLE_LENGTH,
        "live gate needs two full edit cycles and at least 20 GC checks"
    );
    let seed = std::env::var("LATEXD_LIVE_MUTATION_SEED")
        .map(|seed| {
            if let Some(hex) = seed.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).expect("hexadecimal LATEXD_LIVE_MUTATION_SEED")
            } else {
                seed.parse().expect("integer LATEXD_LIVE_MUTATION_SEED")
            }
        })
        .unwrap_or(DEFAULT_SEED);
    run_with_evidence_mode(
        count,
        seed,
        std::env::var_os("LATEXD_LIVE_MUTATION_OUTPUT").map(Into::into),
        ComparisonMode::LivePreview,
    )
    .await;
}
