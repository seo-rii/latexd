use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmModuleCheckpointKind, VmSnapshot};

fn visible_text(outcome: &tex_vm::VmOutcome) -> String {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            RenderEvent::Space(_) => Some(" "),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn citation_keys(outcome: &tex_vm::VmOutcome) -> Vec<&str> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::InlineCitation(citation) => Some(citation.keys.as_slice()),
            _ => None,
        })
        .flatten()
        .map(String::as_str)
        .collect()
}

#[test]
fn direct_builtin_comment_body_is_not_executed() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\begin{document}
Before.\begin{comment}Hidden \cite{hidden} text.\end{comment}After \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Before."));
    assert!(visible_text.contains("After"));
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn macro_generated_builtin_comment_begin_skips_source_body() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\def\hide{\begin{comment}}
\begin{document}
Before.\hide Hidden \cite{hidden} text.\end{comment}After \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn token_register_builtin_comment_body_is_not_executed() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\toks0={\begin{comment}Hidden \cite{hidden} text.\end{comment}Visible token text.}
\begin{document}
Before.\the\toks0 After \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible token text."));
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn token_register_input_does_not_execute_builtin_comment_body() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "child.tex",
        r"\begin{comment}Hidden \cite{hidden} text.\end{comment}Visible child.",
    );
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\toks0={\input{child}}
\begin{document}
Before.\the\toks0 After \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Before."));
    assert!(visible_text.contains("Visible child."));
    assert!(visible_text.contains("After"));
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn builtin_comment_body_does_not_load_nested_inputs() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("nested.tex", r"Nested \cite{nested}.");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\begin{document}
Before.\begin{comment}\input{nested}\end{comment}After \cite{shown}.
\end{document}",
    );

    assert!(
        !outcome
            .loaded_modules
            .iter()
            .any(|path| path == "nested.tex")
    );
    assert!(!visible_text(&outcome).contains("Nested"));
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn custom_excluded_environment_applies_to_dynamic_inputs() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file(
        "child.tex",
        r"\begin{draftnote}Hidden \cite{hidden} text.\end{draftnote}Visible child.",
    );
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\excludecomment{draftnote}
\toks0={\input{child}}
\begin{document}
Before.\the\toks0 After \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible child."));
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn includecomment_reenables_a_custom_environment() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\excludecomment{draftnote}
\includecomment{draftnote}
\begin{document}
\begin{draftnote}Visible \cite{shown} note.\end{draftnote}
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible"), "{visible_text:?}");
    assert_eq!(
        citation_keys(&outcome),
        ["shown"],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn includecomment_can_reenable_the_builtin_comment_environment() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\includecomment{comment}
\begin{document}
\begin{comment}Visible \cite{shown} note.\end{comment}
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible"), "{visible_text:?}");
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn macro_generated_excludecomment_controls_execution() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\def\hideenvironment#1{\excludecomment{#1}}
\hideenvironment{draftnote}
\begin{document}
\begin{draftnote}Hidden \cite{hidden} note.\end{draftnote}
Visible \cite{shown}.
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible"), "{visible_text:?}");
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn macro_generated_includecomment_reenables_builtin_comment() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\def\showcomments{\includecomment{comment}}
\showcomments
\begin{document}
\begin{comment}Visible \cite{shown} note.\end{comment}
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(visible_text.contains("Visible"), "{visible_text:?}");
    assert_eq!(
        citation_keys(&outcome),
        ["shown"],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn included_environment_authority_does_not_replace_later_fallbacks() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\includecomment{comment}
\begin{document}
\begin{comment}Visible comment text.\end{comment}
\begin{unknownenv}Fallback text.\end{unknownenv}
\end{document}",
    );

    assert!(visible_text(&outcome).contains("Visible comment text."));
    assert!(
        outcome.render_events.iter().any(|event| {
            matches!(
                &event.event,
                RenderEvent::RawFallback(fallback)
                    if fallback.environment.as_deref() == Some("unknownenv")
            )
        }),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn runtime_false_unknown_environment_does_not_emit_raw_fallback() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\count0=0
\begin{document}
\ifnum\count0>0
\begin{unknownenv}Wrong fallback text.\end{unknownenv}
\fi
Visible text.
\end{document}",
    );

    assert!(visible_text(&outcome).contains("Visible text."));
    assert!(
        !outcome.render_events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("unknownenv")
        )),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_included_environment_uses_the_call_site_range() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\includecomment{comment}
\def\opencomment{\begin{comment}}
\def\closecomment{\end{comment}}
\begin{document}
\begin{earlierfallback}Earlier fallback text.\end{earlierfallback}
\opencomment Visible comment text. \closecomment
\begin{laterfallback}Later fallback text.\end{laterfallback}
\end{document}",
    );
    let fallback_environments = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::RawFallback(fallback) => fallback.environment.as_deref(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(visible_text(&outcome).contains("Visible comment text."));
    assert_eq!(
        fallback_environments,
        ["earlierfallback", "laterfallback"],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn snapshot_roundtrip_preserves_custom_exclusions_and_builtin_inclusions() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.run_plain(r"\excludecomment{draftnote}\includecomment{comment}");
    let snapshot_json = serde_json::to_vec(&vm.snapshot()).expect("serialize snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    assert_eq!(snapshot.hidden_environments, ["draftnote"]);
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.set_entry_source_path("main.tex");
    restored.enable_render_event_capture();
    let outcome = restored.run_plain(
        r"\begin{document}
\begin{comment}Visible \cite{shown} note.\end{comment}
\begin{draftnote}Hidden \cite{hidden} note.\end{draftnote}
\end{document}",
    );
    let visible_text = visible_text(&outcome);

    assert!(
        visible_text.contains("Visible"),
        "{visible_text:?}; legacy output: {:?}; events: {:#?}",
        outcome.output,
        outcome.render_events
    );
    assert!(!visible_text.contains("Hidden"), "{visible_text:?}");
    assert_eq!(citation_keys(&outcome), ["shown"]);
}

#[test]
fn legacy_snapshot_defaults_to_hiding_the_builtin_comment_environment() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot_json = serde_json::to_value(vm.snapshot()).expect("serialize snapshot");
    let snapshot = snapshot_json.as_object_mut().expect("snapshot object");
    snapshot.remove("hidden_environments");
    snapshot.remove("included_comment_environments");
    let snapshot =
        serde_json::from_value::<VmSnapshot>(snapshot_json).expect("deserialize legacy snapshot");
    assert_eq!(snapshot.hidden_environments, ["comment"]);
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    restored.set_entry_source_path("main.tex");
    restored.enable_render_event_capture();
    let outcome = restored.run_plain(
        r"\begin{document}
Before.\begin{comment}Hidden text.\end{comment}After.
\end{document}",
    );

    assert!(!visible_text(&outcome).contains("Hidden"));
    assert!(!outcome.output.contains("Hidden"), "{:?}", outcome.output);
}

#[test]
fn input_exit_replay_preserves_custom_exclusion_policy() {
    let source = r"\excludecomment{draftnote}
\begin{document}
Before.\input{barrier}
\begin{draftnote}Hidden \cite{hidden} note.\end{draftnote}
After \cite{shown}.
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "");
    vm.enable_render_event_capture();
    let expected = vm.run_plain(source);
    let checkpoint = expected
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
    let output_prefix = expected.output[..checkpoint.output_start_utf8 as usize].to_string();
    let snapshot_json =
        serde_json::to_vec(&checkpoint.snapshot).expect("serialize continuation snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(
        format!("{output_prefix}{}", replayed.output),
        expected.output
    );
    assert_eq!(replayed.render_events, expected.render_events);
    assert!(!visible_text(&replayed).contains("Hidden"));
    assert_eq!(citation_keys(&replayed), ["shown"]);
}

#[test]
fn input_exit_replay_preserves_open_included_environment_authority() {
    let source = r"\def\showcomments{\includecomment{comment}}
\showcomments
\begin{document}
\begin{comment}Before.\input{barrier}After \cite{shown}.\end{comment}
\begin{unknownenv}Fallback text.\end{unknownenv}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "");
    vm.enable_render_event_capture();
    let expected = vm.run_plain(source);
    let checkpoint = expected
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
    let output_prefix = expected.output[..checkpoint.output_start_utf8 as usize].to_string();
    let snapshot_json =
        serde_json::to_vec(&checkpoint.snapshot).expect("serialize continuation snapshot");
    let snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &snapshot);
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");

    assert_eq!(
        format!("{output_prefix}{}", replayed.output),
        expected.output
    );
    assert_eq!(replayed.render_events, expected.render_events);
    assert!(visible_text(&replayed).contains("After"));
    assert_eq!(citation_keys(&replayed), ["shown"]);
    assert!(replayed.render_events.iter().any(|event| {
        matches!(
            &event.event,
            RenderEvent::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("unknownenv")
        )
    }));
}
