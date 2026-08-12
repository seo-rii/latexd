use tex_render_model::{EventProducer, GraphicAssetFormat, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmModuleCheckpointKind, VmSnapshot};

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn executed_includegraphics_event_is_authoritative() {
    let outcome = capture(
        r"\begin{document}
\includegraphics[width=5cm,page=2,pagebox=cropbox]{figures/plot.pdf}
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
    assert_eq!(graphics[0].0.path, "figures/plot.pdf");
    assert_eq!(
        graphics[0].0.options.as_deref(),
        Some("width=5cm,page=2,pagebox=cropbox")
    );
    assert_eq!(graphics[0].0.asset_format, Some(GraphicAssetFormat::Pdf));
    assert_eq!(
        graphics[0]
            .0
            .page_selection
            .as_ref()
            .and_then(|selection| selection.page),
        Some(2)
    );
    assert_eq!(graphics[0].1.meta.producer, EventProducer::Primitive);
    assert_eq!(graphics[0].1.meta.confidence, SemanticConfidence::High);
    assert!(outcome.output.contains("[image]"));
}

#[test]
fn false_conditional_does_not_emit_graphic_events() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\includegraphics{wrong.pdf}
\fi
\includegraphics{right.pdf}
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
        vec![("right.pdf", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn runtime_false_graphic_package_options_do_not_reach_visible_graphics() {
    let outcome = capture(
        r"\count0=0
\ifnum\count0>0
\usepackage[draft]{graphicx}
\fi
\begin{document}
\includegraphics{right.pdf}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("visible graphic");
    let RenderEvent::GraphicRef(graphic_event) = &graphic.event else {
        unreachable!();
    };

    assert_eq!(graphic_event.path, "right.pdf");
    assert_eq!(graphic_event.options, None, "{graphic:#?}");
    assert_eq!(graphic.meta.producer, EventProducer::Primitive);
}

#[test]
fn macro_generated_graphic_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\emitgraphic#1{\includegraphics[width=2cm]{#1}}
\begin{document}
\emitgraphic{figures/generated.pdf}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("macro-generated graphic");
    let RenderEvent::GraphicRef(graphic_event) = &graphic.event else {
        unreachable!();
    };

    assert_eq!(graphic_event.path, "figures/generated.pdf");
    assert_eq!(graphic_event.options.as_deref(), Some("width=2cm"));
    assert_eq!(graphic.meta.producer, EventProducer::Macro);
    assert_eq!(graphic.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        graphic
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("emitgraphic")
    );
}

#[test]
fn includegraphics_expands_its_path_argument() {
    let outcome = capture(
        r"\def\assetpath{figures/expanded.pdf}
\begin{document}
\includegraphics{\assetpath}
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
        vec![("figures/expanded.pdf", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn user_redefinition_bypasses_builtin_graphic_semantics() {
    let outcome = capture(
        r"\def\includegraphics#1{custom}
\begin{document}
\includegraphics{ignored.pdf}
\end{document}",
    );

    assert!(
        !outcome.render_events.iter().any(|event| matches!(
            event.event,
            RenderEvent::GraphicRef(_) | RenderEvent::IncludePdf(_)
        )),
        "{:#?}",
        outcome.render_events
    );
    assert!(outcome.output.contains("custom"));
}

#[test]
fn user_wrapper_can_delegate_to_builtin_graphic_semantics() {
    let outcome = capture(
        r"\let\originalincludegraphics\includegraphics
\def\includegraphics#1{\originalincludegraphics{#1}}
\begin{document}
\includegraphics{delegated.pdf}
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
    assert_eq!(graphics[0].0.path, "delegated.pdf");
    assert_eq!(graphics[0].1.meta.producer, EventProducer::Macro);
}

#[test]
fn graphicspath_and_declared_extensions_affect_executed_resolution() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.mount_file("figures/plot.png", "fixture");
    let outcome = vm.run_plain(
        r"\graphicspath{{figures/}{unused/}}
\DeclareGraphicsExtensions{.png,.pdf}
\begin{document}
\includegraphics{plot}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("resolved graphic");
    let RenderEvent::GraphicRef(graphic_event) = &graphic.event else {
        unreachable!();
    };

    assert_eq!(graphic_event.path, "figures/plot.png");
    assert_eq!(graphic_event.asset_format, Some(GraphicAssetFormat::Png));
    assert!(graphic_event.asset_hash.is_some());
    assert_eq!(graphic.meta.producer, EventProducer::Primitive);
}

#[test]
fn graphic_defaults_follow_class_package_and_gin_options() {
    let outcome = capture(
        r"\documentclass[draft]{article}
\PassOptionsToPackage{final}{graphicx}
\usepackage{graphicx}
\setkeys{Gin}{width=5cm}
\begin{document}
\includegraphics[height=2cm]{figures/plot.pdf}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("configured graphic");
    let RenderEvent::GraphicRef(graphic_event) = &graphic.event else {
        unreachable!();
    };
    let options = graphic_event.options.as_deref().expect("merged options");

    for expected in ["draft", "final", "width=5cm", "height=2cm"] {
        assert!(
            options.split(',').any(|option| option == expected),
            "{options}"
        );
    }
    assert_eq!(graphic.meta.producer, EventProducer::Primitive);
}

#[test]
fn starred_and_two_option_graphics_preserve_clip_and_viewport() {
    let outcome = capture(
        r"\begin{document}
\includegraphics*[0pt,0pt][144pt,72pt]{figures/plot.pdf}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
        .expect("starred graphic");
    let RenderEvent::GraphicRef(graphic_event) = &graphic.event else {
        unreachable!();
    };

    assert_eq!(
        graphic_event.options.as_deref(),
        Some("viewport=0pt 0pt 144pt 72pt,clip")
    );
    assert_eq!(graphic.meta.producer, EventProducer::Primitive);
}

#[test]
fn executed_includepdf_uses_the_same_asset_contract() {
    let outcome = capture(
        r"\begin{document}
\includepdf[page=3,pagebox=trimbox]{papers/appendix.pdf}
\end{document}",
    );
    let included = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::IncludePdf(graphic) => Some((graphic, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(included.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(included[0].0.path, "papers/appendix.pdf");
    assert_eq!(
        included[0]
            .0
            .page_selection
            .as_ref()
            .and_then(|selection| selection.page),
        Some(3)
    );
    assert_eq!(included[0].1, EventProducer::Primitive);
}

#[test]
fn graphic_configuration_survives_continuation_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "barrier");
    vm.mount_file("figures/plot.png", "fixture");
    let full = vm.run_plain(
        r"\graphicspath{{figures/}}
\DeclareGraphicsExtensions{.png}
\setkeys{Gin}{width=5cm}
\input{barrier}
\begin{document}
\includegraphics{plot}
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
    restored.mount_file("figures/plot.png", "fixture");
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
        .expect("resumed graphic event");

    assert_eq!(graphic.path, "figures/plot.png");
    assert_eq!(graphic.options.as_deref(), Some("width=5cm"));
}

#[test]
fn graphic_configuration_persists_across_vm_runs() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.mount_file("figures/plot.png", "fixture");
    vm.run_plain(
        r"\graphicspath{{figures/}}
\DeclareGraphicsExtensions{.png}
\setkeys{Gin}{width=5cm}",
    );
    let configured = vm.snapshot();
    assert_eq!(
        configured
            .graphic_paths
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        vec!["figures"],
        "{configured:#?}"
    );
    assert_eq!(configured.graphic_extensions, vec!["png"]);
    assert_eq!(
        configured.graphic_default_options.as_deref(),
        Some("width=5cm")
    );
    let outcome = vm.run_plain(
        r"\begin{document}
\includegraphics{plot}
\end{document}",
    );
    let graphic = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => Some(graphic),
            _ => None,
        })
        .expect("graphic event");

    assert_eq!(graphic.path, "figures/plot.png");
    assert_eq!(graphic.options.as_deref(), Some("width=5cm"));
}

#[test]
fn floatrow_wrappers_do_not_cross_match_graphic_events() {
    let outcome = capture(
        r"\documentclass{article}
\usepackage{floatrow,caption,subcaption}
\begin{document}
\begin{figure}
\ffigbox{\includegraphics[width=3cm]{figures/first.pdf}}{\caption{First.}}
\end{figure}
\begin{figure}
\fcapside{\caption{Second.}}{\includegraphics[width=3cm]{figures/second.pdf}}
\end{figure}
\begin{figure}
\floatbox{figure}{\includegraphics[width=3cm]{figures/third.pdf}}{\caption{Third.}}
\end{figure}
\end{document}",
    );
    let graphics = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) => {
                Some((graphic.path.clone(), graphic.options.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        graphics,
        vec![
            (
                "figures/first.pdf".to_string(),
                Some("width=3cm".to_string())
            ),
            (
                "figures/second.pdf".to_string(),
                Some("width=3cm".to_string())
            ),
            (
                "figures/third.pdf".to_string(),
                Some("width=3cm".to_string())
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}
