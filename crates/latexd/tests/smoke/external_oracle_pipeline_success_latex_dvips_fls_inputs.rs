#[tokio::test]
async fn external_oracle_reads_fls_inputs_for_latex_dvips_ps2pdf() {
    run_external_oracle_latex_dvips_success(ExternalOracleLatexDvipsSuccessCase::MainAndIntroFls)
        .await;
}
