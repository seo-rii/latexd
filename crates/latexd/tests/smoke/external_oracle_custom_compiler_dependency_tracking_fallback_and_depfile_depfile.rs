#[tokio::test]
async fn external_oracle_prefers_depfile_over_fls_for_custom_compiler() {
    run_external_oracle_custom_compiler_dependency_tracking(
        ExternalOracleCustomCompilerDependencyTrackingCase::DepfilePreferred,
    )
    .await;
}
