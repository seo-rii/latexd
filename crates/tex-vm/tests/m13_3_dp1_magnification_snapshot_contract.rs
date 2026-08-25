use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotCapability, VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES, VM_SNAPSHOT_MAGNIFICATION_STATE_V1_CAPABILITY, Vm,
    VmMagnificationStateV1, VmRestoreError, VmSnapshotDocument,
};

#[test]
fn magnification_state_round_trips_and_unwinds_without_grouping_the_latch() {
    assert!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES
            .contains(&VM_SNAPSHOT_MAGNIFICATION_STATE_V1_CAPABILITY)
    );
    assert!(
        VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES
            .contains(&VM_SNAPSHOT_MAGNIFICATION_STATE_V1_CAPABILITY)
    );

    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    assert!(source.run_plain("{{").diagnostics.is_empty());
    let mut snapshot = source.snapshot();
    assert_eq!(snapshot.magnification_state, None);
    snapshot.magnification_state = Some(VmMagnificationStateV1 {
        requested_layers: vec![Some(2_000), None, Some(1_000)],
        prepared_effective: Some(2_000),
    });
    let expected = snapshot.magnification_state.clone();
    assert_eq!(
        snapshot.required_capabilities(),
        [SnapshotCapability::new(
            VM_SNAPSHOT_MAGNIFICATION_STATE_V1_CAPABILITY,
        )]
        .into_iter()
        .collect()
    );

    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("serialize dormant magnification state");
    assert!(String::from_utf8_lossy(&document).contains("\"magnification_state\""));
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore dormant magnification state");
    assert_eq!(restored.snapshot().magnification_state, expected);

    assert!(restored.run_plain("}").diagnostics.is_empty());
    assert_eq!(
        restored.snapshot().magnification_state,
        Some(VmMagnificationStateV1 {
            requested_layers: vec![Some(2_000), None],
            prepared_effective: Some(2_000),
        })
    );
    assert!(restored.run_plain("}").diagnostics.is_empty());
    assert_eq!(
        restored.snapshot().magnification_state,
        Some(VmMagnificationStateV1 {
            requested_layers: vec![Some(2_000)],
            prepared_effective: Some(2_000),
        })
    );
}

#[test]
fn invalid_magnification_state_fails_before_interner_mutation() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    assert!(source.run_plain("{").diagnostics.is_empty());
    let base = source.snapshot();

    let invalid_states = [
        VmMagnificationStateV1 {
            requested_layers: vec![None],
            prepared_effective: Some(1_000),
        },
        VmMagnificationStateV1 {
            requested_layers: vec![None, None],
            prepared_effective: None,
        },
        VmMagnificationStateV1 {
            requested_layers: vec![Some(1_000), None],
            prepared_effective: None,
        },
        VmMagnificationStateV1 {
            requested_layers: vec![None, None],
            prepared_effective: Some(0),
        },
        VmMagnificationStateV1 {
            requested_layers: vec![None, None],
            prepared_effective: Some(32_769),
        },
    ];
    for state in invalid_states {
        let mut snapshot = base.clone();
        snapshot.magnification_state = Some(state);
        let mut restore_interner = ControlSequenceInterner::new();
        restore_interner.intern("sentinel");
        let restore_len = restore_interner.len();
        assert!(matches!(
            Vm::try_restore(&mut restore_interner, &snapshot),
            Err(VmRestoreError::InvalidMagnificationState(_))
        ));
        assert_eq!(restore_interner.len(), restore_len);
    }
}
