use tex_render_model::{RenderEvent, SemanticConfidence};
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
#[ignore = "known divergence: register assignments are not restored at group end"]
fn group_local_count_assignment_is_restored() {
    let outcome = run(r"\count0=1{\count0=2}\the\count0");

    assert_eq!(outcome.output, "1");
    assert_eq!(outcome.registers.get(&0), Some(&1));
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
