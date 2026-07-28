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
fn simple_display_math_environments_emit_executed_events() {
    let source = r"\begin{document}
\begin{equation}x^2\end{equation}
\begin{displaymath}\alpha \le \beta\end{displaymath}
\end{document}";
    let outcome = capture(source);
    let math = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::DisplayMath(_)))
        .collect::<Vec<_>>();

    assert_eq!(math.len(), 2);
    assert!(matches!(
        &math[0].event,
        RenderEvent::DisplayMath(event) if event.raw_source == "x^2"
    ));
    assert!(matches!(
        &math[1].event,
        RenderEvent::DisplayMath(event) if event.raw_source == r"\alpha \le \beta"
    ));
    for event in &math {
        assert_eq!(event.meta.producer, EventProducer::Primitive);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
    }

    let invocation = r"\begin{equation}x^2\end{equation}";
    let invocation_start = source.find(invocation).expect("equation invocation");
    assert!(math[0].meta.source.related.iter().any(|related| {
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
fn false_conditional_does_not_emit_display_math_environment() {
    let outcome = capture(
        r"\begin{document}
\iffalse
  \begin{align}wrong&=value\end{align}
\fi
\begin{align}right&=value\end{align}
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::DisplayMath(math) => Some(math.raw_source.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec!["right&=value"]);
}

#[test]
fn macro_generated_display_math_environment_preserves_expansion_provenance() {
    let outcome = capture(
        r"\def\formula#1{\begin{gather}#1^2\end{gather}}
\begin{document}
\formula{x}
\end{document}",
    );
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::DisplayMath(_)))
        .expect("macro-generated display math");

    assert!(matches!(
        &event.event,
        RenderEvent::DisplayMath(math) if math.raw_source == "x^{2}"
    ));
    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("formula"))
    );
}

#[test]
fn display_math_environment_aliases_use_execution_semantics() {
    let outcome = capture(
        r"\let\startenv\begin
\let\stopenv\end
\begin{document}
\startenv{multline}x+1\stopenv{multline}
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::DisplayMath(math) => Some((math.raw_source.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec![("x+1", EventProducer::Primitive)]);
}

#[test]
fn overridden_environment_commands_do_not_keep_scanner_math() {
    let outcome = capture(
        r"\begin{document}
\def\begin#1{}
\def\end#1{}
\begin{eqnarray}not&=&math\end{eqnarray}",
    );

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::InlineMath(_) | RenderEvent::DisplayMath(_)
        )
    }));
}

#[test]
fn aligned_math_environments_emit_executed_events() {
    let outcome = capture(
        r"\begin{document}
\begin{align}a&=b\\c&=d\end{align}
\begin{flalign*}e&=f\end{flalign*}
\begin{gather*}g=h\\i=j\end{gather*}
\begin{multline}k+l\\=m+n\end{multline}
\begin{eqnarray*}u&=&v\end{eqnarray*}
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::DisplayMath(_)))
        .collect::<Vec<_>>();

    assert_eq!(math.len(), 5);
    for event in math {
        assert_eq!(event.meta.producer, EventProducer::Primitive);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
    }
}

#[test]
fn alignat_column_count_is_not_math_content() {
    let source = r"\begin{document}\begin{alignat*}{2}x&=y & z&=w\end{alignat*}\end{document}";
    let outcome = capture(source);
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::DisplayMath(_)))
        .expect("alignat display math");

    assert!(matches!(
        &event.event,
        RenderEvent::DisplayMath(math) if math.raw_source == "x&=y & z&=w"
    ));
    assert_eq!(event.meta.producer, EventProducer::Primitive);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
}
