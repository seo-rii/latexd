use tex_render_model::RenderEventEnvelope;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmContinuationBlocker, VmModuleCheckpointKind, VmSnapshot};

#[test]
fn input_exit_snapshot_resumes_pending_tokens_and_source_catcodes() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "resume.sty",
        r"\def\resume{\input{child}\def\pkg@tail{B}\pkg@tail}A\resume",
    );
    vm.mount_file("child.tex", "C");

    let full = vm.run_plain(r"\usepackage{resume}Z");
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child exit checkpoint");

    assert_eq!(full.output, "ACBZ");
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(
        !checkpoint
            .snapshot
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::ActiveInput)
    );

    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    let output_prefix = full.output[..checkpoint.output_start_utf8 as usize].to_string();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(format!("{output_prefix}{}", resumed.output), full.output);
    assert_eq!(resumed.registers, full.registers);
    assert!(resumed.diagnostics.is_empty());
}

#[test]
fn input_enter_snapshot_reexecutes_the_input_primitive_with_current_child_source() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "resume.sty",
        r"\def\resume{\input{child}\def\pkg@tail{B}\pkg@tail}A\resume",
    );
    vm.mount_file("child.tex", "C");

    let full = vm.run_plain(r"\usepackage{resume}Z");
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child enter checkpoint");

    assert_eq!(full.output, "ACBZ");
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(checkpoint.snapshot.continuation_safety.is_safe());

    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    let output_prefix = full.output[..checkpoint.output_start_utf8 as usize].to_string();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("child.tex", "D");
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(format!("{output_prefix}{}", resumed.output), "ADBZ");
    assert!(resumed.diagnostics.is_empty());
}

#[test]
fn input_exit_snapshot_resumes_active_math_capture() {
    for (source, display) in [
        (r"\begin{document}$a\input{barrier}b$\end{document}", false),
        (r"\begin{document}$$a\input{barrier}b$$\end{document}", true),
    ] {
        let (expected, actual) = replay_render_events_after_input_exit(source);
        assert_eq!(
            expected.len(),
            1,
            "expected one {} math event",
            if display { "display" } else { "inline" }
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn input_exit_snapshot_preserves_text_around_active_math() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Before $a\input{barrier}b$ after.\end{document}",
    );

    assert!(expected.len() > 1, "expected text and math events");
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_text_capture() {
    let (expected, actual) =
        replay_render_events_after_input_exit(r"\begin{document}A\input{barrier}B\end{document}");

    assert!(!expected.is_empty(), "expected executed text events");
    assert_eq!(actual, expected);
}

fn replay_render_events_after_input_exit(
    source: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "c");

    let full = vm.run_plain(source);
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
    let expected = full.render_events.clone();
    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");
    (expected, resumed.render_events)
}
