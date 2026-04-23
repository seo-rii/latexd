#[tokio::test]
async fn internal_compiler_falls_back_to_preamble_replay_when_unreadable_dirty_file_precedes_late_input_edit()
 {
    run_replay_selection_input_boundaries_late_input_noise(
        ReplaySelectionLateInputNoiseDirtyKind::Unreadable,
        ReplaySelectionLateInputNoiseDirtyOrder::PrecedesLateInput,
    )
    .await;
}
