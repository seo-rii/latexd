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
fn bibliography_identifiers_execute_values_without_capture_or_command_names() {
    let outcome = run(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\doi{10.1000/example}. \eprint{arXiv:2401.00001}.
\end{thebibliography}
\end{document}"#,
        false,
    );
    let expected = "10.1000/example. arXiv:2401.00001.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in ["\\doi", "\\eprint"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_conditional_and_nested_identifier_values_follow_vm_execution() {
    let outcome = run(
        r#"\def\paperdoi#1{\doi{#1}}
\let\archiveid\eprint
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\paperdoi{\mkbibacro{DOI}:10.1000/example} \archiveid{arXiv:2401.00001}\iffalse \doi{hidden}\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "DOI:10.1000/example arXiv:2401.00001";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(!outcome.output.contains("hidden"), "{}", outcome.output);
}

#[test]
fn user_definition_overrides_builtin_doi_wrapper() {
    let outcome = run(
        r#"\def\doi#1{DOI=#1}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\doi{custom}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(outcome.output.contains("DOI=custom"), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec!["DOI=custom"]);
}
