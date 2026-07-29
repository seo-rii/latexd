use tex_render_model::RenderEvent;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmOutcome};

fn capture(source: &str) -> VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
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
fn bibliography_wrappers_execute_visible_arguments_and_decorations() {
    let outcome = capture(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\mkbibquote{Quote} \mkbibparens{2024} \mkbibbrackets{note} \mkbibbraces{Supplement} \mkbibemph{Emph} \mkbibbold{Bold} \mkbibitalic{Italic} \mkbibnamefamily{Doe} \mkbibnameaffix{Jr.} \mkbibacro{URL}\mkbibsuperscript{2}\mkbibsubscript{a} \enquote{Nested} \parentext{Parent}.
\end{thebibliography}
\end{document}"#,
    );
    let expected =
        "\"Quote\" (2024) [note] {Supplement} Emph Bold Italic Doe Jr. URL2a \"Nested\" (Parent).";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn starred_macro_alias_and_false_conditional_execute_wrappers_only() {
    let outcome = capture(
        r#"\def\decorate#1{\mkbibparens*{#1}}
\let\quoted\mkbibquote
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\quoted*{Alpha} \decorate{2024}\iffalse\mkbibbrackets{Hidden}\fi
\end{thebibliography}
\end{document}"#,
    );
    let expected = "\"Alpha\" (2024)";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn nested_wrappers_do_not_insert_spaces_after_opening_delimiters() {
    let outcome = capture(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\mkbibquote{\mkbibemph{Nested}} \mkbibparens{\mkbibbold{2024}}
\end{thebibliography}
\end{document}"#,
    );
    let expected = "\"Nested\" (2024)";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn user_override_keeps_visible_wrapper_macro_semantics() {
    let outcome = capture(
        r"\def\mkbibquote#1{Override #1!}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\mkbibquote{Visible}
\end{thebibliography}
\end{document}",
    );

    assert!(
        outcome.output.contains("Override Visible!"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["Override Visible!"]);
}
