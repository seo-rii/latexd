#[tokio::test]
async fn internal_compiler_falls_back_to_preamble_replay_when_late_input_edit_is_followed_by_untracked_dirty_file()
 {
    run_replay_selection_input_boundaries_late_input_noise(
        ReplaySelectionLateInputNoiseDirtyKind::Untracked,
        ReplaySelectionLateInputNoiseDirtyOrder::FollowsLateInput,
    )
    .await;
}
