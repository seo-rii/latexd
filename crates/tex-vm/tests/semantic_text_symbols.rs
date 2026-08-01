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
fn visible_text_symbols_execute_with_or_without_event_capture() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Quote\textquotesingle s. Double\textquotedbl q. Angles\textless x\textgreater. Pipe\textbar join. Path\slash name.
\end{thebibliography}
\end{document}"#;
    let expected = "Quote's. Double\"q. Angles<x>. Pipe|join. Path/name.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in [
            "textquotesingle",
            "textquotedbl",
            "textless",
            "textgreater",
            "textbar",
            "slash",
        ] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn tex_latin_letter_symbols_preserve_visible_characters() {
    let source = r"\begin{document}\aa{}\AA{}\ae{}\AE{}\oe{}\OE{}\o{}\O{}\l{}\L{}\ss{}\i{}\j{}; {\L}ukasz and \l{}odz.\end{document}";
    let expected = "åÅæÆœŒøØłŁßıȷ; Łukasz and łodz.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        if capture_events {
            let visible_text =
                outcome
                    .render_events
                    .iter()
                    .fold(String::new(), |mut visible_text, event| {
                        match &event.event {
                            RenderEvent::Text(text) => visible_text.push_str(&text.text),
                            RenderEvent::Space(_) => visible_text.push(' '),
                            _ => {}
                        }
                        visible_text
                    });
            assert!(
                visible_text.contains(expected),
                "{visible_text}; events: {:#?}",
                outcome.render_events
            );
        }
    }
}

#[test]
fn mounted_bibliography_executes_visible_text_symbols() {
    let refs = r#"\begin{thebibliography}{1}
\bibitem{key}Quote\textquotesingle s. Double\textquotedbl q. Angles\textless x\textgreater. Pipe\textbar join. Path\slash name.
\end{thebibliography}"#;
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.mount_file("refs.bbl", refs);
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");
    let expected = "Quote's. Double\"q. Angles<x>. Pipe|join. Path/name.";

    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in [
        "textquotesingle",
        "textquotedbl",
        "textless",
        "textbar",
        "slash",
    ] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_text_symbols() {
    let outcome = run(
        r#"\let\apostrophe\textquotesingle
\def\quoted#1{\textquotedbl #1\textquotedbl}
\def\angles#1{\textless #1\textgreater}
\let\pipe\textbar
\def\path#1{Path\slash #1}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}Quote\apostrophe s. \quoted{double}. \angles{x}. A\pipe B. \path{name}.\iffalse \textbar Hidden\fi
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(
        bibliography_items(&outcome),
        vec!["Quote's. \"double\". <x>. A|B. Path/name."]
    );
    assert!(!outcome.output.contains("Hidden"), "{}", outcome.output);
}

#[test]
fn user_definitions_override_builtin_text_symbols() {
    let outcome = run(
        r#"\def\textquotesingle{[single]}
\def\textquotedbl{[double]}
\def\textless{[less]}
\def\textgreater{[greater]}
\def\textbar{[bar]}
\def\slash{[slash]}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\textquotesingle\textquotedbl\textless\textgreater\textbar\slash
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(
        bibliography_items(&outcome),
        vec!["[single][double][less][greater][bar][slash]"]
    );
}
