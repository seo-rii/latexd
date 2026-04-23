#[tokio::test]
async fn external_oracle_retains_toplevel_in_dep_trace_when_depfile_omits_main() {
    run_external_oracle_pdf_latex_tectonic_success(
        ExternalOraclePdfLatexTectonicDepfileCase::IntroOnly,
    )
    .await;
}
