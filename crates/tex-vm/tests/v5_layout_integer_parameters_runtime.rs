use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    LayoutIntegerParameterId, Vm, VmDiagnosticKind, VmLayoutIntegerParameterAssignmentV1,
    VmLayoutIntegerParameterStateV1,
};

const PLANNED_LAYOUT_INTEGER_PARAMETERS: &[(&str, LayoutIntegerParameterId)] = &[
    ("adjdemerits", LayoutIntegerParameterId::AdjDemerits),
    ("binoppenalty", LayoutIntegerParameterId::BinOpPenalty),
    ("brokenpenalty", LayoutIntegerParameterId::BrokenPenalty),
    ("clubpenalty", LayoutIntegerParameterId::ClubPenalty),
    (
        "displaywidowpenalty",
        LayoutIntegerParameterId::DisplayWidowPenalty,
    ),
    (
        "doublehyphendemerits",
        LayoutIntegerParameterId::DoubleHyphenDemerits,
    ),
    ("exhyphenpenalty", LayoutIntegerParameterId::ExHyphenPenalty),
    (
        "finalhyphendemerits",
        LayoutIntegerParameterId::FinalHyphenDemerits,
    ),
    ("hangafter", LayoutIntegerParameterId::HangAfter),
    ("hyphenpenalty", LayoutIntegerParameterId::HyphenPenalty),
    (
        "interlinepenalty",
        LayoutIntegerParameterId::InterlinePenalty,
    ),
    ("linepenalty", LayoutIntegerParameterId::LinePenalty),
    ("looseness", LayoutIntegerParameterId::Looseness),
    (
        "postdisplaypenalty",
        LayoutIntegerParameterId::PostDisplayPenalty,
    ),
    (
        "predisplaypenalty",
        LayoutIntegerParameterId::PreDisplayPenalty,
    ),
    ("pretolerance", LayoutIntegerParameterId::PreTolerance),
    ("relpenalty", LayoutIntegerParameterId::RelPenalty),
    ("widowpenalty", LayoutIntegerParameterId::WidowPenalty),
];

#[test]
fn layout_integer_parameters_remain_source_unreachable_during_characterization() {
    for &(name, _) in PLANNED_LAYOUT_INTEGER_PARAMETERS {
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

#[test]
fn successful_layout_state_restore_does_not_activate_any_source_name() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut snapshot = Vm::new(&mut source_interner).snapshot();
    let restored_state = VmLayoutIntegerParameterStateV1 {
        layers: vec![
            PLANNED_LAYOUT_INTEGER_PARAMETERS
                .iter()
                .map(|&(_, parameter)| VmLayoutIntegerParameterAssignmentV1 {
                    parameter,
                    value: 123,
                })
                .collect(),
        ],
    };
    snapshot.layout_integer_parameter_state = Some(restored_state.clone());

    let mut restore_interner = ControlSequenceInterner::new();
    let mut restored =
        Vm::try_restore(&mut restore_interner, &snapshot).expect("restore every layout owner");
    assert_eq!(
        restored.snapshot().layout_integer_parameter_state,
        Some(restored_state.clone())
    );

    for &(name, _) in PLANNED_LAYOUT_INTEGER_PARAMETERS {
        let outcome = restored.run_plain(&format!(r"\{name}=456"));
        assert!(
            outcome.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
                    && diagnostic.detail.contains(name)
            }),
            "successful restore unexpectedly activated \\{name}"
        );
    }
    assert_eq!(
        restored.snapshot().layout_integer_parameter_state,
        Some(restored_state)
    );
}
