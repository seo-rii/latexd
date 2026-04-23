#[tokio::test]
async fn external_oracle_reads_fls_inputs_for_pdflatex_fallback() {
    run_external_oracle_pdf_latex_pdflatex_success(
        ExternalOraclePdfLatexPdflatexFlsCase::MainAndIntro,
    )
    .await;
}
