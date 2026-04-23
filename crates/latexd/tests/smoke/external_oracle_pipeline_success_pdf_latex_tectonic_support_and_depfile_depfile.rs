#[tokio::test]
async fn external_oracle_reads_depfile_inputs_for_tectonic() {
    run_external_oracle_pdf_latex_tectonic_success(
        ExternalOraclePdfLatexTectonicDepfileCase::MainAndIntro,
    )
    .await;
}
