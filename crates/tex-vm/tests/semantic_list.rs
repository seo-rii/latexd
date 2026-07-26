use tex_render_model::{
    BlockKind, EventProducer, ListKind, ProvenanceSpan, RenderEvent, SemanticConfidence,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListStructure {
    Begin(ListKind),
    Item(Option<String>),
    End(ListKind),
}

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

fn structures(
    outcome: &tex_vm::VmOutcome,
) -> Vec<(ListStructure, EventProducer, SemanticConfidence)> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginBlock(begin) => match begin.block {
                BlockKind::List { list_kind } => Some((
                    ListStructure::Begin(list_kind),
                    event.meta.producer,
                    event.meta.confidence,
                )),
                _ => None,
            },
            RenderEvent::ListItem(item) => Some((
                ListStructure::Item(item.marker.clone()),
                event.meta.producer,
                event.meta.confidence,
            )),
            RenderEvent::EndBlock(end) => match end.block {
                BlockKind::List { list_kind } => Some((
                    ListStructure::End(list_kind),
                    event.meta.producer,
                    event.meta.confidence,
                )),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn executed_list_boundaries_and_items_are_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{itemize}
\item First.
\item[Custom] Second.
\end{itemize}
\end{document}",
    );

    assert_eq!(
        structures(&outcome),
        vec![
            (
                ListStructure::Begin(ListKind::Unordered),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                ListStructure::Item(None),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                ListStructure::Item(Some("Custom".to_string())),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                ListStructure::End(ListKind::Unordered),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_does_not_emit_list_semantics() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\begin{itemize}\item Wrong.\end{itemize}
\fi
\begin{enumerate}\item Right.\end{enumerate}
\end{document}",
    );

    assert_eq!(
        structures(&outcome),
        vec![
            (
                ListStructure::Begin(ListKind::Ordered),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                ListStructure::Item(None),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                ListStructure::End(ListKind::Ordered),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_list_preserves_execution_order() {
    let outcome = capture(
        r"\def\entries#1{\begin{itemize}\item First #1.\item Second #1.\end{itemize}}
\begin{document}
\entries{entry}
\end{document}",
    );
    let structure = structures(&outcome);

    assert_eq!(
        structure,
        vec![
            (
                ListStructure::Begin(ListKind::Unordered),
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
            (
                ListStructure::Item(None),
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
            (
                ListStructure::Item(None),
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
            (
                ListStructure::End(ListKind::Unordered),
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );

    let begin = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::BeginBlock(_)))
        .expect("list begin");
    let items = outcome
        .render_events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event.event, RenderEvent::ListItem(_)).then_some(index)
        })
        .collect::<Vec<_>>();
    let first_text = outcome
        .render_events
        .iter()
        .position(|event| matches!(&event.event, RenderEvent::Text(text) if text.text == "First"))
        .expect("first item text");
    let second_text = outcome
        .render_events
        .iter()
        .position(|event| matches!(&event.event, RenderEvent::Text(text) if text.text == "Second"))
        .expect("second item text");
    let end = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::EndBlock(_)))
        .expect("list end");

    assert!(
        begin < items[0]
            && items[0] < first_text
            && first_text < items[1]
            && items[1] < second_text
            && second_text < end,
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_item_emits_at_the_macro_invocation() {
    let outcome = capture(
        r"\def\entry#1{\item #1}
\begin{document}
\begin{itemize}
\entry{Body.}
\end{itemize}
\end{document}",
    );
    let item = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::ListItem(_)))
        .expect("macro-generated list item");

    assert_eq!(item.meta.producer, EventProducer::Macro);
    assert_eq!(item.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        item.meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("entry")
    );
}

#[test]
fn description_marker_is_normalized_and_source_backed() {
    let source = r"\begin{document}
\begin{description}
\item[\textbf{Term}] Meaning.
\end{description}
\end{document}";
    let outcome = capture(source);
    let item = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::ListItem(_)))
        .expect("description item");
    let RenderEvent::ListItem(item_event) = &item.event else {
        unreachable!();
    };
    let ProvenanceSpan::File(primary) = &item.meta.source.primary else {
        panic!("description item should retain a file source");
    };

    assert_eq!(item_event.marker.as_deref(), Some("Term"));
    assert_eq!(item.meta.producer, EventProducer::Primitive);
    assert_eq!(
        &source[primary.start_utf8 as usize..primary.end_utf8 as usize],
        r"\item[\textbf{Term}]"
    );
}

#[test]
fn list_item_marker_uses_the_executed_macro_value() {
    let outcome = capture(
        r"\def\term{Term}
\begin{document}
\begin{description}
\item[\term] Meaning.
\end{description}
\end{document}",
    );
    let marker = outcome.render_events.iter().find_map(|event| {
        let RenderEvent::ListItem(item) = &event.event else {
            return None;
        };
        item.marker.as_deref()
    });

    assert_eq!(marker, Some("Term"), "{:#?}", outcome.render_events);
}

#[test]
fn nested_lists_preserve_structural_order() {
    let outcome = capture(
        r"\begin{document}
\begin{itemize}
\item Outer.
\begin{enumerate}
\item Inner.
\end{enumerate}
\item Tail.
\end{itemize}
\end{document}",
    );

    assert_eq!(
        structures(&outcome)
            .into_iter()
            .map(|(structure, _, _)| structure)
            .collect::<Vec<_>>(),
        vec![
            ListStructure::Begin(ListKind::Unordered),
            ListStructure::Item(None),
            ListStructure::Begin(ListKind::Ordered),
            ListStructure::Item(None),
            ListStructure::End(ListKind::Ordered),
            ListStructure::Item(None),
            ListStructure::End(ListKind::Unordered),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn same_kind_nested_lists_preserve_structural_order() {
    let outcome = capture(
        r"\begin{document}
\begin{itemize}
\item Outer.
\begin{itemize}
\item Inner.
\end{itemize}
\item Tail.
\end{itemize}
\end{document}",
    );

    assert_eq!(
        structures(&outcome)
            .into_iter()
            .map(|(structure, _, _)| structure)
            .collect::<Vec<_>>(),
        vec![
            ListStructure::Begin(ListKind::Unordered),
            ListStructure::Item(None),
            ListStructure::Begin(ListKind::Unordered),
            ListStructure::Item(None),
            ListStructure::End(ListKind::Unordered),
            ListStructure::Item(None),
            ListStructure::End(ListKind::Unordered),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn repeated_list_environment_options_do_not_become_visible_text() {
    let outcome = capture(
        r"\begin{document}
\begin{enumerate}[label=(\roman*)][leftmargin=*]
\item Entry.
\end{enumerate}
\end{document}",
    );
    let text = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert_eq!(text, "Entry.", "{:#?}", outcome.render_events);
    assert!(
        !outcome.output.contains("leftmargin"),
        "{:?}",
        outcome.output
    );
}

#[test]
fn item_outside_a_list_is_not_a_semantic_list_item() {
    let outcome = capture(r"\begin{document}Before. \item Stray. After.\end{document}");

    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::ListItem(_))),
        "{:#?}",
        outcome.render_events
    );
}
