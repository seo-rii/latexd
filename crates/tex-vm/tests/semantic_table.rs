use tex_render_model::{EventProducer, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.enable_structured_table_events();
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
            RenderEvent::Table(table) if table.environment == "tabular" => Some((table, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(tables.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(tables[0].0.rows.len(), 2);
    assert_eq!(tables[0].0.rows[0].cells[0].text, "Alpha");
    assert_eq!(tables[0].0.rows[0].cells[1].text, "1");
    assert_eq!(tables[0].0.rows[1].cells[0].text, "Beta");
    assert_eq!(tables[0].0.rows[1].cells[1].text, "2");
    assert_eq!(tables[0].1.meta.producer, EventProducer::Primitive);
    assert_eq!(tables[0].1.meta.confidence, SemanticConfidence::High);
    assert!(!outcome.render_events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::RawFallback(fallback)
            if fallback.environment.as_deref() == Some("tabular")
    )));
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
            RenderEvent::Table(table) if table.environment == "tabular" => {
                Some((table.rows[0].cells[0].text.as_str(), event.meta.producer))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tables,
        vec![("Right", EventProducer::Primitive)],
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
            RenderEvent::Table(table)
                if matches!(table.environment.as_str(), "tabularx" | "longtable") =>
            {
                Some((
                    table.environment.as_str(),
                    table.rows[0].cells[0].text.as_str(),
                    event.meta.producer,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tables,
        vec![
            ("tabularx", "Tabular X", EventProducer::Primitive),
            ("longtable", "Long table", EventProducer::Primitive),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn structured_table_preserves_empty_cells() {
    let outcome = capture(
        r"\begin{document}
\begin{tabular}{lll}
A && C \\
& B &
\end{tabular}
\end{document}",
    );
    let table = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("structured table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(
        table.rows[0]
            .cells
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "", "C"]
    );
    assert_eq!(
        table.rows[1]
            .cells
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>(),
        vec!["", "B", ""]
    );
}

#[test]
fn structured_table_preserves_rules_and_cell_spans() {
    let outcome = capture(
        r"\begin{document}
\begin{tabular}{lll}
\hline
A & \multicolumn{2}{r}{B} \\
\cline{2-3}
C & D & E
\end{tabular}
\end{document}",
    );
    let table = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("structured table");

    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.rows.len(), 2);
    assert!(table.rows[0].rule_above);
    assert_eq!(table.rows[0].cells[1].column_span, 2);
    assert_eq!(
        table.rows[0].cells[1].alignment,
        Some(tex_render_model::TableColumnAlignment::Right)
    );
    assert_eq!(table.rows[0].partial_rules_below.len(), 1);
    assert_eq!(table.rows[0].partial_rules_below[0].start_column, 1);
    assert_eq!(table.rows[0].partial_rules_below[0].end_column, 2);
}

#[test]
fn structured_longtable_extracts_caption_from_recovery_payload() {
    let outcome = capture(
        r"\begin{document}
\begin{longtable}{ll}
\caption{Long table.}\\
Alpha & Beta
\end{longtable}
\end{document}",
    );
    let table = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::Table(table) if table.environment == "longtable" => Some(table),
            _ => None,
        })
        .expect("structured longtable");

    assert_eq!(table.caption.as_deref(), Some("Long table."));
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells[0].text, "Alpha");
    assert_eq!(table.rows[0].cells[1].text, "Beta");
}
