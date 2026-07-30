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
fn leavevmode_and_unskip_execute_with_or_without_event_capture() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\protect\relax\leavevmode\ignorespaces   Visible. Trimmed   \unskip. Solid\unskip.
\end{thebibliography}
\end{document}"#;
    let expected = "Visible. Trimmed. Solid.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in ["leavevmode", "unskip"] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn mounted_bibliography_executes_no_output_state_helpers() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.mount_file(
        "refs.bbl",
        r#"\begin{thebibliography}{1}
\bibitem{key}\leavevmode Visible. Trimmed \unskip.
\end{thebibliography}"#,
    );
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");
    let expected = "Visible. Trimmed.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_state_helpers() {
    let outcome = run(
        r#"\def\trim{\unskip}
\let\enterhmode\leavevmode
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\enterhmode Visible. Trimmed \trim.\iffalse \leavevmode Hidden \unskip.\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "Visible. Trimmed.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(!outcome.output.contains("Hidden"), "{}", outcome.output);
}

#[test]
fn user_definitions_override_builtin_state_helpers() {
    let outcome = run(
        r#"\def\leavevmode{[mode]}
\def\unskip{[skip]}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\leavevmode{} Visible \unskip
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(
        outcome.output.contains("[mode] Visible [skip]"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["[mode] Visible [skip]"]);
}
