#[tokio::test]
async fn internal_compiler_rebuilds_from_base_for_late_multi_bibliography_semantic_change_when_unreadable_dirty_file_precedes()
 {
    run_late_multi_root_case(LateMultiRootCase::UnreadablePrecedes).await;
}
