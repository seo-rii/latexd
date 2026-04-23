#[tokio::test]
async fn external_oracle_accumulates_warnings_across_pipeline_stages() {
    run_external_oracle_custom_compiler_pipeline_warning(
        ExternalOraclePipelineWarningCase::MultiStage,
    )
    .await;
}
