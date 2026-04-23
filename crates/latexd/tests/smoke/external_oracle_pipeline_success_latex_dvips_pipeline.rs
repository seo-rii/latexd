#[tokio::test]
async fn external_oracle_supports_latex_dvips_ps2pdf_pipeline() {
    run_external_oracle_latex_dvips_success(ExternalOracleLatexDvipsSuccessCase::MainOnlyPipeline)
        .await;
}
