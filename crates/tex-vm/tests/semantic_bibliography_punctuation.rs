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
fn bibliography_punctuation_and_delimiters_execute_as_visible_text() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}A\addcomma{}B\addcolon{}C\addsemicolon{}D\adddot{}E\adddotspace{}F\isdot{}G\bibrangedash{}H\addhyphen{}I\textendash{}J\textemdash{}K\addslash{}L\bibnamedash{}M\bibopenparen{}N\bibcloseparen{}\bibopenbracket{}O\bibclosebracket{}\bibopenbrace{}P\bibclosebrace
\end{thebibliography}
\end{document}",
    );
    let expected = "A,B:C;D.E. F.G-H-I-J---K/L---M(N)[O]{P}";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn macro_alias_and_false_conditional_use_executed_punctuation_only() {
    let outcome = capture(
        r"\def\decorate#1{\bibopenparen#1\bibcloseparen\addcomma}
\let\range\bibrangedash
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\decorate{Alpha}Beta\range{}Gamma\iffalse\addcomma\bibopenbracket\fi
\end{thebibliography}
\end{document}",
    );
    let expected = "(Alpha),Beta-Gamma";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn user_override_keeps_visible_macro_semantics() {
    let outcome = capture(
        r"\def\addcomma{!}
\def\bibopenparen{<}
\def\bibcloseparen{>}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\bibopenparen Alpha\bibcloseparen\addcomma
\end{thebibliography}
\end{document}",
    );

    assert!(outcome.output.contains("<Alpha>!"), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec!["<Alpha>!"]);
}
