use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmDiagnosticKind};

const PLANNED_LAYOUT_INTEGER_PARAMETERS: &[&str] = &[
    "adjdemerits",
    "binoppenalty",
    "brokenpenalty",
    "clubpenalty",
    "displaywidowpenalty",
    "doublehyphendemerits",
    "exhyphenpenalty",
    "finalhyphendemerits",
    "hangafter",
    "hyphenpenalty",
    "interlinepenalty",
    "linepenalty",
    "looseness",
    "postdisplaypenalty",
    "predisplaypenalty",
    "pretolerance",
    "relpenalty",
    "widowpenalty",
];

#[test]
fn layout_integer_parameters_remain_source_unreachable_during_characterization() {
    for name in PLANNED_LAYOUT_INTEGER_PARAMETERS {
        let mut interner = ControlSequenceInterner::new();
        let source = format!(r"\{name}=123");
        let outcome = Vm::new(&mut interner).run_plain(&source);

        assert!(
            outcome.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
                    && diagnostic.detail.contains(name)
            }),
            "expected \\{name} to remain source-unreachable"
        );
    }
}
