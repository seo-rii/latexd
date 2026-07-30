use tex_render_model::{EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.enable_structured_table_events();
    vm.run_plain(source)
}

fn capture_without_structured_tables(source: &str) -> tex_vm::VmOutcome {
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
    assert!(tables[0].0.rows.iter().all(|row| row.source.is_none()));
    assert!(
        tables[0]
            .0
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .all(|cell| cell.source.is_none())
    );
    assert_eq!(tables[0].1.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(tables[0].1.meta.confidence, SemanticConfidence::Medium);
    assert!(!outcome.render_events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::RawFallback(fallback)
            if fallback.environment.as_deref() == Some("tabular")
    )));
}

#[test]
fn unskip_removes_trailing_space_inside_structured_table_cells() {
    let outcome = capture(
        r"\begin{document}
\begin{tabular}{ll}
Alpha \unskip. & Solid\unskip.
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

    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells[0].text, "Alpha.");
    assert_eq!(table.rows[0].cells[1].text, "Solid.");
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
        vec![("Right", EventProducer::ScannerRecovery)],
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
            ("tabularx", "Tabular X", EventProducer::ScannerRecovery),
            ("longtable", "Long table", EventProducer::ScannerRecovery),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn structured_table_recovery_omits_ambiguous_empty_placeholders() {
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
        vec!["A", "C"]
    );
    assert_eq!(
        table.rows[1]
            .cells
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>(),
        vec!["B"]
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

#[test]
fn macro_generated_table_is_captured_from_vm_execution() {
    let source = r"\def\makerow#1#2{#1 & #2 \\}
\def\maketable{\begin{tabular}{lr}\makerow{Alpha}{1}\makerow{Beta}{2}\end{tabular}}
\begin{document}
\maketable
\end{document}";
    let outcome = capture(source);
    let tables = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Table(table) if table.environment == "tabular" => Some((table, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(tables.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(tables[0].0.columns.len(), 2);
    assert_eq!(tables[0].0.rows.len(), 2);
    assert_eq!(tables[0].0.rows[0].cells[0].text, "Alpha");
    assert_eq!(tables[0].0.rows[0].cells[1].text, "1");
    assert_eq!(tables[0].0.rows[1].cells[0].text, "Beta");
    assert_eq!(tables[0].0.rows[1].cells[1].text, "2");
    for (cell, expected) in tables[0]
        .0
        .rows
        .iter()
        .flat_map(|row| &row.cells)
        .zip(["Alpha", "1", "Beta", "2"])
    {
        let provenance = cell.source.as_ref().expect("cell source");
        let ProvenanceSpan::File(span) = &provenance.primary else {
            panic!("expected file-backed cell source");
        };
        assert_eq!(
            &source[span.start_utf8 as usize..span.end_utf8 as usize],
            expected
        );
        assert!(
            provenance
                .expansion_stack
                .iter()
                .any(|frame| frame.command_name.as_deref() == Some("makerow")),
            "{provenance:#?}"
        );
    }
    assert!(tables[0].0.rows.iter().all(|row| row.source.is_some()));
    assert_eq!(tables[0].1.meta.producer, EventProducer::Macro);
    assert_eq!(tables[0].1.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        tables[0]
            .1
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("maketable")
    );
}

#[test]
fn false_conditional_does_not_emit_macro_generated_table() {
    let outcome = capture(
        r"\def\maketable#1{\begin{tabular}{l}#1\end{tabular}}
\begin{document}
\iffalse\maketable{Wrong}\fi
\maketable{Right}
\end{document}",
    );
    let tables = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Table(table) if table.environment == "tabular" => {
                Some(table.rows[0].cells[0].text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(tables, vec!["Right"], "{:#?}", outcome.render_events);
}

#[test]
fn disabled_structured_table_capture_preserves_legacy_row_break_arguments() {
    let outcome = capture_without_structured_tables(
        r"\def\start#1{\begin{#1}}
\def\stop#1{\end{#1}}
\def\maketable#1{\start{tabular}{l}#1\\[3pt]After\stop{tabular}}
\begin{document}
\maketable{Visible}
\end{document}",
    );

    assert!(
        outcome.output.contains("[3pt]"),
        "legacy output: {:?}",
        outcome.output
    );
    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(&event.event, RenderEvent::Table(_))),
        "{:#?}",
        outcome.render_events
    );
}
