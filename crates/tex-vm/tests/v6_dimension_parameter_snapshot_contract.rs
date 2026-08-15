use serde_json::{Value, json};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    DimensionParameterId, RawDimensionSp, SnapshotCapability, SnapshotMeaning,
    VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
    VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY, VM_SNAPSHOT_DOCUMENT_FORMAT,
    VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES, Vm, VmDimensionParameterAssignmentV1,
    VmDimensionParameterStateV1, VmRestoreError, VmSnapshot, VmSnapshotDocumentRestoreError,
    decode_vm_snapshot_document,
};

const CONTRACT: &str = include_str!("fixtures/dimension-parameter-state-v1-contract.json");

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

fn canonical_state_document() -> Vec<u8> {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let mut state = versioned_state(&snapshot);
    state["dimension_parameter_state"] =
        contract["document_envelope"]["dimension_parameter_state"].clone();
    encoded_document(
        &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY],
        state,
    )
}

fn command_identity_document(name: &str, required_capabilities: &[&str]) -> Vec<u8> {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let mut state = versioned_state(&snapshot);
    state["scopes"][0]["savedhangindent"] = serde_json::to_value(SnapshotMeaning::Primitive {
        name: name.to_string(),
    })
    .expect("serialize passive command identity");
    encoded_document(required_capabilities, state)
}

#[test]
fn passive_state_reader_decodes_but_cannot_rewrite_restore_or_use_legacy_wire() {
    assert!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES
            .contains(&VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert!(
        !VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES
            .contains(&VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY)
    );

    let encoded = canonical_state_document();
    let decoded = decode_vm_snapshot_document(&encoded).expect("decode passive dimension state");
    assert_eq!(
        decoded.state.dimension_parameter_state,
        Some(VmDimensionParameterStateV1 {
            layers: vec![vec![VmDimensionParameterAssignmentV1 {
                parameter: DimensionParameterId::HangIndent,
                value: RawDimensionSp::new(i32::MAX),
            }]],
        })
    );
    assert_eq!(
        decoded.required_capabilities,
        [SnapshotCapability::new(
            VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
        )]
        .into_iter()
        .collect()
    );

    let mut legacy_output = b"sentinel".to_vec();
    let legacy_error = serde_json::to_writer(&mut legacy_output, &decoded.state)
        .expect_err("passive dimension state must not enter the legacy wire");
    assert!(
        legacy_error
            .to_string()
            .contains(VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert_eq!(legacy_output, b"sentinel");

    let mut document_output = b"sentinel".to_vec();
    let rewrite_error = serde_json::to_writer(&mut document_output, &decoded)
        .expect_err("passive dimension state must not be rewritten");
    assert!(
        rewrite_error
            .to_string()
            .contains(VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert_eq!(document_output, b"sentinel");

    let mut restore_interner = ControlSequenceInterner::new();
    restore_interner.intern("sentinel");
    let original_len = restore_interner.len();
    let restore_error = Vm::try_restore_document(&mut restore_interner, &encoded)
        .expect_err("passive dimension state must not be executable");
    assert_eq!(
        restore_error,
        VmSnapshotDocumentRestoreError::Restore(VmRestoreError::UnsupportedDimensionParameterState)
    );
    assert_eq!(restore_interner.len(), original_len);
}

#[test]
fn state_v1_accepts_the_complete_signed_i32_domain() {
    let values = [
        i32::MIN,
        -1_073_741_824,
        -1_073_741_823,
        -1,
        1,
        1_073_741_823,
        1_073_741_824,
        i32::MAX,
    ];
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();

    for value in values {
        let mut state = versioned_state(&snapshot);
        state["dimension_parameter_state"] = json!({
            "layers": [[{"parameter": "hangindent", "value": value}]]
        });
        let decoded = decode_vm_snapshot_document(&encoded_document(
            &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY],
            state,
        ))
        .unwrap_or_else(|error| panic!("decode full-i32 value {value}: {error}"));
        assert_eq!(
            decoded
                .state
                .dimension_parameter_state
                .expect("dimension parameter state")
                .layers[0][0]
                .value
                .get(),
            value
        );
    }
}

#[test]
fn canonical_layers_preserve_a_local_zero_shadow() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain("{");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut state = versioned_state(&vm.snapshot());
    state["dimension_parameter_state"] = json!({
        "layers": [
            [{"parameter": "hangindent", "value": 1}],
            [{"parameter": "hangindent", "value": 0}]
        ]
    });

    let decoded = decode_vm_snapshot_document(&encoded_document(
        &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY],
        state,
    ))
    .expect("decode local default shadow");
    let layers = decoded
        .state
        .dimension_parameter_state
        .expect("dimension parameter state")
        .layers;
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0][0].value.get(), 1);
    assert_eq!(layers[1][0].value.get(), 0);
}

#[test]
fn passive_reader_rejects_noncanonical_or_out_of_domain_state() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let legacy = versioned_state(&snapshot);
    let cases = [
        ("empty state", json!({"layers": [[]]})),
        ("missing root", json!({"layers": []})),
        (
            "duplicate owner",
            json!({"layers": [[
                {"parameter": "hangindent", "value": 1},
                {"parameter": "hangindent", "value": 2}
            ]]}),
        ),
        (
            "redundant root default",
            json!({"layers": [[{"parameter": "hangindent", "value": 0}]]}),
        ),
        (
            "future owner",
            json!({"layers": [[{"parameter": "future", "value": 1}]]}),
        ),
        (
            "future-like parindent owner",
            json!({"layers": [[{"parameter": "parindent", "value": 1}]]}),
        ),
        (
            "future-like hsize owner",
            json!({"layers": [[{"parameter": "hsize", "value": 1}]]}),
        ),
        (
            "noncanonical case owner",
            json!({"layers": [[{"parameter": "HangIndent", "value": 1}]]}),
        ),
        (
            "noncanonical hyphenated owner",
            json!({"layers": [[{"parameter": "hang-indent", "value": 1}]]}),
        ),
        (
            "above i32",
            json!({"layers": [[{"parameter": "hangindent", "value": 2147483648_i64}]]}),
        ),
        (
            "below i32",
            json!({"layers": [[{"parameter": "hangindent", "value": -2147483649_i64}]]}),
        ),
        (
            "unknown assignment field",
            json!({"layers": [[{
                "parameter": "hangindent",
                "value": 1,
                "future": true
            }]]}),
        ),
        (
            "unknown state field",
            json!({
                "layers": [[{"parameter": "hangindent", "value": 1}]],
                "future": true
            }),
        ),
        (
            "layer count beyond scope",
            json!({"layers": [[], [{"parameter": "hangindent", "value": 1}]]}),
        ),
    ];

    for (case, parameter_state) in cases {
        let mut state = legacy.clone();
        state["dimension_parameter_state"] = parameter_state;
        assert!(
            decode_vm_snapshot_document(&encoded_document(
                &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY],
                state,
            ))
            .is_err(),
            "accepted {case}"
        );
    }
}

#[test]
fn passive_state_reader_requires_exact_capability_equality() {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let legacy = versioned_state(&snapshot);
    let parameter_state = contract["document_envelope"]["dimension_parameter_state"].clone();

    let mut missing_capability = legacy.clone();
    missing_capability["dimension_parameter_state"] = parameter_state;
    assert!(decode_vm_snapshot_document(&encoded_document(&[], missing_capability)).is_err());
    assert!(
        decode_vm_snapshot_document(&encoded_document(
            &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY],
            legacy,
        ))
        .is_err()
    );
}

#[test]
fn passive_command_capability_freezes_identity_and_owner_linkage_only() {
    assert!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES
            .contains(&VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY)
    );
    assert!(
        !VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES
            .contains(&VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY)
    );

    let encoded = command_identity_document(
        "hangindent",
        &[
            VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
            VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
        ],
    );
    let decoded = decode_vm_snapshot_document(&encoded).expect("decode passive command identity");
    assert!(decoded.state.dimension_parameter_state.is_none());
    assert_eq!(
        decoded.state.scopes[0].get("savedhangindent"),
        Some(&SnapshotMeaning::Primitive {
            name: "hangindent".to_string(),
        })
    );

    let mut output = b"sentinel".to_vec();
    let error = serde_json::to_writer(&mut output, &decoded)
        .expect_err("passive command identity must not be writable");
    assert!(
        error
            .to_string()
            .contains(VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY),
        "capability validation must reject before emitting the document: {error}"
    );
    assert_eq!(output, b"sentinel");

    let mut restore_interner = ControlSequenceInterner::new();
    let restore_error = Vm::try_restore_document(&mut restore_interner, &encoded)
        .expect_err("passive command identity must not be executable");
    assert_eq!(
        restore_error,
        VmSnapshotDocumentRestoreError::Restore(
            VmRestoreError::UnsupportedDimensionParameterCommand(DimensionParameterId::HangIndent)
        )
    );
}

#[test]
fn passive_command_requires_both_identity_capabilities_and_rejects_payload_fields() {
    assert!(
        decode_vm_snapshot_document(&command_identity_document(
            "hangindent",
            &[VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,]
        ))
        .is_err(),
        "command identity without state-owner support must fail"
    );
    assert!(
        decode_vm_snapshot_document(&command_identity_document(
            "hangindent",
            &[VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,]
        ))
        .is_err(),
        "state-owner support must not imply command identity support"
    );

    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let mut state = versioned_state(&snapshot);
    state["dimension_parameter_command"] = json!({
        "identity": "hangindent",
        "executable": false
    });
    assert!(
        decode_vm_snapshot_document(&encoded_document(
            &[
                VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
                VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
            ],
            state,
        ))
        .is_err(),
        "command capability must not accept executable or scanner payload fields"
    );
}

#[test]
fn v1_identity_allowlist_cannot_follow_future_neutral_enum_growth() {
    assert_eq!(
        DimensionParameterId::SNAPSHOT_V1_ALLOWED_IDS,
        &[DimensionParameterId::HangIndent]
    );

    for name in ["parindent", "hsize", "HangIndent", "hang-indent"] {
        let encoded = command_identity_document(name, &[]);
        let decoded = decode_vm_snapshot_document(&encoded)
            .unwrap_or_else(|error| panic!("decode unrelated primitive {name}: {error}"));
        assert!(decoded.required_capabilities.is_empty(), "{name}");
    }
}

#[test]
fn passive_command_restore_is_explicit_for_direct_and_combined_snapshots() {
    let command_only = command_identity_document(
        "hangindent",
        &[
            VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
            VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
        ],
    );
    let decoded = decode_vm_snapshot_document(&command_only).expect("decode command identity");
    let mut direct_interner = ControlSequenceInterner::new();
    direct_interner.intern("sentinel");
    let direct_len = direct_interner.len();
    let direct_error = Vm::try_restore(&mut direct_interner, &decoded.state)
        .expect_err("direct command-only snapshot must fail restore preflight");
    assert_eq!(
        direct_error,
        VmRestoreError::UnsupportedDimensionParameterCommand(DimensionParameterId::HangIndent)
    );
    assert_eq!(direct_interner.len(), direct_len);

    let mut combined = serde_json::from_slice::<Value>(&canonical_state_document())
        .expect("parse canonical state document");
    combined["required_capabilities"] = json!([
        VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
        VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
    ]);
    combined["state"]["scopes"][0]["savedhangindent"] =
        serde_json::to_value(SnapshotMeaning::Primitive {
            name: "hangindent".to_string(),
        })
        .expect("serialize passive command identity");
    let combined = serde_json::to_vec(&combined).expect("serialize combined document");
    let decoded_combined =
        decode_vm_snapshot_document(&combined).expect("decode combined passive document");
    let mut combined_output = b"sentinel".to_vec();
    let write_error = serde_json::to_writer(&mut combined_output, &decoded_combined)
        .expect_err("combined passive document must not emit bytes");
    assert!(
        write_error
            .to_string()
            .contains(VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert_eq!(combined_output, b"sentinel");
    let mut combined_interner = ControlSequenceInterner::new();
    combined_interner.intern("sentinel");
    let combined_len = combined_interner.len();
    let combined_error = Vm::try_restore_document(&mut combined_interner, &combined)
        .expect_err("combined passive snapshot must fail restore preflight");
    assert_eq!(
        combined_error,
        VmSnapshotDocumentRestoreError::Restore(VmRestoreError::UnsupportedDimensionParameterState)
    );
    assert_eq!(combined_interner.len(), combined_len);
}

#[test]
fn raw_duplicate_state_members_fail_before_value_collapse() {
    let encoded = String::from_utf8(canonical_state_document()).expect("UTF-8 document");
    let marker = "\"dimension_parameter_state\":";
    assert_eq!(encoded.matches(marker).count(), 1);
    let duplicate = encoded.replacen(
        marker,
        "\"dimension_parameter_state\":null,\"dimension_parameter_state\":",
        1,
    );

    let error = decode_vm_snapshot_document(duplicate.as_bytes())
        .expect_err("duplicate raw dimension state members must fail");
    assert!(
        error.to_string().contains("duplicate state member")
            && error.to_string().contains("dimension_parameter_state"),
        "{error}"
    );

    for (case, duplicate) in [
        (
            "state layers",
            encoded.replacen("\"layers\":", "\"layers\":[],\"layers\":", 1),
        ),
        (
            "assignment parameter",
            encoded.replacen(
                "\"parameter\":",
                "\"parameter\":\"hangindent\",\"parameter\":",
                1,
            ),
        ),
        (
            "assignment value",
            encoded.replacen("\"value\":", "\"value\":1,\"value\":", 1),
        ),
        (
            "escaped-equivalent state member",
            encoded.replacen(
                marker,
                "\"dimension_parameter_stat\\u0065\":null,\"dimension_parameter_state\":",
                1,
            ),
        ),
    ] {
        assert!(
            decode_vm_snapshot_document(duplicate.as_bytes()).is_err(),
            "accepted duplicate {case}"
        );
    }
}

#[test]
fn legacy_bytes_and_unresolved_meanings_remain_outside_w0() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    assert_eq!(
        serde_json::to_vec(&snapshot).expect("serialize legacy projection"),
        serde_json::to_vec(&*snapshot).expect("serialize legacy state")
    );

    let mut vm = Vm::new(&mut interner);
    let outcome =
        vm.run_plain(r"\def\saved{\hangindent}\expandafter\def\csname hangindent\endcsname{macro}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert!(snapshot.dimension_parameter_state.is_none());
    assert!(!snapshot.required_capabilities().iter().any(|capability| {
        matches!(
            capability.as_str(),
            VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY
                | VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY
        )
    }));
    assert!(matches!(
        snapshot.scopes[0].get("hangindent"),
        Some(SnapshotMeaning::Macro { .. })
    ));
}

#[test]
fn frozen_contract_is_exact_and_source_remains_unreachable() {
    let contract = serde_json::from_str::<Value>(CONTRACT).expect("parse frozen contract");
    assert_eq!(
        contract["state_capability"],
        VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY
    );
    assert_eq!(
        contract["command_capability"],
        VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY
    );
    assert_eq!(contract["allowed_ids"], json!(["hangindent"]));
    assert_eq!(contract["durable_domain"]["minimum"], i32::MIN);
    assert_eq!(contract["durable_domain"]["maximum"], i32::MAX);
    assert_eq!(contract["virtual_defaults"]["hangindent"], 0);
    assert_eq!(contract["command_contract"]["executable"], false);
    assert_eq!(contract["command_contract"]["writable"], false);
    assert_eq!(contract["command_contract"]["restore_supported"], false);
    assert_eq!(
        contract["command_contract"]["requires_state_capability"],
        true
    );
    assert_eq!(contract["canonicality"], "semantic-json-data-model");
    assert_eq!(
        contract["required_capabilities_semantics"],
        "set-membership"
    );

    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let before = vm.snapshot();
    assert!(before.dimension_parameter_state.is_none());
    assert!(!before.required_capabilities().iter().any(|capability| {
        matches!(
            capability.as_str(),
            VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY
                | VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY
        )
    }));
    let outcome = vm.run_plain(
        r"\ifcsname hangindent\endcsname T\else F\fi\ifdefined\hangindent T\else F\fi\hangindent=1pt",
    );
    assert_eq!(outcome.output, r"FF\hangindent=1pt");
    assert!(outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == tex_vm::VmDiagnosticKind::UndefinedControlSequence
            && diagnostic.detail == "hangindent"
    }));
}
