use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind};

#[test]
fn tolerance_remains_source_unreachable_with_dormant_owner() {
    let mut interner = ControlSequenceInterner::new();
    let outcome = Vm::new(&mut interner).run_plain(r"\tolerance=123");

    assert!(outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
            && diagnostic.detail.contains("tolerance")
    }));
}
