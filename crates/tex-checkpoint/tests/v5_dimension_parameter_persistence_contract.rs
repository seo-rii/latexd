use camino::Utf8PathBuf;
use tex_checkpoint::{
    CHECKPOINT_VM_SEMANTIC_EPOCH, CheckpointPage, InputBoundaryCheckpoint, SnapshotAttachment,
    build_checkpoint_bundle_with_snapshots, checkpoint_is_replay_safe,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    DimensionParameterId, RawDimensionSp, VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY, Vm,
    VmDimensionParameterAssignmentV1, VmDimensionParameterStateV1, VmModuleCheckpointKind,
};

#[test]
fn dimension_state_changes_semantic_identity_but_stays_out_of_the_legacy_write_lane() {
    assert_eq!(CHECKPOINT_VM_SEMANTIC_EPOCH, 5);

    let mut interner = ControlSequenceInterner::new();
    let mut maximum = Vm::new(&mut interner).snapshot();
    maximum.dimension_parameter_state = Some(VmDimensionParameterStateV1 {
        layers: vec![vec![VmDimensionParameterAssignmentV1 {
            parameter: DimensionParameterId::HangIndent,
            value: RawDimensionSp::new(i32::MAX),
        }]],
    });
    assert!(maximum.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY
    }));
    let mut minimum = maximum.clone();
    minimum
        .dimension_parameter_state
        .as_mut()
        .expect("dimension state")
        .layers[0][0]
        .value = RawDimensionSp::new(i32::MIN);

    let pages = [CheckpointPage {
        page_id: "page-1".to_string(),
        index: 0,
        content_hash: "page-hash".to_string(),
        text_start_utf8: 0,
        text_end_utf8: 1,
    }];
    let maximum_boundaries = [InputBoundaryCheckpoint {
        kind: VmModuleCheckpointKind::Enter,
        module_path: Utf8PathBuf::from("chapter.tex"),
        resume_path: Some(Utf8PathBuf::from("main.tex")),
        source_offset_utf8: 7,
        continuation_stack: Vec::new(),
        output_start_utf8: 0,
        page_index_after: 1,
        snapshot: maximum.clone(),
    }];
    let minimum_boundaries = [InputBoundaryCheckpoint {
        snapshot: minimum.clone(),
        ..maximum_boundaries[0].clone()
    }];
    let maximum_bundle = build_checkpoint_bundle_with_snapshots(
        1,
        &maximum,
        "preamble",
        0,
        &pages,
        &[maximum.clone()],
        &[5],
        &maximum_boundaries,
    )
    .expect("build maximum dimension-state checkpoints");
    let minimum_bundle = build_checkpoint_bundle_with_snapshots(
        1,
        &minimum,
        "preamble",
        0,
        &pages,
        &[minimum.clone()],
        &[5],
        &minimum_boundaries,
    )
    .expect("build minimum dimension-state checkpoints");

    assert_eq!(maximum_bundle.checkpoints.len(), 3);
    assert_eq!(minimum_bundle.checkpoints.len(), 3);
    for (maximum_checkpoint, minimum_checkpoint) in maximum_bundle
        .checkpoints
        .iter()
        .zip(&minimum_bundle.checkpoints)
    {
        assert_ne!(
            maximum_checkpoint.meta.vm_state_hash,
            minimum_checkpoint.meta.vm_state_hash
        );
        assert_ne!(
            maximum_checkpoint.meta.checkpoint_id,
            minimum_checkpoint.meta.checkpoint_id
        );
        for checkpoint in [maximum_checkpoint, minimum_checkpoint] {
            assert!(!checkpoint.meta.snapshot_attached);
            assert!(matches!(
                checkpoint.snapshot_attachment(),
                SnapshotAttachment::None
            ));
            assert!(!checkpoint_is_replay_safe(checkpoint));
        }
    }
}
