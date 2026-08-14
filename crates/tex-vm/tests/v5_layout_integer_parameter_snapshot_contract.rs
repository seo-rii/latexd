use serde_json::{Value, json};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    IntegerParameterId, LayoutIntegerParameterId, SnapshotCapability, SnapshotMeaning,
    VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY, VM_SNAPSHOT_DOCUMENT_FORMAT,
    VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
    VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES, VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY, Vm, VmCodeTableAssignmentV1, VmCodeTableStateV1,
    VmIntegerParameterAssignmentV1, VmIntegerParameterStateV1,
    VmLayoutIntegerParameterAssignmentV1, VmLayoutIntegerParameterStateV1, VmRestoreError,
    VmSnapshot, VmSnapshotDocument, VmSnapshotDocumentError, decode_vm_snapshot_document,
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
fn dormant_reader_rewrites_and_restores_but_keeps_the_legacy_wire_closed() {
    assert!(
        VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES
            .contains(&VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY)
    );
    assert!(
        VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES
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

    let mut document_output = Vec::new();
    serde_json::to_writer(&mut document_output, &decoded)
        .expect("dormant layout state must rewrite losslessly");
    assert_eq!(
        decode_vm_snapshot_document(&document_output).expect("decode rewritten document"),
        decoded
    );

    let mut restore_interner = ControlSequenceInterner::new();
    restore_interner.intern("sentinel");
    let restored = Vm::try_restore_document(&mut restore_interner, &encoded)
        .expect("restore dormant layout owner");
    assert_eq!(
        restored.snapshot().layout_integer_parameter_state,
        decoded.state.layout_integer_parameter_state
    );
}

#[test]
fn dormant_layout_owner_shares_the_complete_group_lattice_and_unwinds_losslessly() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain("{{");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut state = versioned_state(&source.snapshot());
    state["mathcode_state"] = json!({
        "layers": [
            [{"character": 65, "value": 123}],
            [{"character": 65, "value": 124}],
            []
        ]
    });
    state["delcode_state"] = json!({
        "layers": [
            [{"character": 46, "value": 0}],
            [],
            [{"character": 46, "value": 12}]
        ]
    });
    state["integer_parameter_state"] = json!({
        "layers": [
            [{"parameter": "tolerance", "value": 12_000}],
            [{"parameter": "tolerance", "value": 10_000}],
            []
        ]
    });
    state["layout_integer_parameter_state"] = json!({
        "layers": [
            [{"parameter": "pretolerance", "value": 10}],
            [{"parameter": "hangafter", "value": 1}],
            [{"parameter": "pretolerance", "value": 20}]
        ]
    });
    let capabilities = [
        "eqtb.delcode.table-v1",
        "eqtb.integer-parameter-state.v1",
        "eqtb.layout-integer-parameter-state.v1",
        "eqtb.mathcode.table-v1",
    ];
    let encoded = encoded_document(&capabilities, state);
    drop(source);

    let decoded = decode_vm_snapshot_document(&encoded).expect("decode mixed dormant owners");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &encoded)
        .expect("restore mixed dormant owners");
    assert_eq!(restored.snapshot(), decoded.state);

    let outcome = restored.run_plain("}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let after_inner = restored.snapshot();
    assert_eq!(
        after_inner
            .layout_integer_parameter_state
            .as_ref()
            .expect("root and outer layout owners")
            .layers,
        vec![
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::PreTolerance,
                value: 10,
            }],
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::HangAfter,
                value: 1,
            }],
        ]
    );

    let outcome = restored.run_plain("}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let after_outer = restored.snapshot();
    assert_eq!(
        after_outer
            .layout_integer_parameter_state
            .expect("root layout owner")
            .layers,
        vec![vec![VmLayoutIntegerParameterAssignmentV1 {
            parameter: LayoutIntegerParameterId::PreTolerance,
            value: 10,
        }]]
    );
}

#[test]
fn every_nonlegacy_owner_presence_mask_restores_and_unwinds_on_one_lattice() {
    for mask in 1_u8..16 {
        let mut source_interner = ControlSequenceInterner::new();
        let mut source = Vm::new(&mut source_interner);
        let outcome = source.run_plain(r"\def\rootword{R}{{\def\innerword{I}");
        assert!(
            outcome.diagnostics.is_empty(),
            "{mask}: {:#?}",
            outcome.diagnostics
        );
        let mut state = versioned_state(&source.snapshot());
        let mut capabilities = Vec::new();
        if mask & 1 != 0 {
            state["mathcode_state"] = json!({
                "layers": [
                    [{"character": 65, "value": 123}],
                    [],
                    [{"character": 66, "value": 124}]
                ]
            });
            capabilities.push(VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY);
        }
        if mask & 2 != 0 {
            state["delcode_state"] = json!({
                "layers": [
                    [{"character": 46, "value": 123}],
                    [],
                    [{"character": 47, "value": 124}]
                ]
            });
            capabilities.push(VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY);
        }
        if mask & 4 != 0 {
            state["integer_parameter_state"] = json!({
                "layers": [
                    [{"parameter": "tolerance", "value": 123}],
                    [],
                    []
                ]
            });
            capabilities.push(VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY);
        }
        if mask & 8 != 0 {
            state["layout_integer_parameter_state"] = json!({
                "layers": [
                    [{"parameter": "pretolerance", "value": 123}],
                    [],
                    [{"parameter": "pretolerance", "value": 456}]
                ]
            });
            capabilities.push(VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY);
        }
        let encoded = encoded_document(&capabilities, state);
        drop(source);

        let decoded = decode_vm_snapshot_document(&encoded)
            .unwrap_or_else(|error| panic!("decode owner mask {mask:04b}: {error}"));
        let mut restored_interner = ControlSequenceInterner::new();
        let mut restored = Vm::try_restore_document(&mut restored_interner, &encoded)
            .unwrap_or_else(|error| panic!("restore owner mask {mask:04b}: {error}"));
        assert_eq!(restored.snapshot(), decoded.state, "owner mask {mask:04b}");

        let outcome = restored.run_plain("}");
        assert!(
            outcome.diagnostics.is_empty(),
            "{mask}: {:#?}",
            outcome.diagnostics
        );
        let after_inner = restored.snapshot();
        assert_eq!(after_inner.mathcode_state.is_some(), mask & 1 != 0);
        assert_eq!(after_inner.delcode_state.is_some(), mask & 2 != 0);
        assert_eq!(after_inner.integer_parameter_state.is_some(), mask & 4 != 0);
        assert_eq!(
            after_inner.layout_integer_parameter_state.is_some(),
            mask & 8 != 0
        );
        for (layer_count, middle_is_empty) in [
            after_inner
                .mathcode_state
                .as_ref()
                .map(|state| (state.layers.len(), state.layers[1].is_empty())),
            after_inner
                .delcode_state
                .as_ref()
                .map(|state| (state.layers.len(), state.layers[1].is_empty())),
            after_inner
                .integer_parameter_state
                .as_ref()
                .map(|state| (state.layers.len(), state.layers[1].is_empty())),
            after_inner
                .layout_integer_parameter_state
                .as_ref()
                .map(|state| (state.layers.len(), state.layers[1].is_empty())),
        ]
        .into_iter()
        .flatten()
        {
            assert_eq!(layer_count, 2, "owner mask {mask:04b}");
            assert!(middle_is_empty, "owner mask {mask:04b}");
        }
        if mask & 12 == 12 {
            assert_eq!(
                after_inner
                    .integer_parameter_state
                    .as_ref()
                    .expect("tolerance state")
                    .layers[0][0]
                    .value,
                123
            );
            assert_eq!(
                after_inner
                    .layout_integer_parameter_state
                    .as_ref()
                    .expect("layout state")
                    .layers[0][0]
                    .value,
                123
            );
        }

        let outcome = restored.run_plain("}");
        assert!(
            outcome.diagnostics.is_empty(),
            "{mask}: {:#?}",
            outcome.diagnostics
        );
        let after_outer = restored.snapshot();
        for layer_count in [
            after_outer
                .mathcode_state
                .as_ref()
                .map(|state| state.layers.len()),
            after_outer
                .delcode_state
                .as_ref()
                .map(|state| state.layers.len()),
            after_outer
                .integer_parameter_state
                .as_ref()
                .map(|state| state.layers.len()),
            after_outer
                .layout_integer_parameter_state
                .as_ref()
                .map(|state| state.layers.len()),
        ]
        .into_iter()
        .flatten()
        {
            assert_eq!(layer_count, 1, "owner mask {mask:04b}");
        }
    }
}

#[test]
fn late_layout_and_primitive_restore_failures_leave_a_nonempty_interner_unchanged() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain("{{");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut invalid_layout = source.snapshot();
    drop(source);
    invalid_layout.mathcode_state = Some(VmCodeTableStateV1 {
        layers: vec![
            vec![VmCodeTableAssignmentV1 {
                character: b'A',
                value: 123,
            }],
            vec![],
            vec![],
        ],
    });
    invalid_layout.delcode_state = Some(VmCodeTableStateV1 {
        layers: vec![
            vec![VmCodeTableAssignmentV1 {
                character: b'.',
                value: 123,
            }],
            vec![],
            vec![],
        ],
    });
    invalid_layout.integer_parameter_state = Some(VmIntegerParameterStateV1 {
        layers: vec![
            vec![VmIntegerParameterAssignmentV1 {
                parameter: IntegerParameterId::Tolerance,
                value: 123,
            }],
            vec![],
            vec![],
        ],
    });
    invalid_layout.layout_integer_parameter_state = Some(VmLayoutIntegerParameterStateV1 {
        layers: vec![
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::PreTolerance,
                value: 123,
            }],
            vec![],
            vec![
                VmLayoutIntegerParameterAssignmentV1 {
                    parameter: LayoutIntegerParameterId::HangAfter,
                    value: 2,
                },
                VmLayoutIntegerParameterAssignmentV1 {
                    parameter: LayoutIntegerParameterId::HangAfter,
                    value: 3,
                },
            ],
        ],
    });

    let mut restore_interner = ControlSequenceInterner::new();
    let sentinel = restore_interner.intern("sentinel");
    let anchor = restore_interner.intern("anchor");
    let original_len = restore_interner.len();
    let error = match Vm::try_restore(&mut restore_interner, &invalid_layout) {
        Ok(_) => panic!("deepest duplicate layout owner must fail restore"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        VmRestoreError::InvalidLayoutIntegerParameterState(message)
            if message.contains("strictly increasing")
    ));
    assert_eq!(restore_interner.len(), original_len);
    assert_eq!(restore_interner.resolve(sentinel), Some("sentinel"));
    assert_eq!(restore_interner.resolve(anchor), Some("anchor"));

    let mut unknown_primitive = invalid_layout;
    unknown_primitive.layout_integer_parameter_state = Some(VmLayoutIntegerParameterStateV1 {
        layers: vec![
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::PreTolerance,
                value: 123,
            }],
            vec![],
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::HangAfter,
                value: 2,
            }],
        ],
    });
    unknown_primitive.scopes[2].insert(
        "poisoned".to_string(),
        SnapshotMeaning::Primitive {
            name: "unknown-after-valid-layout-state".to_string(),
        },
    );
    let error = match Vm::try_restore(&mut restore_interner, &unknown_primitive) {
        Ok(_) => panic!("unknown primitive after valid layout state must fail restore"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        VmRestoreError::UnknownPrimitive("unknown-after-valid-layout-state".to_string())
    );
    assert_eq!(restore_interner.len(), original_len);
    assert_eq!(restore_interner.resolve(sentinel), Some("sentinel"));
    assert_eq!(restore_interner.resolve(anchor), Some("anchor"));
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
fn capability_set_canonicalizes_duplicate_and_noncanonical_layout_declarations() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let mut state = versioned_state(&snapshot);
    state["integer_parameter_state"] = json!({
        "layers": [[{"parameter": "tolerance", "value": 123}]]
    });
    state["layout_integer_parameter_state"] = json!({
        "layers": [[{"parameter": "pretolerance", "value": 123}]]
    });
    let encoded = encoded_document(
        &[
            VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
        ],
        state,
    );

    let decoded = decode_vm_snapshot_document(&encoded)
        .expect("decode duplicate capability declarations as set membership");
    assert_eq!(decoded.required_capabilities.len(), 2);
    let rewritten = serde_json::to_value(&decoded).expect("rewrite canonical capability set");
    assert_eq!(
        rewritten["required_capabilities"],
        json!([
            VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
        ])
    );
}

#[test]
fn reader_rejects_duplicate_raw_layout_state_members_before_value_collapse() {
    let encoded = String::from_utf8(canonical_document()).expect("UTF-8 canonical document");
    let marker = "\"layout_integer_parameter_state\":";
    assert_eq!(encoded.matches(marker).count(), 1);
    let duplicate = encoded.replacen(
        marker,
        "\"layout_integer_parameter_state\":null,\"layout_integer_parameter_state\":",
        1,
    );

    let error = decode_vm_snapshot_document(duplicate.as_bytes())
        .expect_err("duplicate raw layout state members must be rejected");
    assert!(
        error.to_string().contains("duplicate state member")
            && error.to_string().contains("layout_integer_parameter_state"),
        "{error}"
    );
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
            "tolerance-family parameter",
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
fn malformed_layout_document_emits_no_rewrite_bytes() {
    let mut interner = ControlSequenceInterner::new();
    let mut invalid_root_default = Vm::new(&mut interner).snapshot();
    invalid_root_default.layout_integer_parameter_state = Some(VmLayoutIntegerParameterStateV1 {
        layers: vec![vec![VmLayoutIntegerParameterAssignmentV1 {
            parameter: LayoutIntegerParameterId::HangAfter,
            value: 1,
        }]],
    });
    let mut invalid_root_default = VmSnapshotDocument::from_snapshot(invalid_root_default);
    let mut output = b"sentinel".to_vec();
    serde_json::to_writer(&mut output, &invalid_root_default)
        .expect_err("redundant layout root default must fail before writing");
    assert_eq!(output, b"sentinel");

    invalid_root_default.required_capabilities.clear();
    serde_json::to_writer(&mut output, &invalid_root_default)
        .expect_err("layout capability/state mismatch must fail before writing");
    assert_eq!(output, b"sentinel");
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
