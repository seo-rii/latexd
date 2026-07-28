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
fn ensuremath_emits_an_executed_inline_math_event() {
    let source = r"\begin{document}Value \ensuremath{\alpha_i+\frac{1}{2}}.\end{document}";
    let outcome = capture(source);
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::InlineMath(_)))
        .expect("inline math");

    assert!(matches!(
        &event.event,
        RenderEvent::InlineMath(math) if math.raw_source == r"\alpha_i+\frac{1}{2}"
    ));
    assert_eq!(event.meta.producer, EventProducer::Primitive);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);

    let invocation = r"\ensuremath{\alpha_i+\frac{1}{2}}";
    let invocation_start = source.find(invocation).expect("invocation");
    let content = r"\alpha_i+\frac{1}{2}";
    let content_start = source.find(content).expect("math content");
    assert!(matches!(
        &event.meta.source.primary,
        ProvenanceSpan::File(span)
            if span.start_utf8 as usize == content_start
                && span.end_utf8 as usize == content_start + content.len()
    ));
    assert!(event.meta.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.start_utf8 as usize == invocation_start
                        && span.end_utf8 as usize == invocation_start + invocation.len()
            )
    }));
}

#[test]
fn false_conditional_does_not_emit_ensuremath() {
    let outcome = capture(
        r"\begin{document}
\iffalse
  \ensuremath{wrong}
\fi
\ensuremath{right}
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineMath(math) => Some(math.raw_source.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec!["right"]);
}

#[test]
fn macro_generated_ensuremath_preserves_expansion_provenance() {
    let outcome = capture(
        r"\def\mathwrap#1{\ensuremath{#1^2}}
\begin{document}
\mathwrap{x}
\end{document}",
    );
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::InlineMath(_)))
        .expect("macro-generated inline math");

    assert!(matches!(
        &event.event,
        RenderEvent::InlineMath(math) if math.raw_source == "x^{2}"
    ));
    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("mathwrap"))
    );
}

#[test]
fn ensuremath_alias_uses_execution_semantics() {
    let outcome = capture(
        r"\let\mathify\ensuremath
\begin{document}
\mathify{x+1}
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
fn overridden_ensuremath_does_not_keep_the_scanner_event() {
    let outcome = capture(
        r"\def\ensuremath#1{not math}
\begin{document}
\ensuremath{wrong}
\end{document}",
    );

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::InlineMath(_) | RenderEvent::DisplayMath(_)
        )
    }));
}

#[test]
fn ensuremath_inside_math_does_not_create_a_nested_event() {
    let outcome = capture(r"\begin{document}$a+\ensuremath{b}$\end{document}");
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

    assert_eq!(math.len(), 1);
    assert_eq!(math[0].meta.producer, EventProducer::Primitive);
}
