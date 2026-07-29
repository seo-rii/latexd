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
fn bibliography_state_helpers_do_not_leak_command_names() {
    let outcome = capture(
        r"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Alpha\newunit Beta\unspace\adddot\nopunct Gamma\newunit\urlprefix Delta\finentry
\end{thebibliography}
\end{document}",
    );
    let expected = "Alpha Beta. Gamma Delta";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn macro_alias_and_false_conditional_execute_state_helpers_only() {
    let outcome = capture(
        r"\def\separate#1{#1\newunit}
\let\finish\finentry
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\separate{Alpha}Beta\adddot\nopunct Gamma\finish\iffalse\newunit Hidden\fi
\end{thebibliography}
\end{document}",
    );
    let expected = "Alpha Beta. Gamma";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
}

#[test]
fn user_overrides_keep_visible_state_macro_semantics() {
    let outcome = capture(
        r"\def\newunit{!}
\def\finentry{?}
\def\nopunct{+}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Alpha\newunit Beta\finentry\nopunct
\end{thebibliography}
\end{document}",
    );

    assert!(
        outcome.output.contains("Alpha!Beta?+"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["Alpha!Beta?+"]);
}
