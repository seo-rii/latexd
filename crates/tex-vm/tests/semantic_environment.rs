use tex_render_model::{BlockKind, EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

fn block_events(
    outcome: &tex_vm::VmOutcome,
) -> Vec<(&'static str, BlockKind, EventProducer, SemanticConfidence)> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginBlock(begin) => Some((
                "begin",
                begin.block.clone(),
                event.meta.producer,
                event.meta.confidence,
            )),
            RenderEvent::EndBlock(end) => Some((
                "end",
                end.block.clone(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn executed_abstract_environment_is_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{abstract}
Summary.
\end{abstract}
\end{document}",
    );

    assert_eq!(
        block_events(&outcome),
        vec![
            (
                "begin",
                BlockKind::Abstract,
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                "end",
                BlockKind::Abstract,
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_does_not_emit_environment_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
\begin{quote}Wrong.\end{quote}
\fi
\begin{quote}Right.\end{quote}
\end{document}",
    );
    let blocks = block_events(&outcome);

    assert_eq!(blocks.len(), 2, "{:#?}", outcome.render_events);
    assert!(blocks.iter().all(|(_, block, producer, confidence)| {
        block
            == &BlockKind::Environment {
                name: "quote".to_string(),
            }
            && *producer == EventProducer::Primitive
            && *confidence == SemanticConfidence::High
    }));
}

#[test]
fn macro_generated_environment_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\quotation#1{\begin{quote}#1\end{quote}}
\begin{document}
\quotation{Expanded body.}
\end{document}",
    );
    let environment_events = outcome
        .render_events
        .iter()
        .filter(|event| match &event.event {
            RenderEvent::BeginBlock(event) => {
                event.block
                    == BlockKind::Environment {
                        name: "quote".to_string(),
                    }
            }
            RenderEvent::EndBlock(event) => {
                event.block
                    == BlockKind::Environment {
                        name: "quote".to_string(),
                    }
            }
            _ => false,
        })
        .collect::<Vec<_>>();

    assert_eq!(environment_events.len(), 2, "{:#?}", outcome.render_events);
    for event in environment_events {
        assert_eq!(event.meta.producer, EventProducer::Macro);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert_eq!(
            event
                .meta
                .source
                .expansion_stack
                .last()
                .and_then(|frame| frame.command_name.as_deref()),
            Some("quotation")
        );
        let ProvenanceSpan::File(primary) = &event.meta.source.primary else {
            panic!("macro environment should retain a file source");
        };
        assert!(
            primary.start_utf8 < primary.end_utf8,
            "{:#?}",
            event.meta.source
        );
    }

    let semantic_content = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::BeginBlock(event)
                if event.block
                    == BlockKind::Environment {
                        name: "quote".to_string(),
                    } =>
            {
                Some("begin".to_string())
            }
            RenderEvent::Text(event) if envelope.meta.producer == EventProducer::Macro => {
                Some(event.text.clone())
            }
            RenderEvent::Space(_) if envelope.meta.producer == EventProducer::Macro => {
                Some(" ".to_string())
            }
            RenderEvent::EndBlock(event)
                if event.block
                    == BlockKind::Environment {
                        name: "quote".to_string(),
                    } =>
            {
                Some("end".to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        semantic_content,
        vec!["begin", "Expanded", " ", "body.", "end"],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn nested_environment_execution_preserves_begin_end_order() {
    let outcome = capture(
        r"\begin{document}
\begin{quote}
\begin{center}Nested.\end{center}
\end{quote}
\end{document}",
    );
    let order = block_events(&outcome)
        .into_iter()
        .map(|(boundary, block, producer, _)| (boundary, block, producer))
        .collect::<Vec<_>>();

    assert_eq!(
        order,
        vec![
            (
                "begin",
                BlockKind::Environment {
                    name: "quote".to_string(),
                },
                EventProducer::Primitive,
            ),
            (
                "begin",
                BlockKind::Environment {
                    name: "center".to_string(),
                },
                EventProducer::Primitive,
            ),
            (
                "end",
                BlockKind::Environment {
                    name: "center".to_string(),
                },
                EventProducer::Primitive,
            ),
            (
                "end",
                BlockKind::Environment {
                    name: "quote".to_string(),
                },
                EventProducer::Primitive,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_internal_false_branch_preserves_later_environment() {
    let outcome = capture(
        r"\def\conditionalquote{\iffalse\begin{quote}Wrong.\end{quote}\fi\begin{center}Right.\end{center}}
\begin{document}
\conditionalquote
\end{document}",
    );
    let blocks = block_events(&outcome);

    assert_eq!(
        blocks,
        vec![
            (
                "begin",
                BlockKind::Environment {
                    name: "center".to_string(),
                },
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
            (
                "end",
                BlockKind::Environment {
                    name: "center".to_string(),
                },
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
    assert!(
        outcome.render_events.iter().any(
            |event| matches!(&event.event, RenderEvent::Text(text) if text.text == "Right.")
                && event.meta.producer == EventProducer::Macro
        ),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_discards_scanner_only_float_children() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\begin{figure}
\includegraphics{hidden.pdf}
\caption{Hidden caption.}
\end{figure}
\fi
Visible.
\end{document}",
    );

    assert!(
        !outcome.render_events.iter().any(|event| matches!(
            event.event,
            RenderEvent::BeginBlock(_)
                | RenderEvent::EndBlock(_)
                | RenderEvent::GraphicRef(_)
                | RenderEvent::Caption(_)
        )),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn nested_macro_environment_anchors_to_the_outer_invocation() {
    let source = r"\def\innerquote#1{\begin{quote}#1\end{quote}}
\def\outerquote#1{\innerquote{#1}}
\begin{document}
\outerquote{Nested macro body.}
\end{document}";
    let outcome = capture(source);
    let begin = outcome
        .render_events
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                RenderEvent::BeginBlock(event)
                    if event.block
                        == BlockKind::Environment {
                            name: "quote".to_string(),
                        }
            )
        })
        .expect("nested macro environment");
    let ProvenanceSpan::File(primary) = &begin.meta.source.primary else {
        panic!("nested macro environment should retain a file source");
    };

    assert_eq!(
        &source[primary.start_utf8 as usize..primary.end_utf8 as usize],
        r"\outerquote{Nested macro body.}"
    );
    assert_eq!(
        begin
            .meta
            .source
            .expansion_stack
            .iter()
            .filter_map(|frame| frame.command_name.as_deref())
            .collect::<Vec<_>>(),
        vec!["outerquote", "innerquote"]
    );
    let ordered = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BeginBlock(_) => Some("begin"),
            RenderEvent::Text(text) if text.text == "Nested" => Some("body_start"),
            RenderEvent::EndBlock(_) => Some("end"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        vec!["begin", "body_start", "end"],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn migrated_list_is_authoritative_while_dynamic_theorem_uses_scanner_recovery() {
    let outcome = capture(
        r"\newtheorem{claim}{Claim}
\begin{document}
\begin{itemize}\item Entry.\end{itemize}
\begin{claim}Statement.\end{claim}
\end{document}",
    );
    let blocks = block_events(&outcome);

    assert!(blocks.iter().any(|(_, block, producer, confidence)| {
        matches!(
            block,
            BlockKind::List {
                list_kind: tex_render_model::ListKind::Unordered
            }
        ) && *producer == EventProducer::Primitive
            && *confidence == SemanticConfidence::High
    }));
    assert!(blocks.iter().any(|(_, block, producer, confidence)| {
        block
            == &BlockKind::Environment {
                name: "claim".to_string(),
            }
            && *producer == EventProducer::ScannerRecovery
            && *confidence == SemanticConfidence::Medium
    }));
}
