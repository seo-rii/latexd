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
fn text_scripts_preserve_attachment_and_word_boundaries_with_or_without_capture() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Edition\textsuperscript{2}\textsubscript{a}. Marker\textsuperscript{1}Word. Nested\textsuperscript{a\textsubscript{b}c}. Styled\textsuperscript{\emph{2}}Tail.
\end{thebibliography}
\end{document}"#;
    let expected = "Edition2a. Marker1 Word. Nestedabc. Styled2 Tail.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in [
            "textsuperscript",
            "textsubscript",
            "Edition2 a",
            "Nestedab c",
        ] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn mounted_bibliography_executes_text_scripts() {
    let refs = r#"\begin{thebibliography}{1}
\bibitem{key}Edition\textsuperscript{2}\textsubscript{a}. Marker\textsuperscript{1}Word.
\end{thebibliography}"#;
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.mount_file("refs.bbl", refs);
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");
    let expected = "Edition2a. Marker1 Word.";

    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in ["textsuperscript", "textsubscript", "Edition2 a"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_text_scripts() {
    let outcome = run(
        r#"\let\super\textsuperscript
\def\pair#1#2{\textsuperscript{#1}\textsubscript{#2}}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Edition\pair{2}{a}. Marker\super{1}Word.\iffalse \textsuperscript{Hidden}Wrong\fi
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(
        bibliography_items(&outcome),
        vec!["Edition2a. Marker1 Word."]
    );
    assert!(!outcome.output.contains("Hidden"), "{}", outcome.output);
    assert!(!outcome.output.contains("Wrong"), "{}", outcome.output);
}

#[test]
fn user_definitions_override_builtin_text_scripts() {
    let outcome = run(
        r#"\def\textsuperscript#1{[up:#1]}
\def\textsubscript#1{[down:#1]}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\textsuperscript{A}\textsubscript{B}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(bibliography_items(&outcome), vec!["[up:A][down:B]"]);
}
