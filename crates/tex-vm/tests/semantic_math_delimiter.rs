use tex_render_model::{
    EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence, SourceSpanRole,
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
fn command_math_delimiters_emit_executed_events() {
    let source = r"\begin{document}Inline \(x^2\). Display \[\alpha \le \beta\].\end{document}";
    let outcome = capture(source);
    let math = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::InlineMath(_) | RenderEvent::DisplayMath(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(math.len(), 2);
    assert!(matches!(
        &math[0].event,
        RenderEvent::InlineMath(event) if event.raw_source == "x^2"
    ));
    assert!(matches!(
        &math[1].event,
        RenderEvent::DisplayMath(event) if event.raw_source == r"\alpha \le \beta"
    ));
    for event in math {
        assert_eq!(event.meta.producer, EventProducer::Primitive);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
    }
    for invocation in [r"\(x^2\)", r"\[\alpha \le \beta\]"] {
        assert!(outcome.render_events.iter().any(|event| {
            matches!(
                event.event,
                RenderEvent::InlineMath(_) | RenderEvent::DisplayMath(_)
            ) && event.meta.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if &source[span.start_utf8 as usize..span.end_utf8 as usize]
                                == invocation
                    )
            })
        }));
    }
}

#[test]
fn false_conditional_does_not_emit_command_delimited_math() {
    let outcome = capture(
        r"\begin{document}
\iffalse
  \(wrong\)
  \[also wrong\]
\fi
\(right\)
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => {
                Some(math.raw_source.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec!["right"]);
}

#[test]
fn macro_generated_command_math_preserves_expansion_provenance() {
    let outcome = capture(
        r"\def\emitmath#1{\(#1^2\)}
\begin{document}
\emitmath{x}
\end{document}",
    );
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::InlineMath(_)))
        .expect("macro-generated inline math");

    let RenderEvent::InlineMath(math) = &event.event else {
        panic!("expected inline math");
    };
    assert_eq!(math.raw_source, "x^{2}");
    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("emitmath"))
    );
}

#[test]
fn command_math_aliases_use_execution_semantics() {
    let outcome = capture(
        r"\let\openmath\(
\let\closemath\)
\begin{document}
\openmath x+1\closemath
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineMath(math) => Some((math.raw_source.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec![("x+1", EventProducer::Primitive)]);
}

#[test]
fn overridden_command_math_delimiters_do_not_keep_scanner_events() {
    let outcome = capture(
        r"\def\({}
\def\){}
\begin{document}
\(not math\)
\end{document}",
    );

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::InlineMath(_) | RenderEvent::DisplayMath(_)
        )
    }));
}
