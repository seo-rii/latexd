#[tokio::test]
async fn external_oracle_reports_unreadable_depfile_for_custom_compiler() {
    run_external_oracle_custom_compiler_dependency_tracking(
        ExternalOracleCustomCompilerDependencyTrackingCase::UnreadableDepfile,
    )
    .await;
}
