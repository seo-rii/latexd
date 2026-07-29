use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

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
