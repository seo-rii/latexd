use tex_render_model::{EventProducer, RenderEvent, SemanticConfidence};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(source)
}

fn capture(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

#[test]
fn source_scanner_events_are_explicitly_marked_as_recovery() {
    let outcome = capture(
        r"\begin{document}
\section{Recovered heading}
\end{document}",
    );
    let heading = outcome
        .render_events
        .iter()
        .find(|envelope| matches!(envelope.event, RenderEvent::Heading(_)))
        .expect("scanner should recover the heading");

    assert_eq!(heading.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(heading.meta.confidence, SemanticConfidence::Medium);
}

#[test]
fn group_local_count_assignment_is_restored() {
    let outcome = run(r"\count0=1{\count0=2}\the\count0");

    assert_eq!(outcome.output, "1");
    assert_eq!(outcome.registers.get(&0), Some(&1));
}

#[test]
fn nested_count_assignments_restore_each_group_value() {
    let outcome = run(r"\count0=1{\count0=2{\count0=3}\the\count0}\the\count0");

    assert_eq!(outcome.output, "21");
    assert_eq!(outcome.registers.get(&0), Some(&1));
}

#[test]
fn global_count_assignment_cancels_pending_group_restores() {
    let outcome = run(r"\count0=1{\count0=2{\global\count0=4}\the\count0}\the\count0");

    assert_eq!(outcome.output, "44");
    assert_eq!(outcome.registers.get(&0), Some(&4));
}

#[test]
fn local_count_assignment_after_global_restores_global_value() {
    let outcome = run(r"\count0=1{\global\count0=4\count0=5\the\count0}\the\count0");

    assert_eq!(outcome.output, "54");
    assert_eq!(outcome.registers.get(&0), Some(&4));
}

#[test]
fn globaldefs_controls_count_assignment_scope() {
    let positive = run(r"\count0=1{\globaldefs=1\count0=2}\the\count0");
    let negative = run(r"\count0=1{\globaldefs=-1\global\count0=2}\the\count0");

    assert_eq!(positive.output, "2");
    assert_eq!(positive.registers.get(&0), Some(&2));
    assert_eq!(negative.output, "1");
    assert_eq!(negative.registers.get(&0), Some(&1));
}

#[test]
fn count_arithmetic_uses_the_same_assignment_scope() {
    let local = run(r"\count0=2{\advance\count0 by 3\multiply\count0 by 2}\the\count0");
    let global = run(r"\count0=2{\global\advance\count0 by 3}\the\count0");

    assert_eq!(local.output, "2");
    assert_eq!(local.registers.get(&0), Some(&2));
    assert_eq!(global.output, "5");
    assert_eq!(global.registers.get(&0), Some(&5));
}

#[test]
fn group_local_dimen_and_skip_assignments_are_restored() {
    let outcome = run(r"\dimen0=1pt\skip0=2pt{\dimen0=3pt\skip0=4pt}\the\dimen0|\the\skip0");

    assert_eq!(outcome.output, "1pt|2pt");
}

#[test]
fn global_and_globaldefs_control_dimen_and_skip_assignment_scope() {
    let explicit =
        run(r"\dimen0=1pt\skip0=2pt{\global\dimen0=3pt\global\skip0=4pt}\the\dimen0|\the\skip0");
    let positive =
        run(r"\dimen0=1pt\skip0=2pt{\globaldefs=1\dimen0=3pt\skip0=4pt}\the\dimen0|\the\skip0");
    let negative = run(
        r"\dimen0=1pt\skip0=2pt{\globaldefs=-1\global\dimen0=3pt\global\skip0=4pt}\the\dimen0|\the\skip0",
    );

    assert_eq!(explicit.output, "3pt|4pt");
    assert_eq!(positive.output, "3pt|4pt");
    assert_eq!(negative.output, "1pt|2pt");
}

#[test]
fn dimen_and_skip_arithmetic_uses_the_same_assignment_scope() {
    let local = run(
        r"\dimen0=2pt\skip0=3pt{\advance\dimen0 by 1pt\multiply\skip0 by 2}\the\dimen0|\the\skip0",
    );
    let global = run(
        r"\dimen0=2pt\skip0=3pt{\global\advance\dimen0 by 1pt\global\multiply\skip0 by 2}\the\dimen0|\the\skip0",
    );

    assert_eq!(local.output, "2pt|3pt");
    assert_eq!(global.output, "3pt|6pt");
}

#[test]
fn latex_length_helpers_use_the_same_assignment_scope() {
    let local = run(r"\newlength{\foo}\setlength{\foo}{1pt}{\addtolength{\foo}{2pt}}\the\foo");
    let global =
        run(r"\newlength{\foo}\setlength{\foo}{1pt}{\global\addtolength{\foo}{2pt}}\the\foo");

    assert_eq!(local.output, "1pt");
    assert_eq!(global.output, "3pt");
}

#[test]
fn group_local_token_register_assignment_is_restored() {
    let outcome = run(r"\toks0={outer}{\toks0={inner}}\the\toks0");

    assert_eq!(outcome.output, "outer");
}

#[test]
fn global_and_globaldefs_control_token_register_assignment_scope() {
    let explicit = run(r"\toks0={outer}{\global\toks0={global}}\the\toks0");
    let positive = run(r"\toks0={outer}{\globaldefs=1\toks0={global}}\the\toks0");
    let negative = run(r"\toks0={outer}{\globaldefs=-1\global\toks0={local}}\the\toks0");

    assert_eq!(explicit.output, "global");
    assert_eq!(positive.output, "global");
    assert_eq!(negative.output, "outer");
}

#[test]
#[ignore = "known divergence: macro definitions discard delimited parameter text"]
fn delimited_macro_arguments_follow_parameter_text() {
    let outcome = run(r"\def\pair#1,#2;{#2/#1}\pair a,b;");

    assert_eq!(outcome.output, "b/a");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn false_conditional_does_not_emit_math_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
  $wrong$
\fi
$right$
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => Some((
                math.raw_source.as_str(),
                envelope.meta.confidence,
                envelope.meta.producer,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        math,
        vec![("right", SemanticConfidence::High, EventProducer::Primitive)]
    );
}

#[test]
fn macro_generated_math_emits_an_event() {
    let outcome = capture(
        r"\def\emitmath{$x^2$}
\begin{document}
\emitmath
\end{document}",
    );
    let math = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => {
                Some((math.raw_source.as_str(), envelope.meta.producer))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(math, vec![("x^2", EventProducer::Primitive)]);
}

#[test]
fn false_conditional_does_not_emit_text_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
wrong
\fi
right
\end{document}",
    );
    let visible_text = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(!visible_text.contains("wrong"), "{visible_text:?}");
    assert!(visible_text.contains("right"), "{visible_text:?}");
}

#[test]
fn unselected_else_branch_does_not_emit_text_events() {
    let outcome = capture(
        r"\count0=1
\begin{document}
\ifnum\count0>0
selected
\else
wrong
\fi
\end{document}",
    );
    let visible_text = outcome
        .render_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(visible_text.contains("selected"), "{visible_text:?}");
    assert!(!visible_text.contains("wrong"), "{visible_text:?}");
}

#[test]
fn macro_generated_text_emits_execution_events() {
    let outcome = capture(
        r"\def\emittext{Generated text.}
\begin{document}
Before \emittext, After
\end{document}",
    );
    let generated = outcome
        .render_events
        .iter()
        .filter(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Text(text)
                    if text.text.contains("Generated") || text.text.contains("text.")
            )
        })
        .collect::<Vec<_>>();

    assert!(!generated.is_empty());
    assert!(generated.iter().all(|event| {
        event.meta.confidence == SemanticConfidence::High
            && event.meta.producer == EventProducer::Macro
            && event
                .meta
                .source
                .expansion_stack
                .last()
                .and_then(|frame| frame.command_name.as_deref())
                == Some("emittext")
    }));
    let visible_text = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            _ => None,
        })
        .collect::<String>();

    assert!(
        visible_text.contains("Before Generated text., After"),
        "{visible_text:?}\n{:#?}",
        outcome.render_events
    );
}

#[test]
fn false_conditional_does_not_emit_citation_events() {
    let outcome = capture(
        r"\count0=0
\begin{document}
\ifnum\count0>0
\cite{wrong}
\fi
\cite{right}
\end{document}",
    );
    let citations = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineCitation(citation) => Some((
                citation.keys.clone(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        citations,
        vec![(
            vec!["right".to_string()],
            EventProducer::Primitive,
            SemanticConfidence::High,
        )]
    );
}

#[test]
fn macro_generated_citation_emits_at_the_invocation() {
    let outcome = capture(
        r"\def\emitcite{\cite{key}}
\begin{document}
Before \emitcite, After
\end{document}",
    );
    let citation = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
        .expect("macro-generated citation");
    let RenderEvent::InlineCitation(citation_event) = &citation.event else {
        unreachable!();
    };

    assert_eq!(citation_event.keys, vec!["key"]);
    assert_eq!(citation.meta.producer, EventProducer::Macro);
    assert_eq!(citation.meta.confidence, SemanticConfidence::High);
    assert_eq!(
        citation
            .meta
            .source
            .expansion_stack
            .last()
            .and_then(|frame| frame.command_name.as_deref()),
        Some("emitcite")
    );
    let before = outcome
        .render_events
        .iter()
        .position(|event| matches!(&event.event, RenderEvent::Text(text) if text.text == "Before"))
        .expect("before text");
    let citation_position = outcome
        .render_events
        .iter()
        .position(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
        .expect("citation position");
    let after = outcome
        .render_events
        .iter()
        .position(|event| matches!(&event.event, RenderEvent::Text(text) if text.text == "After"))
        .expect("after text");

    assert!(before < citation_position && citation_position < after);
}

#[test]
fn executed_citation_replaces_recovery_placeholder_inside_visible_wrapper_text() {
    let outcome = capture(
        r"\begin{document}
\textcolor{cyan}{visible \cite{key}}
\end{document}",
    );
    let visible = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            RenderEvent::InlineCitation(_) => Some("[?]"),
            _ => None,
        })
        .collect::<String>();
    let citations = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
        .collect::<Vec<_>>();

    assert_eq!(visible, "visible [?]", "{:#?}", outcome.render_events);
    assert_eq!(citations.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(citations[0].meta.producer, EventProducer::Primitive);
    assert!(
        outcome.render_events.iter().all(
            |event| !matches!(&event.event, RenderEvent::Text(text) if text.text.contains("[?]"))
        ),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn executed_paragraph_breaks_preserve_their_reason() {
    let outcome = capture(
        r"\begin{document}
First\par Second

Third
\end{document}",
    );
    let breaks = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::ParagraphBreak(paragraph) => Some((paragraph.reason, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        breaks,
        vec![
            (
                tex_render_model::ParagraphBreakReason::ParCommand,
                EventProducer::Primitive,
            ),
            (
                tex_render_model::ParagraphBreakReason::BlankLine,
                EventProducer::Primitive,
            ),
        ],
        "{:#?}",
        outcome.render_events,
    );
    assert!(!outcome.render_events.windows(2).any(|events| {
        matches!(events[0].event, RenderEvent::Space(_))
            && matches!(events[1].event, RenderEvent::ParagraphBreak(_))
    }));
}

#[test]
fn executed_text_keeps_semantic_event_order() {
    let outcome = capture(r"\begin{document}A \cite{k} B $x$ C\end{document}");
    let position = |predicate: &dyn Fn(&RenderEvent) -> bool| {
        outcome
            .render_events
            .iter()
            .position(|event| predicate(&event.event))
            .expect("expected render event")
    };
    let a = position(&|event| matches!(event, RenderEvent::Text(text) if text.text == "A"));
    let citation = position(&|event| matches!(event, RenderEvent::InlineCitation(_)));
    let b = position(&|event| matches!(event, RenderEvent::Text(text) if text.text == "B"));
    let math = position(&|event| matches!(event, RenderEvent::InlineMath(_)));
    let c = position(&|event| matches!(event, RenderEvent::Text(text) if text.text == "C"));

    assert!(a < citation && citation < b && b < math && math < c);
    assert!(
        outcome.render_events.iter().all(|event| {
            !matches!(event.event, RenderEvent::Text(_))
                || (event.meta.confidence == SemanticConfidence::High
                    && event.meta.producer == EventProducer::Primitive)
        }),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn runtime_catcode_change_affects_unread_characters() {
    let outcome = run(r"\catcode`\@=11
\def\foo@bar{ok}
\foo@bar");

    assert_eq!(outcome.output.trim(), "ok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn runtime_catcodes_apply_to_mounted_input_characters() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file("defs.tex", r"\def\foo@bar{ok}\foo@bar");

    let outcome = vm.run_plain(r"\catcode`\@=11\input{defs}");

    assert_eq!(outcome.output, "ok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn mounted_input_catcode_changes_apply_to_its_unread_characters() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file("defs.tex", r"\catcode`\@=11\gdef\foo@bar{ok}\foo@bar");

    let outcome = vm.run_plain(r"\input{defs}\foo@bar");

    assert_eq!(outcome.output, "okok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn package_catcode_changes_apply_to_its_unread_characters() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file(
        "runtime-catcode.sty",
        r"\catcode`\!=11\def\pkg!mark{ok}\pkg!mark",
    );

    let outcome = vm.run_plain(r"\usepackage{runtime-catcode}");

    assert_eq!(outcome.output, "ok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn package_catcode_assignment_replaces_the_loader_at_letter_overlay() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file("runtime-at.sty", r"\catcode`\@=12 \foo@bar");

    let outcome = vm.run_plain(r"\usepackage{runtime-at}");

    assert_eq!(
        outcome
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.detail.as_str())
            .collect::<Vec<_>>(),
        vec!["foo"]
    );
}

#[test]
fn package_loader_catcode_overlay_returns_after_local_assignment_scope() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file(
        "scoped-at.sty",
        r"{\catcode`\@=12 }\def\pkg@mark{ok}\pkg@mark",
    );

    let outcome = vm.run_plain(r"\usepackage{scoped-at}");

    assert_eq!(outcome.output.trim(), "ok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn package_end_hooks_keep_the_source_catcode_overlay_after_eof() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file("hooked.sty", r"\AtEndOfPackage{\input{hooked-nested}}");
    vm.mount_file("hooked-nested.tex", r"\def\pkg@mark{ok}\pkg@mark");

    let outcome = vm.run_plain(r"\usepackage{hooked}");

    assert_eq!(outcome.output, "ok");
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn catcode_assignments_follow_local_and_global_group_scope() {
    let local = run(r"{\catcode`\@=11\gdef\foo@bar{ok}}\foo@bar");
    let global = run(r"{\global\catcode`\@=11\gdef\foo@bar{ok}}\foo@bar");

    assert_ne!(local.output, "ok");
    assert!(
        local
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "foo")
    );
    assert_eq!(global.output, "ok");
    assert!(global.diagnostics.is_empty());
}

#[test]
fn snapshot_restore_preserves_runtime_catcodes() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\catcode`\@=11\gdef\foo@bar{ok}");
    let snapshot = vm.snapshot();
    let mut restored = Vm::restore(&mut interner, &snapshot);

    let outcome = restored.run_plain(r"\foo@bar");

    assert_eq!(outcome.output, "ok");
    assert!(outcome.diagnostics.is_empty());
}
