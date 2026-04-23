#[tokio::test]
async fn external_oracle_retains_toplevel_in_dep_trace_when_pdflatex_fls_omits_main() {
    run_external_oracle_pdf_latex_pdflatex_success(
        ExternalOraclePdfLatexPdflatexFlsCase::IntroOnly,
    )
    .await;
}
