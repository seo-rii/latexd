#[tokio::test]
async fn internal_compiler_clamps_toplevel_input_exit_replay_to_last_page() {
    run_replay_selection_input_boundaries_toplevel_exit(
        ReplaySelectionToplevelExitCase::ClampLastPage,
    )
    .await;
}
