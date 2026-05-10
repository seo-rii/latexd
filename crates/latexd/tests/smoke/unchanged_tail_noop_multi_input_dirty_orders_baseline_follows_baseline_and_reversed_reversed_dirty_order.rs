#[tokio::test]
async fn internal_compiler_reuses_full_unchanged_tail_for_nonrendering_multi_input_edits_with_reversed_dirty_order()
 {
    run_input_base_follow_case(InputBaseFollowCase::Reversed).await;
}
