use tex_render_model::{
    EventProducer, FootnoteCommandKind, ProvenanceSpan, RenderEvent, SemanticConfidence,
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
fn executed_footnote_is_authoritative() {
    let source = r"\begin{document}A\footnote[7]{Note.} B\end{document}";
    let outcome = capture(source);
    let boundaries = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::BeginFootnote(_) | RenderEvent::EndFootnote(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(boundaries.len(), 2);
    let RenderEvent::BeginFootnote(begin) = &boundaries[0].event else {
        unreachable!();
    };
    let RenderEvent::EndFootnote(end) = &boundaries[1].event else {
        unreachable!();
    };
    assert_eq!(begin.note_id, end.note_id);
    assert_eq!(begin.marker.as_deref(), Some("7"));
    assert_eq!(begin.command, FootnoteCommandKind::Footnote);
    assert!(begin.draw_reference);
    for event in boundaries {
        assert_eq!(event.meta.producer, EventProducer::Primitive);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(matches!(
            &event.meta.source.primary,
            ProvenanceSpan::File(span)
                if &source[span.start_utf8 as usize..span.end_utf8 as usize]
                    == r"\footnote[7]{Note.}"
        ));
    }
}

#[test]
fn footnote_body_preserves_executed_inline_event_order() {
    let outcome = capture(
        r"\begin{document}A\footnote{Text \cite{key}, \ref{sec:intro}, and $x^2$.} B\end{document}",
    );
    let begin = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .expect("footnote begin");
    let citation = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
        .expect("footnote citation");
    let reference = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::InlineReference(_)))
        .expect("footnote reference");
    let math = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::InlineMath(_)))
        .expect("footnote math");
    let end = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::EndFootnote(_)))
        .expect("footnote end");

    assert!(begin < citation);
    assert!(citation < reference);
    assert!(reference < math);
    assert!(math < end);
    for event in &outcome.render_events[begin..=end] {
        assert_ne!(event.meta.producer, EventProducer::ScannerRecovery);
    }
}

#[test]
fn false_conditional_does_not_emit_footnote_events() {
    let outcome =
        capture(r"\begin{document}A\iffalse\footnote{Wrong.}\fi\footnote{Right.} B\end{document}");
    let begins = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginFootnote(begin) => Some(begin),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(begins.len(), 1);
    let begin_index = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .expect("footnote begin");
    let end_index = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::EndFootnote(_)))
        .expect("footnote end");
    let body_text = outcome.render_events[begin_index + 1..end_index]
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(body_text, "Right.");
}

#[test]
fn macro_generated_footnote_emits_at_the_invocation() {
    let source =
        r"\def\emitnote#1{\footnote{#1}}\begin{document}A\emitnote{Macro note.} B\end{document}";
    let outcome = capture(source);
    let boundaries = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::BeginFootnote(_) | RenderEvent::EndFootnote(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(boundaries.len(), 2);
    for event in boundaries {
        assert_eq!(event.meta.producer, EventProducer::Macro);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(
            event
                .meta
                .source
                .expansion_stack
                .iter()
                .any(|frame| frame.command_name.as_deref() == Some("emitnote"))
        );
        assert!(matches!(
            &event.meta.source.primary,
            ProvenanceSpan::File(span)
                if &source[span.start_utf8 as usize..span.end_utf8 as usize]
                    == r"\emitnote{Macro note.}"
        ));
    }
}

#[test]
fn footnote_alias_uses_primitive_semantics() {
    let outcome = capture(r"\let\note\footnote\begin{document}A\note{Alias note.} B\end{document}");
    let boundaries = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::BeginFootnote(_) | RenderEvent::EndFootnote(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(boundaries.len(), 2);
    assert!(
        boundaries
            .iter()
            .all(|event| event.meta.producer == EventProducer::Primitive)
    );
}

#[test]
fn redefining_footnote_suppresses_scanner_semantics() {
    let outcome =
        capture(r"\begin{document}\def\footnote#1{Visible #1}A\footnote{body} B\end{document}");

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::BeginFootnote(_) | RenderEvent::EndFootnote(_)
        )
    }));
}
