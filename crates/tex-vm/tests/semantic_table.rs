use tex_render_model::{EventProducer, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn executed_tabular_recovery_is_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{tabular}{lr}
Alpha & 1 \\
Beta & 2
\end{tabular}
\end{document}",
    );
    let tables = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tabular") =>
            {
                Some((fallback, event))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(tables.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(
        tables[0].0.normalized_visible_text.as_deref(),
        Some("Alpha | 1 ; Beta | 2")
    );
    assert_eq!(tables[0].1.meta.producer, EventProducer::Primitive);
    assert_eq!(tables[0].1.meta.confidence, SemanticConfidence::High);
}

#[test]
fn false_conditional_table_recovery_is_discarded() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\begin{tabular}{l}Wrong\end{tabular}
\fi
\begin{tabular}{l}Right\end{tabular}
\end{document}",
    );
    let tables = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tabular") =>
            {
                Some((
                    fallback.normalized_visible_text.as_deref(),
                    event.meta.producer,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tables,
        vec![(Some("Right"), EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn executed_table_environment_kinds_do_not_cross_match() {
    let outcome = capture(
        r"\begin{document}
\begin{tabularx}{\textwidth}{l}Tabular X\end{tabularx}
\begin{longtable}{l}Long table\end{longtable}
\end{document}",
    );
    let tables = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("tabularx" | "longtable")
                ) =>
            {
                Some((
                    fallback.environment.as_deref(),
                    fallback.normalized_visible_text.as_deref(),
                    event.meta.producer,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tables,
        vec![
            (
                Some("tabularx"),
                Some("Tabular X"),
                EventProducer::Primitive,
            ),
            (
                Some("longtable"),
                Some("Long table"),
                EventProducer::Primitive,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}
