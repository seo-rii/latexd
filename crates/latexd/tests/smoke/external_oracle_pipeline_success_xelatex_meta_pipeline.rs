#[tokio::test]
async fn external_oracle_supports_xelatex_pipeline() {
    run_external_oracle_xelatex_meta_success(ExternalOracleXelatexMetaSuccessCase::Pipeline).await;
}
