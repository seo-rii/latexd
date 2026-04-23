#[tokio::test]
async fn internal_compiler_replays_from_toplevel_input_exit_boundary() {
    run_replay_selection_input_boundaries_toplevel_exit(ReplaySelectionToplevelExitCase::Baseline)
        .await;
}
