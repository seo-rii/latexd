#[tokio::test]
async fn external_oracle_surfaces_stdout_warnings_as_diagnostics() {
    run_external_oracle_custom_compiler_warning_stream(
        ExternalOracleCustomCompilerWarningStream::Stdout,
    )
    .await;
}
