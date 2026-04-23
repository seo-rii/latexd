#[tokio::test]
async fn external_oracle_writes_build_meta_with_nonreplay_defaults() {
    run_external_oracle_xelatex_meta_success(ExternalOracleXelatexMetaSuccessCase::BuildMeta).await;
}
