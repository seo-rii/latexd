use super::*;
use serde_json::{Value, json};

fn fixture(comments: usize, word: &str) -> String {
    format!(
        "\\documentclass{{article}}\n\\input{{defs.sty}}\n\\begin{{document}}\n{}\
         \\section{{Opening}}\\label{{opening}}\nOpening {word}. Macro \\word.\n\
         \\input{{body.tex}}\nReference \\ref{{opening}} on page \\pageref{{opening}}.\n\
         Inline math $x^2+y^2$.\\end{{document}}\n",
        "% Source positions move; paint stays unchanged.\n".repeat(comments)
    )
}

fn write_fixture(root: &Utf8Path, comments: usize, word: &str) {
    fs::write(root.join("main.tex"), fixture(comments, word)).unwrap();
    fs::write(root.join("defs.sty"), "\\def\\word{alpha}\n").unwrap();
    fs::write(root.join("body.tex"), "Included content.\n").unwrap();
}

fn request(root: &Utf8Path, build_root: &Utf8Path, rev: u64) -> CompileRequest {
    CompileRequest {
        root: root.to_owned(),
        manifest: tex_world::ProjectManifest::discover(root).unwrap(),
        toplevel: "main.tex".into(),
        rev,
        build_root: build_root.to_owned(),
        changed_files: vec!["main.tex".into()],
    }
}

fn read_json(path: &Utf8Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

async fn publish(build_root: &Utf8Path, rev: u64, outcome: &CompileOutcome) {
    let mut live = crate::LivePreviewState::default();
    live.latest_pdf_path = Some(outcome.pdf_path.clone());
    live.page_metadata = outcome.renderer_page_metadata.clone();
    live.snapshot.apply_success(
        rev,
        outcome.diagnostics.clone(),
        viewer_prefixed_path(&format!("/artifacts/rev/{rev}/main.pdf")),
        outcome.renderer_page_metadata.len(),
        outcome
            .renderer_page_metadata
            .iter()
            .map(|page| page.page_id.clone())
            .collect(),
        outcome.page_artifacts.clone(),
    );
    crate::preview_persistence::prepare(build_root, &live)
        .await
        .unwrap()
        .commit()
        .unwrap();
}

async fn compare_pair(
    root: &Utf8Path,
    build_root: &Utf8Path,
    rev: u64,
    applied: u64,
) -> (CompileOutcome, Value) {
    let preview = CompilerDriver::new(Some("internal".into()), vec![])
        .with_preview_artifacts()
        .compile_for_preview(request(root, build_root, rev), applied)
        .await
        .unwrap();
    let clean_root = root.join(format!(".latexd/clean-{rev}"));
    let clean = CompilerDriver::new(Some("internal".into()), vec![])
        .compile(request(root, &clean_root, 1))
        .await
        .unwrap();
    assert_eq!(preview.diagnostics, clean.diagnostics);
    assert_eq!(preview.dep_trace, clean.dep_trace);
    assert_eq!(preview.page_metadata, clean.page_metadata);
    let preview_dir = build_root.join(format!("rev-{rev}"));
    let clean_dir = clean_root.join("rev-1");
    for name in [
        "render-ir/page-display-list.json",
        "page-syncmap.json",
        "sources.json",
        "aux.json",
        "semantic-index.json",
    ] {
        assert_eq!(
            read_json(&preview_dir.join(name)),
            read_json(&clean_dir.join(name)),
            "{name} at rev {rev}"
        );
    }
    for name in ["output.txt", "semantic.aux"] {
        assert_eq!(
            fs::read(preview_dir.join(name)).unwrap(),
            fs::read(clean_dir.join(name)).unwrap(),
            "{name} at rev {rev}"
        );
    }
    let preview_meta = read_json(&preview_dir.join("build-meta.json"));
    let clean_meta = read_json(&clean_dir.join("build-meta.json"));
    assert_eq!(
        preview_meta["checkpoint_write_outcome"]["status"],
        "not_attempted"
    );
    assert_eq!(
        preview_meta["checkpoint_attachment_counts"],
        json!({"none":0,"legacy":0,"versioned":0})
    );
    assert_eq!(
        preview_meta["trace"]["checkpoint_fallback"],
        "live_preview_cold_execution_output_reuse"
    );
    assert_eq!(preview_meta["trace"]["restored_prefix_source_bytes"], 0);
    assert!(preview_meta["trace"]["skipped_execution_tokens"].is_null());
    assert!(preview.reused_checkpoint_id.is_none());
    assert!(!preview_dir.join("checkpoints.json").exists());
    assert!(!preview_dir.join("checkpoint-write-outcome.json").exists());
    assert!(!preview.pdf_path.exists());
    assert!(clean.pdf_path.exists());
    assert_eq!(clean_meta["checkpoint_write_outcome"]["status"], "success");
    assert!(clean_dir.join("checkpoints.json").is_file());
    assert!(
        preview_meta["trace"]["vm_executions"].as_u64().unwrap()
            < clean_meta["trace"]["vm_executions"].as_u64().unwrap()
    );
    assert!(
        preview_meta["trace"]["dispatched_tokens"].as_u64().unwrap()
            < clean_meta["trace"]["dispatched_tokens"].as_u64().unwrap()
    );
    let observation =
        json!({"rev":rev,"preview":preview_meta["trace"],"eager_clean":clean_meta["trace"]});
    fs::remove_dir_all(clean_root).unwrap();
    (preview, observation)
}

#[tokio::test]
async fn live_preview_policy_preserves_twenty_revisions_sources_aux_and_artifact_owners() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let build_root = root.join(".latexd/live");
    let mut owner = None::<Utf8PathBuf>;
    let mut first_paint = None;
    for rev in 1..=20 {
        write_fixture(root, rev as usize % 3, "alpha");
        if rev == 12 {
            fs::remove_file(build_root.join(owner.as_ref().unwrap().with_extension("svg")))
                .unwrap();
        }
        let (preview, observation) = compare_pair(root, &build_root, rev, rev - 1).await;
        assert_eq!(preview.renderer_page_metadata.len(), 1);
        let page = &preview.renderer_page_metadata[0];
        if let Some(paint) = &first_paint {
            assert_eq!(&page.content_hash, paint);
        } else {
            first_paint = Some(page.content_hash.clone());
        }
        if rev == 1 || rev == 12 {
            assert_eq!(observation["preview"]["reused_output_pages"], 0);
            owner = Some(page.pdf_artifact_path.clone());
        } else {
            assert_eq!(observation["preview"]["reused_output_pages"], 1);
            assert_eq!(&page.pdf_artifact_path, owner.as_ref().unwrap());
            assert!(preview.page_patches.is_empty());
        }
        publish(&build_root, rev, &preview).await;
        crate::prune_old_build_revisions(&build_root, rev).await;
        assert!(
            fs::read(build_root.join(&page.pdf_artifact_path))
                .unwrap()
                .starts_with(b"%PDF-")
        );
        assert!(
            build_root
                .join(page.pdf_artifact_path.with_extension("svg"))
                .is_file()
        );
    }
}

#[tokio::test]
async fn live_preview_policy_uses_published_metadata_across_unpublished_failure_gap() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let build_root = root.join(".latexd/live");
    write_fixture(root, 0, "alpha");
    let (first, _) = compare_pair(root, &build_root, 1, 0).await;
    publish(&build_root, 1, &first).await;
    // A published manifest remains usable without any prior execution cache.
    for name in ["checkpoints.json", "sources.json", "page-metadata.json"] {
        fs::write(build_root.join(format!("rev-1/{name}")), b"{invalid").unwrap();
    }
    write_fixture(root, 1, "bravo");
    let (unpublished, _) = compare_pair(root, &build_root, 2, 1).await;
    assert_ne!(
        first.renderer_page_metadata[0].page_id,
        unpublished.renderer_page_metadata[0].page_id
    );
    fs::write(root.join("main.tex"), "\\errmessage{intentional failure}").unwrap();
    let failed = CompilerDriver::new(Some("internal".into()), vec![])
        .with_preview_artifacts()
        .compile_for_preview(request(root, &build_root, 3), 1)
        .await;
    assert!(failed.is_err());
    write_fixture(root, 2, "alpha");
    let (repaired, observation) = compare_pair(root, &build_root, 4, 1).await;
    assert_eq!(observation["preview"]["reused_output_pages"], 1);
    assert_eq!(repaired.page_artifacts, first.page_artifacts);
    assert!(repaired.page_patches.is_empty());
}

#[tokio::test]
#[ignore = "explicit short release live-preview/eager policy measurement"]
async fn live_preview_policy_release_measurement() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let build_root = root.join(".latexd/live");
    let mut rows = Vec::new();
    for rev in 1..=20 {
        let word = if (rev / 3) % 2 == 0 { "alpha" } else { "bravo" };
        write_fixture(root, rev as usize % 3, word);
        let (preview, observation) = compare_pair(root, &build_root, rev, rev - 1).await;
        publish(&build_root, rev, &preview).await;
        crate::prune_old_build_revisions(&build_root, rev).await;
        rows.push(observation);
    }
    let report = json!({
        "profile":"internal_mini_kernel_approximate", "comparisons":rows.len(), "mismatches":0,
        "preview_policy":"cold_execution_output_reuse_without_durable_checkpoints",
        "eager_policy":"independent_clean_with_debug_pdf_and_durable_checkpoints",
        "timing_scope":"compiler only; shared runner; not edit-to-visible", "rows":rows,
    });
    if let Some(path) = std::env::var_os("LATEXD_LIVE_POLICY_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("live preview policy: {} equivalent build pairs", rows.len());
}

#[tokio::test]
#[ignore = "explicit fetched CC0 release measurement; requires corpus and fresh evidence directories"]
async fn live_preview_policy_cc0_release_measurement() {
    let corpus = Utf8PathBuf::from(
        std::env::var("LATEXD_LIVE_CC0_CORPUS")
            .expect("set LATEXD_LIVE_CC0_CORPUS to the fetched corpus"),
    );
    let evidence = Utf8PathBuf::from(
        std::env::var("LATEXD_LIVE_CC0_EVIDENCE")
            .expect("set LATEXD_LIVE_CC0_EVIDENCE to a fresh directory"),
    );
    fs::create_dir(&evidence).expect("evidence directory must not already exist");
    let lock = read_json(&corpus.join("corpus-lock.json"));
    let report_path = evidence.join("report.json");
    let mut report = json!({
        "schema_version": 1,
        "status": "running",
        "support_status": "supported_approximate",
        "engine": "internal_mini_kernel",
        "timing_scope": "compiler only; shared runner; first live request then same-source driver request; no editor, transport, raster or viewport timing",
        "warm_request_scope": "source-identical CompileDriver request with published preview baseline; daemon no-op save filtering is bypassed; semantic execution remains cold",
        "live_policy": "cold_execution_output_reuse_without_durable_checkpoints",
        "eager_policy": "independent_clean_with_debug_pdf_and_durable_checkpoints",
        "corpus_lock": lock,
        "comparisons": [],
        "cases": [],
    });
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    for (id, version) in [("2508.10038", "v1"), ("2404.05196", "v2")] {
        let locked = report["corpus_lock"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["arxiv_id"] == id)
            .expect("locked corpus case")
            .clone();
        assert_eq!(locked["version"], version);
        assert_eq!(
            locked["license"],
            "http://creativecommons.org/publicdomain/zero/1.0/"
        );
        let toplevel = Utf8PathBuf::from(locked["toplevel"].as_str().unwrap());
        let source = corpus.join("sources").join(id);
        let case_dir = evidence.join(format!("{id}{version}"));
        let root = case_dir.join("source");
        fs::create_dir_all(&root).unwrap();
        // Copy the fetched inputs once, retaining every asset and entrypoint.
        // Both compiler modes see these exact bytes at the same source paths.
        let mut directories = vec![Utf8PathBuf::new()];
        let mut source_hashes = BTreeMap::new();
        while let Some(relative_dir) = directories.pop() {
            for entry in fs::read_dir(source.join(&relative_dir)).unwrap() {
                let entry = entry.unwrap();
                let name = entry
                    .file_name()
                    .into_string()
                    .expect("UTF-8 corpus filename");
                assert_ne!(
                    name, ".latexd",
                    "fetched corpus must contain no build cache"
                );
                let relative = relative_dir.join(name);
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    fs::create_dir_all(root.join(&relative)).unwrap();
                    directories.push(relative);
                } else {
                    assert!(
                        kind.is_file(),
                        "unexpected non-regular corpus input {relative}"
                    );
                    let bytes = fs::read(entry.path()).unwrap();
                    source_hashes
                        .insert(relative.clone(), blake3::hash(&bytes).to_hex().to_string());
                    fs::write(root.join(relative), bytes).unwrap();
                }
            }
        }
        // Match tests/arxiv_oracle.rs: arXiv's 00README.json schema is not
        // latexd's schema, so the corpus recipe supplies this local override.
        let generated_manifest = format!("compiler: pdf_latex\ntoplevel:\n  - {toplevel}\n");
        assert!(!root.join("00README.yaml").exists());
        fs::write(root.join("00README.yaml"), &generated_manifest).unwrap();
        source_hashes.insert(
            Utf8PathBuf::from("00README.yaml"),
            blake3::hash(generated_manifest.as_bytes())
                .to_hex()
                .to_string(),
        );
        let manifest = tex_world::ProjectManifest::discover(&root).unwrap();
        let live_root = case_dir.join("live");
        let clean_root = case_dir.join("eager-clean");
        let make_request = |build_root: &Utf8Path, rev| CompileRequest {
            root: root.clone(),
            manifest: manifest.clone(),
            toplevel: toplevel.clone(),
            rev,
            build_root: build_root.to_owned(),
            changed_files: vec![toplevel.clone()],
        };
        let live = CompilerDriver::new(Some("internal".into()), vec![]).with_preview_artifacts();
        let row_index = report["cases"].as_array().unwrap().len();
        report["cases"].as_array_mut().unwrap().push(json!({
            "locked_case": locked, "source_file_blake3": source_hashes,
            "generated_manifest_override": generated_manifest,
            "cold_scope": "fresh project and output roots; the two papers share a process and may share font/runtime caches",
            "status": "live_cold_running", "observations": [], "equality": [],
        }));
        fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        let mut outcomes = Vec::new();
        for rev in 1..=2 {
            let start = std::time::Instant::now();
            let outcome = live
                .compile_for_preview(make_request(&live_root, rev), rev - 1)
                .await
                .unwrap_or_else(|failure| panic!("{id} live revision {rev}: {}", failure.message));
            let wall_us = start.elapsed().as_micros();
            let revision_dir = live_root.join(format!("rev-{rev}"));
            let metadata = read_json(&revision_dir.join("build-meta.json"));
            assert_eq!(
                metadata["trace"]["checkpoint_fallback"],
                "live_preview_cold_execution_output_reuse"
            );
            assert_eq!(metadata["trace"]["full_pdf_exports"], 0);
            assert_eq!(metadata["trace"]["debug_artifact_exports"], 0);
            assert!(outcome.reused_checkpoint_id.is_none());
            assert!(!revision_dir.join("checkpoints.json").exists());
            assert!(!outcome.pdf_path.exists());
            let publish_start = std::time::Instant::now();
            publish(&live_root, rev, &outcome).await;
            let publish_us = publish_start.elapsed().as_micros();
            for page in &outcome.renderer_page_metadata {
                assert!(
                    fs::read(live_root.join(&page.pdf_artifact_path))
                        .unwrap()
                        .starts_with(b"%PDF-")
                );
                assert!(
                    live_root
                        .join(page.pdf_artifact_path.with_extension("svg"))
                        .is_file()
                );
            }
            report["cases"][row_index]["observations"].as_array_mut().unwrap().push(json!({
                "kind": if rev == 1 { "live_cold" } else { "live_same_source_request" },
                "compiler_wall_us": wall_us, "publish_wall_us": publish_us,
                "trace": metadata["trace"], "rendered_pages": outcome.renderer_page_metadata.len(),
                "page_artifact_owners": outcome.renderer_page_metadata.iter().map(|page| page.pdf_artifact_path.clone()).collect::<Vec<_>>(),
            }));
            report["cases"][row_index]["status"] = json!(if rev == 1 {
                "live_same_source_running"
            } else {
                "eager_clean_running"
            });
            fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
            outcomes.push(outcome);
        }
        assert_eq!(
            outcomes[0].page_artifacts, outcomes[1].page_artifacts,
            "same-source immutable artifact ownership for {id}"
        );
        assert!(outcomes[1].page_patches.is_empty());
        assert_eq!(
            report["cases"][row_index]["observations"][1]["trace"]["reused_output_pages"]
                .as_u64()
                .unwrap() as usize,
            outcomes[0].renderer_page_metadata.len()
        );
        let start = std::time::Instant::now();
        let eager = CompilerDriver::new(Some("internal".into()), vec![])
            .compile(make_request(&clean_root, 1))
            .await
            .unwrap_or_else(|failure| panic!("{id} independent eager: {}", failure.message));
        let wall_us = start.elapsed().as_micros();
        let clean_dir = clean_root.join("rev-1");
        let metadata = read_json(&clean_dir.join("build-meta.json"));
        report["cases"][row_index]["observations"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "kind": "independent_eager_clean", "compiler_wall_us": wall_us,
                "trace": metadata["trace"], "rendered_pages": eager.renderer_page_metadata.len(),
            }));
        for (index, outcome) in outcomes.iter().enumerate() {
            let revision_dir = live_root.join(format!("rev-{}", index + 1));
            let mut equality = BTreeMap::new();
            equality.insert("diagnostics", outcome.diagnostics == eager.diagnostics);
            equality.insert("dependency_readset", outcome.dep_trace == eager.dep_trace);
            equality.insert(
                "execution_page_metadata",
                outcome.page_metadata == eager.page_metadata,
            );
            for name in [
                "render-ir/page-display-list.json",
                "page-syncmap.json",
                "sources.json",
                "aux.json",
                "semantic-index.json",
            ] {
                equality.insert(
                    name,
                    read_json(&revision_dir.join(name)) == read_json(&clean_dir.join(name)),
                );
            }
            for name in ["output.txt", "semantic.aux"] {
                equality.insert(
                    name,
                    fs::read(revision_dir.join(name)).unwrap()
                        == fs::read(clean_dir.join(name)).unwrap(),
                );
            }
            report["cases"][row_index]["equality"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "live_rev": index + 1, "matches_clean": equality }));
        }
        for (relative, expected_hash) in source_hashes {
            assert_eq!(
                blake3::hash(&fs::read(root.join(relative)).unwrap())
                    .to_hex()
                    .as_str(),
                expected_hash,
                "compiler must not mutate source inputs"
            );
        }
        let all_match = report["cases"][row_index]["equality"]
            .as_array()
            .unwrap()
            .iter()
            .all(|comparison| {
                comparison["matches_clean"]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|value| value == true)
            });
        report["cases"][row_index]["status"] =
            json!(if all_match { "equivalent" } else { "mismatch" });
        fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        assert!(
            all_match,
            "{id} live/clean mismatch; all artifacts and exact comparison fields retained in {report_path}"
        );
    }
    report["status"] = json!("complete");
    report["comparisons"] = json!(4);
    report["mismatches"] = json!(0);
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("two CC0 papers: 4 live/clean comparisons, report {report_path}");
}

#[tokio::test]
async fn live_preview_policy_mode_switch_cannot_restore_an_older_eager_preamble() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let build_root = root.join(".latexd/build");
    let eager = CompilerDriver::new(Some("internal".into()), vec![]);
    write_fixture(root, 0, "alpha");
    let first = eager.compile(request(root, &build_root, 1)).await.unwrap();
    publish(&build_root, 1, &first).await;

    fs::write(root.join("defs.sty"), "\\def\\word{bravo}\n").unwrap();
    fs::write(
        root.join("main.tex"),
        fixture(1, "alpha").replace("{opening}", "{renamed}"),
    )
    .unwrap();
    let second = CompilerDriver::new(Some("internal".into()), vec![])
        .with_preview_artifacts()
        .compile_for_preview(request(root, &build_root, 2), 1)
        .await
        .unwrap();
    publish(&build_root, 2, &second).await;
    assert!(!build_root.join("rev-2/checkpoints.json").exists());
    // The durable policy barrier does not depend on execution-cache JSON.
    fs::write(build_root.join("rev-2/build-meta.json"), b"{invalid").unwrap();
    fs::write(build_root.join("rev-2/sources.json"), b"{invalid").unwrap();
    fs::write(
        root.join("main.tex"),
        fixture(2, "alpha").replace("{opening}", "{renamed}"),
    )
    .unwrap();
    let third = eager
        .compile_for_preview(request(root, &build_root, 3), 2)
        .await
        .unwrap();
    let clean_root = root.join(".latexd/independent");
    let clean = eager.compile(request(root, &clean_root, 1)).await.unwrap();
    assert!(third.reused_checkpoint_id.is_none());
    assert_eq!(third.page_artifacts, second.page_artifacts);
    assert_eq!(third.diagnostics, clean.diagnostics);
    assert_eq!(third.dep_trace, clean.dep_trace);
    for name in [
        "render-ir/events.json",
        "render-ir/document-ir.json",
        "render-ir/page-display-list.json",
        "page-syncmap.json",
        "sources.json",
        "aux.json",
        "semantic-index.json",
    ] {
        assert_eq!(
            read_json(&build_root.join(format!("rev-3/{name}"))),
            read_json(&clean_root.join(format!("rev-1/{name}"))),
            "{name}"
        );
    }
    assert_eq!(
        fs::read(build_root.join("rev-3/output.txt")).unwrap(),
        fs::read(clean_root.join("rev-1/output.txt")).unwrap()
    );
    let trace = &read_json(&build_root.join("rev-3/build-meta.json"))["trace"];
    assert_eq!(
        trace["checkpoint_fallback"],
        "previous_live_preview_has_no_execution_checkpoint"
    );
    assert_eq!(trace["reused_output_pages"], 1);
}
