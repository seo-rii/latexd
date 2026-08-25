use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotCapability, VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES, VM_SNAPSHOT_MAGNIFICATION_STATE_V1_CAPABILITY, Vm,
    VmMagnificationStateV1, VmRestoreError, VmSnapshotDocument, VmSnapshotDocumentError,
    decode_vm_snapshot_document,
};

fn canonical_magnification_document() -> String {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.magnification_state = Some(VmMagnificationStateV1 {
        requested_layers: vec![Some(2_000)],
        prepared_effective: Some(2_000),
    });
    serde_json::to_string(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("serialize canonical magnification document")
}

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

#[test]
fn missing_root_scope_wins_before_magnification_validation_and_interner_mutation() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let mut snapshot = source.snapshot();
    snapshot.scopes.clear();
    snapshot.magnification_state = Some(VmMagnificationStateV1 {
        requested_layers: vec![],
        prepared_effective: Some(1_000),
    });

    let mut restore_interner = ControlSequenceInterner::new();
    restore_interner.intern("sentinel");
    let restore_len = restore_interner.len();
    assert!(matches!(
        Vm::try_restore(&mut restore_interner, &snapshot),
        Err(VmRestoreError::MissingRootControlSequenceScope)
    ));
    assert_eq!(restore_interner.len(), restore_len);
}

#[test]
fn versioned_reader_rejects_unknown_nested_magnification_fields() {
    let encoded = canonical_magnification_document().replace(
        "\"prepared_effective\":2000",
        "\"prepared_effective\":2000,\"unexpected\":1",
    );

    assert!(matches!(
        decode_vm_snapshot_document(encoded.as_bytes()),
        Err(VmSnapshotDocumentError::InvalidState(error))
            if error.contains("unknown field") && error.contains("unexpected")
    ));
}

#[test]
fn versioned_reader_rejects_duplicate_nested_magnification_fields() {
    let encoded = canonical_magnification_document().replace(
        "\"prepared_effective\":2000",
        "\"prepared_effective\":0,\"prepared_effective\":2000",
    );

    assert!(matches!(
        decode_vm_snapshot_document(encoded.as_bytes()),
        Err(VmSnapshotDocumentError::InvalidState(error))
            if error.contains("duplicate field") && error.contains("prepared_effective")
    ));
}
