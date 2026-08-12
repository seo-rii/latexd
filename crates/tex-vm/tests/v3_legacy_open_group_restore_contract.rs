use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmSnapshot};

#[test]
fn restored_legacy_open_group_does_not_invent_register_restore_history() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"{\def\local{L}\count0=1");

    let snapshot_json = serde_json::to_vec(&vm.snapshot()).expect("serialize open snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("decode open snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let outcome = restored.run_plain(
        r"\count0=2}\number\count0\ifdefined\local BAD\else GOOD\fi",
    );

    assert_eq!(outcome.output, "2GOOD");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}
