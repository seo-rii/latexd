use tex_render_model::{
    CaptionInlinePlaceholderEvent, CaptionKind, EventProducer, RenderEvent, SemanticConfidence,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn executed_caption_event_is_authoritative() {
    let outcome = capture(
        r"\begin{document}
\begin{figure}
\caption{Visible \cite{paper}.}
\end{figure}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((caption, event)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(captions.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(captions[0].0.text, "Visible [?].");
    assert_eq!(captions[0].0.numbered, true);
    assert_eq!(captions[0].1.meta.producer, EventProducer::Primitive);
    assert_eq!(captions[0].1.meta.confidence, SemanticConfidence::High);
    assert!(matches!(
        captions[0].0.inline_placeholders.as_slice(),
        [CaptionInlinePlaceholderEvent::Citation(citation)]
            if citation.keys == ["paper"] && citation.command == "cite"
    ));
    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::InlineCitation(_))),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_does_not_emit_caption_events() {
    let outcome = capture(
        r"\begin{document}
\iffalse
\caption{Wrong}
\fi
\caption{Right}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((
                caption.text.as_str(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![("Right", EventProducer::Primitive, SemanticConfidence::High,)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_caption_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\emitcaption#1{\caption{Expanded #1}}
\begin{document}
\emitcaption{title}
\end{document}",
    );
    let caption = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Caption(_)))
        .expect("macro-generated caption");
    let RenderEvent::Caption(caption_event) = &caption.event else {
        unreachable!();
    };

    assert_eq!(caption_event.text, "Expanded title");
    assert_eq!(caption.meta.producer, EventProducer::Macro);
    assert_eq!(caption.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        caption
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("emitcaption")
    );
}

#[test]
fn caption_variants_hide_short_titles_and_preserve_kind() {
    let outcome = capture(
        r"\begin{document}
\caption*[Hidden short]{Unnumbered}
\captionof{figure}[Hidden figure]{Figure title}
\captionof*{table}{Table title}
\figcaption[Hidden fig]{Figure alias}
\tabcaption*{Table alias}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((
                caption.text.as_str(),
                caption.numbered,
                caption.caption_kind,
                event.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![
            ("Unnumbered", false, None, EventProducer::Primitive),
            (
                "Figure title",
                true,
                Some(CaptionKind::Figure),
                EventProducer::Primitive,
            ),
            (
                "Table title",
                false,
                Some(CaptionKind::Table),
                EventProducer::Primitive,
            ),
            (
                "Figure alias",
                true,
                Some(CaptionKind::Figure),
                EventProducer::Primitive,
            ),
            (
                "Table alias",
                false,
                Some(CaptionKind::Table),
                EventProducer::Primitive,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
    assert!(
        !outcome.render_events.iter().any(
            |event| matches!(&event.event, RenderEvent::Text(text) if text.text.contains("Hidden"))
        ),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn builtin_caption_package_shims_preserve_execution_semantics() {
    let outcome = capture(
        r"\documentclass{article}
\usepackage{caption,subcaption,ccaption}
\begin{document}
\subcaption[Hidden]{Sub}
\captionabove{Above}
\captionbelow*{Below}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => {
                Some((caption.text.as_str(), caption.numbered, event.meta.producer))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![
            ("Sub", true, EventProducer::Primitive),
            ("Above", true, EventProducer::Primitive),
            ("Below", false, EventProducer::Primitive),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn caption_body_macro_expansion_replaces_scanner_recovery() {
    let outcome = capture(
        r"\def\captionword{Expanded}
\begin{document}
\caption{The \captionword: caption}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((caption.text.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![("The Expanded: caption", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn caption_preserves_placeholders_nested_inside_links() {
    let outcome = capture(
        r"\begin{document}
\caption{See \href{https://example.test}{Paper \cite{paper} and \ref{fig}}.}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((caption, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(captions.len(), 1, "{:#?}", outcome.render_events);
    let (caption, producer) = captions[0];

    assert_eq!(caption.text, "See Paper [?] and [?].");
    assert_eq!(producer, EventProducer::Primitive);
    assert!(matches!(
        caption.inline_placeholders.as_slice(),
        [
            CaptionInlinePlaceholderEvent::Citation(citation),
            CaptionInlinePlaceholderEvent::Reference(reference),
        ] if citation.keys == ["paper"]
            && citation.command == "cite"
            && reference.keys == ["fig"]
            && reference.command == "ref"
    ));
}

#[test]
fn lossy_caption_execution_keeps_one_scanner_recovery_event() {
    let outcome = capture(
        r"\begin{document}
\caption{Before \unsupportedcaption{Visible} after}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((
                caption.text.as_str(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![(
            "Before Visible after",
            EventProducer::ScannerRecovery,
            SemanticConfidence::Medium,
        )],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn captionof_expands_its_kind_argument() {
    let outcome = capture(
        r"\def\captionkind{figure}
\begin{document}
\captionof{\captionkind}{Expanded kind}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((
                caption.text.as_str(),
                caption.caption_kind,
                event.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![(
            "Expanded kind",
            Some(CaptionKind::Figure),
            EventProducer::Primitive,
        )],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn caption_expands_threeparttable_note_markers() {
    let outcome = capture(
        r"\documentclass{article}
\usepackage{threeparttable}
\begin{document}
\caption{Measured\tnote{a} table.}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((caption.text.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![("Measured[a] table.", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn caption_body_uses_only_the_executed_conditional_branch() {
    let outcome = capture(
        r"\def\pickcaption#1{\ifnum#1>0 Shown\else Hidden\fi}
\begin{document}
\caption{Result: \pickcaption{1}.}
\end{document}",
    );
    let captions = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some((caption.text.as_str(), event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        captions,
        vec![("Result: Shown.", EventProducer::Primitive)],
        "{:#?}",
        outcome.render_events
    );
}
