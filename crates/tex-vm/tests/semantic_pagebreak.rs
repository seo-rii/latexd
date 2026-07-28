use tex_render_model::{
    EventProducer, PageBreakKind, ProvenanceSpan, RenderEvent, SemanticConfidence,
};
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
fn executed_page_break_commands_are_authoritative() {
    let source = r"\begin{document}A\newpage B\clearpage C\cleardoublepage D\end{document}";
    let outcome = capture(source);
    let breaks = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::PageBreak(page_break) => Some((page_break.kind, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        breaks.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        [
            PageBreakKind::NewPage,
            PageBreakKind::ClearPage,
            PageBreakKind::ClearDoublePage,
        ]
    );
    for ((_, event), expected_source) in
        breaks
            .iter()
            .zip([r"\newpage", r"\clearpage", r"\cleardoublepage"])
    {
        assert_eq!(event.meta.producer, EventProducer::Primitive);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(matches!(
            &event.meta.source.primary,
            ProvenanceSpan::File(span)
                if &source[span.start_utf8 as usize..span.end_utf8 as usize] == expected_source
        ));
    }
}

#[test]
fn false_conditional_does_not_emit_page_break_events() {
    let outcome =
        capture(r"\begin{document}A\iffalse\newpage WRONG\fi B\clearpage C\end{document}");
    let kinds = outcome
        .render_events
        .iter()
        .filter_map(|event| match event.event {
            RenderEvent::PageBreak(ref page_break) => Some(page_break.kind),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(kinds, [PageBreakKind::ClearPage]);
}

#[test]
fn macro_generated_page_break_emits_at_the_invocation() {
    let source = r"\def\breakpage{\newpage}\begin{document}A\breakpage B\end{document}";
    let outcome = capture(source);
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::PageBreak(_)))
        .expect("macro-generated page break");

    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("breakpage"))
    );
    assert!(matches!(
        &event.meta.source.primary,
        ProvenanceSpan::File(span)
            if &source[span.start_utf8 as usize..span.end_utf8 as usize] == r"\breakpage"
    ));
}

#[test]
fn page_break_alias_preserves_the_primitive_kind() {
    let outcome = capture(r"\let\breakpage\clearpage\begin{document}A\breakpage B\end{document}");
    let breaks = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::PageBreak(page_break) => Some((page_break.kind, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        breaks,
        [(PageBreakKind::ClearPage, EventProducer::Primitive)]
    );
}

#[test]
fn redefining_a_page_break_command_suppresses_scanner_semantics() {
    let outcome = capture(r"\begin{document}\def\newpage{not a break}A\newpage B\end{document}");

    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::PageBreak(_)))
    );
}
