use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind};

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    Vm::new(&mut interner).run_plain(source)
}

#[test]
fn hangindent_is_absent_from_builtin_existence_checks_before_dp1() {
    let outcome =
        run(r"\ifcsname hangindent\endcsname T\else F\fi\ifdefined\hangindent T\else F\fi");

    assert_eq!(outcome.output, "FF");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn source_hangindent_is_undefined_before_dp1_activation() {
    let outcome = run(r"\hangindent=1pt");

    assert!(outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
            && diagnostic.detail == "hangindent"
    }));
}

#[test]
fn user_defined_hangindent_remains_an_ordinary_control_sequence() {
    let outcome = run(r"\def\hangindent{ordinary}[\hangindent]");

    assert_eq!(outcome.output, "[ordinary]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn current_vm_dimen0_characterization_stays_distinct_from_native_tex82_scanning() {
    let outcome = run(
        r"[\the\dimen0]\dimen0=1.5pt[\the\dimen0]\dimen0=.5sp[\the\dimen0]\dimen0=-5sp\divide\dimen0 by2[\the\dimen0]",
    );

    assert_eq!(outcome.output, "[0pt][1.5pt][0.00002pt][-0.00003pt]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}
