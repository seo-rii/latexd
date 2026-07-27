use tex_render_model::{
    EventProducer, GraphicAssetFormat, ProvenanceSpan, RenderEvent, SourceSpanRole,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind, VmModuleCheckpointKind, VmSnapshot};

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn executed_epsfig_and_psfig_events_are_authoritative() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.mount_file("figures/plot.eps", "fixture");
    vm.mount_file("figures/other.eps", "fixture");
    let source = r"\begin{document}
\epsfig{file=figures/plot,width=5cm}
\psfig{figure={figures/other},height=2cm}
\end{document}";
    let outcome = vm.run_plain(source);
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some((graphic, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(graphics.len(), 2, "{:#?}", outcome.render_events);
    assert_eq!(graphics[0].0.path, "figures/plot.eps");
    assert_eq!(
        graphics[0].0.options.as_deref(),
        Some("file=figures/plot,width=5cm")
    );
    assert_eq!(graphics[0].0.asset_format, Some(GraphicAssetFormat::Eps));
    assert_eq!(graphics[0].1.meta.producer, EventProducer::Primitive);
    let path_span = graphics[0]
        .1
        .meta
        .source
        .related
        .iter()
        .find_map(|related| {
            if related.role != SourceSpanRole::ArgumentContent {
                return None;
            }
            match &related.span {
                ProvenanceSpan::File(span) => Some(span),
                ProvenanceSpan::Generated(_) => None,
            }
        })
        .expect("legacy graphic path span");
    assert_eq!(
        &source[path_span.start_utf8 as usize..path_span.end_utf8 as usize],
        "figures/plot"
    );
    assert_eq!(graphics[1].0.path, "figures/other.eps");
    assert_eq!(
        graphics[1].0.options.as_deref(),
        Some("figure={figures/other},height=2cm")
    );
    assert_eq!(graphics[1].1.meta.producer, EventProducer::Primitive);
    assert_eq!(outcome.output.matches("[image]").count(), 2);
    assert!(!outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
            && matches!(diagnostic.detail.as_str(), "epsfig" | "psfig")
    }));
}

#[test]
fn false_conditional_does_not_emit_legacy_graphics() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\epsfig{file=wrong.eps}
\fi
\epsfig{file=right.eps}
\end{document}",
    );
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some((graphic.path.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        graphics,
        vec![("right.eps", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_legacy_graphic_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\emitplot#1{\epsfig{file=#1,width=3cm}}
\begin{document}
\emitplot{figures/generated.eps}
\end{document}",
    );
    let event = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("macro-generated graphic");
    let RenderEvent::GraphicRef(graphic) = &event.event else {
        unreachable!();
    };

    assert_eq!(graphic.path, "figures/generated.eps");
    assert_eq!(
        graphic.options.as_deref(),
        Some("file=figures/generated.eps,width=3cm")
    );
    assert_eq!(event.meta.producer, EventProducer::Macro);
    assert_eq!(
        event
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("emitplot")
    );
}

#[test]
fn epsf_dimensions_apply_to_only_the_next_file() {
    let outcome = capture(
        r"\begin{document}
\epsfxsize=4cm
\epsfysize=2cm
\epsfbox{figures/first.eps}
\epsffile{figures/second.eps}
\end{document}",
    );
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some((graphic, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(graphics.len(), 2, "{:#?}", outcome.render_events);
    assert_eq!(graphics[0].0.path, "figures/first.eps");
    assert_eq!(
        graphics[0].0.options.as_deref(),
        Some("width=4cm,height=2cm")
    );
    assert_eq!(graphics[0].1, EventProducer::Primitive);
    assert_eq!(graphics[1].0.path, "figures/second.eps");
    assert_eq!(graphics[1].0.options, None);
    assert_eq!(graphics[1].1, EventProducer::Primitive);
    assert!(!outcome.output.contains("epsfxsize"));
    assert!(!outcome.output.contains("epsfysize"));
}

#[test]
fn epsf_pending_dimensions_survive_continuation_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "barrier");
    let full = vm.run_plain(
        r"\epsfxsize=4cm
\epsfysize=2cm
\input{barrier}
\begin{document}
\epsfbox{figures/restored.eps}
\end{document}",
    );
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("input exit checkpoint");
    let snapshot = serde_json::from_slice::<VmSnapshot>(
        &serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot"),
    )
    .expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.enable_render_event_capture();
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");
    let graphic = resumed
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some(graphic),
            _ => None,
        })
        .expect("restored EPS graphic");

    assert_eq!(graphic.path, "figures/restored.eps");
    assert_eq!(graphic.options.as_deref(), Some("width=4cm,height=2cm"));
}

#[test]
fn user_redefinition_bypasses_legacy_graphic_semantics() {
    let outcome = capture(
        r"\def\epsfig#1{custom}
\begin{document}
\epsfig{file=ignored.eps}
\end{document}",
    );

    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::GraphicRef(_))),
        "{:#?}",
        outcome.render_events
    );
    assert!(outcome.output.contains("custom"));
}

#[test]
fn user_wrapper_can_delegate_to_builtin_legacy_graphic_semantics() {
    let outcome = capture(
        r"\let\originalepsfig\epsfig
\def\epsfig#1{\originalepsfig{#1}}
\begin{document}
\epsfig{file=delegated.eps}
\end{document}",
    );
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some((graphic, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(graphics.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(graphics[0].0.path, "delegated.eps");
    assert_eq!(graphics[0].1.meta.producer, EventProducer::Macro);
}

#[test]
fn legacy_graphic_path_macros_expand_before_asset_resolution() {
    let outcome = capture(
        r"\def\plotpath{figures/expanded.eps}
\begin{document}
\epsfig{file=\plotpath,width=5cm}
\epsfbox{\plotpath}
\end{document}",
    );
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some((graphic.path.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        graphics,
        vec![
            ("figures/expanded.eps", EventProducer::Primitive),
            ("figures/expanded.eps", EventProducer::Primitive),
        ],
        "{:#?}",
        outcome.render_events
    );
}
