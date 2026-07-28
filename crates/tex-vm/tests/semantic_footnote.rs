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

#[test]
fn executed_footnotemark_pairs_with_executed_footnotetext() {
    let outcome = capture(
        r"\begin{document}A\footnotemark[7] B\footnotetext{Note \cite{key}.}\end{document}",
    );
    let mark = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::FootnoteMark(_)))
        .expect("footnote mark");
    let begin = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .expect("footnote text begin");
    let end = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::EndFootnote(_)))
        .expect("footnote text end");
    let RenderEvent::FootnoteMark(mark_payload) = &mark.event else {
        unreachable!();
    };
    let RenderEvent::BeginFootnote(begin_payload) = &begin.event else {
        unreachable!();
    };
    let RenderEvent::EndFootnote(end_payload) = &end.event else {
        unreachable!();
    };

    assert_eq!(mark_payload.note_id, begin_payload.note_id);
    assert_eq!(begin_payload.note_id, end_payload.note_id);
    assert_eq!(mark_payload.marker.as_deref(), Some("7"));
    assert_eq!(begin_payload.marker.as_deref(), Some("7"));
    assert_eq!(begin_payload.command, FootnoteCommandKind::FootnoteText);
    assert!(!begin_payload.draw_reference);
    assert_eq!(mark.meta.producer, EventProducer::Primitive);
    assert_eq!(begin.meta.producer, EventProducer::Primitive);
    assert_eq!(end.meta.producer, EventProducer::Primitive);
    assert!(outcome.render_events.iter().all(|event| {
        !matches!(
            event.event,
            RenderEvent::FootnoteMark(_)
                | RenderEvent::BeginFootnote(_)
                | RenderEvent::EndFootnote(_)
        ) || event.meta.producer != EventProducer::ScannerRecovery
    }));
}

#[test]
fn false_conditional_does_not_emit_detached_footnotes() {
    let outcome = capture(
        r"\begin{document}\iffalse\footnotemark[1]\footnotetext{Wrong.}\fi\footnotemark[2]\footnotetext{Right.}\end{document}",
    );
    let marks = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::FootnoteMark(_)))
        .collect::<Vec<_>>();
    let begins = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .collect::<Vec<_>>();

    assert_eq!(marks.len(), 1);
    assert_eq!(begins.len(), 1);
    assert!(matches!(
        &marks[0].event,
        RenderEvent::FootnoteMark(mark) if mark.marker.as_deref() == Some("2")
    ));
    assert_eq!(marks[0].meta.producer, EventProducer::Primitive);
    assert_eq!(begins[0].meta.producer, EventProducer::Primitive);
}

#[test]
fn macro_generated_detached_footnotes_preserve_pairing_and_provenance() {
    let outcome = capture(
        r"\def\marknote{\footnotemark[3]}\def\writenote#1{\footnotetext{#1}}\begin{document}A\marknote B\writenote{Macro note.}\end{document}",
    );
    let events = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::FootnoteMark(_)
                    | RenderEvent::BeginFootnote(_)
                    | RenderEvent::EndFootnote(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|event| event.meta.producer == EventProducer::Macro)
    );
    assert!(events.iter().all(|event| {
        event.meta.source.expansion_stack.iter().any(|frame| {
            matches!(
                frame.command_name.as_deref(),
                Some("marknote" | "writenote")
            )
        })
    }));
    let note_ids = events
        .iter()
        .map(|event| match &event.event {
            RenderEvent::FootnoteMark(mark) => mark.note_id,
            RenderEvent::BeginFootnote(begin) => begin.note_id,
            RenderEvent::EndFootnote(end) => end.note_id,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert!(note_ids.iter().all(|note_id| *note_id == note_ids[0]));
}

#[test]
fn detached_footnote_aliases_use_primitive_semantics() {
    let outcome = capture(
        r"\let\marknote\footnotemark\let\writenote\footnotetext\begin{document}A\marknote[4]\writenote{Alias note.}\end{document}",
    );
    let events = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::FootnoteMark(_)
                    | RenderEvent::BeginFootnote(_)
                    | RenderEvent::EndFootnote(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|event| event.meta.producer == EventProducer::Primitive)
    );
}

#[test]
fn redefining_detached_footnotes_suppresses_scanner_semantics() {
    let outcome = capture(
        r"\begin{document}\def\footnotemark{MARK}\def\footnotetext#1{TEXT #1}\footnotemark\footnotetext{body}\end{document}",
    );

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::FootnoteMark(_)
                | RenderEvent::BeginFootnote(_)
                | RenderEvent::EndFootnote(_)
        )
    }));
}

#[test]
fn pending_footnote_mark_does_not_leak_into_the_next_document() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let first = vm.run_plain(r"\begin{document}A\footnotemark[9]\end{document}");
    let second = vm.run_plain(
        r"\def\writenote#1{\footnotetext{#1}}\begin{document}\writenote{Independent.}\end{document}",
    );
    let first_note_id = first
        .render_events
        .iter()
        .find_map(|event| match event.event {
            RenderEvent::FootnoteMark(ref mark) => Some(mark.note_id),
            _ => None,
        })
        .expect("first document footnote mark");
    let second_note_id = second
        .render_events
        .iter()
        .find_map(|event| match event.event {
            RenderEvent::BeginFootnote(ref begin) => Some(begin.note_id),
            _ => None,
        })
        .expect("second document footnote text");

    assert_ne!(first_note_id, second_note_id);
}

#[test]
fn nested_footnote_mark_stays_paired_with_later_footnote_text() {
    let outcome = capture(
        r"\begin{document}A\footnote{Outer \footnotemark[6].} B\footnotetext{Detached.}\end{document}",
    );
    let mark_note_id = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::FootnoteMark(mark) => Some(mark.note_id),
            _ => None,
        })
        .expect("nested footnote mark");
    let text_note_id = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BeginFootnote(begin)
                if begin.command == FootnoteCommandKind::FootnoteText =>
            {
                Some(begin.note_id)
            }
            _ => None,
        })
        .expect("detached footnote text");

    assert_eq!(mark_note_id, text_note_id);
}

#[test]
fn only_the_latest_pending_footnote_mark_is_consumed_once() {
    let outcome = capture(
        r"\begin{document}\footnotemark[1]\footnotemark[2]\footnotetext{Second.}\footnotetext{Standalone.}\end{document}",
    );
    let mark_ids = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::FootnoteMark(mark) => Some(mark.note_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let body_ids = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginFootnote(begin)
                if begin.command == FootnoteCommandKind::FootnoteText =>
            {
                Some(begin.note_id)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(mark_ids.len(), 2);
    assert_eq!(body_ids.len(), 2);
    assert_eq!(body_ids[0], mark_ids[1]);
    assert_ne!(body_ids[1], mark_ids[0]);
    assert_ne!(body_ids[1], mark_ids[1]);
}

#[test]
fn conflicting_explicit_markers_do_not_pair() {
    let outcome = capture(
        r"\begin{document}\footnotemark[4]\footnotetext[5]{Five.}\footnotetext{Four.}\end{document}",
    );
    let mark = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::FootnoteMark(mark) => Some(mark),
            _ => None,
        })
        .expect("footnote mark");
    let bodies = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginFootnote(begin)
                if begin.command == FootnoteCommandKind::FootnoteText =>
            {
                Some(begin)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(bodies.len(), 2);
    assert_ne!(bodies[0].note_id, mark.note_id);
    assert_eq!(bodies[0].marker.as_deref(), Some("5"));
    assert_eq!(bodies[1].note_id, mark.note_id);
    assert_eq!(bodies[1].marker.as_deref(), Some("4"));
}
