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
fn explicit_bibliography_spacing_helpers_execute_as_spaces() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}A\addspace B\addabbrvspace C\addnbspace D\addthinspace E\addlowpenspace F\addhighpenspace G
\end{thebibliography}
\end{document}",
    );
    let expected = "A B C D E F G";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn article_newblock_preserves_the_bibliography_word_boundary() {
    let outcome = capture(
        r"\documentclass{article}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Author.\newblock Long title.
\end{thebibliography}
\end{document}",
    );

    assert!(
        outcome.output.contains("Author. Long title."),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["Author. Long title."]);
}

#[test]
fn macro_alias_and_false_conditional_use_executed_spacing_only() {
    let outcome = capture(
        r"\def\join#1#2{#1\addnbspace#2}
\let\thin\addthinspace
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\join{Alpha}{Beta}\thin Gamma\iffalse\addspace Hidden\fi
\end{thebibliography}
\end{document}",
    );
    let expected = "Alpha Beta Gamma";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn user_override_keeps_visible_spacing_macro_semantics() {
    let outcome = capture(
        r"\def\addspace{!}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Alpha\addspace Beta
\end{thebibliography}
\end{document}",
    );

    assert!(outcome.output.contains("Alpha!Beta"), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec!["Alpha!Beta"]);
}
