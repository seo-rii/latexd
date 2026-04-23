#[tokio::test]
async fn external_oracle_preserves_stderr_warnings_as_diagnostics() {
    run_external_oracle_custom_compiler_warning_stream(
        ExternalOracleCustomCompilerWarningStream::Stderr,
    )
    .await;
}
