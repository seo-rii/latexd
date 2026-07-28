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
fn executed_label_is_authoritative() {
    let source = r"\begin{document}\label{sec:intro}\end{document}";
    let outcome = capture(source);
    let labels = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::LabelDefinition(_)))
        .collect::<Vec<_>>();

    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].meta.producer, EventProducer::Primitive);
    assert_eq!(labels[0].meta.confidence, SemanticConfidence::High);
    assert!(matches!(
        &labels[0].meta.source.primary,
        ProvenanceSpan::File(span)
            if &source[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
    ));
    assert!(labels[0].meta.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if &source[span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\label{sec:intro}"
            )
    }));
}

#[test]
fn false_conditional_does_not_emit_label_definitions() {
    let outcome = capture(r"\begin{document}\iffalse\label{wrong}\fi\label{right}\end{document}");
    let keys = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::LabelDefinition(label) => Some(label.key.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["right"]);
}

#[test]
fn macro_generated_label_emits_at_the_invocation() {
    let source = r"\def\emitlabel#1{\label{#1}}\begin{document}\emitlabel{sec:macro}\end{document}";
    let outcome = capture(source);
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::LabelDefinition(_)))
        .expect("macro-generated label");

    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(event.meta.confidence, SemanticConfidence::High);
    assert!(
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("emitlabel"))
    );
}

#[test]
fn overridden_label_does_not_retain_scanner_semantics() {
    let outcome = capture(r"\def\label#1{}\begin{document}\label{wrong}\end{document}");

    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::LabelDefinition(_)))
    );
}

#[test]
fn label_alias_reuses_the_scanner_event_identity() {
    let outcome = capture(r"\let\mylabel\label\begin{document}\mylabel{sec:alias}\end{document}");
    let labels = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::LabelDefinition(label) => Some((event, label)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].0.meta.producer, EventProducer::Primitive);
    assert_eq!(labels[0].1.key, "sec:alias");
    assert_eq!(labels[0].1.command, "label");
}
