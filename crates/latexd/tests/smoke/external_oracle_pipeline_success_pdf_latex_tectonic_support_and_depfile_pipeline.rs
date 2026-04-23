#[tokio::test]
async fn external_oracle_supports_pdf_latex_pipeline_via_tectonic() {
    run_external_oracle_pdf_latex_tectonic_success(
        ExternalOraclePdfLatexTectonicDepfileCase::MainOnly,
    )
    .await;
}
