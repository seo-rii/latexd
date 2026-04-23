#[tokio::test]
async fn external_oracle_surfaces_dvips_warnings_as_diagnostics() {
    run_external_oracle_custom_compiler_pipeline_warning(ExternalOraclePipelineWarningCase::Dvips)
        .await;
}
