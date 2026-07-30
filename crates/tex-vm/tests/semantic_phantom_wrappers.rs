use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmOutcome};

fn run(source: &str, capture_events: bool) -> VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    if capture_events {
        vm.enable_render_event_capture();
    }
    vm.run_plain(source)
}

#[test]
fn phantom_wrappers_hide_arguments_with_or_without_event_capture() {
    let source =
        r"\begin{document}Visible \phantom{Ghost}\hphantom{Wide}\vphantom{Tall}Text.\end{document}";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(
            outcome.output.contains("Visible Text."),
            "{}",
            outcome.output
        );
        for hidden in ["Ghost", "Wide", "Tall", "phantom", "hphantom", "vphantom"] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
    }
}

#[test]
fn mounted_bibliography_executes_phantom_wrappers() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "refs.bbl",
        r"\begin{thebibliography}{1}\bibitem{key}Visible \phantom{Ghost}\hphantom{Wide}\vphantom{Tall}Text.\end{thebibliography}",
    );
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");

    assert!(
        outcome.output.contains("Visible Text."),
        "{}",
        outcome.output
    );
    for hidden in ["Ghost", "Wide", "Tall", "phantom", "hphantom", "vphantom"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_phantoms() {
    let outcome = run(
        r"\def\hide#1{\phantom{#1}}\let\widehide\hphantom\begin{document}Before \hide{Ghost}\widehide{Wide}\iffalse\vphantom{Skipped}\fi After.\end{document}",
        true,
    );

    assert!(
        outcome.output.contains("Before After."),
        "{}",
        outcome.output
    );
    for hidden in [
        "Ghost", "Wide", "Skipped", "phantom", "hphantom", "vphantom",
    ] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn phantom_arguments_do_not_emit_nested_inline_semantics() {
    let outcome = run(
        r"\begin{document}Before \phantom{Ghost \cite{secret} $x^2$}\hphantom{\ref{hidden}} After \cite{shown}.\end{document}",
        true,
    );

    let citations = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineCitation(citation) => Some(citation.keys.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(citations, vec![vec!["shown".to_string()]]);
    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            &event.event,
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math)
                if math.raw_source.contains("x^2")
        )
    }));
}

#[test]
fn user_definition_overrides_builtin_phantom_wrapper() {
    let outcome = run(
        r"\def\phantom#1{[#1]}\begin{document}Visible \phantom{custom}.\end{document}",
        true,
    );

    assert!(
        outcome.output.contains("Visible [custom]."),
        "{}",
        outcome.output
    );
}
