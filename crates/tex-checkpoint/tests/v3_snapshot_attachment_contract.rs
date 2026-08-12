use camino::Utf8PathBuf;
use serde_json::{Value, json};
use tex_checkpoint::{
    CheckpointBundle, CheckpointPage, InputBoundaryCheckpoint, SnapshotAttachment,
    build_checkpoint_bundle, build_checkpoint_bundle_with_snapshots, checkpoint_is_replay_safe,
    save_checkpoint_bundle,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{VM_SNAPSHOT_DOCUMENT_FORMAT, VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION, Vm, VmSnapshot};

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

    assert!(checkpoint["snapshot"].is_object());
    assert!(checkpoint.get("versioned_snapshot").is_none());
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
            "required_capabilities": ["eqtb.muskip.scalar-v1"],
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
