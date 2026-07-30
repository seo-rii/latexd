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
fn case_wrappers_execute_visible_arguments_with_or_without_event_capture() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\NoCaseChange{NASA}. \MakeSentenceCase{alpha title}. \MakeTitleCase*{beta title}.
\end{thebibliography}
\end{document}"#;
    let expected = "NASA. alpha title. beta title.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in ["NoCaseChange", "MakeSentenceCase", "MakeTitleCase"] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn mounted_bibliography_executes_case_wrappers() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.mount_file(
        "refs.bbl",
        r#"\begin{thebibliography}{1}
\bibitem{key}\NoCaseChange{NASA}. \MakeSentenceCase*{alpha title}. \MakeTitleCase{beta title}.
\end{thebibliography}"#,
    );
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");
    let expected = "NASA. alpha title. beta title.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn case_wrappers_preserve_adjacent_text_boundaries() {
    let outcome = run(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Mc\NoCaseChange{Donald} pre\MakeSentenceCase{view} title\MakeTitleCase{case}
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "McDonald preview titlecase";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_case_wrappers() {
    let outcome = run(
        r#"\def\sentence#1{\MakeSentenceCase*{#1}}
\let\titlecase\MakeTitleCase
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\NoCaseChange{NASA}. \sentence{alpha title}. \titlecase{beta title}.\iffalse \MakeTitleCase{hidden title}.\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "NASA. alpha title. beta title.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(
        !outcome.output.contains("hidden title"),
        "{}",
        outcome.output
    );
}

#[test]
fn user_definition_overrides_builtin_case_wrapper() {
    let outcome = run(
        r#"\def\MakeTitleCase#1{Override #1!}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\MakeTitleCase{custom title}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(
        outcome.output.contains("Override custom title!"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["Override custom title!"]);
}
