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
fn raw_bbl_natexlab_suffixes_and_newblock_execute_without_capture_or_markup() {
    let outcome = run(
        r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Alpha \newblock 2024\natexlab{a}, 2025\NAT@exlab{b}.
\end{thebibliography}
\end{document}"#,
        false,
    );
    let expected = "Alpha 2024a, 2025b.";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in ["natexlab", "NAT@exlab", "newblock"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_and_false_conditional_execute_only_visible_suffixes() {
    let outcome = run(
        r#"\def\suffix#1{\natexlab{#1}}
\makeatletter
\let\natsuffix\NAT@exlab
\makeatother
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}2024\suffix{a}, 2025\natsuffix{b}\iffalse\natexlab{hidden}\fi
\end{thebibliography}
\end{document}"#,
        true,
    );
    let expected = "2024a, 2025b";

    assert!(outcome.output.contains(expected), "{}", outcome.output);
    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(!outcome.output.contains("hidden"), "{}", outcome.output);
}

#[test]
fn user_definition_overrides_builtin_natexlab_suffix() {
    let outcome = run(
        r#"\def\natexlab#1{[#1]}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}2024\natexlab{custom}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert!(
        outcome.output.contains("2024[custom]"),
        "{}",
        outcome.output
    );
    assert_eq!(bibliography_items(&outcome), vec!["2024[custom]"]);
}
