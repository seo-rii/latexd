use tex_render_model::{
    EventProducer, ProvenanceSpan, RenderEvent, SemanticConfidence, SourceSpanRole,
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
fn executed_heading_events_are_authoritative() {
    let outcome = capture(
        r"\begin{document}
\section{Executed heading}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find(|envelope| matches!(envelope.event, RenderEvent::Heading(_)))
        .expect("VM should emit the heading");

    assert_eq!(heading.meta.producer, EventProducer::Primitive);
    assert_eq!(heading.meta.confidence, SemanticConfidence::High);
}

#[test]
fn false_conditional_does_not_emit_heading_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
\section{Wrong}
\fi
\section{Right}
\end{document}",
    );
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some((
                heading.text.as_str(),
                heading.number.as_deref(),
                event.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        headings,
        vec![("Right", Some("1"), EventProducer::Primitive)]
    );
}

#[test]
fn macro_generated_heading_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\emitsection#1{\section{Expanded #1}}
\begin{document}
\emitsection{Title}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Heading(_)))
        .expect("macro-generated heading");
    let RenderEvent::Heading(heading_event) = &heading.event else {
        unreachable!();
    };

    assert_eq!(heading_event.level, 1);
    assert_eq!(heading_event.text, "Expanded Title");
    assert_eq!(heading_event.number.as_deref(), Some("1"));
    assert_eq!(heading.meta.producer, EventProducer::Macro);
    assert_eq!(heading.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        heading
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("emitsection")
    );
}

#[test]
fn heading_aliases_preserve_level_and_canonical_numbering() {
    let outcome = capture(
        r"\let\topic\subsection
\begin{document}
\section{Parent}
\topic{Child}
\topic*{Unnumbered}
\end{document}",
    );
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some((
                heading.level,
                heading.text.as_str(),
                heading.number.as_deref(),
                event.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        headings,
        vec![
            (1, "Parent", Some("1"), EventProducer::Primitive),
            (2, "Child", Some("1.1"), EventProducer::Primitive),
            (2, "Unnumbered", None, EventProducer::Primitive),
        ]
    );
}

#[test]
fn heading_title_executes_and_is_folded_into_one_event() {
    let outcome = capture(
        r"\def\topicword{Visible}
\begin{document}
\section[Short]{Intro \textbf{\topicword} \cite{paper}}
\end{document}",
    );
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some((heading, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(headings.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(headings[0].0.text, "Intro Visible [?]");
    assert_eq!(headings[0].0.number.as_deref(), Some("1"));
    assert_eq!(headings[0].1, EventProducer::Primitive);
    assert!(
        !outcome
            .render_events
            .iter()
            .any(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
    );
}

#[test]
fn builtin_article_shim_does_not_hide_part_heading_execution() {
    let outcome = capture(
        r"\documentclass{article}
\begin{document}
\part{Front Matter}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Heading(_)))
        .expect("part heading");
    let RenderEvent::Heading(heading_event) = &heading.event else {
        unreachable!();
    };

    assert_eq!(heading_event.level, 0);
    assert_eq!(heading_event.text, "Front Matter");
    assert_eq!(heading_event.number.as_deref(), Some("1"));
    assert_eq!(heading.meta.producer, EventProducer::Primitive);
}

#[test]
fn one_macro_call_preserves_multiple_heading_order() {
    let outcome = capture(
        r"\def\twosections#1#2{\section{#1}\section{#2}}
\begin{document}
\twosections{First}{Second}
\end{document}",
    );
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some((
                heading.text.as_str(),
                heading.number.as_deref(),
                event.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        headings,
        vec![
            ("First", Some("1"), EventProducer::Macro),
            ("Second", Some("2"), EventProducer::Macro),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn heading_numbering_restarts_for_each_document_run() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    let first = vm.run_plain(r"\begin{document}\section{First}\end{document}");
    let second = vm.run_plain(r"\begin{document}\section{Second}\end{document}");
    let number = |outcome: &tex_vm::VmOutcome| {
        outcome.render_events.iter().find_map(|event| {
            let RenderEvent::Heading(heading) = &event.event else {
                return None;
            };
            heading.number.clone()
        })
    };

    assert_eq!(number(&first).as_deref(), Some("1"));
    assert_eq!(number(&second).as_deref(), Some("1"));
}

#[test]
fn lossy_heading_execution_keeps_scanner_recovery_confidence() {
    let outcome = capture(
        r"\begin{document}
\section{Before \unsupportedtitle{Visible} After}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Heading(_)))
        .expect("recovered heading");

    assert_eq!(heading.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(heading.meta.confidence, SemanticConfidence::Medium);
}

#[test]
fn macro_internal_false_branch_does_not_emit_heading() {
    let outcome = capture(
        r"\def\maybeheading#1{\ifnum#1>0\section{Shown}\fi}
\begin{document}
\maybeheading{0}
\maybeheading{1}
\end{document}",
    );
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some(heading.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(headings, vec!["Shown"], "{:#?}", outcome.render_events);
}

#[test]
fn starred_and_optional_heading_aliases_keep_visible_argument_provenance() {
    let source = r"\let\topic\section
\begin{document}
\topic*{Unnumbered}
\topic[Short]{Long}
\end{document}";
    let outcome = capture(source);
    let headings = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some((heading, &event.meta.source)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(headings.len(), 2, "{:#?}", outcome.render_events);
    for (heading, provenance) in headings {
        let ProvenanceSpan::File(primary) = &provenance.primary else {
            panic!("heading primary source should be a file span");
        };
        assert_eq!(
            &source[primary.start_utf8 as usize..primary.end_utf8 as usize],
            heading.text
        );
        let invocation = provenance
            .related
            .iter()
            .find(|related| related.role == SourceSpanRole::Invocation)
            .expect("heading invocation");
        let ProvenanceSpan::File(invocation) = &invocation.span else {
            panic!("heading invocation should be a file span");
        };
        assert!(
            source[invocation.start_utf8 as usize..invocation.end_utf8 as usize]
                .starts_with(r"\topic"),
            "{provenance:#?}"
        );
    }
}

#[test]
fn heading_title_folds_link_and_math_events() {
    let outcome = capture(
        r"\begin{document}
\section{See \href{https://example.test}{paper} and $x^2$}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some(heading),
            _ => None,
        })
        .expect("heading");

    assert!(heading.text.contains("paper"), "{heading:#?}");
    assert!(heading.text.contains("x^2"), "{heading:#?}");
    assert!(!outcome.render_events.iter().any(|event| matches!(
        event.event,
        RenderEvent::InlineLink(_) | RenderEvent::InlineMath(_)
    )));
}
