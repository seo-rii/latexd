use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use camino::Utf8PathBuf;
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use std::{fs, io::Write};
use tex_checkpoint::{
    CHECKPOINT_VM_SEMANTIC_EPOCH, CheckpointBundle, CheckpointBundleReuse,
    CheckpointCacheMissReason, CheckpointPage, InputBoundaryCheckpoint, SnapshotAttachment,
    build_checkpoint_bundle, build_checkpoint_bundle_with_snapshots, checkpoint_is_replay_safe,
    load_checkpoint_bundle, load_checkpoint_bundle_for_reuse, save_checkpoint_bundle,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    IntegerParameterId, LayoutIntegerParameterId, VM_SNAPSHOT_DOCUMENT_FORMAT,
    VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION, VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm, VmIntegerParameterAssignmentV1,
    VmIntegerParameterStateV1, VmLayoutIntegerParameterAssignmentV1,
    VmLayoutIntegerParameterStateV1, VmSnapshot,
};

const MUSKIP_ALIAS_V1_CAPABILITY: &str = "eqtb.muskip.alias-v1";
const MUSKIP_SCALAR_V1_CAPABILITY: &str = "eqtb.muskip.scalar-v1";

fn legacy_bundle_json() -> Value {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\snapshotword{R}");
    let bundle = build_checkpoint_bundle(1, &vm.snapshot(), "preamble", &[])
        .expect("build legacy checkpoint bundle");
    serde_json::to_value(bundle).expect("serialize legacy checkpoint bundle")
}

fn versioned_slot(snapshot: Value) -> Value {
    json!({
        "document": {
            "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
            "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            "required_capabilities": [],
            "state": snapshot,
        }
    })
}

fn muskip_snapshot() -> VmSnapshot {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.muskip_registers.insert(17, 123);
    snapshot.next_muskip_register = 301;
    snapshot
}

#[test]
fn legacy_only_writer_keeps_the_existing_checkpoint_shape() {
    let bundle = legacy_bundle_json();
    let checkpoint = &bundle["checkpoints"][0];

    assert_eq!(
        bundle["vm_semantic_epoch"],
        json!(CHECKPOINT_VM_SEMANTIC_EPOCH)
    );
    assert!(checkpoint["snapshot"].is_object());
    assert!(checkpoint.get("versioned_snapshot").is_none());
}

#[test]
fn reuse_rejects_a_bundle_without_the_current_vm_semantic_epoch() {
    let mut legacy_wire = legacy_bundle_json();
    legacy_wire
        .as_object_mut()
        .expect("checkpoint bundle object")
        .remove("vm_semantic_epoch");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("pre-activation.json"))
        .expect("UTF-8 checkpoint path");
    fs::write(
        &path,
        serde_json::to_vec(&legacy_wire).expect("encode pre-activation checkpoint bundle"),
    )
    .expect("write pre-activation checkpoint bundle");

    assert_eq!(
        load_checkpoint_bundle(&path)
            .expect("pre-activation bundle remains readable for inspection")
            .vm_semantic_epoch,
        0
    );
    assert_eq!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    );
}

#[test]
fn reuse_rejects_the_epoch_enabled_pre_mathcode_bundle_regime() {
    let mut pre_mathcode_wire = legacy_bundle_json();
    pre_mathcode_wire["vm_semantic_epoch"] = json!(1);
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("epoch-1-pre-mathcode.json"))
        .expect("UTF-8 checkpoint path");
    fs::write(
        &path,
        serde_json::to_vec(&pre_mathcode_wire).expect("encode epoch-1 checkpoint bundle"),
    )
    .expect("write epoch-1 checkpoint bundle");

    assert_eq!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    );
}

#[test]
fn reuse_rejects_the_epoch_two_pre_delcode_bundle_regime() {
    let mut pre_delcode_wire = legacy_bundle_json();
    pre_delcode_wire["vm_semantic_epoch"] = json!(2);
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("epoch-2-pre-delcode.json"))
        .expect("UTF-8 checkpoint path");
    fs::write(
        &path,
        serde_json::to_vec(&pre_delcode_wire).expect("encode epoch-2 checkpoint bundle"),
    )
    .expect("write epoch-2 checkpoint bundle");

    assert_eq!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    );
}

#[test]
fn reuse_rejects_the_epoch_three_pre_tolerance_bundle_regime() {
    let mut pre_tolerance_wire = legacy_bundle_json();
    pre_tolerance_wire["vm_semantic_epoch"] = json!(3);
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("epoch-3-pre-tolerance.json"))
        .expect("UTF-8 checkpoint path");
    fs::write(
        &path,
        serde_json::to_vec(&pre_tolerance_wire).expect("encode epoch-3 checkpoint bundle"),
    )
    .expect("write epoch-3 checkpoint bundle");

    assert_eq!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    );
}

#[test]
fn reuse_rejects_a_future_vm_semantic_epoch() {
    let mut future_wire = legacy_bundle_json();
    future_wire["vm_semantic_epoch"] = json!(CHECKPOINT_VM_SEMANTIC_EPOCH + 1);
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("future-epoch.json"))
        .expect("UTF-8 checkpoint path");
    fs::write(
        &path,
        serde_json::to_vec(&future_wire).expect("encode future checkpoint bundle"),
    )
    .expect("write future checkpoint bundle");

    assert_eq!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    );
}

#[test]
fn production_capture_suppresses_muskip_state_in_every_checkpoint_category() {
    let snapshot = muskip_snapshot();
    let pages = [CheckpointPage {
        page_id: "page-1".to_string(),
        index: 0,
        content_hash: "page-hash".to_string(),
        text_start_utf8: 0,
        text_end_utf8: 1,
    }];
    let input_boundaries = [InputBoundaryCheckpoint {
        kind: tex_vm::VmModuleCheckpointKind::Enter,
        module_path: Utf8PathBuf::from("chapter.tex"),
        resume_path: Some(Utf8PathBuf::from("main.tex")),
        source_offset_utf8: 7,
        continuation_stack: Vec::new(),
        output_start_utf8: 0,
        page_index_after: 1,
        snapshot: snapshot.clone(),
    }];

    let bundle = build_checkpoint_bundle_with_snapshots(
        1,
        &snapshot,
        "preamble",
        0,
        &pages,
        &[snapshot.clone()],
        &[5],
        &input_boundaries,
    )
    .expect("nonlegacy attachments must be suppressed, not fail the build");
    let wire = serde_json::to_value(&bundle).expect("serialize suppressed bundle");

    assert_eq!(bundle.checkpoints.len(), 3);
    for (checkpoint, checkpoint_wire) in bundle
        .checkpoints
        .iter()
        .zip(wire["checkpoints"].as_array().expect("checkpoint array"))
    {
        assert!(!checkpoint.meta.snapshot_attached);
        assert!(matches!(
            checkpoint.snapshot_attachment(),
            SnapshotAttachment::None
        ));
        assert!(checkpoint_wire["snapshot"].is_null());
        assert!(checkpoint_wire.get("versioned_snapshot").is_none());
    }
}

#[test]
fn source_created_alias_only_muskip_state_is_suppressed_at_every_capture_boundary() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\muskipdef\fixed=17");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert!(snapshot.muskip_registers.is_empty());
    assert_eq!(snapshot.next_muskip_register, 256);
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
    let pages = [CheckpointPage {
        page_id: "page-1".to_string(),
        index: 0,
        content_hash: "page-hash".to_string(),
        text_start_utf8: 0,
        text_end_utf8: 1,
    }];
    let input_boundaries = [InputBoundaryCheckpoint {
        kind: tex_vm::VmModuleCheckpointKind::Enter,
        module_path: Utf8PathBuf::from("chapter.tex"),
        resume_path: Some(Utf8PathBuf::from("main.tex")),
        source_offset_utf8: 7,
        continuation_stack: Vec::new(),
        output_start_utf8: 0,
        page_index_after: 1,
        snapshot: snapshot.clone(),
    }];

    let bundle = build_checkpoint_bundle_with_snapshots(
        1,
        &snapshot,
        "preamble",
        0,
        &pages,
        &[snapshot.clone()],
        &[5],
        &input_boundaries,
    )
    .expect("source-created alias state must suppress attachments without failing the build");

    assert_eq!(bundle.checkpoints.len(), 3);
    for checkpoint in &bundle.checkpoints {
        assert!(!checkpoint.meta.snapshot_attached);
        assert!(matches!(
            checkpoint.snapshot_attachment(),
            SnapshotAttachment::None
        ));
        assert!(!checkpoint_is_replay_safe(checkpoint));
    }
}

#[test]
fn dynamic_muskip_name_state_is_suppressed_at_every_capture_boundary() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\def\later{\csname muskip\endcsname0=9mu}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert!(snapshot.muskip_registers.is_empty());
    assert_eq!(snapshot.next_muskip_register, 256);
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
    let pages = [CheckpointPage {
        page_id: "page-1".to_string(),
        index: 0,
        content_hash: "page-hash".to_string(),
        text_start_utf8: 0,
        text_end_utf8: 1,
    }];
    let input_boundaries = [InputBoundaryCheckpoint {
        kind: tex_vm::VmModuleCheckpointKind::Enter,
        module_path: Utf8PathBuf::from("chapter.tex"),
        resume_path: Some(Utf8PathBuf::from("main.tex")),
        source_offset_utf8: 7,
        continuation_stack: Vec::new(),
        output_start_utf8: 0,
        page_index_after: 1,
        snapshot: snapshot.clone(),
    }];

    let bundle = build_checkpoint_bundle_with_snapshots(
        1,
        &snapshot,
        "preamble",
        0,
        &pages,
        &[snapshot.clone()],
        &[5],
        &input_boundaries,
    )
    .expect("dynamic muskip state must suppress attachments without failing the build");

    assert_eq!(bundle.checkpoints.len(), 3);
    for checkpoint in &bundle.checkpoints {
        assert!(!checkpoint.meta.snapshot_attached);
        assert!(matches!(
            checkpoint.snapshot_attachment(),
            SnapshotAttachment::None
        ));
        assert!(!checkpoint_is_replay_safe(checkpoint));
    }
}

#[test]
fn suppressed_muskip_snapshots_keep_state_sensitive_fingerprints() {
    let first = muskip_snapshot();
    let mut second = first.clone();
    second.muskip_registers.insert(17, 124);

    let first_bundle =
        build_checkpoint_bundle(1, &first, "preamble", &[]).expect("build first bundle");
    let second_bundle =
        build_checkpoint_bundle(1, &second, "preamble", &[]).expect("build second bundle");

    assert_ne!(
        first_bundle.checkpoints[0].meta.vm_state_hash,
        second_bundle.checkpoints[0].meta.vm_state_hash
    );
}

#[test]
fn cursor_only_muskip_state_is_suppressed_before_legacy_capture() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.next_muskip_register += 1;

    let bundle = build_checkpoint_bundle(1, &snapshot, "preamble", &[])
        .expect("cursor-only nonlegacy state must be suppressed");

    assert!(!bundle.checkpoints[0].meta.snapshot_attached);
    assert!(matches!(
        bundle.checkpoints[0].snapshot_attachment(),
        SnapshotAttachment::None
    ));
}

#[test]
fn suppressed_recapture_replaces_existing_legacy_attachment_on_disk() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let eligible = vm.snapshot();
    let first =
        build_checkpoint_bundle(1, &eligible, "preamble", &[]).expect("build eligible checkpoint");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
        .expect("UTF-8 checkpoint path");
    save_checkpoint_bundle(&path, &first).expect("save eligible checkpoint");
    let mut ineligible = eligible;
    ineligible.muskip_registers.insert(17, 123);
    let second = build_checkpoint_bundle(1, &ineligible, "preamble", &[])
        .expect("build suppressed recapture");

    save_checkpoint_bundle(&path, &second).expect("replace checkpoint bundle");

    let CheckpointBundleReuse::Hit(reloaded) = load_checkpoint_bundle_for_reuse(&path) else {
        panic!("suppressed bundle must remain readable");
    };
    assert!(!reloaded.checkpoints[0].meta.snapshot_attached);
    assert!(matches!(
        reloaded.checkpoints[0].snapshot_attachment(),
        SnapshotAttachment::None
    ));
    assert!(!checkpoint_is_replay_safe(&reloaded.checkpoints[0]));
}

#[test]
fn reader_exposes_one_legacy_or_versioned_snapshot_attachment() {
    let legacy_json = legacy_bundle_json();
    let legacy = serde_json::from_value::<CheckpointBundle>(legacy_json.clone())
        .expect("decode legacy checkpoint bundle");
    let legacy_checkpoint = &legacy.checkpoints[0];
    assert!(matches!(
        legacy_checkpoint.snapshot_attachment(),
        SnapshotAttachment::Legacy(_)
    ));
    assert!(checkpoint_is_replay_safe(legacy_checkpoint));

    let mut versioned_json = legacy_json;
    let snapshot = versioned_json["checkpoints"][0]["snapshot"].take();
    versioned_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
    versioned_json["checkpoints"][0]["snapshot"] = Value::Null;
    let versioned = serde_json::from_value::<CheckpointBundle>(versioned_json)
        .expect("decode versioned-only checkpoint bundle");
    let versioned_checkpoint = &versioned.checkpoints[0];

    assert!(matches!(
        versioned_checkpoint.snapshot_attachment(),
        SnapshotAttachment::Versioned(_)
    ));
    let restore = versioned_checkpoint
        .snapshot_for_restore()
        .expect("versioned restore view");
    assert!(restore.is_versioned());
    assert_eq!(restore.required_capabilities().count(), 0);
    assert!(checkpoint_is_replay_safe(versioned_checkpoint));
}

#[test]
fn manually_injected_integer_parameter_attachment_requires_semantic_rekey() {
    let mut bundle_json = legacy_bundle_json();
    let mut snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    snapshot["integer_parameter_state"] = json!({
        "layers": [[{"parameter": "tolerance", "value": 123}]]
    });
    let mut slot = versioned_slot(snapshot);
    slot["document"]["required_capabilities"] =
        json!([VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY]);
    bundle_json["checkpoints"][0]["versioned_snapshot"] = slot;
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;

    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode passive integer-parameter attachment for inspection");
    let checkpoint = &bundle.checkpoints[0];
    let SnapshotAttachment::Versioned(document) = checkpoint.snapshot_attachment() else {
        panic!("passive state must retain its versioned attachment");
    };

    assert!(document.state.integer_parameter_state.is_some());
    assert!(!checkpoint_is_replay_safe(checkpoint));
}

#[test]
fn legacy_only_writer_suppresses_test_constructed_dormant_integer_parameter_state() {
    let mut interner = ControlSequenceInterner::new();
    let mut snapshot = Vm::new(&mut interner).snapshot();
    snapshot.integer_parameter_state = Some(VmIntegerParameterStateV1 {
        layers: vec![vec![VmIntegerParameterAssignmentV1 {
            parameter: IntegerParameterId::Tolerance,
            value: 12_000,
        }]],
    });

    let bundle = build_checkpoint_bundle(1, &snapshot, "preamble", &[])
        .expect("build production-policy bundle");
    let checkpoint = &bundle.checkpoints[0];
    assert!(!checkpoint.meta.snapshot_attached);
    assert!(matches!(
        checkpoint.snapshot_attachment(),
        SnapshotAttachment::None
    ));
}

#[test]
fn prepromotion_epoch_four_layout_attachment_requires_current_semantic_rekey() {
    let mut bundle_json = legacy_bundle_json();
    let mut snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    snapshot["layout_integer_parameter_state"] = json!({
        "layers": [[{"parameter": "pretolerance", "value": 123}]]
    });
    let mut slot = versioned_slot(snapshot);
    slot["document"]["required_capabilities"] =
        json!([VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY]);
    bundle_json["checkpoints"][0]["versioned_snapshot"] = slot;
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;

    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode passive layout-parameter attachment for inspection");
    let checkpoint = &bundle.checkpoints[0];
    let SnapshotAttachment::Versioned(document) = checkpoint.snapshot_attachment() else {
        panic!("passive layout state must retain its versioned attachment");
    };

    assert!(document.state.layout_integer_parameter_state.is_some());
    assert!(
        !checkpoint_is_replay_safe(checkpoint),
        "capability support alone must not trust the pre-promotion VM hash"
    );
}

#[test]
fn legacy_only_writer_suppresses_test_constructed_passive_layout_integer_state() {
    let mut interner = ControlSequenceInterner::new();
    let mut snapshot = Vm::new(&mut interner).snapshot();
    snapshot.layout_integer_parameter_state = Some(VmLayoutIntegerParameterStateV1 {
        layers: vec![vec![VmLayoutIntegerParameterAssignmentV1 {
            parameter: LayoutIntegerParameterId::PreTolerance,
            value: 123,
        }]],
    });

    let bundle = build_checkpoint_bundle(1, &snapshot, "preamble", &[])
        .expect("build production-policy bundle");
    let checkpoint = &bundle.checkpoints[0];
    assert!(!checkpoint.meta.snapshot_attached);
    assert!(matches!(
        checkpoint.snapshot_attachment(),
        SnapshotAttachment::None
    ));
}

#[test]
fn local_layout_default_suppresses_legacy_attachment_until_restored_owner_unwinds() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain("{");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut snapshot = source.snapshot();
    drop(source);
    snapshot.layout_integer_parameter_state = Some(VmLayoutIntegerParameterStateV1 {
        layers: vec![
            vec![],
            vec![VmLayoutIntegerParameterAssignmentV1 {
                parameter: LayoutIntegerParameterId::PreTolerance,
                value: 0,
            }],
        ],
    });

    let mut restore_interner = ControlSequenceInterner::new();
    let mut restored =
        Vm::try_restore(&mut restore_interner, &snapshot).expect("restore local default owner");
    let suppressed = build_checkpoint_bundle(1, &restored.snapshot(), "preamble", &[])
        .expect("build suppressed local-default checkpoint");
    assert!(!suppressed.checkpoints[0].meta.snapshot_attached);
    assert!(matches!(
        suppressed.checkpoints[0].snapshot_attachment(),
        SnapshotAttachment::None
    ));

    let outcome = restored.run_plain("}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let unwound = restored.snapshot();
    assert!(unwound.layout_integer_parameter_state.is_none());
    let eligible = build_checkpoint_bundle(2, &unwound, "preamble", &[])
        .expect("build attachment-eligible unwound checkpoint");
    assert!(eligible.checkpoints[0].meta.snapshot_attached);
    assert!(matches!(
        eligible.checkpoints[0].snapshot_attachment(),
        SnapshotAttachment::Legacy(_)
    ));
    assert!(checkpoint_is_replay_safe(&eligible.checkpoints[0]));
}

#[test]
fn reader_treats_a_checkpoint_without_either_lane_as_unattached() {
    let mut bundle_json = legacy_bundle_json();
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    let bundle =
        serde_json::from_value::<CheckpointBundle>(bundle_json).expect("decode empty attachment");
    let checkpoint = &bundle.checkpoints[0];

    assert!(matches!(
        checkpoint.snapshot_attachment(),
        SnapshotAttachment::None
    ));
    assert!(checkpoint.snapshot_for_restore().is_none());
    assert!(!checkpoint_is_replay_safe(checkpoint));
}

#[test]
fn replay_safety_requires_metadata_and_exactly_one_restorable_attachment() {
    for (attachment, snapshot_attached, expected_replay_safe) in [
        ("none", false, false),
        ("none", true, false),
        ("legacy", false, false),
        ("legacy", true, true),
        ("versioned", false, false),
        ("versioned", true, true),
    ] {
        let mut bundle_json = legacy_bundle_json();
        bundle_json["checkpoints"][0]["meta"]["snapshot_attached"] = Value::Bool(snapshot_attached);
        if attachment == "none" {
            bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
        } else if attachment == "versioned" {
            let snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
            bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
            bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
        }
        let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
            .expect("decode truth-table checkpoint");

        assert_eq!(
            checkpoint_is_replay_safe(&bundle.checkpoints[0]),
            expected_replay_safe,
            "attachment={attachment}, snapshot_attached={snapshot_attached}"
        );
    }
}

#[test]
fn reader_rejects_ambiguous_dual_lane_checkpoint() {
    let mut bundle_json = legacy_bundle_json();
    let snapshot = bundle_json["checkpoints"][0]["snapshot"].clone();
    bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);

    let error = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect_err("both snapshot lanes must be rejected");

    assert!(error.to_string().contains("both snapshot lanes"), "{error}");
}

#[test]
fn reader_rejects_unsupported_versioned_capability_before_state() {
    let mut bundle_json = legacy_bundle_json();
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    bundle_json["checkpoints"][0]["versioned_snapshot"] = json!({
        "document": {
            "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
            "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            "required_capabilities": ["future.capability-v1"],
            "state": "not a VM snapshot",
        }
    });

    let error = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect_err("unsupported capability must reject the checkpoint");

    assert!(
        error
            .to_string()
            .contains("unsupported VM snapshot capability"),
        "{error}"
    );
}

#[test]
fn manually_injected_versioned_muskip_checkpoint_requires_semantic_rekey() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    source.run_plain(r"\newmuskip\first\first=2.5mu");
    let snapshot = source.snapshot();
    let mut state = serde_json::to_value(&*snapshot).expect("serialize legacy projection");
    state["muskip_registers"] = json!(snapshot.muskip_registers);
    state["next_muskip_register"] = json!(snapshot.next_muskip_register);
    let mut bundle_json = legacy_bundle_json();
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    bundle_json["checkpoints"][0]["versioned_snapshot"] = json!({
        "document": {
            "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
            "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            "required_capabilities": [
                MUSKIP_ALIAS_V1_CAPABILITY,
                MUSKIP_SCALAR_V1_CAPABILITY
            ],
            "state": state,
        }
    });
    drop(source);

    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode versioned muskip checkpoint");
    let checkpoint = &bundle.checkpoints[0];
    assert!(!checkpoint_is_replay_safe(checkpoint));
    let restore = checkpoint
        .snapshot_for_restore()
        .expect("versioned muskip restore state");
    assert!(restore.is_versioned());
    assert_eq!(
        restore
            .required_capabilities()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        vec![MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY]
    );
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, restore.state())
        .expect("restore versioned muskip checkpoint");
    let replay = restored.run_plain(r"[\the\first]");
    assert_eq!(replay.output, "[2.5mu]");
    assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
}

#[test]
fn versioned_checkpoint_rejects_duplicate_muskip_state_members_before_value_collapse() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    source.run_plain(r"\newmuskip\first\first=2.5mu");
    let snapshot = source.snapshot();
    let mut state = serde_json::to_value(&*snapshot).expect("serialize legacy projection");
    state["muskip_registers"] = json!(snapshot.muskip_registers);
    state["next_muskip_register"] = json!(snapshot.next_muskip_register);
    let mut bundle_json = legacy_bundle_json();
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    bundle_json["checkpoints"][0]["versioned_snapshot"] = json!({
        "document": {
            "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
            "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            "required_capabilities": [
                MUSKIP_ALIAS_V1_CAPABILITY,
                MUSKIP_SCALAR_V1_CAPABILITY
            ],
            "state": state,
        }
    });
    let encoded = serde_json::to_string(&bundle_json).expect("serialize checkpoint fixture");
    let cursor_member = format!(
        r#""next_muskip_register":{}"#,
        snapshot.next_muskip_register
    );
    assert_eq!(encoded.matches(&cursor_member).count(), 1);
    let duplicate = encoded.replacen(
        &cursor_member,
        &format!(r#""next_muskip_register":"invalid",{cursor_member}"#),
        1,
    );
    drop(source);

    let error = serde_json::from_str::<CheckpointBundle>(&duplicate)
        .expect_err("duplicate nested muskip state members must be rejected");

    assert!(
        error
            .to_string()
            .contains("duplicate state member `next_muskip_register`"),
        "{error}"
    );
}

#[test]
fn production_reader_treats_reserved_muskip_fields_in_legacy_lane_as_unreadable() {
    let mut bundle = legacy_bundle_json();
    bundle["checkpoints"][0]["snapshot"]["muskip_registers"] = json!({"17": 123});
    bundle["checkpoints"][0]["snapshot"]["next_muskip_register"] = json!(301);
    let payload = serde_json::to_vec(&bundle).expect("serialize malformed legacy payload");
    let mut compressor = GzEncoder::new(Vec::new(), Compression::fast());
    compressor
        .write_all(&payload)
        .expect("compress malformed legacy payload");
    let compressed = compressor.finish().expect("finish checkpoint compression");
    let envelope = json!({
        "schema_version": 2,
        "encoding": "gzip+base64",
        "payload": BASE64_STANDARD.encode(compressed),
        "uncompressed_len": payload.len(),
        "uncompressed_blake3": blake3::hash(&payload).to_hex().to_string(),
    });
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
        .expect("UTF-8 checkpoint path");
    std::fs::write(
        &path,
        serde_json::to_vec(&envelope).expect("serialize checkpoint envelope"),
    )
    .expect("write checkpoint envelope");

    assert!(matches!(
        load_checkpoint_bundle_for_reuse(&path),
        CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
    ));
}

#[test]
fn versioned_checkpoint_serialization_is_disabled() {
    let mut bundle_json = legacy_bundle_json();
    let snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode versioned-only checkpoint bundle");

    let mut output = Vec::new();
    let error = serde_json::to_writer(&mut output, &bundle)
        .expect_err("versioned writer must remain disabled");

    assert!(
        error
            .to_string()
            .contains("versioned snapshot writer is disabled"),
        "{error}"
    );
    assert!(output.is_empty(), "serializer wrote {output:?}");
}

#[test]
fn production_save_rejects_versioned_attachment_before_file_changes() {
    let mut bundle_json = legacy_bundle_json();
    let snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode versioned-only checkpoint bundle");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
        .expect("UTF-8 checkpoint path");
    std::fs::write(&path, b"sentinel").expect("write sentinel");
    let entries_before = std::fs::read_dir(tempdir.path())
        .expect("read tempdir before save")
        .count();

    let error = save_checkpoint_bundle(&path, &bundle)
        .expect_err("versioned production save must remain disabled");

    assert!(
        error
            .to_string()
            .contains("versioned snapshot writer is disabled")
    );
    assert_eq!(std::fs::read(&path).expect("read sentinel"), b"sentinel");
    assert_eq!(
        std::fs::read_dir(tempdir.path())
            .expect("read tempdir after save")
            .count(),
        entries_before
    );
}

#[test]
fn versioned_snapshot_state_is_available_to_restore_consumers() {
    let mut bundle_json = legacy_bundle_json();
    let snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode versioned-only checkpoint bundle");
    let restore = bundle.checkpoints[0]
        .snapshot_for_restore()
        .expect("versioned snapshot state");
    assert!(restore.is_versioned());
    let snapshot: &VmSnapshot = restore.state();
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::try_restore(&mut interner, snapshot).expect("restore versioned state");

    assert_eq!(vm.run_plain(r"\snapshotword").output, "R");
}

#[test]
fn restore_invalid_versioned_state_is_not_replay_safe() {
    let mut bundle_json = legacy_bundle_json();
    let mut snapshot = bundle_json["checkpoints"][0]["snapshot"].take();
    snapshot["scopes"] = json!([]);
    bundle_json["checkpoints"][0]["versioned_snapshot"] = versioned_slot(snapshot);
    bundle_json["checkpoints"][0]["snapshot"] = Value::Null;
    let bundle = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect("decode restore-invalid versioned checkpoint");

    assert!(!checkpoint_is_replay_safe(&bundle.checkpoints[0]));
}

#[test]
fn dual_lane_is_rejected_before_poisoned_versioned_document_decode() {
    let mut bundle_json = legacy_bundle_json();
    bundle_json["checkpoints"][0]["versioned_snapshot"] = json!({
        "document": {
            "format": VM_SNAPSHOT_DOCUMENT_FORMAT,
            "schema_version": VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            "required_capabilities": ["poisoned.capability-v1"],
            "state": "not a VM snapshot",
        }
    });

    let error = serde_json::from_value::<CheckpointBundle>(bundle_json)
        .expect_err("both snapshot lanes must be rejected before document decoding");

    assert!(error.to_string().contains("both snapshot lanes"), "{error}");
}

#[test]
fn legacy_bundle_still_selects_with_changed_main_source() {
    let bundle = serde_json::from_value::<CheckpointBundle>(legacy_bundle_json())
        .expect("decode legacy checkpoint bundle");
    let selected = tex_checkpoint::select_reusable_preamble(
        &bundle,
        &[Utf8PathBuf::from("main.tex")],
        "preamble",
    );

    assert!(selected.is_some());
}
