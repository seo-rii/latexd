use tex_render_model::{EventProducer, RenderEvent, RenderEventEnvelope, SemanticConfidence};
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
fn input_exit_snapshot_resumes_active_math_capture() {
    for (source, display) in [
        (r"\begin{document}$a\input{barrier}b$\end{document}", false),
        (r"\begin{document}$$a\input{barrier}b$$\end{document}", true),
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

fn replay_render_events_after_input_exit(
    source: &str,
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
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
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
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");
    (expected, resumed.render_events)
}
