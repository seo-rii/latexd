use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind, VmOutcome};

fn capture(source: &str) -> VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.run_plain(source)
}

fn visible_text(outcome: &VmOutcome) -> String {
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

#[test]
fn bibliography_metadata_commands_consume_non_visible_arguments() {
    let outcome = capture(
        r"\addbibresource[location=local][datatype=bibtex]{refs.bib}
\bibliographystyle{plain}
\defcitealias{paper}{Paper I}
\begin{document}
\nocite{hidden,*}
Visible.
\end{document}",
    );
    let text = visible_text(&outcome);

    assert_eq!(text.trim(), "Visible.", "{:#?}", outcome.render_events);
    for hidden in [
        "refs.bib", "location", "datatype", "plain", "paper", "Paper I", "hidden", "*",
    ] {
        assert!(
            !outcome.output.contains(hidden),
            "{hidden}: {}",
            outcome.output
        );
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }
    assert!(
        !outcome.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
                && [
                    "addbibresource",
                    "bibliographystyle",
                    "defcitealias",
                    "nocite",
                ]
                .iter()
                .any(|command| diagnostic.detail.contains(command))
        }),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn url_style_consumes_non_visible_argument_without_package_shim() {
    let outcome = capture(r"\begin{document}\urlstyle{same}Visible.\end{document}");
    let text = visible_text(&outcome);

    assert_eq!(outcome.output, "Visible.");
    assert_eq!(text.trim(), "Visible.", "{:#?}", outcome.render_events);
    assert!(
        !outcome.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
                && diagnostic.detail.contains("urlstyle")
        }),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn macro_generated_bibliography_metadata_does_not_leak_arguments() {
    let outcome = capture(
        r"\def\setbibliography#1{%
\addbibresource{#1}%
\bibliographystyle{plain}%
\defcitealias{paper}{Paper I}%
\nocite{hidden,*}}
\begin{document}
\setbibliography{refs.bib}
Visible.
\end{document}",
    );
    let text = visible_text(&outcome);

    assert_eq!(text.trim(), "Visible.", "{:#?}", outcome.render_events);
    for hidden in ["refs.bib", "plain", "paper", "Paper I", "hidden", "*"] {
        assert!(
            !outcome.output.contains(hidden),
            "{hidden}: {}",
            outcome.output
        );
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }
}

#[test]
fn user_overrides_keep_their_visible_execution_semantics() {
    let outcome = capture(
        r"\def\addbibresource#1{Resource #1.}
\def\bibliographystyle#1{Style #1.}
\def\urlstyle#1{URL style #1.}
\def\defcitealias#1#2{Alias #1 is #2.}
\def\nocite#1{Keys #1.}
\begin{document}
\addbibresource{shown.bib}
\bibliographystyle{visible}
\urlstyle{shown}
\defcitealias{paper}{Visible Alias}
\nocite{shown}
\end{document}",
    );
    let text = visible_text(&outcome);

    for visible in [
        "Resource shown.bib.",
        "Style visible.",
        "URL style shown.",
        "Alias paper is Visible Alias.",
        "Keys shown.",
    ] {
        assert!(
            outcome.output.contains(visible),
            "{visible}: {}",
            outcome.output
        );
        assert!(text.contains(visible), "{visible}: {text}");
    }
}
