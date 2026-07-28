use tex_layout::{PageDisplayListOptions, build_document_ir, build_page_display_lists};
use tex_render_model::{
    EventProducer, RenderEvent, RenderEventEnvelope, RenderEventStream, SemanticConfidence,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmContinuationBlocker, VmModuleCheckpointKind, VmSnapshot};

#[test]
fn input_exit_snapshot_resumes_pending_tokens_and_source_catcodes() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "resume.sty",
        r"\def\resume{\input{child}\def\pkg@tail{B}\pkg@tail}A\resume",
    );
    vm.mount_file("child.tex", "C");

    let full = vm.run_plain(r"\usepackage{resume}Z");
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child exit checkpoint");

    assert_eq!(full.output, "ACBZ");
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(
        !checkpoint
            .snapshot
            .continuation_safety
            .blockers
            .contains(&VmContinuationBlocker::ActiveInput)
    );

    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    let output_prefix = full.output[..checkpoint.output_start_utf8 as usize].to_string();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(format!("{output_prefix}{}", resumed.output), full.output);
    assert_eq!(resumed.registers, full.registers);
    assert!(resumed.diagnostics.is_empty());
}

#[test]
fn input_enter_snapshot_reexecutes_the_input_primitive_with_current_child_source() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "resume.sty",
        r"\def\resume{\input{child}\def\pkg@tail{B}\pkg@tail}A\resume",
    );
    vm.mount_file("child.tex", "C");

    let full = vm.run_plain(r"\usepackage{resume}Z");
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child enter checkpoint");

    assert_eq!(full.output, "ACBZ");
    assert!(checkpoint.snapshot.input_continuation.is_some());
    assert!(checkpoint.snapshot.continuation_safety.is_safe());

    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    let output_prefix = full.output[..checkpoint.output_start_utf8 as usize].to_string();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("child.tex", "D");
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(format!("{output_prefix}{}", resumed.output), "ADBZ");
    assert!(resumed.diagnostics.is_empty());
}

#[test]
fn input_enter_snapshot_rebases_a_semantically_equivalent_parent_source() {
    let previous_source = "\\begin{document}% old comment\n\\input{child} After.\\end{document}";
    let current_source =
        "\\begin{document}% a longer replacement comment\n\\input{child} After.\\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "Child.");
    let previous = vm.run_plain(previous_source);
    let checkpoint = previous
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child enter checkpoint");
    let output_prefix = previous.output[..checkpoint.output_start_utf8 as usize].to_string();
    let snapshot = checkpoint.snapshot.clone();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("child.tex", "Child.");
    assert!(restored.rebase_restored_input_sources([current_source]));
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");

    let mut clean_interner = ControlSequenceInterner::new();
    let mut clean = Vm::new(&mut clean_interner);
    clean.set_entry_source_path("main.tex");
    clean.mount_file("child.tex", "Child.");
    let expected = clean.run_plain(current_source);

    assert_eq!(
        format!("{output_prefix}{}", replayed.output),
        expected.output
    );
}

#[test]
fn input_enter_snapshot_replaces_changed_child_render_events() {
    let (expected, replayed) =
        replay_render_events_after_changed_child("Old child.", "A much newer child.");

    assert_eq!(replayed, expected);
}

#[test]
fn input_enter_snapshot_replaces_changed_child_heading_event() {
    let (expected, replayed) =
        replay_render_events_after_changed_child(r"\section{Old}", r"\section{New heading}");

    assert_eq!(replayed, expected);
}

#[test]
fn input_enter_snapshot_replaces_changed_child_citation_event() {
    let (expected, replayed) =
        replay_render_events_after_changed_child(r"\cite{old}", r"\cite{new-key}");

    assert_eq!(replayed, expected);
}

#[test]
fn input_enter_snapshot_replaces_changed_child_math_event() {
    for (previous_child, current_child) in [
        ("$x^2$", r"$\alpha \le \beta$"),
        (r"\(x^2\)", r"\(\alpha \le \beta\)"),
        (r"\[x^2\]", r"\[\alpha \le \beta\]"),
        (r"\ensuremath{x^2}", r"\ensuremath{\alpha \le \beta}"),
        (
            r"\begin{equation}x^2\end{equation}",
            r"\begin{equation}\alpha \le \beta\end{equation}",
        ),
        (
            r"\begin{align}x&=y\end{align}",
            r"\begin{align}\alpha&\le\beta\end{align}",
        ),
    ] {
        let (expected, replayed) =
            replay_render_events_after_changed_child(previous_child, current_child);

        assert_eq!(replayed, expected);
    }
}

#[test]
fn input_enter_snapshot_replaces_changed_child_structural_events() {
    for (case, previous_child, current_child) in [
        ("label", r"\label{old}", r"\label{new}"),
        ("reference", r"\ref{old}", r"\pageref{new}"),
        (
            "link",
            r"\href{https://old.test}{Old}",
            r"\href{https://new.test}{New link}",
        ),
        (
            "graphic",
            r"\includegraphics{old.png}",
            r"\includegraphics[width=2cm]{new.png}",
        ),
        (
            "list",
            r"\begin{itemize}\item Old\end{itemize}",
            r"\begin{enumerate}\item New item\end{enumerate}",
        ),
        (
            "environment",
            r"\begin{quote}Old\end{quote}",
            r"\begin{quotation}New text\end{quotation}",
        ),
        (
            "table",
            r"\begin{tabular}{l}Old\end{tabular}",
            r"\begin{tabular}{ll}New & Cell\end{tabular}",
        ),
        ("caption", r"\caption{Old}", r"\caption{New caption}"),
        ("page break", r"\newpage", r"\clearpage"),
    ] {
        let (expected, replayed) =
            replay_render_events_after_changed_child(previous_child, current_child);

        assert_eq!(replayed, expected, "{case}");
    }
}

fn replay_render_events_after_changed_child(
    previous_child: &str,
    current_child: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    let source = r"\begin{document}Before \input{child} After.\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", previous_child);
    let previous = vm.run_plain(source);
    let checkpoint = previous
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("child enter checkpoint");
    let output_prefix = previous.output[..checkpoint.output_start_utf8 as usize].to_string();
    let snapshot = checkpoint.snapshot.clone();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("child.tex", current_child);
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");

    let mut clean_interner = ControlSequenceInterner::new();
    let mut clean = Vm::new(&mut clean_interner);
    clean.enable_render_event_capture();
    clean.set_entry_source_path("main.tex");
    clean.mount_file("child.tex", current_child);
    let expected = clean.run_plain(source);

    assert_eq!(
        format!("{output_prefix}{}", replayed.output),
        expected.output
    );
    (expected.render_events, replayed.render_events)
}

#[test]
fn input_exit_snapshot_preserves_observation_prefixes() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "c");

    let full = vm
        .run_plain(r"\count0=1\undefinedbefore\input{barrier}\advance\count0 by 1\undefinedafter");
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(resumed.diagnostics, full.diagnostics);
    assert_eq!(resumed.transcript, full.transcript);
    assert_eq!(resumed.module_traces, full.module_traces);
    assert_eq!(resumed.registers, full.registers);
}

#[test]
fn input_boundary_snapshot_preserves_document_ir_and_display_lists() {
    let source = r"\documentclass[11pt]{article}
\title{Replay Paper}
\author{Ada}
\begin{document}
\maketitle
\begin{abstract}Short abstract.\end{abstract}
\section{Intro}\label{sec:intro}
First line\\Second line\footnote{A note.}
\input{barrier}
\begin{minipage}{0.5\textwidth}Container text.\end{minipage}
\[
x^2
\]
\newpage
\begin{thebibliography}{1}
\bibitem{k} Author. Title.
\end{thebibliography}
\end{document}";

    for checkpoint_kind in [VmModuleCheckpointKind::Enter, VmModuleCheckpointKind::Exit] {
        let (expected, actual) = replay_render_events_at_input_boundary(source, checkpoint_kind);

        assert!(expected.iter().any(|event| matches!(
            event.event,
            RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
        )));
        assert!(
            expected
                .iter()
                .any(|event| matches!(event.event, RenderEvent::LineBreak(_)))
        );
        let page_break = expected
            .iter()
            .find(|event| matches!(event.event, RenderEvent::PageBreak(_)))
            .expect("page break event");
        assert_eq!(page_break.meta.producer, EventProducer::Primitive);
        assert_eq!(page_break.meta.confidence, SemanticConfidence::High);
        assert_eq!(actual, expected);

        let expected_stream = RenderEventStream::new(Some("full".to_string()), expected);
        let actual_stream = RenderEventStream::new(Some("full".to_string()), actual);
        let expected_ir = build_document_ir(&expected_stream, &());
        let actual_ir = build_document_ir(&actual_stream, &());
        assert_eq!(actual_ir, expected_ir);

        let expected_pages = build_page_display_lists(
            &expected_ir,
            PageDisplayListOptions::for_document_ir(&expected_ir),
        );
        let actual_pages = build_page_display_lists(
            &actual_ir,
            PageDisplayListOptions::for_document_ir(&actual_ir),
        );
        assert_eq!(actual_pages, expected_pages);
    }
}

#[test]
fn input_exit_snapshot_resumes_active_math_capture() {
    for (source, display) in [
        (r"\begin{document}$a\input{barrier}b$\end{document}", false),
        (r"\begin{document}$$a\input{barrier}b$$\end{document}", true),
        (
            r"\begin{document}\(a\input{barrier}b\)\end{document}",
            false,
        ),
        (r"\begin{document}\[a\input{barrier}b\]\end{document}", true),
        (
            r"\begin{document}\ensuremath{a\input{barrier}b}\end{document}",
            false,
        ),
        (
            r"\begin{document}\begin{equation}a\input{barrier}b\end{equation}\end{document}",
            true,
        ),
        (
            r"\begin{document}\begin{alignat}{2}a&=b & c&=\input{barrier}d\end{alignat}\end{document}",
            true,
        ),
    ] {
        let (expected, actual) = replay_render_events_after_input_exit(source);
        assert_eq!(
            expected.len(),
            1,
            "expected one {} math event",
            if display { "display" } else { "inline" }
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn input_exit_snapshot_preserves_text_around_active_math() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Before $a\input{barrier}b$ after.\end{document}",
    );

    assert!(expected.len() > 1, "expected text and math events");
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_text_capture() {
    let (expected, actual) =
        replay_render_events_after_input_exit(r"\begin{document}A\input{barrier}B\end{document}");

    assert!(!expected.is_empty(), "expected executed text events");
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_graphics_around_boundary() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\includegraphics[width=2cm]{before.png}\input{barrier}\includegraphics{after.png}\end{document}",
    );

    assert_eq!(
        expected
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::GraphicRef(_)))
            .count(),
        2
    );
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_open_list() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{itemize}\item Before\input{barrier}\item After\end{itemize}\end{document}",
    );

    assert_eq!(
        expected
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::ListItem(_)))
            .count(),
        2
    );
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_open_environment() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{quote}Before\input{barrier}After\end{quote}\end{document}",
    );

    assert_eq!(
        expected
            .iter()
            .filter(|event| {
                matches!(
                    event.event,
                    RenderEvent::BeginBlock(_) | RenderEvent::EndBlock(_)
                )
            })
            .count(),
        2
    );
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_open_table_cell() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{tabular}{ll}A & before\input{barrier}after \\ C & D\end{tabular}\end{document}",
    );

    let table = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected one structured table event");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[1].text, "beforebarrierafter");
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_macro_expansion() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\def\wrap#1{Before #1 after.}\begin{document}\wrap{\input{barrier}}\end{document}",
    );

    assert!(expected.iter().any(|event| {
        event
            .meta
            .source
            .expansion_stack
            .iter()
            .any(|frame| frame.command_name.as_deref() == Some("wrap"))
    }));
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_completed_inline_events() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}A \cite{a}\ref{x}\href{https://example.test}{B}\input{barrier}\cite{b}\pageref{y}.\end{document}",
    );

    assert_eq!(
        expected
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::InlineCitation(_)))
            .count(),
        2
    );
    assert_eq!(
        expected
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::InlineReference(_)))
            .count(),
        2
    );
    assert_eq!(
        expected
            .iter()
            .filter(|event| matches!(event.event, RenderEvent::InlineLink(_)))
            .count(),
        1
    );
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_link_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Lead \href{https://example.test}{Before \cite{k} and \ref{x} $m$ \input{barrier} After} Tail.\end{document}",
    );

    let links = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineLink(link) => Some(link),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(links.len(), 1);
    assert!(links[0].text.contains("Before"));
    assert!(links[0].text.contains("After"));
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_heading_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Lead\section{Before \cite{k} \href{https://example.test}{L} $m$ \input{barrier} After}Tail\end{document}",
    );

    let headings = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Heading(heading) => Some(heading),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(headings.len(), 1);
    assert!(headings[0].text.contains("Before"));
    assert!(headings[0].text.contains("After"));
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_lossy_heading_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\section{Before \unsupportedtitle{Visible} \input{barrier} After}\end{document}",
    );

    let heading = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Heading(_)))
        .expect("recovered heading");
    assert_eq!(heading.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(heading.meta.confidence, SemanticConfidence::Medium);
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_caption_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{figure}\caption{Before \cite{k} \ref{x} \href{https://example.test}{L} $m$ \input{barrier} After}\end{figure}\end{document}",
    );

    let captions = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Caption(caption) => Some(caption),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(captions.len(), 1);
    assert!(captions[0].text.contains("Before"));
    assert!(captions[0].text.contains("After"));
    assert_eq!(captions[0].inline_placeholders.len(), 2);
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_active_lossy_caption_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\caption{Before \unsupportedcaption{Visible} \input{barrier} After}\end{document}",
    );

    let caption = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::Caption(_)))
        .expect("recovered caption");
    assert_eq!(caption.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(caption.meta.confidence, SemanticConfidence::Medium);
    assert_eq!(actual, expected);
}

fn replay_render_events_after_input_exit(
    source: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    replay_render_events_at_input_boundary(source, VmModuleCheckpointKind::Exit)
}

fn replay_render_events_at_input_boundary(
    source: &str,
    checkpoint_kind: VmModuleCheckpointKind,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.enable_structured_table_events();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "c");

    let full = vm.run_plain(source);
    let checkpoint = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == checkpoint_kind && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier input checkpoint");
    assert!(
        checkpoint.snapshot.continuation_safety.is_safe(),
        "render continuation must be replay-safe: {:#?}",
        checkpoint.snapshot.continuation_safety.blockers
    );
    let semantic_capture = checkpoint
        .snapshot
        .semantic_capture
        .as_ref()
        .expect("semantic capture snapshot");
    assert!(
        semantic_capture.is_restorable(),
        "semantic capture must be restorable: {semantic_capture:#?}"
    );
    let expected = full.render_events.clone();
    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("barrier.tex", "c");
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");
    (expected, resumed.render_events)
}
