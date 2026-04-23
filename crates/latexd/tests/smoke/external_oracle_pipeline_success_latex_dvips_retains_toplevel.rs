#[tokio::test]
async fn external_oracle_retains_toplevel_in_dep_trace_when_latex_dvips_fls_omits_main() {
    run_external_oracle_latex_dvips_success(ExternalOracleLatexDvipsSuccessCase::IntroOnlyFls)
        .await;
}
