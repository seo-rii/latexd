use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmModuleCheckpointKind, VmSnapshot};

#[test]
fn control_sequence_scope_replay_matches_clean_execution() {
    assert_control_sequence_scope_replay_matches_clean_execution();
}

#[test]
fn render_event_capture_preserves_globaldefs_definition_scope() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"{\globaldefs=1\def\seed{S}\let\foo\seed}{\globaldefs=-1\global\def\discarded{D}}\ifdefined\foo T\else F\fi\ifdefined\discarded T\else F\fi\foo",
    );

    assert_eq!(outcome.output, "TFS");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn render_event_capture_preserves_register_alias_assignment_syntax() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\def\countalias{\count0}\def\dimenalias{\dimen0}\def\skipalias{\skip0}\def\toksalias{\toks0}\countalias=7\dimenalias=2pt\skipalias=3pt\toksalias={OK}[\the\countalias][\the\dimenalias][\the\skipalias][\the\toksalias]",
    );

    assert_eq!(outcome.output, "[7][2pt][3pt][OK]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn global_control_sequence_assignments_cancel_pending_local_restores() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);

    let outcome = vm.run_plain(
        r"\def\foo{O}{\def\foo{L}{\def\foo{N}\global\def\foo{G}\foo}\foo}\foo
\def\left{A}\def\right{B}\let\slot\left{\let\slot\right{\let\slot\left\global\let\slot\right\slot}\slot}\slot
\def\mode{O}{\def\mode{L}\globaldefs=1\def\mode{G}\mode}\mode",
    );

    assert_eq!(
        outcome.output.split_whitespace().collect::<String>(),
        "GGGBBBGG"
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

fn assert_control_sequence_scope_replay_matches_clean_execution() {
    let source = r"\def\kept{R}{\def\kept{L}{\def\kept{N}\global\let\alias\kept}}{\globaldefs=1\def\persist{P}\let\persistalias\persist}{\globaldefs=-1\global\def\discarded{D}\global\let\discardedalias\persist}\count0=0\ifnum\count0>0\input{missing}\fi\input{barrier}\begin{document}[\kept][\alias][\persist][\persistalias]\ifdefined\discarded BAD\else GOOD\fi\begin{overpic}[width=4cm]{right.pdf}\end{overpic}\undefinedafter\end{document}";

    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();
    vm.set_entry_source_path("main.tex");
    vm.mount_file("barrier.tex", "|");
    let clean = vm.run_plain(source);
    let checkpoint = clean
        .module_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == VmModuleCheckpointKind::Exit
                && checkpoint.module_path.as_str() == "barrier.tex"
        })
        .expect("barrier exit checkpoint");
    assert!(checkpoint.snapshot.continuation_safety.is_safe());
    assert!(
        checkpoint
            .snapshot
            .semantic_capture
            .as_ref()
            .expect("semantic capture")
            .is_restorable()
    );
    let output_prefix = clean.output[..checkpoint.output_start_utf8 as usize].to_string();
    let snapshot_json = serde_json::to_vec(&checkpoint.snapshot).expect("serialize checkpoint");
    let checkpoint_snapshot =
        serde_json::from_slice::<VmSnapshot>(&snapshot_json).expect("deserialize checkpoint");
    let clean_scopes = vm.snapshot().scopes;
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::restore(&mut restored_interner, &checkpoint_snapshot);
    restored.mount_file("barrier.tex", "|");
    let replayed = restored
        .resume_continuation()
        .expect("restored input continuation");
    let replayed_scopes = restored.snapshot().scopes;

    assert_eq!(format!("{output_prefix}{}", replayed.output), clean.output);
    assert!(
        clean.output.contains("|[R][N][P][P]GOOD"),
        "{}",
        clean.output
    );
    assert_eq!(replayed.render_events, clean.render_events);
    assert_eq!(replayed.diagnostics, clean.diagnostics);
    assert_eq!(replayed.transcript, clean.transcript);
    assert_eq!(replayed.registers, clean.registers);
    assert_eq!(replayed_scopes, clean_scopes);
    assert!(clean.render_events.iter().any(|event| {
        matches!(&event.event, RenderEvent::GraphicRef(graphic) if graphic.path == "right.pdf")
    }));
    assert!(!clean.render_events.iter().any(|event| {
        matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("missing input")
        )
    }));
    assert!(
        clean
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail.contains("undefinedafter")),
        "{:#?}",
        clean.diagnostics
    );
}
