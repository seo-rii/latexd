use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind};

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(source)
}

#[test]
fn protected_macros_stay_deferred_during_full_expansion() {
    let outcome = run(
        r"\def\eager{A}\edef\eagercopy{\eager}\def\eager{B}\protected\def\deferred{A}\edef\deferredcopy{\deferred}\def\deferred{B}[\eagercopy][\deferredcopy]",
    );

    assert_eq!(outcome.output, "[A][B]");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn declaration_flags_are_part_of_ifx_macro_meaning() {
    let outcome = run(
        r"\def\plain#1{#1}\long\def\longmacro#1{#1}\outer\def\outermacro#1{#1}\protected\def\protectedmacro#1{#1}\ifx\plain\longmacro T\else F\fi\ifx\plain\outermacro T\else F\fi\ifx\plain\protectedmacro T\else F\fi",
    );

    assert_eq!(outcome.output, "FFF");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn snapshot_restore_preserves_protected_macro_expansion() {
    let mut initial_interner = ControlSequenceInterner::new();
    let snapshot = {
        let mut vm = Vm::new(&mut initial_interner);
        vm.run_plain(r"\protected\global\def\deferred{A}");
        vm.snapshot()
    };
    let snapshot =
        serde_json::from_str(&serde_json::to_string(&snapshot).expect("serialize VM snapshot"))
            .expect("deserialize VM snapshot");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut restored_interner, &snapshot);

    let outcome = vm.run_plain(r"\edef\captured{\deferred}\def\deferred{B}[\captured]");

    assert_eq!(outcome.output, "[B]");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn non_long_macros_reject_paragraphs_in_grouped_arguments() {
    let outcome = run(r"\def\short#1{BAD}\short{before\par after}tail");

    assert_eq!(outcome.output, "aftertail");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].kind, VmDiagnosticKind::ExplicitError);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "paragraph ended before \\short was complete"
    );
}

#[test]
fn long_macros_accept_paragraphs_in_grouped_arguments() {
    let outcome = run(r"\long\def\accept#1{[#1]}\accept{before\par after}");

    assert_eq!(outcome.output, "[beforeafter]");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn non_long_delimited_arguments_reject_paragraph_content() {
    let outcome = run(r"\def\short#1;{BAD}\short before\par after;tail");

    assert_eq!(outcome.output, "after;tail");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "paragraph ended before \\short was complete"
    );
}

#[test]
fn paragraph_tokens_can_terminate_non_long_delimited_arguments() {
    let outcome = run(r"\def\paragraph#1\par{[#1]}\paragraph before\par tail");

    assert_eq!(outcome.output, "[before]tail");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn unstarred_newcommand_definitions_accept_paragraph_arguments() {
    let outcome = run(r"\newcommand{\accept}[1]{[#1]}\accept{before\par after}");

    assert_eq!(outcome.output, "[beforeafter]");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn starred_newcommand_definitions_reject_paragraph_arguments() {
    let outcome = run(r"\newcommand*{\short}[1]{BAD}\short{before\par after}tail");

    assert_eq!(outcome.output, "aftertail");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "paragraph ended before \\short was complete"
    );
}

#[test]
fn starred_newcommand_definitions_reject_paragraphs_in_optional_arguments() {
    let outcome = run(r"\newcommand*{\short}[1][default]{BAD}\short[before\par after]tail");

    assert_eq!(outcome.output, "after]tail");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "paragraph ended before \\short was complete"
    );
}

#[test]
fn outer_macros_expand_normally_at_execution_level() {
    let outcome = run(r"\outer\def\stop{OUT}\stop");

    assert_eq!(outcome.output, "OUT");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn outer_macros_interrupt_macro_argument_scanning_without_disappearing() {
    let outcome = run(r"\outer\def\stop{OUT}\def\use#1{BAD}before\use{\stop}after");

    assert_eq!(outcome.output, "beforeOUTafter");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while scanning use of \\use"
    );
}

#[test]
fn outer_macros_interrupt_delimited_argument_scanning() {
    let outcome = run(r"\outer\def\stop{OUT}\def\use#1;{BAD}before\use value\stop;after");

    assert_eq!(outcome.output, "beforeOUT;after");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while scanning use of \\use"
    );
}

#[test]
fn outer_macros_interrupt_optional_argument_scanning() {
    let outcome =
        run(r"\outer\def\stop{OUT}\newcommand{\use}[1][default]{BAD}before\use[\stop]after");

    assert_eq!(outcome.output, "beforeOUT]after");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while scanning use of \\use"
    );
}

#[test]
fn outer_macros_close_macro_replacement_scanning_at_the_error_site() {
    let outcome = run(r"\outer\def\stop{OUT}\def\use{A\stop}before\use{}after");

    assert_eq!(outcome.output, "OUTbeforeAafter");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while scanning definition of \\use"
    );
}

#[test]
fn outer_macros_close_parameter_text_with_an_empty_replacement() {
    let outcome = run(r"\outer\def\stop{OUT}\def\use#1\stop{BAD}\use{X}\stop tail");

    assert_eq!(outcome.output, "OUTBADOUTtail");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while scanning definition of \\use"
    );
}

#[test]
fn outer_macros_interrupt_false_branch_skipping() {
    let outcome = run(r"\outer\def\stop{OUT}before\iffalse WRONG\stop\fi after");

    assert_eq!(outcome.output, "beforeOUTafter");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while skipping conditional text"
    );
}

#[test]
fn outer_macros_interrupt_else_branch_skipping() {
    let outcome = run(r"\outer\def\stop{OUT}\iftrue before\else WRONG\stop\fi after");

    assert_eq!(outcome.output, "beforeOUTafter");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(
        outcome.diagnostics[0].detail,
        "forbidden control sequence \\stop while skipping conditional text"
    );
}

#[test]
fn snapshot_restore_preserves_outer_scanner_restrictions() {
    let mut initial_interner = ControlSequenceInterner::new();
    let snapshot = {
        let mut vm = Vm::new(&mut initial_interner);
        vm.run_plain(r"\outer\global\def\stop{OUT}");
        vm.snapshot()
    };
    let mut restored_interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut restored_interner, &snapshot);

    let outcome = vm.run_plain(r"\def\use#1{BAD}before\use{\stop}after");

    assert_eq!(outcome.output, "beforeOUTafter");
    assert_eq!(outcome.diagnostics.len(), 1);
}
