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
fn bibliography_fields_execute_values_without_capture_or_field_names() {
    let outcome = run(
        r#"\def\selector{HiddenSelector}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\bibinfo{\selector}{10.1000/example}. \bibfield{journal}{Journal of Tests}.
\end{thebibliography}
\end{document}"#,
        false,
    );
    let expected = "10.1000/example. Journal of Tests.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in ["bibinfo", "bibfield", "HiddenSelector", "journal"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_conditional_and_nested_wrappers_follow_vm_execution() {
    let outcome = run(
        r#"\def\titlefield#1{\bibinfo{title}{#1}}
\let\storedfield\bibfield
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\titlefield{\mkbibquote{Visible}} \storedfield{year}{2024}\iffalse \bibinfo{title}{Hidden}\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "\"Visible\" 2024";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(!outcome.output.contains("Hidden"), "{}", outcome.output);
}

#[test]
fn user_definition_overrides_builtin_bibliography_field() {
    let outcome = run(
        r#"\def\bibinfo#1#2{#1=#2}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\bibinfo{title}{Custom}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(
        outcome.output.contains("title=Custom"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["title=Custom"]);
}
