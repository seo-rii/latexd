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
fn box_wrappers_execute_visible_bodies_with_or_without_event_capture() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\framebox[2em][c]{Wide}. \raisebox{0.5ex}[1ex][0ex]{Raised}. \parbox[t][5em][b]{4em}{Paragraph}. \makebox[3em][l]{Inline}.
\end{thebibliography}
\end{document}"#;
    let expected = "Wide. Raised. Paragraph. Inline.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in [
            "framebox", "raisebox", "parbox", "makebox", "2em", "0.5ex", "1ex", "0ex", "5em",
            "4em", "3em",
        ] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn picture_mode_box_wrappers_consume_geometry_and_execute_body() {
    let source = r#"\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\framebox(2,1)[c]{Picture}. \makebox(3,1)[l]{Canvas}.
\end{thebibliography}
\end{document}"#;
    let expected = "Picture. Canvas.";

    for capture_events in [false, true] {
        let outcome = run(source, capture_events);

        assert!(outcome.output.contains(expected), "{}", outcome.output);
        for hidden in ["framebox", "makebox", "(2,1)", "(3,1)"] {
            assert!(!outcome.output.contains(hidden), "{}", outcome.output);
        }
        if capture_events {
            assert_eq!(bibliography_items(&outcome), vec![expected]);
        }
    }
}

#[test]
fn mounted_bibliography_executes_box_wrappers() {
    let refs = r#"\begin{thebibliography}{1}
\bibitem{key}\framebox[2em][c]{Wide}. \raisebox{0.5ex}[1ex][0ex]{Raised}. \parbox[t]{4em}{Paragraph}. \makebox[3em][l]{Inline}.
\end{thebibliography}"#;
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    vm.mount_file("refs.bbl", refs);
    let outcome = vm.run_plain(r"\begin{document}\input{refs.bbl}\end{document}");
    let expected = "Wide. Raised. Paragraph. Inline.";

    assert_eq!(bibliography_items(&outcome), vec![expected]);
    assert!(outcome.output.contains(expected), "{}", outcome.output);
    for hidden in ["framebox", "raisebox", "parbox", "makebox", "2em", "0.5ex"] {
        assert!(!outcome.output.contains(hidden), "{}", outcome.output);
    }
}

#[test]
fn macro_alias_and_false_conditional_execute_only_reached_box_wrappers() {
    let outcome = run(
        r#"\def\wide#1{\framebox[2em][c]{#1}}
\let\lift\raisebox
\def\paragraph#1{\parbox[t]{4em}{#1}}
\let\inline\makebox
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\wide{Wide}. \lift{0.5ex}[1ex][0ex]{Raised}. \paragraph{Paragraph}. \inline[3em][l]{Inline}.\iffalse \framebox{Hidden}.\fi
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(
        bibliography_items(&outcome),
        vec!["Wide. Raised. Paragraph. Inline."]
    );
    assert!(!outcome.output.contains("Hidden"), "{}", outcome.output);
}

#[test]
fn user_definitions_override_builtin_box_wrappers() {
    let outcome = run(
        r#"\def\framebox#1{[frame:#1]}
\def\raisebox#1#2{[raise:#1:#2]}
\def\parbox#1#2{[par:#1:#2]}
\def\makebox#1{[make:#1]}
\begin{document}
\begin{thebibliography}{1}
\bibitem{key}\framebox{A} \raisebox{B}{C} \parbox{D}{E} \makebox{F}
\end{thebibliography}
\end{document}"#,
        true,
    );

    assert_eq!(
        bibliography_items(&outcome),
        vec!["[frame:A] [raise:B:C] [par:D:E] [make:F]"]
    );
}
