struct ShiftedUnchangedTailMutationRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    first: CompileOutcome,
    second: CompileOutcome,
    build_meta: BuildMeta,
    original_source: String,
    current_source: String,
}

enum ShiftedUnchangedTailMutationCase {
    TailAndPatches,
    BuildMetaAndCheckpoints,
}

type ShiftTail = ShiftedUnchangedTailMutationCase;

async fn run_shift_tail(case: ShiftTail) {
    run_shifted_unchanged_tail_mutation(case).await;
}

async fn compile_shifted_unchanged_tail_mutation() -> ShiftedUnchangedTailMutationRun {
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
    fs::write(root.join("article.cls"), "").expect("write class");
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        original_body
    );
    fs::write(root.join("main.tex"), &original_source).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let inserted_page = (0..384)
        .map(|index| format!("x{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let shifted_body = format!(
        "{} {} {}",
        (0..384)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" "),
        inserted_page,
        (384..1536)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let current_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        shifted_body
    );
    fs::write(root.join("main.tex"), &current_source).expect("rewrite main tex");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second build should succeed");

    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");

    ShiftedUnchangedTailMutationRun {
        _tempdir: tempdir,
        build_root,
        first,
        second,
        build_meta,
        original_source,
        current_source,
    }
}

fn assert_shifted_unchanged_tail_build_meta(run: &ShiftedUnchangedTailMutationRun) {
    assert!(!run.build_meta.aux_sensitive);
    assert_eq!(
        run.build_meta.dirty_files,
        vec![Utf8PathBuf::from("main.tex")]
    );
    assert_eq!(
        run.build_meta.start_checkpoint_id,
        run.second.reused_checkpoint_id
    );
    assert_eq!(run.build_meta.start_page_index, 0);
    assert_eq!(run.build_meta.page_count, run.second.page_metadata.len());
    assert_eq!(run.build_meta.rebuilt_page_count, 1);
    assert_eq!(
        run.build_meta.reused_page_count,
        run.second.page_metadata.len() - 1
    );
    assert_eq!(run.build_meta.semantic_pass_count, 0);
    assert_eq!(run.build_meta.semantic_rerun_count, 0);
    assert!(run.build_meta.semantic_fixpoint_reached);
    assert!(!run.build_meta.semantic_aux_backdated);
    assert_eq!(
        run.build_meta.rebuilt_page_count + run.build_meta.reused_page_count,
        run.build_meta.page_count
    );
}

async fn run_shifted_unchanged_tail_mutation(case: ShiftedUnchangedTailMutationCase) {
    let run = compile_shifted_unchanged_tail_mutation().await;

    assert_shifted_unchanged_tail_build_meta(&run);
    match case {
        ShiftedUnchangedTailMutationCase::TailAndPatches => {
            assert_page_patches_transform(
                &run.first
                    .renderer_page_metadata
                    .iter()
                    .map(|page| page.page_id.clone())
                    .collect::<Vec<_>>(),
                &run.second.page_patches,
                &run.second
                    .renderer_page_metadata
                    .iter()
                    .map(|page| page.page_id.clone())
                    .collect::<Vec<_>>(),
            );
            assert_renderer_page_artifact_reuse(&run.first, &run.second, 1, 2);
            assert_eq!(run.first.page_metadata.len(), 4);
            let tail = run.second.unchanged_tail.expect("unchanged tail");
            assert_eq!(tail.previous_rev, 1);
            assert_eq!(tail.previous_page_start, 1);
            assert_eq!(tail.current_page_start, 2);
            assert_eq!(tail.page_count, 3);
            assert!(!run.second.page_patches.is_empty());
            assert!(run.second.page_artifacts.iter().any(|page| {
                page.pdf_url.starts_with("/artifacts/rev/2/pages/")
            }));
        }
        ShiftedUnchangedTailMutationCase::BuildMetaAndCheckpoints => {
            let tail = run.second.unchanged_tail.as_ref().expect("unchanged tail");
            let first_checkpoints =
                load_checkpoint_bundle(&run.build_root.join("rev-1/checkpoints.json"))
                    .expect("load rev1 checkpoints");
            let second_checkpoints =
                load_checkpoint_bundle(&run.build_root.join("rev-2/checkpoints.json"))
                    .expect("load rev2 checkpoints");
            let source_delta = run.current_source.len() as i64 - run.original_source.len() as i64;
            let shared_prefix = run
                .original_source
                .bytes()
                .zip(run.current_source.bytes())
                .take_while(|(left, right)| left == right)
                .count();
            macro_rules! shipout_checkpoint {
                ($bundle:expr, $page_index:expr) => {
                    $bundle
                        .checkpoints
                        .iter()
                        .find(|checkpoint| {
                            checkpoint.meta.kind == CheckpointKind::Shipout
                                && checkpoint.meta.page_index_after == $page_index + 1
                        })
                        .expect("shipout checkpoint")
                };
            }

            for offset in 0..tail.page_count {
                let previous_page_index = tail.previous_page_start + offset;
                let current_page_index = tail.current_page_start + offset;
                let previous_checkpoint =
                    shipout_checkpoint!(first_checkpoints, previous_page_index);
                let current_checkpoint =
                    shipout_checkpoint!(second_checkpoints, current_page_index);
                let mut rebased_offset = previous_checkpoint.meta.source_offset_utf8 as usize;
                if rebased_offset > shared_prefix {
                    rebased_offset = (rebased_offset as i64 + source_delta)
                        .clamp(0, run.current_source.len() as i64)
                        as usize;
                } else {
                    rebased_offset = rebased_offset.min(run.current_source.len());
                }
                let page_floor = run.second.page_metadata[..=current_page_index]
                    .iter()
                    .flat_map(|page| page.source_spans.iter())
                    .filter(|span| span.file == Utf8PathBuf::from("main.tex"))
                    .map(|span| span.end_utf8 as usize)
                    .max()
                    .unwrap_or_default();
                assert_eq!(
                    current_checkpoint.meta.source_offset_utf8 as usize,
                    rebased_offset.max(page_floor)
                );
            }
        }
    }
}
