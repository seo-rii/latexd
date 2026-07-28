use tex_render_model::{EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn executed_line_break_commands_are_authoritative() {
    for (command, expected_source) in [
        (r"\\[0.5em]", r"\\[0.5em]"),
        (r"\\*[1ex]", r"\\*[1ex]"),
        (r"\newline ", r"\newline"),
        (r"\linebreak[4]", r"\linebreak[4]"),
    ] {
        let source = format!(r"\begin{{document}}First{command}Second\end{{document}}");
        let outcome = capture(&source);
        let breaks = outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::LineBreak(_)))
            .collect::<Vec<_>>();

        assert_eq!(breaks.len(), 1, "{command}");
        assert_eq!(breaks[0].meta.producer, EventProducer::Primitive);
        assert_eq!(breaks[0].meta.confidence, SemanticConfidence::High);
        assert!(matches!(
            &breaks[0].meta.source.primary,
            ProvenanceSpan::File(span)
                if &source[span.start_utf8 as usize..span.end_utf8 as usize] == expected_source
        ));
    }
}

#[test]
fn false_conditional_does_not_emit_line_break_events() {
    let outcome = capture(r"\begin{document}First\iffalse\linebreak WRONG\fi Second\end{document}");

    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::LineBreak(_)))
    );
}

#[test]
fn macro_generated_line_break_emits_at_the_invocation() {
    let source = r"\def\breakit{\\}\begin{document}First\breakit Second\end{document}";
    let outcome = capture(source);
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::LineBreak(_)))
        .expect("macro-generated line break");

    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("breakit"))
    );
    assert!(matches!(
        &event.meta.source.primary,
        ProvenanceSpan::File(span)
            if &source[span.start_utf8 as usize..span.end_utf8 as usize] == r"\breakit"
    ));
}
