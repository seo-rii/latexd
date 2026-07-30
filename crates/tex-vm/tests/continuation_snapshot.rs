use tex_layout::{PageDisplayListOptions, build_document_ir, build_page_display_lists};
use tex_render_model::{
    EventProducer, MetadataField, RenderEvent, RenderEventEnvelope, RenderEventStream,
    SemanticConfidence,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    Vm, VmContinuationBlocker, VmExecutionAnchor, VmModuleCheckpoint, VmModuleCheckpointKind,
    VmSnapshot,
};

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
fn input_enter_snapshot_replaces_each_changed_child_occurrence() {
    let (expected, replayed) = replay_render_events_after_changed_child_in(
        r"\begin{document}Before \input{child} Between \input{child} After.\end{document}",
        "Old child.",
        "New child.",
    );

    assert_eq!(
        replayed
            .iter()
            .filter(|event| matches!(&event.event, RenderEvent::Text(text) if text.text == "New"))
            .count(),
        2,
        "{replayed:#?}"
    );
    assert_eq!(replayed, expected);
}

#[test]
fn repeated_dynamic_input_checkpoints_distinguish_execution_occurrences() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "Child.");
    let outcome = vm.run_plain(
        r"\begin{document}
\toks0={\input{child}}
\the\toks0\the\toks0
\end{document}",
    );
    let checkpoint = outcome
        .module_checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .next_back()
        .expect("second child exit checkpoint");
    let (completed_anchors, active_anchor) = child_text_execution_anchors(checkpoint);

    assert_eq!(completed_anchors.len(), 1, "{completed_anchors:#?}");
    assert_eq!(completed_anchors[0].occurrence, 0);
    assert_eq!(active_anchor.occurrence, 1);
    assert_ne!(completed_anchors[0], active_anchor);
}

#[test]
fn restored_dynamic_input_continuation_preserves_next_execution_occurrence() {
    let source = r"\begin{document}
\toks0={\input{child}}
\the\toks0\the\toks0
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "Child.");
    let full = vm.run_plain(source);
    let first_exit = full
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("first child exit checkpoint")
        .snapshot
        .clone();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &first_exit);
    restored.mount_file("child.tex", "Child.");
    let resumed = restored
        .resume_continuation()
        .expect("restored input continuation");
    let second_exit = resumed
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .expect("replayed second child exit checkpoint");
    let (completed_anchors, active_anchor) = child_text_execution_anchors(second_exit);

    assert_eq!(completed_anchors.len(), 1, "{completed_anchors:#?}");
    assert_eq!(completed_anchors[0].occurrence, 0);
    assert_eq!(active_anchor.occurrence, 1);
    assert_ne!(completed_anchors[0], active_anchor);
}

#[test]
fn semantic_snapshot_rejects_invalid_execution_occurrence_allocator() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("child.tex", "Child.");
    let outcome = vm.run_plain(
        r"\begin{document}
\toks0={\input{child}}
\the\toks0\the\toks0
\end{document}",
    );
    let semantic = outcome
        .module_checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "child.tex"
        })
        .next_back()
        .and_then(|checkpoint| checkpoint.snapshot.semantic_capture.clone())
        .expect("second child semantic checkpoint");

    assert!(semantic.is_restorable());
    let mut stale = semantic.clone();
    stale
        .execution_occurrences
        .iter_mut()
        .find(|occurrence| occurrence.base_anchor.path.as_str() == "child.tex")
        .expect("child occurrence allocator")
        .next_occurrence = 1;
    assert!(!stale.is_restorable());

    let mut duplicate = semantic;
    duplicate
        .execution_occurrences
        .push(duplicate.execution_occurrences[0].clone());
    assert!(!duplicate.is_restorable());
}

#[test]
fn restored_continuation_executes_changed_bbl_from_the_input_checkpoint() {
    let source = r"\begin{document}Before \input{barrier} \bibliography{refs}\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "Barrier.");
    vm.mount_file(
        "main.bbl",
        r"\begin{thebibliography}{1}\bibitem{k} Old entry.\end{thebibliography}",
    );
    let previous = vm.run_plain(source);
    let checkpoint = previous
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier enter checkpoint");
    let snapshot = checkpoint.snapshot.clone();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("barrier.tex", "Barrier.");
    restored.mount_file(
        "main.bbl",
        r"\begin{thebibliography}{2}\bibitem{k} New entry.\bibitem{second} Second entry.\end{thebibliography}",
    );
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");

    let mut clean_interner = ControlSequenceInterner::new();
    let mut clean = Vm::new(&mut clean_interner);
    clean.enable_render_event_capture();
    clean.set_entry_source_path("main.tex");
    clean.mount_file("barrier.tex", "Barrier.");
    clean.mount_file(
        "main.bbl",
        r"\begin{thebibliography}{2}\bibitem{k} New entry.\bibitem{second} Second entry.\end{thebibliography}",
    );
    let expected = clean.run_plain(source);

    assert_eq!(replayed.render_events, expected.render_events);
    for kind in [VmModuleCheckpointKind::Enter, VmModuleCheckpointKind::Exit] {
        assert!(replayed.module_checkpoints.iter().any(|checkpoint| {
            checkpoint.kind == kind && checkpoint.module_path.as_str() == "main.bbl"
        }));
    }
}

#[test]
fn bibliography_input_boundaries_restore_equivalent_events_and_dependencies() {
    let source = r"\begin{document}Before \bibliography{refs} After.\end{document}";
    let bbl = r"\begin{thebibliography}{1}\bibitem{k} Author. Title.\end{thebibliography}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("main.bbl", bbl);
    let expected = vm.run_plain(source);

    for checkpoint_kind in [VmModuleCheckpointKind::Enter, VmModuleCheckpointKind::Exit] {
        let checkpoint = expected
            .module_checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.kind == checkpoint_kind && checkpoint.module_path.as_str() == "main.bbl"
            })
            .expect("bibliography input checkpoint");
        assert!(checkpoint.snapshot.continuation_safety.is_safe());
        let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize snapshot");
        let snapshot =
            serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
        let output_prefix = expected.output[..checkpoint.output_start_utf8 as usize].to_string();
        let restored_module_trace_count = snapshot.module_traces.len();

        let mut restored_interner = ControlSequenceInterner::new();
        let mut restored = Vm::restore(&mut restored_interner, &snapshot);
        restored.mount_file("main.bbl", bbl);
        let actual = restored
            .resume_continuation()
            .expect("restored bibliography continuation");
        let mut actual_module_traces = actual.module_traces.clone();
        let output_prefix_len = output_prefix.len() as u32;
        for trace in actual_module_traces
            .iter_mut()
            .skip(restored_module_trace_count)
        {
            trace.output_start_utf8 += output_prefix_len;
            trace.output_end_utf8 += output_prefix_len;
        }

        assert_eq!(
            format!("{output_prefix}{}", actual.output),
            expected.output,
            "{checkpoint_kind:?}"
        );
        assert_eq!(
            actual.render_events, expected.render_events,
            "{checkpoint_kind:?}"
        );
        assert_eq!(
            actual.loaded_modules, expected.loaded_modules,
            "{checkpoint_kind:?}"
        );
        assert_eq!(
            actual_module_traces, expected.module_traces,
            "{checkpoint_kind:?}"
        );
        assert_eq!(
            actual.diagnostics, expected.diagnostics,
            "{checkpoint_kind:?}"
        );
    }
}

#[test]
fn legacy_bibliography_input_checkpoint_infers_jobname_from_the_root_source() {
    let source = r"\begin{document}\bibliography{refs}\end{document}";
    let bbl = r"\begin{thebibliography}{1}\bibitem{k} Entry.\end{thebibliography}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("main.bbl", bbl);
    let expected = vm.run_plain(source);
    let checkpoint = expected
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "main.bbl"
        })
        .expect("bibliography enter checkpoint");
    let mut snapshot_value =
        serde_json::to_value(&checkpoint.snapshot).expect("serialize snapshot");
    snapshot_value
        .as_object_mut()
        .expect("snapshot object")
        .remove("jobname_source_path");
    let snapshot = serde_json::from_value::<VmSnapshot>(snapshot_value).expect("legacy snapshot");

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.mount_file("main.bbl", bbl);
    let actual = restored
        .resume_continuation()
        .expect("legacy bibliography continuation");

    assert_eq!(actual.render_events, expected.render_events);
    assert!(
        actual
            .loaded_modules
            .iter()
            .any(|path| path.as_str() == "main.bbl")
    );
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
        (
            "footnote",
            r"\footnote{Old note.}",
            r"\footnote{New note \cite{key}.}",
        ),
        (
            "detached footnote",
            r"\footnotemark[1]\footnotetext{Old note.}",
            r"\footnotemark[2]\footnotetext{New note.}",
        ),
        (
            "table footnote",
            r"\tablefootnote{Old note.}",
            r"\tablefootnote[3]{New note.}",
        ),
    ] {
        let (expected, replayed) =
            replay_render_events_after_changed_child(previous_child, current_child);

        assert_eq!(replayed, expected, "{case}");
    }
}

#[test]
fn input_enter_snapshot_rebuilds_an_active_bibliography_item() {
    let (expected, replayed) = replay_render_events_after_changed_child_in(
        r"\begin{document}\begin{thebibliography}{1}\bibitem{k}Before \input{child} After.\end{thebibliography}\end{document}",
        "Old child.",
        "New child with \\footnote{nested note}.",
    );
    let items = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some((item, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(items.len(), 1);
    assert!(items[0].0.text.contains("New child"));
    assert!(items[0].0.text.contains("nested note"));
    assert_eq!(items[0].1, EventProducer::Primitive);
    assert!(!expected.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::BeginFootnote(_)
                | RenderEvent::EndFootnote(_)
                | RenderEvent::FootnoteMark(_)
        )
    }));
    assert_eq!(replayed, expected);
}

fn replay_render_events_after_changed_child(
    previous_child: &str,
    current_child: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    let source = r"\begin{document}Before \input{child} After.\end{document}";
    replay_render_events_after_changed_child_in(source, previous_child, current_child)
}

fn replay_render_events_after_changed_child_in(
    source: &str,
    previous_child: &str,
    current_child: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
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
fn changed_child_footnote_text_stays_paired_with_parent_mark() {
    let (expected, replayed) = replay_render_events_after_changed_child_in(
        r"\begin{document}Before\footnotemark[4]\input{child}After.\end{document}",
        r"\footnotetext{Old note.}",
        r"\footnotetext{New note \cite{key}.}",
    );
    let mark_id = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::FootnoteMark(mark) => Some(mark.note_id),
            _ => None,
        })
        .expect("parent footnote mark");
    let body_id = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BeginFootnote(begin) => Some(begin.note_id),
            _ => None,
        })
        .expect("child footnote text");

    assert_eq!(mark_id, body_id);
    assert_eq!(replayed, expected);
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
\affiliation{Analytical Engine Institute}
\email{ada@example.test}
\keywords{incremental preview}
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
        for field in [
            MetadataField::Affiliation,
            MetadataField::Correspondence,
            MetadataField::Keywords,
        ] {
            let metadata = expected
                .iter()
                .find(|event| {
                    matches!(
                        &event.event,
                        RenderEvent::SetDocumentMetadata(metadata) if metadata.field == field
                    )
                })
                .expect("profile metadata event");
            assert_eq!(metadata.meta.producer, EventProducer::Primitive);
            assert_eq!(metadata.meta.confidence, SemanticConfidence::High);
        }
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
        let footnote = expected
            .iter()
            .find(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
            .expect("footnote event");
        assert_eq!(footnote.meta.producer, EventProducer::Primitive);
        assert_eq!(footnote.meta.confidence, SemanticConfidence::High);
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
fn input_exit_snapshot_replays_icml_profile_metadata() {
    let source = r"\usepackage{icml2020}
\begin{document}
\input{barrier}
\icmltitle{Replay Paper}
\icmlauthor{Ada Lovelace\thanks{Equal contribution}}{engine}
\icmlaffiliation{engine}{Analytical Engine Institute}
\icmlcorrespondingauthor{Ada Lovelace}{ada@example.test}
\icmlkeywords{incremental preview}
\printAffiliationsAndNotice{}
\end{document}";
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let metadata = expected
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::SetDocumentMetadata(_)))
        .collect::<Vec<_>>();
    assert_eq!(metadata.len(), 6, "{expected:#?}");
    assert!(metadata.iter().all(|event| {
        event.meta.producer == EventProducer::Primitive
            && event.meta.confidence == SemanticConfidence::High
    }));
    let flush = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::FlushTitleBlock(_)))
        .expect("ICML title-block flush");
    assert_eq!(flush.meta.producer, EventProducer::Primitive);
    assert_eq!(flush.meta.confidence, SemanticConfidence::High);
}

#[test]
fn input_exit_snapshot_replays_non_visible_bibliography_metadata() {
    let source = r"\let\resource\addbibresource
\def\configurebibliography{%
\bibliographystyle{plain}%
\defcitealias{paper}{Paper I}%
\nocite{hidden,*}}
\begin{document}
\input{barrier}
\resource[location=local]{refs.bib}
\configurebibliography
Visible.
\end{document}";
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let visible_text = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(visible_text.contains("Visible."), "{visible_text}");
    for hidden in [
        "refs.bib", "location", "plain", "paper", "Paper I", "hidden", "*",
    ] {
        assert!(!visible_text.contains(hidden), "{hidden}: {visible_text}");
    }
}

#[test]
fn input_exit_snapshot_replays_bibliography_punctuation_aliases() {
    let source = r"\let\range\bibrangedash
\def\decorate#1{\bibopenparen#1\bibcloseparen\addcomma}
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\decorate{Alpha}Beta\range{}Gamma
\end{thebibliography}
\end{document}";
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "(Alpha),Beta-Gamma");
}

#[test]
fn input_exit_snapshot_replays_bibliography_spacing_aliases() {
    let source = r"\let\thin\addthinspace
\def\join#1#2{#1\addnbspace#2}
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\join{Alpha}{Beta}\thin Gamma
\end{thebibliography}
\end{document}";
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "Alpha Beta Gamma");
}

#[test]
fn input_exit_snapshot_replays_bibliography_state_helpers() {
    let source = r"\let\finish\finentry
\def\separate#1{#1\newunit}
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\separate{Alpha}Beta\adddot\nopunct Gamma\finish
\end{thebibliography}
\end{document}";
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "Alpha Beta. Gamma");
}

#[test]
fn input_exit_snapshot_replays_bibliography_wrappers() {
    let source = r#"\let\quoted\mkbibquote
\def\decorate#1{\mkbibparens*{#1}}
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\quoted*{Alpha} \decorate{2024}
\end{thebibliography}
\end{document}"#;
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "\"Alpha\" (2024)");
}

#[test]
fn input_exit_snapshot_replays_bibliography_string_lookup() {
    let source = r#"\def\term{andothers}
\let\localized\bibstring
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\localized{\term}
\end{thebibliography}
\end{document}"#;
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "et al");
}

#[test]
fn input_exit_snapshot_replays_bibliography_field_wrappers() {
    let source = r#"\def\titlefield#1{\bibinfo{title}{#1}}
\let\storedfield\bibfield
\begin{document}
\input{barrier}
\begin{thebibliography}{1}
\bibitem{key}\titlefield{Alpha} \storedfield{year}{2024}
\end{thebibliography}
\end{document}"#;
    let (expected, actual) = replay_render_events_after_input_exit(source);

    assert_eq!(actual, expected);
    let item = expected
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item),
            _ => None,
        })
        .expect("bibliography item");
    assert_eq!(item.text, "Alpha 2024");
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
fn input_exit_snapshot_preserves_active_footnote_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Lead\footnote{Before \cite{k}, \ref{x}, and $m$ \input{barrier} After.}Tail\end{document}",
    );

    let begin = expected
        .iter()
        .position(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .expect("footnote begin");
    let end = expected
        .iter()
        .position(|event| matches!(event.event, RenderEvent::EndFootnote(_)))
        .expect("footnote end");
    assert!(expected[begin..=end].iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::InlineCitation(_)
                | RenderEvent::InlineReference(_)
                | RenderEvent::InlineMath(_)
        )
    }));
    assert!(
        expected[begin..=end]
            .iter()
            .all(|event| event.meta.producer != EventProducer::ScannerRecovery)
    );
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_pending_detached_footnote_mark() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}Lead\footnotemark[4]\input{barrier}\footnotetext{After.}Tail\end{document}",
    );
    let mark = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::FootnoteMark(_)))
        .expect("footnote mark");
    let begin = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BeginFootnote(_)))
        .expect("footnote text begin");
    let RenderEvent::FootnoteMark(mark_payload) = &mark.event else {
        unreachable!();
    };
    let RenderEvent::BeginFootnote(begin_payload) = &begin.event else {
        unreachable!();
    };

    assert_eq!(mark_payload.note_id, begin_payload.note_id);
    assert_eq!(mark_payload.marker, begin_payload.marker);
    assert_eq!(mark.meta.producer, EventProducer::Primitive);
    assert_eq!(begin.meta.producer, EventProducer::Primitive);
    assert_eq!(actual, expected);
}

#[test]
fn semantic_snapshot_rejects_a_dangling_pending_footnote_mark() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "");
    let outcome = vm.run_plain(
        r"\begin{document}Lead\footnotemark[4]\input{barrier}\footnotetext{After.}\end{document}",
    );
    let mut semantic_capture = outcome
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .and_then(|checkpoint| checkpoint.snapshot.semantic_capture.clone())
        .expect("semantic capture with pending mark");

    assert!(semantic_capture.is_restorable());
    let mut invalid_allocator = semantic_capture.clone();
    let pending_note_id = invalid_allocator
        .footnote
        .pending_mark
        .as_ref()
        .expect("pending mark")
        .note_id;
    invalid_allocator.footnote.next_note_id = pending_note_id;
    assert!(!invalid_allocator.is_restorable());

    semantic_capture
        .footnote
        .pending_mark
        .as_mut()
        .expect("pending mark")
        .note_id += 1_000;
    assert!(!semantic_capture.is_restorable());
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

#[test]
fn input_exit_snapshot_preserves_active_bibliography_item_capture() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{thebibliography}{1}\bibitem[Alpha]{alpha}Before \cite{k} \input{barrier} After.\end{thebibliography}\end{document}",
    );

    let items = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some((item, event.meta.producer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.key, "alpha");
    assert_eq!(items[0].0.label_hint.as_deref(), Some("Alpha"));
    assert!(items[0].0.text.contains("Before"));
    assert!(items[0].0.text.contains("After"));
    assert_eq!(items[0].1, EventProducer::Primitive);
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_a_lossy_bibliography_prefix() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\begin{document}\begin{thebibliography}{1}\bibitem{k}Before \undefinedentry \input{barrier} After.\end{thebibliography}\end{document}",
    );

    let item = expected
        .iter()
        .find(|event| matches!(event.event, RenderEvent::BibliographyItem(_)))
        .expect("bibliography item");
    assert_eq!(item.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(item.meta.confidence, SemanticConfidence::Medium);
    assert_eq!(actual, expected);
}

#[test]
fn input_exit_snapshot_preserves_forced_text_after_a_misplaced_bibitem() {
    let (expected, actual) = replay_render_events_after_input_exit(
        r"\count0=0\begin{document}\ifnum\count0>0\begin{thebibliography}{1}\fi\bibitem{misplaced}Before \input{barrier} After.\end{document}",
    );
    let visible_text = expected
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            _ => None,
        })
        .collect::<String>();

    assert!(
        !expected
            .iter()
            .any(|event| matches!(event.event, RenderEvent::BibliographyItem(_)))
    );
    assert!(visible_text.contains("Before"));
    assert!(visible_text.contains("After"));
    assert_eq!(actual, expected);
}

#[test]
fn semantic_snapshot_rejects_invalid_active_bibliography_marks() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "Child.");
    let outcome = vm.run_plain(
        r"\begin{document}\begin{thebibliography}{1}\bibitem{k}Before \input{barrier} After.\end{thebibliography}\end{document}",
    );
    let semantic_capture = outcome
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .and_then(|checkpoint| checkpoint.snapshot.semantic_capture.clone())
        .expect("semantic capture with active bibliography item");

    assert!(semantic_capture.is_restorable());
    let mut invalid_text = semantic_capture.clone();
    invalid_text
        .bibliography
        .active_item
        .as_mut()
        .expect("active item")
        .text_event_mark = u64::MAX;
    assert!(!invalid_text.is_restorable());

    let mut invalid_inline = semantic_capture.clone();
    invalid_inline
        .bibliography
        .active_item
        .as_mut()
        .expect("active item")
        .inline_event_mark
        .citations = u64::MAX;
    assert!(!invalid_inline.is_restorable());

    let mut invalid_math = semantic_capture;
    invalid_math
        .bibliography
        .active_item
        .as_mut()
        .expect("active item")
        .math_event_mark = u64::MAX;
    assert!(!invalid_math.is_restorable());
}

#[test]
fn v17_semantic_capture_deserializes_before_schema_rejection() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "Child.");
    let outcome = vm.run_plain(
        r"\begin{document}\bibitem{misplaced}Stray.
\begin{thebibliography}{1}\bibitem{k}Before \input{barrier} After.
\end{thebibliography}\end{document}",
    );
    let checkpoint = outcome
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Enter
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier input checkpoint");
    let mut value = serde_json::to_value(&checkpoint.snapshot).expect("snapshot json");
    let semantic = value
        .get_mut("semantic_capture")
        .and_then(serde_json::Value::as_object_mut)
        .expect("semantic capture object");
    semantic.insert("schema_version".to_string(), serde_json::json!(17));
    semantic.remove("scanner_event_anchors");

    let text = semantic
        .get_mut("text")
        .and_then(serde_json::Value::as_object_mut)
        .expect("text snapshot");
    text.remove("executed_event_anchors");
    for slot in text
        .get_mut("scanner_slots")
        .and_then(serde_json::Value::as_array_mut)
        .expect("scanner slots")
    {
        slot.as_object_mut()
            .expect("scanner slot object")
            .remove("execution_anchor");
    }
    for range in text
        .get_mut("forced_execution_ranges")
        .and_then(serde_json::Value::as_array_mut)
        .expect("execution authority ranges")
    {
        range
            .as_object_mut()
            .expect("authority range object")
            .remove("execution_anchor");
    }
    if let Some(active_capture) = text
        .get_mut("active_capture")
        .and_then(serde_json::Value::as_object_mut)
    {
        active_capture.remove("execution_anchor");
    }

    let bibliography = semantic
        .get_mut("bibliography")
        .and_then(serde_json::Value::as_object_mut)
        .expect("bibliography snapshot");
    bibliography.remove("scanner_event_anchors");
    bibliography.remove("executed_event_anchors");
    if let Some(active_item) = bibliography
        .get_mut("active_item")
        .and_then(serde_json::Value::as_object_mut)
    {
        active_item.remove("execution_anchor");
    }

    let snapshot = serde_json::from_value::<VmSnapshot>(value)
        .expect("v17 snapshot must deserialize for graceful schema rejection");
    assert!(
        !snapshot
            .semantic_capture
            .as_ref()
            .expect("semantic capture")
            .is_restorable()
    );
}

fn replay_render_events_after_input_exit(
    source: &str,
) -> (Vec<RenderEventEnvelope>, Vec<RenderEventEnvelope>) {
    replay_render_events_at_input_boundary(source, VmModuleCheckpointKind::Exit)
}

fn child_text_execution_anchors(
    checkpoint: &VmModuleCheckpoint,
) -> (Vec<VmExecutionAnchor>, VmExecutionAnchor) {
    let semantic = checkpoint
        .snapshot
        .semantic_capture
        .as_ref()
        .expect("semantic capture");
    let child_event_ids = semantic
        .text
        .executed_events
        .iter()
        .filter_map(|event| {
            matches!(&event.event, RenderEvent::Text(text) if text.text == "Child.")
                .then_some(event.meta.sequence)
        })
        .collect::<Vec<_>>();
    let completed = semantic
        .text
        .executed_event_anchors
        .iter()
        .filter(|anchor| child_event_ids.contains(&anchor.event_sequence))
        .map(|anchor| anchor.execution_anchor.clone())
        .collect();
    let active = semantic
        .text
        .active_capture
        .as_ref()
        .expect("active child text capture")
        .execution_anchor
        .clone();
    (completed, active)
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
