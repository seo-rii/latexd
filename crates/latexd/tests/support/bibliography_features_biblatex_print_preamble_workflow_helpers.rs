struct SplitPreambleBiblatexPaperFamilyWorkflowRun {
    _tempdir: tempfile::TempDir,
    final_outcome: CompileOutcome,
    output: String,
    build_meta: BuildMeta,
    stored_sources: StoredSources,
    previous_sources: StoredSources,
}

enum SplitPreambleBiblatexPaperFamilyWorkflowCase {
    RenderOutput,
    BuildMeta,
}

async fn run_split_preamble_biblatex_paper_family_workflow()
-> SplitPreambleBiblatexPaperFamilyWorkflowRun {
    let tempdir = tempdir().expect("tempdir");
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/split-preamble-biblatex-paper-family-workflow");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    copy_test_fixture_tree(&fixture_root, &root);

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let mut final_outcome = None;

    for rev in 1..=8u64 {
        let mut changed_files = Vec::new();
        if rev > 1 {
            let overlay_root = fixture_root.join(format!("rev{rev}"));
            if overlay_root.exists() {
                let mut overlay_dirs = vec![overlay_root.clone()];
                while let Some(source_dir) = overlay_dirs.pop() {
                    for entry in fs::read_dir(source_dir.as_std_path())
                        .expect("read overlay dir")
                        .filter_map(|entry| entry.ok())
                    {
                        let source_path = Utf8PathBuf::from_path_buf(entry.path())
                            .expect("overlay path should be utf8");
                        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                            overlay_dirs.push(source_path);
                            continue;
                        }
                        let relative_path = source_path
                            .strip_prefix(&overlay_root)
                            .expect("overlay path should be relative to overlay root");
                        let target_path = root.join(relative_path);
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent.as_std_path()).expect("create overlay dir");
                        }
                        fs::copy(source_path.as_std_path(), target_path.as_std_path())
                            .expect("copy overlay file");
                        changed_files.push(relative_path.to_owned());
                    }
                }
            }

            let delete_path = fixture_root.join(format!("REV{rev}-DELETE.txt"));
            if delete_path.exists() {
                let deletes =
                    fs::read_to_string(delete_path.as_std_path()).expect("read delete list");
                for relative_path in deletes
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    let relative_path = Utf8PathBuf::from(relative_path);
                    let target_path = root.join(&relative_path);
                    if target_path.exists() {
                        if target_path.as_std_path().is_dir() {
                            fs::remove_dir_all(target_path.as_std_path())
                                .expect("remove directory");
                        } else {
                            fs::remove_file(target_path.as_std_path()).expect("remove file");
                        }
                    }
                    changed_files.push(relative_path);
                }
            }
            changed_files.sort();
        } else {
            changed_files.push(Utf8PathBuf::from("main.tex"));
        }

        let outcome = driver
            .compile(CompileRequest {
                root: root.clone(),
                manifest: world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev,
                build_root: build_root.clone(),
                changed_files,
            })
            .await
            .unwrap_or_else(|error| panic!("rev {rev} should succeed: {error:?}"));
        if rev == 8 {
            final_outcome = Some(outcome);
        }
    }

    let final_outcome = final_outcome.expect("rev 8 outcome");
    let output = fs::read_to_string(build_root.join("rev-8/output.txt")).expect("read output");
    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-8/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let previous_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-7/sources.json")).expect("read previous sources"),
    )
    .expect("parse previous sources");
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-8/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");

    SplitPreambleBiblatexPaperFamilyWorkflowRun {
        _tempdir: tempdir,
        final_outcome,
        output,
        build_meta,
        stored_sources,
        previous_sources,
    }
}

async fn run_split_preamble_biblatex_paper_family_workflow_case(
    case: SplitPreambleBiblatexPaperFamilyWorkflowCase,
) {
    let run = run_split_preamble_biblatex_paper_family_workflow().await;
    match case {
        SplitPreambleBiblatexPaperFamilyWorkflowCase::RenderOutput => {
            let main_tex = Utf8PathBuf::from("main.tex");
            let executed_main = &run.stored_sources.executed_files[&main_tex];
            let expected_render = "See Section 1, Figure 1, and Observation Lemma A; compare Alpha (2025) and (see Beta et al., 2023, pp.~1--2).";
            let stale_duplicated_tail = "References and Observation Lemma A; compare Alpha (2025) and (see Beta et al., 2023, pp.~1--2). References";
            let stale_degraded_cite = "and (see beta).";
            let debug_context = format!(
                "compiler output: {}\nexecuted main: {}\nprevious spans: {:?}\ncurrent spans: {:?}",
                run.output,
                executed_main,
                run.previous_sources.rewrite_spans.get(&main_tex),
                run.stored_sources.rewrite_spans.get(&main_tex)
            );

            assert!(
                executed_main.contains(expected_render),
                "executed main: {executed_main}"
            );
            assert!(
                !executed_main.contains(stale_degraded_cite),
                "executed main should not retain stale degraded cite fallback: {executed_main}"
            );
            assert!(run.output.contains(expected_render), "{debug_context}");
            assert!(
                !run.output.contains(stale_duplicated_tail),
                "{debug_context}"
            );
            assert!(!run.output.contains(stale_degraded_cite), "{debug_context}");
        }
        SplitPreambleBiblatexPaperFamilyWorkflowCase::BuildMeta => {
            assert!(run.build_meta.aux_sensitive);
            assert_eq!(
                run.build_meta.dirty_files,
                vec![Utf8PathBuf::from("refs-b.bbl")]
            );
            assert!(!run.build_meta.semantic_aux_backdated);
            assert!(run.build_meta.semantic_fixpoint_reached);
            assert_eq!(run.build_meta.semantic_pass_count, 2);
            assert_eq!(run.build_meta.semantic_rerun_count, 1);
            assert_eq!(
                run.build_meta.page_count,
                run.final_outcome.page_metadata.len()
            );
            assert_eq!(
                run.build_meta.rebuilt_page_count + run.build_meta.reused_page_count,
                run.build_meta.page_count
            );
        }
    }
}

type BibPrintWorkflow = SplitPreambleBiblatexPaperFamilyWorkflowCase;

async fn run_bib_print_workflow(case: BibPrintWorkflow) {
    run_split_preamble_biblatex_paper_family_workflow_case(case).await;
}
