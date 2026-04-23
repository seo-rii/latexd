#[tokio::test]
async fn internal_compiler_reports_replaced_pages_before_stable_unchanged_tail_from_prior_input() {
    run_inserted_or_replaced_unchanged_tail_mutation(
        InsertedAndReplacedUnchangedTailMutationKind::Replaced,
    )
    .await;
}
