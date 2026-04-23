#[tokio::test]
async fn external_oracle_reads_fls_inputs_for_custom_compiler() {
    run_external_oracle_custom_compiler_dependency_tracking(
        ExternalOracleCustomCompilerDependencyTrackingCase::FlsInputs,
    )
    .await;
}
