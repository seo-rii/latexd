#[tokio::test]
async fn internal_compiler_still_replays_late_input_edit_when_untracked_dirty_file_precedes_it() {
    run_replay_selection_input_boundaries_late_input_noise(
        ReplaySelectionLateInputNoiseDirtyKind::Untracked,
        ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput,
    )
    .await;
}
