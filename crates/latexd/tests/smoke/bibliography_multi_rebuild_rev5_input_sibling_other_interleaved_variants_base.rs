#[tokio::test]
async fn internal_compiler_rebuilds_from_base_snapshot_for_semantic_multi_bibliography_edit_even_with_later_tracked_input_change_in_fixture_branch_when_unchanged_sibling_bibliography_is_also_dirty_with_other_interleaved_dirty_order()
 {
    run_tracked_tail_input_sibling_dirty_case_with_dirty_files([
        "sections/tail.tex",
        "refsa.bbl",
        "refsb.bbl",
    ])
    .await;
}
