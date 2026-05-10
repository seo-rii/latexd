struct DeletedUnchangedTailMutationRun {
    fixture: UnchangedTailMutationFixture,
    second: CompileOutcome,
    build_meta: BuildMeta,
    deleted_pages: Vec<usize>,
}

enum DeletedUnchangedTailMutationCase {
    TailAndPatches,
    BuildMeta,
}

async fn compile_deleted_unchanged_tail_mutation() -> DeletedUnchangedTailMutationRun {
    let fixture = prepare_unchanged_tail_mutation_fixture().await;
    let root = fixture.root.clone();
    let build_root = fixture.build_root.clone();
    let shrink_path = fixture.shrink_path.clone();
    let deleted_pages = fixture.shrink_only_pages.clone();
    let first_tail_page = fixture.first_tail_page;

    let _stable_tail_start = fixture
        .first
        .page_metadata
        .iter()
        .skip(first_tail_page)
        .find(|page| {
            page.source_spans
                .iter()
                .all(|span| span.file == fixture.tail_path)
        })
        .map(|page| page.index)
        .unwrap_or_else(|| {
            panic!(
                "expected a pure tail page after the mixed tail boundary and a shrink-only suffix before it, saw {:?}",
                &fixture.page_files
            )
        });

    let (delete_start, delete_end) =
        shrink_span_range(&fixture.first, &fixture.shrink_path, &deleted_pages);
    let shrunk_source = format!(
        "{}{}",
        &fixture.shrink_source[..delete_start],
        &fixture.shrink_source[delete_end..]
    );
    fs::write(root.join("sections/shrink.tex"), &shrunk_source).expect("rewrite shrink input");

    let second = fixture
        .driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![shrink_path],
        })
        .await
        .expect("second build should succeed");
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");

    DeletedUnchangedTailMutationRun {
        fixture,
        second,
        build_meta,
        deleted_pages,
    }
}

async fn run_deleted_unchanged_tail_mutation(case: DeletedUnchangedTailMutationCase) {
    let run = compile_deleted_unchanged_tail_mutation().await;

    let tail = run.second.unchanged_tail.as_ref().expect("unchanged tail");
    match case {
        DeletedUnchangedTailMutationCase::TailAndPatches => {
            assert_eq!(tail.previous_rev, 1);
            assert_eq!(tail.previous_page_start, run.fixture.first_tail_page);
            assert_eq!(
                tail.current_page_start,
                run.fixture.first_tail_page - run.deleted_pages.len()
            );
            assert_eq!(
                tail.page_count,
                run.fixture.first.page_metadata.len() - run.fixture.first_tail_page
            );
            assert_eq!(
                tail.page_count,
                run.second.page_metadata.len() - tail.current_page_start
            );
            let delete_indexes = run
                .second
                .page_patches
                .iter()
                .filter_map(|patch| match patch {
                    PagePatchOp::DeletePage { index } => Some(*index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(delete_indexes.len(), run.deleted_pages.len());
            assert_eq!(
                delete_indexes,
                (tail.current_page_start..run.fixture.first_tail_page)
                    .rev()
                    .collect::<Vec<_>>()
            );
            assert!(
                !run.second
                    .page_patches
                    .iter()
                    .any(|patch| matches!(patch, PagePatchOp::InsertPage { .. }))
            );
            for offset in 0..tail.page_count {
                assert_eq!(
                    run.second.page_metadata[tail.current_page_start + offset].page_id,
                    run.fixture.first.page_metadata[run.fixture.first_tail_page + offset].page_id
                );
                assert!(
                    run.second.page_artifacts[tail.current_page_start + offset]
                        .pdf_url
                        .starts_with("/artifacts/rev/1/pages/")
                );
            }
        }
        DeletedUnchangedTailMutationCase::BuildMeta => {
            assert!(!run.build_meta.aux_sensitive);
            assert_eq!(
                run.build_meta.dirty_files,
                vec![run.fixture.shrink_path.clone()]
            );
            assert_eq!(
                run.build_meta.start_checkpoint_id,
                run.second.reused_checkpoint_id
            );
            assert_eq!(run.build_meta.page_count, run.second.page_metadata.len());
            assert_eq!(
                run.build_meta.rebuilt_page_count + run.build_meta.reused_page_count,
                run.build_meta.page_count
            );
            assert!(run.build_meta.reused_page_count >= tail.page_count);
            assert!(run.build_meta.start_page_index <= tail.current_page_start);
            assert_eq!(run.build_meta.semantic_pass_count, 0);
            assert_eq!(run.build_meta.semantic_rerun_count, 0);
            assert!(run.build_meta.semantic_fixpoint_reached);
            assert!(!run.build_meta.semantic_aux_backdated);
        }
    }
}
