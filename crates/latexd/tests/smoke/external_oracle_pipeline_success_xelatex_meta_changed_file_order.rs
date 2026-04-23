#[tokio::test]
async fn external_oracle_preserves_changed_file_order_in_build_meta() {
    run_external_oracle_xelatex_meta_success(
        ExternalOracleXelatexMetaSuccessCase::ChangedFileOrder,
    )
    .await;
}
