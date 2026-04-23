#[tokio::test]
async fn external_oracle_retains_toplevel_in_dep_trace_when_fls_omits_main() {
    run_external_oracle_xelatex_recorder_success(ExternalOracleXelatexRecorderFlsCase::IntroOnly)
        .await;
}
