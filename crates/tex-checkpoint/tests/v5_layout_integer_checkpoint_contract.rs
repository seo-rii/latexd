use std::collections::BTreeMap;
use std::fs;

use camino::Utf8PathBuf;
use serde_json::{json, to_value};
use tex_checkpoint::{
    CHECKPOINT_VM_SEMANTIC_EPOCH, CheckpointBundleReuse, CheckpointCacheMissReason, CheckpointPage,
    InputBoundaryCheckpoint, SnapshotAttachment, build_checkpoint_bundle_with_snapshots,
    checkpoint_is_replay_safe, load_checkpoint_bundle_for_reuse,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm, VmActiveSourceFrameSnapshot,
    VmInputContinuationSnapshot, VmQueueItemSnapshot,
};

#[test]
fn layout_integer_source_activation_advances_the_checkpoint_vm_semantic_epoch_to_five() {
    assert_eq!(CHECKPOINT_VM_SEMANTIC_EPOCH, 5);

    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let bundle =
        build_checkpoint_bundle_with_snapshots(1, &snapshot, "preamble", 0, &[], &[], &[], &[])
            .expect("build epoch-five checkpoint bundle");
    assert_eq!(
        to_value(bundle).expect("encode bundle")["vm_semantic_epoch"],
        json!(5)
    );
}

#[test]
fn historical_and_future_layout_epochs_are_not_reusable() {
    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let bundle =
        build_checkpoint_bundle_with_snapshots(1, &snapshot, "preamble", 0, &[], &[], &[], &[])
            .expect("build current checkpoint bundle");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let current_wire = to_value(bundle).expect("encode current checkpoint bundle");
    for epoch in [4, 6, u32::MAX] {
        let mut wire = current_wire.clone();
        wire["vm_semantic_epoch"] = json!(epoch);
        let path = Utf8PathBuf::from_path_buf(
            tempdir
                .path()
                .join(format!("epoch-{epoch}-layout-activation.json")),
        )
        .expect("UTF-8 checkpoint path");
        fs::write(
            &path,
            serde_json::to_vec(&wire).expect("encode mismatched-epoch bundle"),
        )
        .expect("write mismatched-epoch bundle");

        assert_eq!(
            load_checkpoint_bundle_for_reuse(&path),
            CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable),
            "epoch {epoch}"
        );
    }
}

#[test]
fn layout_state_and_latent_sources_are_suppressed_at_every_capture_category() {
    let mut state_interner = ControlSequenceInterner::new();
    let mut state_vm = Vm::new(&mut state_interner);
    let outcome = state_vm.run_plain(r"\pretolerance=123");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let state_snapshot = state_vm.snapshot();
    assert!(state_snapshot.layout_integer_parameter_state.is_some());
    drop(state_vm);

    let mut macro_interner = ControlSequenceInterner::new();
    let mut macro_vm = Vm::new(&mut macro_interner);
    let outcome = macro_vm.run_plain(r"\def\later{\pretolerance=456}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let macro_snapshot = macro_vm.snapshot();
    assert!(macro_snapshot.layout_integer_parameter_state.is_none());
    drop(macro_vm);

    let mut pending_interner = ControlSequenceInterner::new();
    let pending_vm = Vm::new(&mut pending_interner);
    let mut pending_source_snapshot = pending_vm.snapshot();
    pending_source_snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: serde_json::from_value(json!({
                "input": r"\pretolerance=789",
                "position_utf8": 0,
                "state": "new_line",
            }))
            .expect("decode pending mouth snapshot"),
        }],
        source_stack: vec![VmActiveSourceFrameSnapshot {
            path: Utf8PathBuf::from("chapter.tex"),
            output_start_utf8: 0,
            execution_anchor: None,
            return_to_parent: None,
            global_definition_base_scope: None,
            module_kind: None,
            catcode_overrides: BTreeMap::new(),
            suppressed_catcode_overrides: BTreeMap::new(),
            end_hooks: Vec::new(),
            module_options: None,
        }],
        last_token_end_utf8: 0,
    });

    for (case, snapshot, expects_command, expects_tolerance) in [
        ("source-created state", state_snapshot, false, false),
        ("latent macro", macro_snapshot, true, false),
        (
            "pending character source",
            pending_source_snapshot,
            true,
            true,
        ),
    ] {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            "{case} omitted the layout integer capability"
        );
        assert_eq!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
            }),
            expects_command,
            "{case} source-command capability"
        );
        assert_eq!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            expects_tolerance,
            "{case} tolerance ambiguity"
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
        .unwrap_or_else(|error| panic!("{case} must suppress legacy attachments: {error}"));

        assert_eq!(bundle.checkpoints.len(), 3);
        for checkpoint in &bundle.checkpoints {
            assert!(!checkpoint.meta.snapshot_attached, "{case}");
            assert!(
                matches!(checkpoint.snapshot_attachment(), SnapshotAttachment::None),
                "{case}"
            );
            assert!(!checkpoint_is_replay_safe(checkpoint), "{case}");
        }
    }
}
