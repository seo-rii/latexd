use tex_render_model::{EventProducer, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(source)
}

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn source_scanner_events_are_explicitly_marked_as_recovery() {
    let outcome = capture(
        r"\begin{document}
Recovered text.
\end{document}",
    );
    let text = outcome
        .render_events
        .iter()
        .find(|envelope| matches!(envelope.event, RenderEvent::Text(_)))
        .expect("scanner should recover visible text");

    assert_eq!(text.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(text.meta.confidence, SemanticConfidence::Medium);
}

#[test]
fn group_local_count_assignment_is_restored() {
    let outcome = run(r"\count0=1{\count0=2}\the\count0");

    assert_eq!(outcome.output, "1");
    assert_eq!(outcome.registers.get(&0), Some(&1));
}

#[test]
fn nested_count_assignments_restore_each_group_value() {
    let outcome = run(r"\count0=1{\count0=2{\count0=3}\the\count0}\the\count0");

    assert_eq!(outcome.output, "21");
    assert_eq!(outcome.registers.get(&0), Some(&1));
}

#[test]
fn global_count_assignment_cancels_pending_group_restores() {
    let outcome = run(r"\count0=1{\count0=2{\global\count0=4}\the\count0}\the\count0");

    assert_eq!(outcome.output, "44");
    assert_eq!(outcome.registers.get(&0), Some(&4));
}

#[test]
fn local_count_assignment_after_global_restores_global_value() {
    let outcome = run(r"\count0=1{\global\count0=4\count0=5\the\count0}\the\count0");

    assert_eq!(outcome.output, "54");
    assert_eq!(outcome.registers.get(&0), Some(&4));
}

#[test]
fn globaldefs_controls_count_assignment_scope() {
    let positive = run(r"\count0=1{\globaldefs=1\count0=2}\the\count0");
    let negative = run(r"\count0=1{\globaldefs=-1\global\count0=2}\the\count0");

    assert_eq!(positive.output, "2");
    assert_eq!(positive.registers.get(&0), Some(&2));
    assert_eq!(negative.output, "1");
    assert_eq!(negative.registers.get(&0), Some(&1));
}

#[test]
fn count_arithmetic_uses_the_same_assignment_scope() {
    let local = run(r"\count0=2{\advance\count0 by 3\multiply\count0 by 2}\the\count0");
    let global = run(r"\count0=2{\global\advance\count0 by 3}\the\count0");

    assert_eq!(local.output, "2");
    assert_eq!(local.registers.get(&0), Some(&2));
    assert_eq!(global.output, "5");
    assert_eq!(global.registers.get(&0), Some(&5));
}

#[test]
#[ignore = "known divergence: macro definitions discard delimited parameter text"]
fn delimited_macro_arguments_follow_parameter_text() {
    let outcome = run(r"\def\pair#1,#2;{#2/#1}\pair a,b;");

    assert_eq!(outcome.output, "b/a");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
#[ignore = "known divergence: source recovery scans false conditional bodies"]
fn false_conditional_does_not_emit_math_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
  $wrong$
\fi
$right$
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => {
                Some((math.raw_source.as_str(), envelope.meta.confidence))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec![("right", SemanticConfidence::High)]);
}

#[test]
#[ignore = "known divergence: source recovery does not observe VM macro expansion"]
fn macro_generated_math_emits_an_event() {
    let outcome = capture(
        r"\def\emitmath{$x^2$}
\begin{document}
\emitmath
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => {
                Some(math.raw_source.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec!["x^2"]);
}

#[test]
#[ignore = "known divergence: the VM eagerly tokenizes before catcode assignment"]
fn runtime_catcode_change_affects_unread_characters() {
    let outcome = run(r"\catcode`\@=11
\def\foo@bar{ok}
\foo@bar");

    assert_eq!(outcome.output.trim(), "ok");
    assert!(outcome.diagnostics.is_empty());
}
