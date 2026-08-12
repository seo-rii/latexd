use tex_render_model::{RenderEvent, SpaceKind};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

#[test]
fn runtime_false_algorithmic_commands_do_not_leak_scanner_text() {
    let source = r"\count0=0
\begin{document}
\ifnum\count0>0
\If{Hidden condition}\Else\Comment{Hidden note}
\fi
\If{Visible condition}\Else\Comment{Visible note}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(source);
    let text = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text,
        [
            "if ",
            "Visible",
            "condition",
            " then",
            "else",
            "Visible note"
        ]
    );
    assert_eq!(
        outcome
            .render_events
            .iter()
            .filter(|event| {
                matches!(
                    event.event,
                    RenderEvent::Space(ref space) if space.kind == SpaceKind::Explicit
                )
            })
            .count(),
        1
    );
}
