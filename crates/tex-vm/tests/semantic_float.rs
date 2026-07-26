use tex_render_model::{BlockKind, EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

#[derive(Debug, Clone, PartialEq, Eq)]
enum FloatBoundary {
    Begin(BlockKind),
    End(BlockKind),
}

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

fn boundaries(
    outcome: &tex_vm::VmOutcome,
) -> Vec<(FloatBoundary, EventProducer, SemanticConfidence)> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginBlock(begin) if is_float(&begin.block) => Some((
                FloatBoundary::Begin(begin.block.clone()),
                event.meta.producer,
                event.meta.confidence,
            )),
            RenderEvent::EndBlock(end) if is_float(&end.block) => Some((
                FloatBoundary::End(end.block.clone()),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect()
}

fn is_float(block: &BlockKind) -> bool {
    matches!(
        block,
        BlockKind::Figure
            | BlockKind::FullWidthFigure
            | BlockKind::Table
            | BlockKind::FullWidthTable
    )
}

#[test]
fn direct_float_boundaries_are_vm_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{figure}\end{figure}
\begin{figure*}\end{figure*}
\begin{table}\end{table}
\begin{table*}\end{table*}
\end{document}",
    );

    assert_eq!(
        boundaries(&outcome),
        vec![
            (
                FloatBoundary::Begin(BlockKind::Figure),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::End(BlockKind::Figure),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::Begin(BlockKind::FullWidthFigure),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::End(BlockKind::FullWidthFigure),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::Begin(BlockKind::Table),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::End(BlockKind::Table),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::Begin(BlockKind::FullWidthTable),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::End(BlockKind::FullWidthTable),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_does_not_emit_float_boundaries() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\begin{figure}\end{figure}
\begin{table*}\end{table*}
\fi
\begin{table}\end{table}
\end{document}",
    );

    assert_eq!(
        boundaries(&outcome),
        vec![
            (
                FloatBoundary::Begin(BlockKind::Table),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                FloatBoundary::End(BlockKind::Table),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_float_preserves_order_and_invocation_provenance() {
    let source = r"\def\widefigure#1{\begin{figure*}#1\end{figure*}}
\begin{document}
\widefigure{Body.}
\end{document}";
    let outcome = capture(source);
    let begin = outcome
        .render_events
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                RenderEvent::BeginBlock(begin) if begin.block == BlockKind::FullWidthFigure
            )
        })
        .expect("macro-generated float begin");
    let ProvenanceSpan::File(primary) = &begin.meta.source.primary else {
        panic!("macro-generated float should retain an invocation source");
    };

    assert_eq!(begin.meta.producer, EventProducer::Macro);
    assert_eq!(begin.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        &source[primary.start_utf8 as usize..primary.end_utf8 as usize],
        r"\widefigure{Body.}"
    );
    assert_eq!(
        begin
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("widefigure")
    );

    let structure = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginBlock(begin) if begin.block == BlockKind::FullWidthFigure => {
                Some("begin")
            }
            RenderEvent::Text(text) if text.text == "Body." => Some("body"),
            RenderEvent::EndBlock(end) if end.block == BlockKind::FullWidthFigure => Some("end"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(structure, vec!["begin", "body", "end"]);
}

#[test]
fn supported_float_aliases_keep_their_layout_kind() {
    let cases = [
        ("figwindow", BlockKind::Figure),
        ("figure", BlockKind::Figure),
        ("wrapfigure", BlockKind::Figure),
        ("wrapfigure*", BlockKind::Figure),
        ("SCfigure", BlockKind::Figure),
        ("floatingfigure", BlockKind::Figure),
        ("marginfigure", BlockKind::Figure),
        ("measuredfigure", BlockKind::Figure),
        ("figure*", BlockKind::FullWidthFigure),
        ("sidewaysfigure", BlockKind::FullWidthFigure),
        ("sidewaysfigure*", BlockKind::FullWidthFigure),
        ("SCfigure*", BlockKind::FullWidthFigure),
        ("marginfigure*", BlockKind::FullWidthFigure),
        ("tabwindow", BlockKind::Table),
        ("table", BlockKind::Table),
        ("wraptable", BlockKind::Table),
        ("wraptable*", BlockKind::Table),
        ("SCtable", BlockKind::Table),
        ("floatingtable", BlockKind::Table),
        ("margintable", BlockKind::Table),
        ("table*", BlockKind::FullWidthTable),
        ("sidewaystable", BlockKind::FullWidthTable),
        ("sidewaystable*", BlockKind::FullWidthTable),
        ("SCtable*", BlockKind::FullWidthTable),
        ("margintable*", BlockKind::FullWidthTable),
    ];

    for (environment, expected) in cases {
        let source = format!(
            "\\begin{{document}}\\begin{{{environment}}}\\end{{{environment}}}\\end{{document}}"
        );
        let outcome = capture(&source);
        assert_eq!(
            boundaries(&outcome),
            vec![
                (
                    FloatBoundary::Begin(expected.clone()),
                    EventProducer::Primitive,
                    SemanticConfidence::High,
                ),
                (
                    FloatBoundary::End(expected),
                    EventProducer::Primitive,
                    SemanticConfidence::High,
                ),
            ],
            "{environment}: {:#?}",
            outcome.render_events
        );
    }
}

#[test]
fn float_placement_options_do_not_become_visible_text() {
    let outcome = capture(r"\begin{document}\begin{figure}[htbp]\end{figure}\end{document}");
    assert!(!outcome.render_events.iter().any(
        |event| matches!(&event.event, RenderEvent::Text(text) if text.text.contains("htbp"))
    ));
    assert!(!outcome.output.contains("htbp"), "{:?}", outcome.output);
}
