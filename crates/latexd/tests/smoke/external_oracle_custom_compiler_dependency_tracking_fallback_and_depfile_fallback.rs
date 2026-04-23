#[tokio::test]
async fn external_oracle_falls_back_to_toplevel_dep_trace_for_custom_compiler_without_fls() {
    run_external_oracle_custom_compiler_dependency_tracking(
        ExternalOracleCustomCompilerDependencyTrackingCase::FallbackWithoutFls,
    )
    .await;
}
