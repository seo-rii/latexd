use tex_render_model::{
    BibliographyItemEvent, BlockKind, EventProducer, ProvenanceSpan, RenderEvent,
    SemanticConfidence, SourceSpanRole,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmModuleCheckpointKind, VmOutcome};

fn capture(source: &str) -> VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

fn bibliography_items(
    outcome: &VmOutcome,
) -> Vec<(&BibliographyItemEvent, EventProducer, SemanticConfidence)> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => {
                Some((item, event.meta.producer, event.meta.confidence))
            }
            _ => None,
        })
        .collect()
}

fn top_level_text(outcome: &VmOutcome) -> String {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            _ => None,
        })
        .collect()
}

#[test]
fn direct_bibliography_items_are_vm_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\bibitem[Alpha 2024]{alpha} Expanded \textbf{entry}.
\end{thebibliography}
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.key, "alpha");
    assert_eq!(items[0].0.label_hint.as_deref(), Some("Alpha 2024"));
    assert_eq!(items[0].0.text, "Expanded entry.");
    assert_eq!(items[0].1, EventProducer::Primitive);
    assert_eq!(items[0].2, SemanticConfidence::High);
    let item_event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BibliographyItem(_)))
        .expect("bibliography item event");
    assert!(
        item_event
            .meta
            .source
            .related
            .iter()
            .any(|span| span.role == SourceSpanRole::CitationKey)
    );
}

#[test]
fn false_conditionals_do_not_emit_bibliography_items() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\iffalse\bibitem{wrong} Wrong entry.\fi
\bibitem{right} Right entry.
\end{thebibliography}
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.key, "right");
    assert_eq!(items[0].0.text, "Right entry.");
    assert_eq!(items[0].1, EventProducer::Primitive);
}

#[test]
fn macro_generated_bibliography_items_track_expansion_provenance() {
    let outcome = capture(
        r"\def\paperitem#1#2{\bibitem{#1} #2}
\begin{document}
\begin{thebibliography}{1}
\paperitem{macro}{Macro-generated entry.}
\end{thebibliography}
\end{document}",
    );

    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BibliographyItem(_)))
        .expect("macro-generated bibliography item");
    let RenderEvent::BibliographyItem(item) = &event.event else {
        unreachable!();
    };
    assert_eq!(item.key, "macro");
    assert_eq!(item.text, "Macro-generated entry.");
    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(!event.meta.source.expansion_stack.is_empty());
    assert!(!top_level_text(&outcome).contains("Macro-generated entry"));
}

#[test]
fn bibliography_item_aliases_keep_primitive_semantics() {
    let outcome = capture(
        r"\let\paperitem\bibitem
\begin{document}
\begin{thebibliography}{1}
\paperitem{alias} Alias entry.
\end{thebibliography}
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.key, "alias");
    assert_eq!(items[0].0.text, "Alias entry.");
    assert_eq!(items[0].1, EventProducer::Primitive);
    assert!(!top_level_text(&outcome).contains("Alias entry"));
}

#[test]
fn user_overrides_do_not_retain_scanner_bibliography_items() {
    let outcome = capture(
        r"\def\bibitem#1{Overridden #1.}
\begin{document}
\begin{thebibliography}{1}
\bibitem{ghost} Body text.
\end{thebibliography}
\end{document}",
    );

    assert!(bibliography_items(&outcome).is_empty());
}

#[test]
fn bibliography_items_outside_thebibliography_do_not_capture_following_text() {
    let outcome = capture(
        r"\begin{document}
Before \bibitem{misplaced} visible body after the misplaced command.
\end{document}",
    );

    assert!(bibliography_items(&outcome).is_empty());
    assert!(!outcome.render_events.iter().any(|event| {
        matches!(&event.event, RenderEvent::BeginBlock(begin) if begin.block == BlockKind::Bibliography)
            || matches!(&event.event, RenderEvent::EndBlock(end) if end.block == BlockKind::Bibliography)
    }));
    let visible_text = top_level_text(&outcome);
    assert!(
        visible_text.contains("visible body after"),
        "{visible_text:?}"
    );
}

#[test]
fn misplaced_bibliography_items_do_not_retype_later_structure() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.enable_structured_table_events();
    let outcome = vm.run_plain(
        r"\begin{document}
Before \bibitem{misplaced} visible body after the misplaced command.
\section{After misplaced item}
Paragraph after the heading.\footnote{Note after the heading.}
\begin{itemize}\item Item after the heading.\end{itemize}
\begin{tabular}{l}Cell after the heading.\end{tabular}
\ifnum1=0 Hidden conditional.\else Visible conditional.\fi
\end{document}",
    );

    assert!(bibliography_items(&outcome).is_empty());
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::Heading(_)))
            .count(),
        1
    );
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
            .count(),
        1
    );
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::ListItem(_)))
            .count(),
        1
    );
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::Table(_)))
            .count(),
        1
    );
    let visible_text = top_level_text(&outcome);
    assert_eq!(visible_text.matches("visible body after").count(), 1);
    assert_eq!(visible_text.matches("Visible conditional").count(), 1);
    assert!(!visible_text.contains("Hidden conditional"));
}

#[test]
fn scanner_resynchronization_closes_bibliography_text_authority() {
    let source = r"\count0=0
\begin{document}
\ifnum\count0>0\begin{thebibliography}{1}\fi
Before \bibitem{misplaced} gap text.
\end{thebibliography}
\section{After synchronization}
After synchronization text.
\input{barrier}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "Barrier.");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    let checkpoint = outcome
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier input checkpoint");
    let semantic_capture = checkpoint
        .snapshot
        .semantic_capture
        .as_ref()
        .expect("semantic capture");
    let resynchronization_end = source
        .find(r"\end{thebibliography}")
        .map(|start| start + r"\end{thebibliography}".len())
        .expect("bibliography end");
    assert_eq!(
        semantic_capture
            .text
            .forced_execution_ranges
            .iter()
            .map(|range| range.end_utf8)
            .collect::<Vec<_>>(),
        vec![resynchronization_end as u32],
        "bibliography recovery authority must end at structural resynchronization"
    );
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::Heading(_)))
            .count(),
        1
    );
    assert_eq!(
        top_level_text(&outcome)
            .matches("After synchronization text")
            .count(),
        1
    );
}

#[test]
fn repeated_input_bibliography_items_keep_execution_order() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "child.tex",
        r"\ifnum\count0>0\begin{thebibliography}{1}\fi
\bibitem{shared}
\ifnum\count0>0 Second item.\else First body.\fi
\ifnum\count0>0\end{thebibliography}\fi",
    );
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(
        r"\begin{document}
\count0=0 Before first. \input{child} After first.
\count0=1 Before second. \input{child} After second.
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.key, "shared");
    assert_eq!(items[0].0.text, "Second item.");
    let trace = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.clone()),
            RenderEvent::Space(_) => Some(" ".to_string()),
            RenderEvent::BibliographyItem(item) => {
                Some(format!("<bib:{}:{}>", item.key, item.text))
            }
            _ => None,
        })
        .collect::<String>();
    let before_second = trace.find("Before second").expect("second input prefix");
    let bibliography_item = trace.find("<bib:shared:").expect("executed item");
    assert!(
        bibliography_item > before_second,
        "second-execution item was ordered at the first scanner occurrence: {trace:?}"
    );
    assert_eq!(trace.matches("First body.").count(), 1);
}

#[test]
fn repeated_dynamic_input_bibliography_items_keep_occurrence_authority() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "child.tex",
        r"\ifnum\count0>0\begin{thebibliography}{1}\fi
\bibitem{shared}
\ifnum\count0>0 Second item.\else First body.\fi
\ifnum\count0>0\end{thebibliography}\fi",
    );
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(
        r"\begin{document}
\toks0={\input{child}}
\count0=0 Before first. \the\toks0 After first.
\count0=1 Before second. \the\toks0 After second.
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(items[0].0.key, "shared");
    assert_eq!(items[0].0.text, "Second item.");
    let trace = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.clone()),
            RenderEvent::Space(_) => Some(" ".to_string()),
            RenderEvent::BibliographyItem(item) => {
                Some(format!("<bib:{}:{}>", item.key, item.text))
            }
            _ => None,
        })
        .collect::<String>();
    let before_second = trace.find("Before second").expect("second input prefix");
    let bibliography_item = trace.find("<bib:shared:").expect("executed item");
    assert!(bibliography_item > before_second, "{trace:?}");
    assert_eq!(trace.matches("First body.").count(), 1, "{trace:?}");
    assert!(
        !trace.contains("0=0") && !trace.contains("0=1"),
        "{trace:?}"
    );
}

#[test]
fn unexecuted_bibliography_begin_does_not_make_scanner_items_authoritative() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0\begin{thebibliography}{1}\fi
Before \bibitem{misplaced} visible body after the skipped environment.
\end{document}",
    );

    assert!(bibliography_items(&outcome).is_empty());
    let visible_text = top_level_text(&outcome);
    assert!(
        visible_text.contains("visible body after the skipped environment"),
        "{visible_text:?}"
    );
}

#[test]
fn bibliography_items_absorb_nested_semantic_events() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\bibitem{nested} Entry \footnote{Note body} \includegraphics{figure.pdf}.
\section{Nested heading}
\begin{itemize}\item Nested item\end{itemize}
\begin{tabular}{l}Nested cell\end{tabular}
\caption{Nested caption}
\end{thebibliography}
\end{document}",
    );

    let items = bibliography_items(&outcome);
    assert_eq!(items.len(), 1);
    assert!(items[0].0.text.contains("Entry"));
    assert!(items[0].0.text.contains("Note body"));
    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::BeginFootnote(_)
                | RenderEvent::EndFootnote(_)
                | RenderEvent::FootnoteMark(_)
                | RenderEvent::GraphicRef(_)
                | RenderEvent::IncludePdf(_)
                | RenderEvent::Heading(_)
                | RenderEvent::ListItem(_)
                | RenderEvent::Table(_)
                | RenderEvent::Caption(_)
        )
    }));
}

#[test]
fn expanded_bibliography_arguments_preserve_invocation_and_key_spans() {
    let source = r"\def\entrykey{alpha}
\def\entrylabel{Alpha 2024}
\begin{document}
\begin{thebibliography}{1}
\bibitem[\entrylabel]{\entrykey} Entry.
\end{thebibliography}
\end{document}";
    let outcome = capture(source);

    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BibliographyItem(_)))
        .expect("bibliography item event");
    let invocation_start = source.find("\\bibitem").expect("invocation start") as u32;
    let invocation_end = source[invocation_start as usize..]
        .find(" Entry.")
        .map(|offset| invocation_start + offset as u32)
        .expect("invocation end");
    let ProvenanceSpan::File(primary) = &event.meta.source.primary else {
        panic!("file provenance");
    };
    assert_eq!(primary.start_utf8, invocation_start);
    assert_eq!(primary.end_utf8, invocation_end);

    let key_start = source.find("{\\entrykey}").expect("key argument") as u32 + 1;
    let key_end = key_start + "\\entrykey".len() as u32;
    let key_span = event
        .meta
        .source
        .related
        .iter()
        .find(|span| span.role == SourceSpanRole::CitationKey)
        .expect("citation key span");
    let ProvenanceSpan::File(key_span) = &key_span.span else {
        panic!("file key span");
    };
    assert_eq!(
        (key_span.start_utf8, key_span.end_utf8),
        (key_start, key_end)
    );
}
