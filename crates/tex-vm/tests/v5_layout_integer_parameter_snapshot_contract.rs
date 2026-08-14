use serde_json::{Value, json};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    LayoutIntegerParameterId, SnapshotCapability, VM_SNAPSHOT_DOCUMENT_FORMAT,
    VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm,
    VmLayoutIntegerParameterAssignmentV1, VmLayoutIntegerParameterStateV1, VmRestoreError,
    VmSnapshot, VmSnapshotDocumentError, VmSnapshotDocumentRestoreError,
    decode_vm_snapshot_document,
};

const CONTRACT: &str = include_str!("fixtures/layout-integer-parameter-state-v1-contract.json");

fn versioned_state(snapshot: &VmSnapshot) -> Value {
    let mut state = serde_json::to_value(&**snapshot).expect("serialize legacy state projection");
    state["muskip_registers"] = json!(snapshot.muskip_registers);
    state["next_muskip_register"] = json!(snapshot.next_muskip_register);
    state
}

fn encoded_document(required_capabilities: &[&str], state: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": required_capabilities,
        "state": state,
    }))
    .expect("serialize test snapshot document")
}

fn canonical_document() -> Vec<u8> {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let mut state = versioned_state(&snapshot);
    state["layout_integer_parameter_state"] =
        contract["document_envelope"]["layout_integer_parameter_state"].clone();
    encoded_document(
        &[VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY],
        state,
    )
}

#[test]
fn passive_reader_decodes_but_cannot_rewrite_restore_or_use_the_legacy_wire() {
    assert!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES
            .contains(&VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert!(
        !VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES
            .contains(&VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );

    let encoded = canonical_document();
    let decoded = decode_vm_snapshot_document(&encoded).expect("decode passive layout state");
    assert_eq!(
        decoded.state.layout_integer_parameter_state,
        Some(VmLayoutIntegerParameterStateV1 {
            layers: vec![vec![
                VmLayoutIntegerParameterAssignmentV1 {
                    parameter: LayoutIntegerParameterId::AdjDemerits,
                    value: 123,
                },
                VmLayoutIntegerParameterAssignmentV1 {
                    parameter: LayoutIntegerParameterId::HangAfter,
                    value: 2,
                },
            ]],
        })
    );
    assert_eq!(
        decoded.required_capabilities,
        [SnapshotCapability::new(
            VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
        )]
        .into_iter()
        .collect()
    );

    let mut legacy_output = b"sentinel".to_vec();
    let legacy_error = serde_json::to_writer(&mut legacy_output, &decoded.state)
        .expect_err("passive layout state must not enter the legacy wire");
    assert!(
        legacy_error
            .to_string()
            .contains(VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert_eq!(legacy_output, b"sentinel");

    let mut document_output = b"sentinel".to_vec();
    let rewrite_error = serde_json::to_writer(&mut document_output, &decoded)
        .expect_err("passive layout state must not be rewritten before owner support");
    assert!(
        rewrite_error
            .to_string()
            .contains(VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert_eq!(document_output, b"sentinel");

    let mut restore_interner = ControlSequenceInterner::new();
    restore_interner.intern("sentinel");
    let original_len = restore_interner.len();
    let restore_error = match Vm::try_restore_document(&mut restore_interner, &encoded) {
        Ok(_) => panic!("passive layout state must not be executable"),
        Err(error) => error,
    };
    assert_eq!(
        restore_error,
        VmSnapshotDocumentRestoreError::Restore(
            VmRestoreError::UnsupportedLayoutIntegerParameterState
        )
    );
    assert_eq!(restore_interner.len(), original_len);
}

#[test]
fn layout_integer_parameter_v1_wire_contract_is_exact_and_separate_from_tolerance() {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let ids = [
        LayoutIntegerParameterId::AdjDemerits,
        LayoutIntegerParameterId::BinOpPenalty,
        LayoutIntegerParameterId::BrokenPenalty,
        LayoutIntegerParameterId::ClubPenalty,
        LayoutIntegerParameterId::DisplayWidowPenalty,
        LayoutIntegerParameterId::DoubleHyphenDemerits,
        LayoutIntegerParameterId::ExHyphenPenalty,
        LayoutIntegerParameterId::FinalHyphenDemerits,
        LayoutIntegerParameterId::HangAfter,
        LayoutIntegerParameterId::HyphenPenalty,
        LayoutIntegerParameterId::InterlinePenalty,
        LayoutIntegerParameterId::LinePenalty,
        LayoutIntegerParameterId::Looseness,
        LayoutIntegerParameterId::PostDisplayPenalty,
        LayoutIntegerParameterId::PreDisplayPenalty,
        LayoutIntegerParameterId::PreTolerance,
        LayoutIntegerParameterId::RelPenalty,
        LayoutIntegerParameterId::WidowPenalty,
    ];
    let actual_ids = ids
        .into_iter()
        .map(|id| serde_json::to_value(id).expect("serialize layout parameter ID"))
        .collect::<Vec<_>>();

    assert_eq!(
        contract["capability"],
        VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    );
    assert_eq!(contract["allowed_ids"], json!(actual_ids));
    assert_eq!(contract["root_default_encoding"], "omitted");
    assert_eq!(contract["future_ids_require"], "new-capability-version");
    assert!(
        contract["allowed_ids"]
            .as_array()
            .expect("allowed ID array")
            .iter()
            .all(|id| id != "tolerance")
    );
    assert_eq!(
        contract["document_envelope"]["required_capabilities"],
        json!([VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY])
    );
    decode_vm_snapshot_document(&canonical_document()).expect("decode frozen sample envelope");
}

#[test]
fn passive_reader_rejects_noncanonical_layout_integer_parameter_state() {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let legacy = versioned_state(&source.snapshot());
    let mut cases = vec![
        ("empty state", json!({"layers": [[]]})),
        ("missing root layer", json!({"layers": []})),
        (
            "duplicate parameter",
            json!({"layers": [[
                {"parameter": "pretolerance", "value": 1},
                {"parameter": "pretolerance", "value": 2}
            ]]}),
        ),
        (
            "out-of-order parameter",
            json!({"layers": [[
                {"parameter": "hangafter", "value": 2},
                {"parameter": "adjdemerits", "value": 1}
            ]]}),
        ),
        (
            "unknown parameter",
            json!({"layers": [[{"parameter": "tolerance", "value": 1}]]}),
        ),
        (
            "unknown assignment field",
            json!({"layers": [[{
                "parameter": "pretolerance",
                "value": 1,
                "future": true
            }]]}),
        ),
        (
            "unknown state field",
            json!({
                "layers": [[{"parameter": "pretolerance", "value": 1}]],
                "future": true
            }),
        ),
        (
            "integer above the snapshot domain",
            json!({"layers": [[{
                "parameter": "pretolerance",
                "value": 2_147_483_648_i64
            }]]}),
        ),
        (
            "layer count beyond scope depth",
            json!({"layers": [[], [{"parameter": "pretolerance", "value": 1}]]}),
        ),
    ];
    for (name, default) in contract["defaults"].as_object().expect("default map") {
        cases.push((
            "redundant root default",
            json!({"layers": [[{"parameter": name, "value": default}]]}),
        ));
    }

    for (case, parameter_state) in cases {
        let mut state = legacy.clone();
        state["layout_integer_parameter_state"] = parameter_state;
        assert!(
            decode_vm_snapshot_document(&encoded_document(
                &[VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY],
                state,
            ))
            .is_err(),
            "accepted {case}"
        );
    }
}

#[test]
fn passive_reader_requires_exact_layout_capability_state_equality() {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let legacy = versioned_state(&snapshot);
    let parameter_state = contract["document_envelope"]["layout_integer_parameter_state"].clone();

    let mut missing_capability = legacy.clone();
    missing_capability["layout_integer_parameter_state"] = parameter_state;
    assert!(
        decode_vm_snapshot_document(&encoded_document(&[], missing_capability)).is_err(),
        "accepted layout state without its capability"
    );
    assert!(
        decode_vm_snapshot_document(&encoded_document(
            &[VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY],
            legacy,
        ))
        .is_err(),
        "accepted layout capability without state"
    );
}

#[test]
fn capability_free_snapshot_keeps_exact_legacy_shape_and_omits_layout_state() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let legacy_projection =
        serde_json::to_vec(&*snapshot).expect("serialize exact legacy projection");
    let legacy_snapshot = serde_json::to_vec(&snapshot).expect("serialize legacy snapshot");
    let state = versioned_state(&snapshot);

    assert_eq!(legacy_snapshot, legacy_projection);
    assert!(state.get("layout_integer_parameter_state").is_none());
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .all(|capability| capability.as_str()
                != VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
}

#[test]
fn unknown_layout_capability_remains_an_unsupported_document_error() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let error = decode_vm_snapshot_document(&encoded_document(
        &["eqtb.layout-integer-parameter-state.v2"],
        versioned_state(&snapshot),
    ))
    .expect_err("unknown future layout capability must fail closed");

    assert_eq!(
        error,
        VmSnapshotDocumentError::UnsupportedCapability(
            "eqtb.layout-integer-parameter-state.v2".to_string()
        )
    );
}
