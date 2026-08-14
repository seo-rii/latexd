use serde_json::json;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    IntegerParameterId, SnapshotCapability, SnapshotMeaning, VM_SNAPSHOT_DOCUMENT_FORMAT,
    VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES, VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    Vm, VmIntegerParameterAssignmentV1, VmIntegerParameterStateV1, VmRestoreError, VmSnapshot,
    VmSnapshotDocument, VmSnapshotDocumentError, VmSnapshotDocumentRestoreError,
    decode_vm_snapshot_document, normalize_legacy_vm_snapshot,
};

const MUSKIP_ALIAS_V1_CAPABILITY: &str = "eqtb.muskip.alias-v1";
const MUSKIP_SCALAR_V1_CAPABILITY: &str = "eqtb.muskip.scalar-v1";
const MATHCODE_TABLE_V1_CAPABILITY: &str = "eqtb.mathcode.table-v1";
const DELCODE_TABLE_V1_CAPABILITY: &str = "eqtb.delcode.table-v1";
const INTEGER_PARAMETER_STATE_V1_CAPABILITY: &str = "eqtb.integer-parameter-state.v1";

fn muskip_snapshot() -> VmSnapshot {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.muskip_registers.insert(17, 123);
    snapshot.next_muskip_register = 301;
    snapshot
}

fn encoded_document(state: serde_json::Value) -> Vec<u8> {
    encoded_document_with_capabilities(&[], state)
}

fn encoded_document_with_capabilities(
    required_capabilities: &[&str],
    state: serde_json::Value,
) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": required_capabilities,
        "state": state,
    }))
    .expect("serialize test snapshot document")
}

fn encoded_document_with_raw_state(required_capabilities: &[&str], state: &str) -> Vec<u8> {
    format!(
        r#"{{"format":{},"schema_version":{},"required_capabilities":{},"state":{state}}}"#,
        serde_json::to_string(VM_SNAPSHOT_DOCUMENT_FORMAT).expect("serialize document format"),
        VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        serde_json::to_string(required_capabilities).expect("serialize document capabilities"),
    )
    .into_bytes()
}

fn legacy_state_with_raw_fields(snapshot: &VmSnapshot, fields: &str) -> String {
    let mut state = serde_json::to_string(&**snapshot).expect("serialize legacy state projection");
    assert_eq!(state.pop(), Some('}'));
    state.push(',');
    state.push_str(fields);
    state.push('}');
    state
}

fn versioned_state(snapshot: &VmSnapshot) -> serde_json::Value {
    let mut state = serde_json::to_value(&**snapshot).expect("serialize legacy state projection");
    state["muskip_registers"] = json!(snapshot.muskip_registers);
    state["next_muskip_register"] = json!(snapshot.next_muskip_register);
    state
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
fn passive_code_table_reader_round_trips_math_del_and_combined_documents() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let snapshot = vm.snapshot();
    let fixtures = [
        (
            "mathcode_state",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            json!({"layers": [[{"character": 65, "value": 123}]]}),
        ),
        (
            "delcode_state",
            vec![DELCODE_TABLE_V1_CAPABILITY],
            json!({"layers": [[{"character": 46, "value": -2}]]}),
        ),
    ];

    for (field, capabilities, code_state) in fixtures {
        let mut state = versioned_state(&snapshot);
        state[field] = code_state.clone();
        let encoded = encoded_document_with_capabilities(&capabilities, state);

        let decoded = decode_vm_snapshot_document(&encoded)
            .unwrap_or_else(|error| panic!("decode {field}: {error}"));
        let reencoded = serde_json::to_vec(&decoded)
            .unwrap_or_else(|error| panic!("reencode {field}: {error}"));
        let wire: serde_json::Value =
            serde_json::from_slice(&reencoded).expect("inspect code-table document");
        let mut legacy_output = Vec::new();
        let legacy_error = serde_json::to_writer(&mut legacy_output, &decoded.state)
            .expect_err("code-table state must not enter the legacy wire");

        assert_eq!(wire["state"][field], code_state);
        assert_eq!(
            decoded.required_capabilities,
            capabilities
                .into_iter()
                .map(SnapshotCapability::new)
                .collect()
        );
        assert!(legacy_error.to_string().contains("table-v1"));
        assert!(legacy_output.is_empty());
        assert_eq!(
            decode_vm_snapshot_document(&reencoded)
                .expect("decode canonical code-table document")
                .state,
            decoded.state
        );
    }

    let mut combined_state = versioned_state(&snapshot);
    combined_state["mathcode_state"] = json!({"layers": [[{"character": 65, "value": 32768}]]});
    combined_state["delcode_state"] = json!({"layers": [[{"character": 46, "value": 16777215}]]});
    let combined = decode_vm_snapshot_document(&encoded_document_with_capabilities(
        &[MATHCODE_TABLE_V1_CAPABILITY, DELCODE_TABLE_V1_CAPABILITY],
        combined_state,
    ))
    .expect("decode combined code-table document");

    assert_eq!(combined.required_capabilities.len(), 2);
}

#[test]
fn code_table_document_writer_revalidates_mutated_state_before_output() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let snapshot = vm.snapshot();
    let mut state = versioned_state(&snapshot);
    state["mathcode_state"] = json!({"layers": [[{"character": 65, "value": 123}]]});
    let encoded = encoded_document_with_capabilities(&[MATHCODE_TABLE_V1_CAPABILITY], state);
    let mut document = decode_vm_snapshot_document(&encoded).expect("decode valid mathcode state");
    document
        .state
        .mathcode_state
        .as_mut()
        .expect("mathcode state")
        .layers[0][0]
        .value = 32_769;
    let mut output = Vec::new();

    let error = serde_json::to_writer(&mut output, &document)
        .expect_err("writer must reject a mutated out-of-range mathcode");

    assert!(error.to_string().contains("mathcode value"));
    assert!(output.is_empty());
}

#[test]
fn passive_code_table_reader_rejects_noncanonical_or_mismatched_state() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let snapshot = vm.snapshot();
    let legacy = versioned_state(&snapshot);
    let cases = [
        (
            "empty mathcode state",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[]]}),
        ),
        (
            "missing root layer",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": []}),
        ),
        (
            "duplicate character",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[
                {"character": 65, "value": 1},
                {"character": 65, "value": 2}
            ]]}),
        ),
        (
            "unordered characters",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[
                {"character": 66, "value": 1},
                {"character": 65, "value": 2}
            ]]}),
        ),
        (
            "mathcode value above active sentinel",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[{"character": 65, "value": 32769}]]}),
        ),
        (
            "delcode value below TeX integer minimum",
            vec![DELCODE_TABLE_V1_CAPABILITY],
            "delcode_state",
            json!({"layers": [[{"character": 65, "value": -2147483648_i64}]]}),
        ),
        (
            "delcode value above packed maximum",
            vec![DELCODE_TABLE_V1_CAPABILITY],
            "delcode_state",
            json!({"layers": [[{"character": 65, "value": 16777216}]]}),
        ),
        (
            "character above V1 domain",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[{"character": 256, "value": 1}]]}),
        ),
        (
            "unknown assignment field",
            vec![MATHCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[{"character": 65, "value": 1, "future": true}]]}),
        ),
        (
            "math state with del capability",
            vec![DELCODE_TABLE_V1_CAPABILITY],
            "mathcode_state",
            json!({"layers": [[{"character": 65, "value": 1}]]}),
        ),
        (
            "math state without capability",
            vec![],
            "mathcode_state",
            json!({"layers": [[{"character": 65, "value": 1}]]}),
        ),
    ];

    for (name, capabilities, field, code_state) in cases {
        let mut state = legacy.clone();
        state[field] = code_state;
        assert!(
            decode_vm_snapshot_document(&encoded_document_with_capabilities(&capabilities, state))
                .is_err(),
            "accepted {name}"
        );
    }

    assert!(
        decode_vm_snapshot_document(&encoded_document_with_capabilities(
            &[MATHCODE_TABLE_V1_CAPABILITY],
            legacy,
        ))
        .is_err(),
        "accepted math capability without state"
    );
}

#[test]
fn passive_code_table_reader_restores_and_unwinds_layered_state() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let mut state = versioned_state(&source.snapshot());
    state["scopes"]
        .as_array_mut()
        .expect("snapshot scopes")
        .push(json!({}));
    state["mathcode_state"] = json!({"layers": [
        [{"character": 65, "value": 100}],
        [{"character": 65, "value": 101}]
    ]});
    state["delcode_state"] = json!({"layers": [
        [{"character": 46, "value": -2}],
        []
    ]});
    let encoded = encoded_document_with_capabilities(
        &[MATHCODE_TABLE_V1_CAPABILITY, DELCODE_TABLE_V1_CAPABILITY],
        state,
    );
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &encoded)
        .expect("restore layered code-table state");

    let open_group = restored.snapshot();
    assert_eq!(
        serde_json::to_value(&VmSnapshotDocument::from_snapshot(open_group))
            .expect("serialize restored open-group state")["state"]["mathcode_state"],
        json!({"layers": [
            [{"character": 65, "value": 100}],
            [{"character": 65, "value": 101}]
        ]})
    );

    let outcome = restored.run_plain("}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let unwound = restored.snapshot();
    let unwound_wire = serde_json::to_value(&VmSnapshotDocument::from_snapshot(unwound))
        .expect("serialize unwound code-table state");
    assert_eq!(
        unwound_wire["state"]["mathcode_state"],
        json!({"layers": [[{"character": 65, "value": 100}]]})
    );
    assert_eq!(
        unwound_wire["state"]["delcode_state"],
        json!({"layers": [[{"character": 46, "value": -2}]]})
    );
}

#[test]
fn passive_code_table_restore_keeps_fresh_defaults_implicit() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let mut state = versioned_state(&source.snapshot());
    state["scopes"]
        .as_array_mut()
        .expect("snapshot scopes")
        .push(json!({}));
    state["mathcode_state"] = json!({"layers": [
        [],
        [{"character": 65, "value": 100}]
    ]});
    let encoded = encoded_document_with_capabilities(&[MATHCODE_TABLE_V1_CAPABILITY], state);
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &encoded)
        .expect("restore local mathcode over an implicit default");

    let open_group = restored.snapshot();
    assert_eq!(
        open_group
            .mathcode_state
            .as_ref()
            .expect("local mathcode state")
            .layers,
        vec![
            vec![],
            vec![tex_vm::VmCodeTableAssignmentV1 {
                character: b'A',
                value: 100,
            }],
        ]
    );

    let outcome = restored.run_plain("}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let unwound = restored.snapshot();
    assert_eq!(unwound.mathcode_state, None);
    assert!(
        unwound
            .required_capabilities()
            .iter()
            .all(|capability| capability.as_str() != MATHCODE_TABLE_V1_CAPABILITY)
    );
}

#[test]
fn passive_integer_parameter_reader_preserves_state_but_rejects_executable_restore() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let mut state = versioned_state(&source.snapshot());
    state["integer_parameter_state"] = json!({
        "layers": [[{"parameter": "tolerance", "value": 123}]]
    });
    let encoded =
        encoded_document_with_capabilities(&[INTEGER_PARAMETER_STATE_V1_CAPABILITY], state);

    let decoded = decode_vm_snapshot_document(&encoded)
        .expect("decode structurally valid passive integer-parameter state");
    assert_eq!(
        decoded.state.integer_parameter_state,
        Some(VmIntegerParameterStateV1 {
            layers: vec![vec![VmIntegerParameterAssignmentV1 {
                parameter: IntegerParameterId::Tolerance,
                value: 123,
            }]],
        })
    );
    assert!(decoded.required_capabilities.iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));

    let mut legacy_output = Vec::new();
    let legacy_error = serde_json::to_writer(&mut legacy_output, &decoded.state)
        .expect_err("passive integer-parameter state must not enter the legacy wire");
    assert!(
        legacy_error
            .to_string()
            .contains(INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert!(legacy_output.is_empty());

    let mut document_output = Vec::new();
    let document_error = serde_json::to_writer(&mut document_output, &decoded)
        .expect_err("passive state must not be rewritten before dormant restore exists");
    assert!(
        document_error
            .to_string()
            .contains(INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert!(document_output.is_empty());

    let mut restored_interner = ControlSequenceInterner::new();
    assert!(matches!(
        Vm::try_restore_document(&mut restored_interner, &encoded),
        Err(VmSnapshotDocumentRestoreError::Restore(
            VmRestoreError::UnsupportedIntegerParameterState
        ))
    ));
}

#[test]
fn passive_integer_parameter_reader_rejects_noncanonical_state() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let legacy = versioned_state(&source.snapshot());
    let cases = [
        ("empty state", json!({"layers": [[]]})),
        ("missing root layer", json!({"layers": []})),
        (
            "duplicate parameter",
            json!({"layers": [[
                {"parameter": "tolerance", "value": 1},
                {"parameter": "tolerance", "value": 2}
            ]]}),
        ),
        (
            "unknown parameter",
            json!({"layers": [[{"parameter": "futureparameter", "value": 1}]]}),
        ),
        (
            "unknown assignment field",
            json!({"layers": [[{
                "parameter": "tolerance",
                "value": 1,
                "future": true
            }]]}),
        ),
        (
            "unknown state field",
            json!({
                "layers": [[{"parameter": "tolerance", "value": 1}]],
                "future": true
            }),
        ),
        (
            "integer above the snapshot domain",
            json!({"layers": [[{
                "parameter": "tolerance",
                "value": 2_147_483_648_i64
            }]]}),
        ),
        (
            "layer count beyond scope depth",
            json!({"layers": [[], [{"parameter": "tolerance", "value": 1}]]}),
        ),
    ];

    for (name, parameter_state) in cases {
        let mut state = legacy.clone();
        state["integer_parameter_state"] = parameter_state;
        assert!(
            decode_vm_snapshot_document(&encoded_document_with_capabilities(
                &[INTEGER_PARAMETER_STATE_V1_CAPABILITY],
                state,
            ))
            .is_err(),
            "accepted {name}"
        );
    }
}

#[test]
fn passive_integer_parameter_reader_requires_exact_capability_equality() {
    let mut source_interner = ControlSequenceInterner::new();
    let source = Vm::new(&mut source_interner);
    let legacy = versioned_state(&source.snapshot());
    let parameter_state = json!({
        "layers": [[{"parameter": "tolerance", "value": 123}]]
    });

    let mut missing_capability = legacy.clone();
    missing_capability["integer_parameter_state"] = parameter_state;
    assert!(
        decode_vm_snapshot_document(&encoded_document(missing_capability)).is_err(),
        "accepted parameter state without its capability"
    );
    assert!(
        decode_vm_snapshot_document(&encoded_document_with_capabilities(
            &[INTEGER_PARAMETER_STATE_V1_CAPABILITY],
            legacy,
        ))
        .is_err(),
        "accepted parameter capability without state"
    );
}

#[test]
fn passive_integer_parameter_field_is_absent_from_capability_free_legacy_state() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let snapshot = vm.snapshot();
    let legacy_projection =
        serde_json::to_vec(&*snapshot).expect("serialize exact legacy projection");
    let legacy_snapshot = serde_json::to_vec(&snapshot).expect("serialize legacy snapshot");
    let document = serde_json::to_value(VmSnapshotDocument::from_snapshot(snapshot))
        .expect("serialize capability-free document");

    assert_eq!(legacy_snapshot, legacy_projection);
    assert!(document["state"].get("integer_parameter_state").is_none());
    assert!(
        document["required_capabilities"]
            .as_array()
            .expect("capability array")
            .is_empty()
    );
}

#[test]
fn complete_muskip_snapshot_restores_values_and_independent_cursor() {
    let snapshot = muskip_snapshot();
    let mut interner = ControlSequenceInterner::new();
    let restored = Vm::try_restore(&mut interner, &snapshot).expect("restore muskip snapshot");
    let round_trip = restored.snapshot();

    assert_eq!(round_trip.muskip_registers.get(&17), Some(&123));
    assert_eq!(round_trip.next_muskip_register, 301);
    assert_eq!(round_trip.next_skip_register, 256);
}

#[test]
fn muskip_state_derives_capability_and_raw_legacy_write_fails_before_output() {
    let snapshot = muskip_snapshot();
    let capabilities = snapshot.required_capabilities();
    let mut output = Vec::new();

    let error = serde_json::to_writer(&mut output, &snapshot)
        .expect_err("muskip state must not enter the legacy wire shape");

    assert_eq!(capabilities.len(), 1);
    assert!(
        capabilities
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_SCALAR_V1_CAPABILITY)
    );
    assert!(error.to_string().contains(MUSKIP_SCALAR_V1_CAPABILITY));
    assert!(output.is_empty(), "legacy serializer wrote {output:?}");
}

#[test]
fn muskip_cursor_progress_alone_requires_the_muskip_capability() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.next_muskip_register += 1;

    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_SCALAR_V1_CAPABILITY)
    );
}

#[test]
fn capability_state_cannot_be_laundered_by_legacy_normalization() {
    let document = normalize_legacy_vm_snapshot(muskip_snapshot());

    assert!(
        document
            .required_capabilities
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_SCALAR_V1_CAPABILITY)
    );
}

#[test]
fn canonical_versioned_writer_round_trips_each_muskip_capability_shape() {
    let scalar = muskip_snapshot();

    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    alias_vm.run_plain(r"\muskipdef\fixed=17");
    let alias = alias_vm.snapshot();

    let mut combined_interner = ControlSequenceInterner::new();
    let mut combined_vm = Vm::new(&mut combined_interner);
    combined_vm.run_plain(r"\newmuskip\first\first=2.5mu");
    let combined = combined_vm.snapshot();
    let mut golden_bytes = Vec::new();

    for (name, snapshot) in [("scalar", scalar), ("alias", alias), ("combined", combined)] {
        let document = VmSnapshotDocument::from_snapshot(snapshot.clone());
        let legacy_state = serde_json::to_value(&*snapshot).expect("serialize legacy projection");

        let encoded = serde_json::to_vec(&document)
            .unwrap_or_else(|error| panic!("serialize {name} versioned document: {error}"));
        let encoded_again = serde_json::to_vec(&document)
            .unwrap_or_else(|error| panic!("reserialize {name} versioned document: {error}"));
        let decoded = decode_vm_snapshot_document(&encoded)
            .unwrap_or_else(|error| panic!("decode {name} versioned document: {error}"));
        let wire: serde_json::Value =
            serde_json::from_slice(&encoded).expect("inspect canonical document wire");

        assert_eq!(encoded, encoded_again, "{name} writer is not deterministic");
        assert!(legacy_state.get("muskip_registers").is_none());
        assert!(legacy_state.get("next_muskip_register").is_none());
        assert_eq!(decoded.state, snapshot, "{name} state changed");
        assert_eq!(
            decoded.required_capabilities,
            snapshot.required_capabilities(),
            "{name} capabilities changed"
        );
        assert_eq!(
            wire["state"]["muskip_registers"],
            json!(snapshot.muskip_registers),
            "{name} muskip map changed"
        );
        assert_eq!(
            wire["state"]["next_muskip_register"],
            json!(snapshot.next_muskip_register),
            "{name} muskip cursor changed"
        );
        golden_bytes.push((
            name,
            encoded.len(),
            blake3::hash(&encoded).to_hex().to_string(),
        ));
    }

    assert_eq!(
        golden_bytes,
        [
            (
                "scalar",
                10_473,
                "e906ca9926966f1361c61556e7283249cdb2b6de57ab9d1b94c00c3fa87de440".to_string(),
            ),
            (
                "alias",
                10_873,
                "40f9acf0f2ffeafb2a04aa8935499dc324ddc1bceb40e24b1f0291702cc8bca0".to_string(),
            ),
            (
                "combined",
                10_994,
                "adc85cee74857e2d140f7d194fd834c03b429edcb2c7707216c00a8d1a4d80c4".to_string(),
            ),
        ]
    );
}

#[test]
fn canonical_versioned_writer_is_stable_across_independent_equal_snapshots() {
    let mut first_interner = ControlSequenceInterner::new();
    let mut first_vm = Vm::new(&mut first_interner);
    let first_outcome = first_vm
        .run_plain(r"\def\zeta{Z}\def\alpha{A}\newmuskip\first\first=2.5mu\muskipdef\fixed=17");
    assert!(
        first_outcome.diagnostics.is_empty(),
        "{:#?}",
        first_outcome.diagnostics
    );
    let first = first_vm.snapshot();

    let mut second_interner = ControlSequenceInterner::new();
    let mut second_vm = Vm::new(&mut second_interner);
    let second_outcome = second_vm
        .run_plain(r"\def\zeta{Z}\def\alpha{A}\newmuskip\first\first=2.5mu\muskipdef\fixed=17");
    assert!(
        second_outcome.diagnostics.is_empty(),
        "{:#?}",
        second_outcome.diagnostics
    );
    let second = second_vm.snapshot();

    assert_eq!(first, second, "fixtures must be semantically equal");
    assert_eq!(
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(first))
            .expect("serialize first independent document"),
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(second))
            .expect("serialize second independent document"),
        "canonical bytes depend on HashMap construction identity"
    );
}

#[test]
fn canonical_versioned_writer_rejects_header_state_laundering_before_output() {
    let mut document = VmSnapshotDocument::from_snapshot(muskip_snapshot());
    document.required_capabilities.clear();
    let mut output = Vec::new();

    let error = serde_json::to_writer(&mut output, &document)
        .expect_err("canonical writer must reject a laundered capability header");

    assert!(
        error.to_string().contains("declared capabilities"),
        "{error}"
    );
    assert!(output.is_empty(), "versioned writer wrote {output:?}");
}

#[test]
fn canonical_versioned_writer_rejects_restore_invalid_state_before_output() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);

    let mut missing_root_scope = vm.snapshot();
    missing_root_scope.scopes.clear();

    let mut invalid_muskip_cursor = vm.snapshot();
    invalid_muskip_cursor.next_muskip_register = 255;

    let mut unknown_primitive = vm.snapshot();
    unknown_primitive.scopes[0].insert(
        "poisoned".to_string(),
        SnapshotMeaning::Primitive {
            name: "unknown-versioned-writer-primitive".to_string(),
        },
    );

    for (name, snapshot) in [
        ("missing root scope", missing_root_scope),
        ("invalid muskip cursor", invalid_muskip_cursor),
        ("unknown primitive", unknown_primitive),
    ] {
        let document = VmSnapshotDocument::from_snapshot(snapshot);
        let mut output = Vec::new();

        let error = serde_json::to_writer(&mut output, &document)
            .expect_err(&format!("{name} must fail writer validation"));

        assert!(
            error
                .to_string()
                .contains("invalid VM snapshot document state")
        );
        assert!(output.is_empty(), "{name} writer produced {output:?}");
    }
}

#[test]
fn legacy_decode_initializes_empty_muskip_state_independently_of_skip_cursor() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut legacy = serde_json::to_value(vm.snapshot()).expect("serialize legacy snapshot");
    legacy["next_skip_register"] = json!(400);

    let snapshot = serde_json::from_value::<VmSnapshot>(legacy).expect("decode legacy snapshot");

    assert!(snapshot.muskip_registers.is_empty());
    assert_eq!(snapshot.next_muskip_register, 256);
    assert_eq!(snapshot.next_skip_register, 400);
    assert!(snapshot.required_capabilities().is_empty());
}

#[test]
fn raw_legacy_snapshot_rejects_reserved_muskip_fields() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut legacy = serde_json::to_value(vm.snapshot()).expect("serialize legacy snapshot");
    legacy["muskip_registers"] = json!({"17": 123});
    legacy["next_muskip_register"] = json!(301);

    assert!(serde_json::from_value::<VmSnapshot>(legacy).is_err());
}

#[test]
fn restore_rejects_muskip_cursor_below_dynamic_register_base_before_mutation() {
    let mut source_interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut source_interner);
    let mut snapshot = vm.snapshot();
    snapshot.next_muskip_register = 255;
    let mut restored_interner = ControlSequenceInterner::new();
    restored_interner.intern("sentinel");
    let original_len = restored_interner.len();

    let error = match Vm::try_restore(&mut restored_interner, &snapshot) {
        Ok(_) => panic!("invalid muskip cursor must be rejected"),
        Err(error) => error,
    };

    assert_eq!(error, VmRestoreError::InvalidMuskipCursor(255));
    assert_eq!(restored_interner.len(), original_len);
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
fn versioned_muskip_capability_reader_restores_values_aliases_and_cursor() {
    assert_eq!(
        VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES,
        [
            MUSKIP_ALIAS_V1_CAPABILITY,
            MUSKIP_SCALAR_V1_CAPABILITY,
            MATHCODE_TABLE_V1_CAPABILITY,
            DELCODE_TABLE_V1_CAPABILITY,
        ]
    );
    assert_eq!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES,
        [
            MUSKIP_ALIAS_V1_CAPABILITY,
            MUSKIP_SCALAR_V1_CAPABILITY,
            MATHCODE_TABLE_V1_CAPABILITY,
            DELCODE_TABLE_V1_CAPABILITY,
            INTEGER_PARAMETER_STATE_V1_CAPABILITY,
        ]
    );
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(r"\newmuskip\first\first=2.25mu");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = source.snapshot();
    let document_json = encoded_document_with_capabilities(
        &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
        versioned_state(&snapshot),
    );
    drop(source);

    let document = decode_vm_snapshot_document(&document_json)
        .expect("decode versioned muskip snapshot document");
    assert_eq!(
        document.required_capabilities,
        snapshot.required_capabilities()
    );
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &document.state)
        .expect("restore versioned muskip snapshot state");
    let replay = restored.run_plain(r"\newmuskip\second\second=4mu[\the\first][\the\second]");

    assert_eq!(replay.output, "[2.25mu][4mu]");
    assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
    assert_eq!(restored.snapshot().next_muskip_register, 258);
}

#[test]
fn versioned_muskip_reader_rejects_header_state_capability_laundering() {
    let scalar_snapshot = muskip_snapshot();
    let omitted_header = encoded_document(versioned_state(&scalar_snapshot));
    assert!(matches!(
        decode_vm_snapshot_document(&omitted_header),
        Err(VmSnapshotDocumentError::InvalidState(error))
            if error.contains("declared capabilities")
                && error.contains(MUSKIP_SCALAR_V1_CAPABILITY)
    ));

    let mut legacy_interner = ControlSequenceInterner::new();
    let legacy_vm = Vm::new(&mut legacy_interner);
    let false_header = encoded_document_with_capabilities(
        &[MUSKIP_SCALAR_V1_CAPABILITY],
        serde_json::to_value(legacy_vm.snapshot()).expect("serialize legacy state"),
    );
    assert!(matches!(
        decode_vm_snapshot_document(&false_header),
        Err(VmSnapshotDocumentError::InvalidState(error))
            if error.contains("declared capabilities")
                && error.contains(MUSKIP_SCALAR_V1_CAPABILITY)
    ));

    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    alias_vm.run_plain(r"\muskipdef\fixed=17");
    let alias_snapshot = alias_vm.snapshot();
    let missing_alias = encoded_document_with_capabilities(
        &[MUSKIP_SCALAR_V1_CAPABILITY],
        versioned_state(&alias_snapshot),
    );
    assert!(matches!(
        decode_vm_snapshot_document(&missing_alias),
        Err(VmSnapshotDocumentError::InvalidState(error))
            if error.contains("declared capabilities")
                && error.contains(MUSKIP_ALIAS_V1_CAPABILITY)
    ));
}

#[test]
fn versioned_muskip_reader_rejects_duplicate_cursor_declarations() {
    let snapshot = muskip_snapshot();
    let state = legacy_state_with_raw_fields(
        &snapshot,
        r#""muskip_registers":{"17":123},"next_muskip_register":"invalid","next_muskip_register":301"#,
    );
    let document = encoded_document_with_raw_state(&[MUSKIP_SCALAR_V1_CAPABILITY], &state);

    let error = decode_vm_snapshot_document(&document)
        .expect_err("duplicate muskip cursor declarations must be rejected");

    assert!(
        matches!(&error, VmSnapshotDocumentError::InvalidState(message)
            if message.contains("duplicate") && message.contains("next_muskip_register")),
        "{error:?}"
    );
}

#[test]
fn versioned_muskip_reader_rejects_duplicate_map_declarations() {
    let snapshot = muskip_snapshot();
    let state = legacy_state_with_raw_fields(
        &snapshot,
        r#""muskip_registers":"invalid","muskip_registers":{"17":123},"next_muskip_register":301"#,
    );
    let document = encoded_document_with_raw_state(&[MUSKIP_SCALAR_V1_CAPABILITY], &state);

    let error = decode_vm_snapshot_document(&document)
        .expect_err("duplicate muskip map declarations must be rejected");

    assert!(
        matches!(&error, VmSnapshotDocumentError::InvalidState(message)
            if message.contains("duplicate") && message.contains("muskip_registers")),
        "{error:?}"
    );
}

#[test]
fn versioned_muskip_reader_rejects_duplicate_decoded_register_indices() {
    let snapshot = muskip_snapshot();
    let state = legacy_state_with_raw_fields(
        &snapshot,
        r#""muskip_registers":{"17":"invalid","17":123},"next_muskip_register":301"#,
    );
    let document = encoded_document_with_raw_state(&[MUSKIP_SCALAR_V1_CAPABILITY], &state);

    let error = decode_vm_snapshot_document(&document)
        .expect_err("duplicate decoded muskip register indices must be rejected");

    assert!(
        matches!(&error, VmSnapshotDocumentError::InvalidState(message)
            if message.contains("duplicate muskip register") && message.contains("17")),
        "{error:?}"
    );
}

#[test]
fn versioned_alias_only_capability_reader_restores_deferred_source_state() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    source.run_plain(r"\muskipdef\fixed=17\def\later{\fixed=6mu}");
    let snapshot = source.snapshot();
    assert!(snapshot.muskip_registers.is_empty());
    assert_eq!(snapshot.next_muskip_register, 256);
    let document_json = encoded_document_with_capabilities(
        &[MUSKIP_ALIAS_V1_CAPABILITY],
        versioned_state(&snapshot),
    );
    drop(source);

    let document = decode_vm_snapshot_document(&document_json)
        .expect("decode alias-only versioned snapshot document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &document.state)
        .expect("restore alias-only versioned state");
    let replay = restored.run_plain(r"\later[\the\fixed]");

    assert_eq!(replay.output, "[6mu]");
    assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
}

#[test]
fn document_restore_is_transactional_across_header_and_state_validation() {
    let unsupported_capability = serde_json::to_vec(&json!({
        "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
        "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
        "required_capabilities": ["future.capability-v1"],
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
            "future.capability-v1".to_string()
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
        "required_capabilities": ["future.capability-v1"],
        "state": "not a VM snapshot",
    }))
    .expect("serialize unsupported-capability document");
    assert_eq!(
        decode_vm_snapshot_document(&unsupported_capability),
        Err(VmSnapshotDocumentError::UnsupportedCapability(
            "future.capability-v1".to_string()
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
