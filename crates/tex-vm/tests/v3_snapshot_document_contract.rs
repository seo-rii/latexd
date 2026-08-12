use serde_json::json;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotCapability, VM_SNAPSHOT_DOCUMENT_FORMAT, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION, Vm,
    VmRestoreError, VmSnapshot, VmSnapshotDocument, VmSnapshotDocumentError,
    VmSnapshotDocumentRestoreError, decode_vm_snapshot_document, normalize_legacy_vm_snapshot,
};

fn encoded_document(state: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": [],
        "state": state,
    }))
    .expect("serialize test snapshot document")
}

#[test]
fn legacy_snapshot_and_versioned_document_shapes_are_distinct() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let legacy_json = serde_json::to_vec(&vm.snapshot()).expect("serialize legacy snapshot");

    assert!(matches!(
        decode_vm_snapshot_document(&legacy_json),
        Err(VmSnapshotDocumentError::MalformedDocument(_))
    ));

    let document_json = encoded_document(
        serde_json::to_value(vm.snapshot()).expect("serialize snapshot document state"),
    );
    assert!(serde_json::from_slice::<VmSnapshot>(&document_json).is_err());
}

#[test]
fn legacy_snapshot_normalizer_preserves_state_without_claiming_capabilities() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\legacyword{L}");
    let legacy_json = serde_json::to_vec(&vm.snapshot()).expect("serialize legacy snapshot");
    let legacy =
        serde_json::from_slice::<VmSnapshot>(&legacy_json).expect("decode legacy snapshot");

    let document: VmSnapshotDocument = normalize_legacy_vm_snapshot(legacy.clone());

    assert_eq!(document.format, VM_SNAPSHOT_DOCUMENT_FORMAT);
    assert_eq!(document.schema_version, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION);
    assert!(document.required_capabilities.is_empty());
    assert_eq!(document.state, legacy);

    let future = SnapshotCapability::new("future.capability-v1");
    assert_eq!(future.as_str(), "future.capability-v1");
}

#[test]
fn versioned_document_decodes_legacy_state_for_exact_restore() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    source.run_plain(r"\def\snapshotword{R}");
    let document_json = encoded_document(
        serde_json::to_value(source.snapshot()).expect("serialize snapshot document state"),
    );
    drop(source);

    let document = decode_vm_snapshot_document(&document_json).expect("decode snapshot document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &document.state)
        .expect("restore document snapshot state");
    let outcome = restored.run_plain(r"\snapshotword");

    assert_eq!(outcome.output, "R");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn document_restore_is_transactional_across_header_and_state_validation() {
    let unsupported_capability = serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": ["eqtb.muskip.scalar-v1"],
        "state": "not a VM snapshot",
    }))
    .expect("serialize unsupported-capability document");
    let mut interner = ControlSequenceInterner::new();
    interner.intern("sentinel");
    let original_len = interner.len();

    let error = match Vm::try_restore_document(&mut interner, &unsupported_capability) {
        Ok(_) => panic!("unsupported capability must be rejected"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        VmSnapshotDocumentRestoreError::Document(VmSnapshotDocumentError::UnsupportedCapability(
            "eqtb.muskip.scalar-v1".to_string()
        ))
    );
    assert_eq!(interner.len(), original_len);

    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let mut rootless = serde_json::to_value(source.snapshot()).expect("serialize snapshot state");
    rootless["scopes"] = json!([]);
    let rootless_document = encoded_document(rootless);

    let error = match Vm::try_restore_document(&mut interner, &rootless_document) {
        Ok(_) => panic!("rootless document state must be rejected"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        VmSnapshotDocumentRestoreError::Restore(VmRestoreError::MissingRootControlSequenceScope)
    );
    assert_eq!(interner.len(), original_len);
}

#[test]
fn document_header_and_capabilities_are_validated_before_state() {
    let wrong_format = serde_json::to_vec(&json!({
        "format": "not-latexd",
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": [],
        "state": "not a VM snapshot",
    }))
    .expect("serialize wrong-format document");
    assert_eq!(
        decode_vm_snapshot_document(&wrong_format),
        Err(VmSnapshotDocumentError::UnsupportedFormat(
            "not-latexd".to_string()
        ))
    );

    let unsupported_schema = serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION + 1,
        "required_capabilities": [],
        "state": "not a VM snapshot",
    }))
    .expect("serialize unsupported-schema document");
    assert_eq!(
        decode_vm_snapshot_document(&unsupported_schema),
        Err(VmSnapshotDocumentError::UnsupportedSchemaVersion(
            VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION + 1
        ))
    );

    let unsupported_capability = serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": ["eqtb.muskip.scalar-v1"],
        "state": "not a VM snapshot",
    }))
    .expect("serialize unsupported-capability document");
    assert_eq!(
        decode_vm_snapshot_document(&unsupported_capability),
        Err(VmSnapshotDocumentError::UnsupportedCapability(
            "eqtb.muskip.scalar-v1".to_string()
        ))
    );
}

#[test]
fn valid_document_header_reports_invalid_state_separately() {
    let document_json = encoded_document(json!("not a VM snapshot"));

    assert!(matches!(
        decode_vm_snapshot_document(&document_json),
        Err(VmSnapshotDocumentError::InvalidState(_))
    ));
}

#[test]
fn known_document_schema_rejects_undeclared_state_fields() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut state = serde_json::to_value(vm.snapshot()).expect("serialize snapshot state");
    state["muskip_registers"] = json!({"0": 123});
    let document_json = encoded_document(state);

    assert!(matches!(
        decode_vm_snapshot_document(&document_json),
        Err(VmSnapshotDocumentError::InvalidState(_))
    ));
}

#[test]
fn known_document_schema_rejects_unknown_document_fields() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let document_json = serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": [],
        "state": vm.snapshot(),
        "future_semantics": true,
    }))
    .expect("serialize document with an unknown field");

    assert!(matches!(
        decode_vm_snapshot_document(&document_json),
        Err(VmSnapshotDocumentError::MalformedDocument(_))
    ));
}
