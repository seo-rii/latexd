use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

#[test]
fn newmuskip_and_muskipdef_reject_unsupported_components_without_mutation() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\newmuskip\first\newmuskip\second\first=1.5mu plus 2mu minus 0.5mu\muskipdef\alias=252\alias=4mu[\the\first][\the\second][\the\alias]",
    );

    assert_eq!(outcome.output, "[0mu][0mu][4mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn muskip_assignments_follow_local_global_and_globaldefs_scope() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\newmuskip\value\value=1mu{\value=2mu[\the\value]}[\the\value]{\global\value=3mu}[\the\value]{\globaldefs=-1\global\value=4mu}[\the\value]",
    );

    assert_eq!(outcome.output, "[2mu][1mu][3mu][3mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn muskip_arithmetic_uses_math_units_and_aliases() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\newmuskip\value\value=1.5mu\advance\value by 0.5mu\multiply\value by 3\divide\value by 2[\the\value]",
    );

    assert_eq!(outcome.output, "[3mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn muskip_allocator_and_values_survive_in_memory_snapshot_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\newmuskip\first\first=2.25mu");
    let snapshot = vm.snapshot();

    let mut restored = Vm::try_restore(&mut interner, &snapshot).expect("restore muskip snapshot");
    let outcome = restored.run_plain(r"\newmuskip\second\second=4mu[\the\first][\the\second]");

    assert_eq!(outcome.output, "[2.25mu][4mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}
