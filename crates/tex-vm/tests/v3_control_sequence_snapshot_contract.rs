use std::collections::BTreeMap;

use serde_json::{Value, json};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotMeaning, VM_CONTINUATION_SAFETY_SCHEMA_VERSION, VM_SEMANTIC_CAPTURE_SCHEMA_VERSION, Vm,
    VmContinuationBlocker, VmSnapshot,
};

const CONTROL_SEQUENCE_SNAPSHOT_V1: &str =
    include_str!("fixtures/v3-control-sequence-snapshot-v1.json");

fn v3_control_sequence_contract(snapshot: &VmSnapshot) -> Value {
    let scopes = snapshot
        .scopes
        .iter()
        .filter_map(|scope| {
            let selected = scope
                .iter()
                .filter(|(name, _)| name.starts_with("vthree"))
                .map(|(name, meaning)| (name.clone(), meaning.clone()))
                .collect::<BTreeMap<String, SnapshotMeaning>>();
            (!selected.is_empty()).then_some(selected)
        })
        .collect::<Vec<_>>();
    json!({ "scopes": scopes })
}

#[test]
fn control_sequence_snapshot_shape_and_versions_are_stable_for_v3() {
    assert_eq!(VM_CONTINUATION_SAFETY_SCHEMA_VERSION, 2);
    assert_eq!(VM_SEMANTIC_CAPTURE_SCHEMA_VERSION, 22);

    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(
        r"\def\vthreeroot{R}
\let\vthreealias=\vthreeroot
\let\vthreeprimitive=\def
\let\vthreetoken=Z
{\def\vthreeroot{L}\global\let\vthreeglobalalias=\vthreeroot}
{\globaldefs=1\def\vthreepositive{P}}
{\globaldefs=-1\global\def\vthreenegative{N}}",
    );
    let snapshot = vm.snapshot();
    let expected = serde_json::from_str::<Value>(CONTROL_SEQUENCE_SNAPSHOT_V1)
        .expect("V3 snapshot fixture must be valid JSON");

    assert_eq!(v3_control_sequence_contract(&snapshot), expected);

    let encoded = serde_json::to_vec(&snapshot).expect("serialize VM snapshot");
    let decoded = serde_json::from_slice::<VmSnapshot>(&encoded).expect("decode VM snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &decoded);
    let outcome = restored.run_plain(
        r"\vthreeroot\vthreealias\vthreeglobalalias\vthreepositive
\ifdefined\vthreenegative X\else A\fi
\vthreeprimitive\vthreemade{M}\vthreemade\vthreetoken",
    );

    assert_eq!(
        outcome.output.split_whitespace().collect::<String>(),
        "RRLPAMZ"
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn open_group_snapshot_reconstructs_control_sequence_restore_history() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\vthreestate{R}{\def\vthreestate{L}");
    let snapshot = vm.snapshot();

    assert_eq!(snapshot.scopes.len(), 2);
    assert_eq!(
        snapshot.continuation_safety.blockers,
        vec![VmContinuationBlocker::OpenGroup]
    );
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let outcome = restored.run_plain(r"\vthreestate}\vthreestate");

    assert_eq!(outcome.output, "LR");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
#[should_panic(expected = "VM snapshot must contain a root control-sequence scope")]
fn empty_control_sequence_scope_list_is_rejected_before_restore() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.scopes.clear();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let _ = Vm::restore(&mut restored_interner, &snapshot);
}

#[test]
fn empty_control_sequence_layers_preserve_legacy_restore_depth() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let base_snapshot = vm.snapshot();
    drop(vm);

    for scope_depth in [1, 4, 1_000, 1_001] {
        let mut snapshot = base_snapshot.clone();
        snapshot.scopes = vec![Default::default(); scope_depth];

        let mut restored_interner = ControlSequenceInterner::new();
        let restored = Vm::restore(&mut restored_interner, &snapshot);

        assert_eq!(
            restored.snapshot().scopes,
            snapshot.scopes,
            "scope_depth={scope_depth}"
        );
    }
}

#[test]
fn unsupported_control_sequence_meaning_is_rejected_during_decode() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut value = serde_json::to_value(vm.snapshot()).expect("serialize VM snapshot");
    value["scopes"][0]["vthreeunsupported"] = json!({ "kind": "undefined" });

    assert!(serde_json::from_value::<VmSnapshot>(value).is_err());
}
