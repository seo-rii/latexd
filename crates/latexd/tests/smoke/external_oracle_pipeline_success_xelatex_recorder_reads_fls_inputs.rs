#[tokio::test]
async fn external_oracle_reads_fls_inputs_for_xelatex() {
    run_external_oracle_xelatex_recorder_success(
        ExternalOracleXelatexRecorderFlsCase::MainAndIntro,
    )
    .await;
}
