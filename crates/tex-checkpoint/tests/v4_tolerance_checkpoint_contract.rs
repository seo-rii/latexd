use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde_json::{json, to_value};
use tex_checkpoint::{
    CHECKPOINT_VM_SEMANTIC_EPOCH, CheckpointPage, InputBoundaryCheckpoint, SnapshotAttachment,
    build_checkpoint_bundle_with_snapshots, checkpoint_is_replay_safe,
};
use tex_tokens::{CatCode, ControlSequenceInterner};
use tex_vm::{
    SnapshotToken, SnapshotTokenKind, VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm,
    VmActiveSourceFrameSnapshot, VmInputContinuationSnapshot, VmQueueItemSnapshot,
};

#[test]
fn tolerance_activation_advances_the_checkpoint_vm_semantic_epoch_to_four() {
    assert_eq!(CHECKPOINT_VM_SEMANTIC_EPOCH, 4);

    let mut interner = ControlSequenceInterner::new();
    let snapshot = Vm::new(&mut interner).snapshot();
    let bundle =
        build_checkpoint_bundle_with_snapshots(1, &snapshot, "preamble", 0, &[], &[], &[], &[])
            .expect("build epoch-four checkpoint bundle");
    assert_eq!(
        to_value(bundle).expect("encode bundle")["vm_semantic_epoch"],
        json!(4)
    );
}

#[test]
fn tolerance_state_and_pending_sources_are_suppressed_at_every_capture_category() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\tolerance=123");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let state_snapshot = vm.snapshot();
    assert!(state_snapshot.integer_parameter_state.is_some());
    drop(vm);

    let mut pending_interner = ControlSequenceInterner::new();
    let pending_vm = Vm::new(&mut pending_interner);
    let mut pending_source_snapshot = pending_vm.snapshot();
    pending_source_snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: serde_json::from_value(json!({
                "input": r"\tolerance=456",
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
    assert!(pending_source_snapshot.integer_parameter_state.is_none());

    let mut dynamic_interner = ControlSequenceInterner::new();
    let mut dynamic_vm = Vm::new(&mut dynamic_interner);
    let outcome = dynamic_vm.run_plain(r"\let\buildcs\csname");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut dynamic_snapshot = dynamic_vm.snapshot();
    let mut dynamic_queue = vec![SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "buildcs".to_string(),
        },
        start_utf8: 0,
        end_utf8: 0,
    }];
    dynamic_queue.extend("tolerance".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: CatCode::Letter,
        },
        start_utf8: 0,
        end_utf8: 0,
    }));
    dynamic_queue.push(SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "endcsname".to_string(),
        },
        start_utf8: 0,
        end_utf8: 0,
    });
    dynamic_queue.extend("=456".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: CatCode::Other,
        },
        start_utf8: 0,
        end_utf8: 0,
    }));
    dynamic_snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: dynamic_queue
            .into_iter()
            .map(|token| VmQueueItemSnapshot::Token { token })
            .collect(),
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    assert!(dynamic_snapshot.integer_parameter_state.is_none());

    for (case, snapshot) in [
        ("source-created state", state_snapshot),
        ("pending character source", pending_source_snapshot),
        ("aliased dynamic builder", dynamic_snapshot),
    ] {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            "{case} omitted the tolerance capability"
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

#[test]
fn mixed_capture_categories_keep_only_the_independently_replay_safe_boundaries() {
    let mut safe_interner = ControlSequenceInterner::new();
    let safe = Vm::new(&mut safe_interner).snapshot();

    let mut unsafe_interner = ControlSequenceInterner::new();
    let mut unsafe_vm = Vm::new(&mut unsafe_interner);
    let outcome = unsafe_vm.run_plain(r"\tolerance=123");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let unsafe_snapshot = unsafe_vm.snapshot();

    let pages = [CheckpointPage {
        page_id: "page-1".to_string(),
        index: 0,
        content_hash: "page-hash".to_string(),
        text_start_utf8: 0,
        text_end_utf8: 1,
    }];
    let safe_boundary = InputBoundaryCheckpoint {
        kind: tex_vm::VmModuleCheckpointKind::Enter,
        module_path: Utf8PathBuf::from("chapter.tex"),
        resume_path: Some(Utf8PathBuf::from("main.tex")),
        source_offset_utf8: 7,
        continuation_stack: Vec::new(),
        output_start_utf8: 0,
        page_index_after: 1,
        snapshot: safe.clone(),
    };

    for (case, preamble, page, expected_attached) in [
        (
            "safe preamble then unsafe page",
            safe.clone(),
            unsafe_snapshot.clone(),
            [true, false, true],
        ),
        (
            "unsafe preamble then safe page",
            unsafe_snapshot.clone(),
            safe.clone(),
            [false, true, true],
        ),
    ] {
        let bundle = build_checkpoint_bundle_with_snapshots(
            1,
            &preamble,
            "preamble",
            0,
            &pages,
            &[page],
            &[5],
            &[safe_boundary.clone()],
        )
        .unwrap_or_else(|error| panic!("build {case}: {error}"));

        assert_eq!(bundle.checkpoints.len(), 3);
        for (checkpoint, expected_attached) in bundle.checkpoints.iter().zip(expected_attached) {
            assert_eq!(
                checkpoint.meta.snapshot_attached, expected_attached,
                "{case}"
            );
            assert_eq!(
                checkpoint_is_replay_safe(checkpoint),
                expected_attached,
                "{case}"
            );
        }
        let wire = serde_json::to_value(&bundle).expect("encode mixed checkpoint bundle");
        for (checkpoint, expected_attached) in wire["checkpoints"]
            .as_array()
            .expect("checkpoint array")
            .iter()
            .zip(expected_attached)
        {
            assert_eq!(checkpoint["meta"]["snapshot_attached"], expected_attached);
            assert_eq!(checkpoint["snapshot"].is_object(), expected_attached);
        }
    }
}
