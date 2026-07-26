use camino::Utf8PathBuf;
use tex_checkpoint::{
    CHECKPOINT_UNSAFE_STATE, CheckpointKind, InputBoundaryCheckpoint, build_checkpoint_bundle,
    build_checkpoint_bundle_with_snapshots, checkpoint_is_replay_safe, checkpoint_reuse_diagnostic,
    preamble_key_for_source, select_reusable_preamble,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmContinuationBlocker, VmModuleCheckpoint, VmModuleCheckpointKind, VmSnapshot};

fn snapshot_after(source: &str) -> VmSnapshot {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(source);
    vm.snapshot()
}

#[test]
fn quiescent_snapshot_can_be_selected_for_preamble_replay() {
    let snapshot = snapshot_after(r"\def\foo{bar}");
    let preamble_key = preamble_key_for_source(r"\documentclass{article}");
    let bundle =
        build_checkpoint_bundle(1, &snapshot, &preamble_key, &[]).expect("checkpoint bundle");

    assert!(snapshot.continuation_safety.is_safe());
    assert!(bundle.checkpoints[0].meta.continuation_safety.is_safe());
    assert!(bundle.checkpoints[0].meta.snapshot_attached);
    assert!(bundle.checkpoints[0].snapshot.is_some());
    assert!(
        select_reusable_preamble(&bundle, &[Utf8PathBuf::from("main.tex")], &preamble_key)
            .is_some()
    );
}

#[test]
fn open_group_snapshot_is_not_attached_or_selected_for_replay() {
    let snapshot = snapshot_after(r"{\def\foo{bar}");
    let preamble_key = preamble_key_for_source(r"\documentclass{article}");
    let bundle =
        build_checkpoint_bundle(1, &snapshot, &preamble_key, &[]).expect("checkpoint bundle");

    assert_eq!(
        snapshot.continuation_safety.blockers,
        vec![VmContinuationBlocker::OpenGroup]
    );
    assert!(!bundle.checkpoints[0].meta.snapshot_attached);
    assert!(bundle.checkpoints[0].snapshot.is_none());
    let diagnostic =
        checkpoint_reuse_diagnostic(&bundle.checkpoints[0]).expect("rejection diagnostic");
    assert_eq!(diagnostic.code, CHECKPOINT_UNSAFE_STATE);
    assert_eq!(diagnostic.blockers, vec![VmContinuationBlocker::OpenGroup]);
    assert!(
        select_reusable_preamble(&bundle, &[Utf8PathBuf::from("main.tex")], &preamble_key)
            .is_none()
    );
}

#[test]
fn open_conditional_snapshot_records_a_replay_blocker() {
    let snapshot = snapshot_after(r"\ifnum1=1");

    assert!(
        snapshot
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::OpenConditional)
    );
}

#[test]
fn pending_global_prefix_snapshot_records_a_replay_blocker() {
    let snapshot = snapshot_after(r"\global");

    assert!(
        snapshot
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::PendingGlobalPrefix)
    );
}

#[test]
fn render_event_capture_snapshot_records_sink_state_as_a_replay_blocker() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(r"\begin{document}Body.\end{document}");

    let snapshot = vm.snapshot();

    assert!(
        snapshot
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::RenderEventSink)
    );
}

#[test]
fn module_enter_checkpoint_serializes_active_input_as_a_continuation() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "child");

    let outcome = vm.run_plain(r"\input{child.tex}");
    let checkpoint = outcome
        .module_checkpoints
        .first()
        .expect("input checkpoint");

    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(checkpoint.snapshot.continuation_safety.is_safe());
}

#[test]
fn input_enter_with_serialized_pre_execution_continuation_is_replay_safe() {
    let checkpoint = capture_input_checkpoint(r"\input{child.tex}", VmModuleCheckpointKind::Enter);
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(checkpoint.snapshot.continuation_safety.is_safe());
    let bundle = bundle_with_input_checkpoint(checkpoint);
    let stored = bundle
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
        .expect("stored input checkpoint");

    assert!(stored.meta.continuation_safety.is_safe());
    assert!(stored.meta.snapshot_attached);
    assert!(checkpoint_is_replay_safe(stored));
}

#[test]
fn input_enter_metadata_without_serialized_continuation_is_not_replay_safe() {
    let mut checkpoint =
        capture_input_checkpoint(r"\input{child.tex}", VmModuleCheckpointKind::Enter);
    checkpoint.snapshot.input_continuation = None;
    checkpoint.snapshot.continuation_safety.blockers = vec![VmContinuationBlocker::ActiveInput];

    let bundle = bundle_with_input_checkpoint(checkpoint);
    let stored = bundle
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
        .expect("stored input checkpoint");

    assert!(!stored.meta.snapshot_attached);
    assert!(!checkpoint_is_replay_safe(stored));
}

#[test]
fn input_boundary_rejects_an_invalid_serialized_continuation() {
    let mut checkpoint =
        capture_input_checkpoint(r"\input{child.tex}tail", VmModuleCheckpointKind::Exit);
    assert!(checkpoint.snapshot.continuation_safety.is_safe());
    checkpoint
        .snapshot
        .input_continuation
        .as_mut()
        .expect("input continuation")
        .source_stack
        .clear();

    let bundle = bundle_with_input_checkpoint(checkpoint);
    let stored = bundle
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
        .expect("stored input checkpoint");

    assert!(!stored.meta.snapshot_attached);
    assert!(!checkpoint_is_replay_safe(stored));
}

#[test]
fn input_exit_with_serialized_continuation_is_replay_safe() {
    let checkpoint =
        capture_input_checkpoint(r"\input{child.tex}tail", VmModuleCheckpointKind::Exit);
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(checkpoint.snapshot.continuation_safety.is_safe());

    let bundle = bundle_with_input_checkpoint(checkpoint);
    let json = serde_json::to_vec(&bundle).expect("serialize checkpoint bundle");
    let restored =
        serde_json::from_slice::<tex_checkpoint::CheckpointBundle>(&json).expect("restore bundle");
    let stored = restored
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
        .expect("stored input checkpoint");

    assert!(stored.meta.continuation_safety.is_safe());
    assert!(stored.meta.snapshot_attached);
    assert!(checkpoint_is_replay_safe(stored));
}

#[test]
fn input_boundary_metadata_does_not_certify_an_open_group() {
    let checkpoint = capture_input_checkpoint(r"{\input{child.tex}", VmModuleCheckpointKind::Enter);
    let bundle = bundle_with_input_checkpoint(checkpoint);
    let stored = bundle
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
        .expect("stored input checkpoint");

    assert!(
        stored
            .meta
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::OpenGroup)
    );
    assert_eq!(
        stored.meta.continuation_safety.blockers,
        vec![VmContinuationBlocker::OpenGroup]
    );
    assert!(!stored.meta.snapshot_attached);
    assert!(!checkpoint_is_replay_safe(stored));
}

#[test]
fn snapshot_without_safety_metadata_is_unverified() {
    let snapshot = snapshot_after(r"\def\foo{bar}");
    let mut json = serde_json::to_value(snapshot).expect("serialize snapshot");
    json.as_object_mut()
        .expect("snapshot object")
        .remove("continuation_safety");

    let legacy = serde_json::from_value::<VmSnapshot>(json).expect("deserialize legacy snapshot");

    assert_eq!(
        legacy.continuation_safety.blockers,
        vec![VmContinuationBlocker::UnverifiedSnapshot]
    );
    assert!(!legacy.continuation_safety.is_safe());
}

fn capture_input_checkpoint(source: &str, kind: VmModuleCheckpointKind) -> VmModuleCheckpoint {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "child");
    vm.run_plain(source)
        .module_checkpoints
        .into_iter()
        .find(|checkpoint| checkpoint.kind == kind)
        .expect("input checkpoint")
}

fn bundle_with_input_checkpoint(
    checkpoint: VmModuleCheckpoint,
) -> tex_checkpoint::CheckpointBundle {
    let preamble_snapshot = snapshot_after("");
    build_checkpoint_bundle_with_snapshots(
        1,
        &preamble_snapshot,
        &preamble_key_for_source(""),
        0,
        &[],
        &[],
        &[],
        &[InputBoundaryCheckpoint {
            kind: checkpoint.kind,
            module_path: checkpoint.module_path,
            resume_path: checkpoint.resume_path,
            source_offset_utf8: checkpoint.source_offset_utf8,
            continuation_stack: checkpoint.continuation_stack,
            output_start_utf8: checkpoint.output_start_utf8,
            page_index_after: 0,
            snapshot: checkpoint.snapshot,
        }],
    )
    .expect("checkpoint bundle")
}
