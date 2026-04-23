#[tokio::test]
async fn external_oracle_supports_pdf_latex_pipeline_via_pdflatex_fallback() {
    run_external_oracle_pdf_latex_pdflatex_success(ExternalOraclePdfLatexPdflatexFlsCase::MainOnly)
        .await;
}
