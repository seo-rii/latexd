use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmOutcome};

fn run(source: &str, capture_events: bool) -> VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    if capture_events {
        vm.enable_render_event_capture();
    }
    vm.run_plain(source)
}

fn bibliography_items(outcome: &VmOutcome) -> Vec<&str> {
    outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::BibliographyItem(item) => Some(item.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn bibstring_lookup_runs_without_render_event_capture() {
    let outcome = run(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Alpha \bibstring{andothers}. \bibstring{customterm}. A\bibstring{}B
\end{thebibliography}
\end{document}"#,
        false,
    );

    assert!(
        outcome.output.contains("Alpha et al. customterm. AB"),
        "{}",
        outcome.output
    );
    assert!(!outcome.output.contains("andothers"), "{}", outcome.output);
    assert!(!outcome.output.contains("bibstring"), "{}", outcome.output);
}

#[test]
fn macro_expanded_keys_aliases_and_conditionals_follow_vm_execution() {
    let outcome = run(
        r#"\def\term{andothers}
\def\localized#1{\bibstring{#1}}
\let\samebibstring\bibstring
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\localized{\term} \samebibstring{andothers}\iffalse \bibstring{hidden}\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "et al et al";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(!outcome.output.contains("hidden"), "{}", outcome.output);
}

#[test]
fn user_definition_overrides_builtin_bibstring_lookup() {
    let outcome = run(
        r#"\def\bibstring#1{localized=#1}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\bibstring{custom}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(
        outcome.output.contains("localized=custom"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["localized=custom"]);
}
