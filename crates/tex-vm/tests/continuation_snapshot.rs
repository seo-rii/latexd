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
