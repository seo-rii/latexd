use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use tex_tokens::{CatCode, ControlSequenceInterner};
use tex_vm::{
    LayoutIntegerParameterId, SnapshotMeaning, SnapshotToken, SnapshotTokenKind,
    VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm, VmActiveModuleOptionsSnapshot,
    VmActiveSourceFrameSnapshot, VmDiagnosticKind, VmInputContinuationSnapshot,
    VmQueueItemSnapshot, VmSnapshotDocument,
};

const LAYOUT_INTEGER_PARAMETERS: &[(&str, LayoutIntegerParameterId, i32)] = &[
    ("adjdemerits", LayoutIntegerParameterId::AdjDemerits, 0),
    ("binoppenalty", LayoutIntegerParameterId::BinOpPenalty, 0),
    ("brokenpenalty", LayoutIntegerParameterId::BrokenPenalty, 0),
    ("clubpenalty", LayoutIntegerParameterId::ClubPenalty, 0),
    (
        "displaywidowpenalty",
        LayoutIntegerParameterId::DisplayWidowPenalty,
        0,
    ),
    (
        "doublehyphendemerits",
        LayoutIntegerParameterId::DoubleHyphenDemerits,
        0,
    ),
    (
        "exhyphenpenalty",
        LayoutIntegerParameterId::ExHyphenPenalty,
        0,
    ),
    (
        "finalhyphendemerits",
        LayoutIntegerParameterId::FinalHyphenDemerits,
        0,
    ),
    ("hangafter", LayoutIntegerParameterId::HangAfter, 1),
    ("hyphenpenalty", LayoutIntegerParameterId::HyphenPenalty, 0),
    (
        "interlinepenalty",
        LayoutIntegerParameterId::InterlinePenalty,
        0,
    ),
    ("linepenalty", LayoutIntegerParameterId::LinePenalty, 0),
    ("looseness", LayoutIntegerParameterId::Looseness, 0),
    (
        "postdisplaypenalty",
        LayoutIntegerParameterId::PostDisplayPenalty,
        0,
    ),
    (
        "predisplaypenalty",
        LayoutIntegerParameterId::PreDisplayPenalty,
        0,
    ),
    ("pretolerance", LayoutIntegerParameterId::PreTolerance, 0),
    ("relpenalty", LayoutIntegerParameterId::RelPenalty, 0),
    ("widowpenalty", LayoutIntegerParameterId::WidowPenalty, 0),
];

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    Vm::new(&mut interner).run_plain(source)
}

#[test]
fn every_layout_integer_parameter_exposes_its_tex82_default_and_assignment() {
    for &(name, _, default) in LAYOUT_INTEGER_PARAMETERS {
        let outcome = run(&format!(r"[\the\{name}]\{name}=123[\number\{name}]"));

        assert_eq!(outcome.output, format!("[{default}][123]"), "\\{name}");
        assert!(
            outcome.diagnostics.is_empty(),
            "\\{name}: {:#?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn pretolerance_numeric_group_global_and_afterassignment_contract_matches_tex82() {
    let outcome = run(
        r#"\pretolerance '123[\number\pretolerance]\pretolerance="1234[\the\pretolerance]\pretolerance=`A[\number\pretolerance]\pretolerance --+17[\the\pretolerance]\pretolerance=100{\pretolerance=101[\the\pretolerance]}[\the\pretolerance]{\global\pretolerance=102}[\the\pretolerance]{\globaldefs=-1\global\pretolerance=103}[\the\pretolerance]{\globaldefs=1\pretolerance=104}[\the\pretolerance]\def\mark{M}\afterassignment\mark\pretolerance=105[\the\pretolerance]"#,
    );

    assert_eq!(
        outcome.output,
        "[83][4660][65][17][101][100][102][102][104]M[105]"
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn pretolerance_is_an_integer_expression_and_arithmetic_lvalue() {
    let outcome = run(
        r"\pretolerance=12\advance\pretolerance by 5[\the\pretolerance]\multiply\pretolerance by -3[\number\pretolerance]\divide\pretolerance by 2[\the\pretolerance]\ifnum\pretolerance=-25T\else F\fi\pretolerance=2147483647\advance\pretolerance by1[\number\pretolerance]\ifnum\pretolerance<0T\else F\fi",
    );

    assert_eq!(outcome.output, "[17][-51][-25]T[-2147483648]T");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn pretolerance_bad_numbers_and_arithmetic_preserve_state_and_hooks() {
    let outcome = run(
        r"\def\mark{M}\afterassignment\mark\pretolerance=2147483648[\the\pretolerance]\pretolerance=-2147483648[\the\pretolerance]\afterassignment\mark\pretolerance=\relax[\the\pretolerance]\pretolerance=1073741824\afterassignment\mark\multiply\pretolerance by2[\the\pretolerance]\pretolerance=17\afterassignment\mark\divide\pretolerance by0[\the\pretolerance]\pretolerance=2147483647\advance\pretolerance by1\afterassignment\mark\divide\pretolerance by-1[\the\pretolerance]",
    );

    assert_eq!(
        outcome.output,
        "M[2147483647][-2147483647]M[0]M[1073741824]M[17]M[-2147483648]"
    );
    assert_eq!(outcome.diagnostics.len(), 6, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.detail.contains("Number too big"))
            .count(),
        2
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail.contains("Missing number"))
    );
    assert_eq!(
        outcome
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.detail.contains("Arithmetic overflow"))
            .count(),
        3
    );
}

#[test]
fn layout_parameter_alias_has_a_stable_wire_name_and_survives_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\let\savedpretolerance\pretolerance\savedpretolerance=222[\the\savedpretolerance]",
    );

    assert_eq!(outcome.output, "[222]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot.scopes[0].get("savedpretolerance"),
        Some(&SnapshotMeaning::Primitive {
            name: "pretolerance".to_string(),
        })
    );
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    let document = VmSnapshotDocument::from_snapshot(snapshot);
    let wire = serde_json::to_value(&document).expect("encode layout parameter alias");
    assert_eq!(
        wire["state"]["scopes"][0]["savedpretolerance"]["name"],
        "pretolerance"
    );

    drop(vm);
    let bytes = serde_json::to_vec(&document).expect("encode layout parameter document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &bytes)
        .expect("restore layout parameter alias");
    let outcome = restored.run_plain(r"\savedpretolerance=223[\number\pretolerance]");
    assert_eq!(outcome.output, "[223]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn restored_layout_alias_keeps_primitive_identity_while_the_canonical_name_is_shadowed() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\let\savedhangafter\hangafter{\def\hangafter{S}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(vm.snapshot()))
        .expect("encode shadowed layout alias");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore shadowed layout alias");
    let outcome = restored
        .run_plain(r"[\hangafter]\savedhangafter=9[\number\savedhangafter]}[\the\hangafter]");
    assert_eq!(outcome.output, "[S][9][1]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn source_layout_state_and_latent_tokens_separate_state_and_command_capabilities() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\pretolerance=100{\hangafter=2");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot
            .layout_integer_parameter_state
            .as_ref()
            .expect("source-created layout state")
            .layers
            .len(),
        2
    );
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(!snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(!snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&snapshot).is_err());

    let mut latent_interner = ControlSequenceInterner::new();
    let mut latent_vm = Vm::new(&mut latent_interner);
    let outcome = latent_vm.run_plain(r"\def\deferred{\pretolerance=200}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let latent = latent_vm.snapshot();
    assert!(latent.layout_integer_parameter_state.is_none());
    assert!(latent.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(latent.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
    }));
    assert!(!latent.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&latent).is_err());
}

#[test]
fn explicit_redefinition_and_existence_checks_treat_layout_names_as_builtins() {
    let outcome = run(
        r"\ifcsname pretolerance\endcsname T\else F\fi\ifdefined\pretolerance T\else F\fi\let\savedpretolerance\pretolerance{\def\pretolerance{46}[\pretolerance]}[\the\pretolerance]\savedpretolerance=45[\number\savedpretolerance]",
    );

    assert_eq!(outcome.output, "TT[46][0][45]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn layout_parameter_assignment_does_not_activate_current_rendering_semantics() {
    let baseline = run("one two three four five six seven eight nine ten");
    let assigned = run(r"\pretolerance=123 one two three four five six seven eight nine ten");

    assert_eq!(assigned.output, baseline.output);
    assert_eq!(assigned.render_events, baseline.render_events);
    assert!(
        assigned.diagnostics.is_empty(),
        "{:#?}",
        assigned.diagnostics
    );
}

#[test]
fn activation_failures_are_not_undefined_control_sequence_diagnostics() {
    let outcome = run(r"\pretolerance=2147483648");
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.kind != VmDiagnosticKind::UndefinedControlSequence })
    );
}

#[test]
fn source_default_assignment_is_root_quiescent_but_local_default_is_materialized() {
    let mut root_interner = ControlSequenceInterner::new();
    let mut root = Vm::new(&mut root_interner);
    let outcome = root.run_plain(r"\pretolerance=0");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let root_snapshot = root.snapshot();
    assert!(root_snapshot.layout_integer_parameter_state.is_none());
    assert!(
        !root_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            })
    );

    let mut local_interner = ControlSequenceInterner::new();
    let mut local = Vm::new(&mut local_interner);
    let outcome = local.run_plain(r"{\pretolerance=0");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let local_snapshot = local.snapshot();
    assert_eq!(
        local_snapshot
            .layout_integer_parameter_state
            .as_ref()
            .expect("local default owner")
            .layers
            .len(),
        2
    );
    assert!(
        local_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            })
    );
    let outcome = local.run_plain(r"}[\the\pretolerance]");
    assert_eq!(outcome.output, "[0]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn every_serialized_pending_owner_derives_the_layout_capability() {
    let mut interner = ControlSequenceInterner::new();
    let base = Vm::new(&mut interner).snapshot();
    let parameter_token = SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "pretolerance".to_string(),
        },
        start_utf8: 0,
        end_utf8: 0,
    };
    let source_frame =
        |end_hooks: Vec<Vec<SnapshotToken>>,
         module_options: Option<VmActiveModuleOptionsSnapshot>| {
            VmActiveSourceFrameSnapshot {
                path: Utf8PathBuf::from("package.sty"),
                output_start_utf8: 0,
                execution_anchor: None,
                return_to_parent: None,
                global_definition_base_scope: None,
                module_kind: None,
                catcode_overrides: BTreeMap::new(),
                suppressed_catcode_overrides: BTreeMap::new(),
                end_hooks,
                module_options,
            }
        };
    let mut cases = Vec::new();

    let mut token_register = base.clone();
    token_register
        .token_registers
        .insert(17, vec![parameter_token.clone()]);
    cases.push(("token register", token_register));

    let mut aftergroup = base.clone();
    aftergroup.aftergroup_tokens = vec![vec![parameter_token.clone()]];
    cases.push(("aftergroup", aftergroup));

    let mut after_assignment = base.clone();
    after_assignment.after_assignment_token = Some(parameter_token.clone());
    cases.push(("after-assignment", after_assignment));

    let mut end_document = base.clone();
    end_document.at_end_document_hooks = vec![vec![parameter_token.clone()]];
    cases.push(("end-document hook", end_document));

    let mut continuation_queue = base.clone();
    continuation_queue.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::Token {
            token: parameter_token.clone(),
        }],
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    cases.push(("continuation queue", continuation_queue));

    let mut character_source = base.clone();
    character_source.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\pretolerance=200").snapshot(),
        }],
        source_stack: vec![source_frame(Vec::new(), None)],
        last_token_end_utf8: 0,
    });
    cases.push(("continuation character source", character_source));

    let mut source_end_hook = base.clone();
    source_end_hook.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(vec![vec![parameter_token.clone()]], None)],
        last_token_end_utf8: 0,
    });
    cases.push(("source end hook", source_end_hook));

    let mut declared_option = base.clone();
    declared_option.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(
            Vec::new(),
            Some(VmActiveModuleOptionsSnapshot {
                default_options: Vec::new(),
                passed_options: Vec::new(),
                forwarded_options: Vec::new(),
                declared_options: BTreeMap::from([(
                    "option".to_string(),
                    vec![parameter_token.clone()],
                )]),
                default_option_body: None,
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("declared option", declared_option));

    let mut default_option = base;
    default_option.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(
            Vec::new(),
            Some(VmActiveModuleOptionsSnapshot {
                default_options: Vec::new(),
                passed_options: Vec::new(),
                forwarded_options: Vec::new(),
                declared_options: BTreeMap::new(),
                default_option_body: Some(vec![parameter_token]),
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("default option", default_option));

    for (owner, snapshot) in cases {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            "{owner} omitted the layout integer capability"
        );
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
            }),
            "{owner} omitted the layout source-command capability"
        );
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }) == (owner == "continuation character source"),
            "{owner} classified raw-source ambiguity incorrectly"
        );
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &snapshot)
            .expect_err("pending layout parameter command must not enter the legacy wire shape");
        assert!(bytes.is_empty(), "{owner} wrote legacy bytes: {bytes:?}");
    }
}

#[test]
fn aliased_dynamic_builder_and_split_name_tokens_require_layout_before_execution() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\let\buildcs\csname");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut snapshot = vm.snapshot();
    let mut queue = vec![SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "buildcs".to_string(),
        },
        start_utf8: 0,
        end_utf8: 0,
    }];
    queue.extend("pretolerance".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: CatCode::Letter,
        },
        start_utf8: 0,
        end_utf8: 0,
    }));
    queue.push(SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "endcsname".to_string(),
        },
        start_utf8: 0,
        end_utf8: 0,
    });
    queue.extend("=456".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: CatCode::Other,
        },
        start_utf8: 0,
        end_utf8: 0,
    }));
    snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: queue
            .into_iter()
            .map(|token| VmQueueItemSnapshot::Token { token })
            .collect(),
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });

    assert!(snapshot.layout_integer_parameter_state.is_none());
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
    }));
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&snapshot).is_err());
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("encode dynamically constructed layout continuation");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore dynamically constructed layout continuation");
    let resumed = restored
        .resume_continuation()
        .expect("resume dynamically constructed layout continuation");
    assert!(resumed.diagnostics.is_empty(), "{:#?}", resumed.diagnostics);
    let queried = restored.run_plain(r"[\number\pretolerance]");
    assert_eq!(queried.output, "[456]");
    assert!(queried.diagnostics.is_empty(), "{:#?}", queried.diagnostics);
}

#[test]
fn raw_character_sources_fail_closed_across_mutable_catcode_boundaries() {
    for (setup, pending, parameter) in [
        (
            r"\catcode`x=10",
            r"\hangafterx=7",
            LayoutIntegerParameterId::HangAfter,
        ),
        (
            r"\def\pr{}\catcode`q=0",
            r"\prqhangafter=7",
            LayoutIntegerParameterId::HangAfter,
        ),
    ] {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        let outcome = vm.run_plain(setup);
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let mut snapshot = vm.snapshot();
        snapshot.input_continuation = Some(VmInputContinuationSnapshot {
            queue: vec![VmQueueItemSnapshot::CharacterSource {
                mouth: tex_lexer::Mouth::new(pending).snapshot(),
            }],
            source_stack: vec![VmActiveSourceFrameSnapshot {
                path: Utf8PathBuf::from("catcode-boundary.tex"),
                output_start_utf8: 0,
                execution_anchor: None,
                return_to_parent: None,
                global_definition_base_scope: None,
                module_kind: None,
                catcode_overrides: BTreeMap::new(),
                suppressed_catcode_overrides: BTreeMap::new(),
                end_hooks: Vec::new(),
                module_options: None,
            }],
            last_token_end_utf8: 0,
        });

        assert!(snapshot.required_capabilities().iter().any(|capability| {
            capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
        }));
        assert!(snapshot.required_capabilities().iter().any(|capability| {
            capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
        }));
        let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
            .expect("encode catcode-dependent pending source");
        drop(vm);

        let mut restored_interner = ControlSequenceInterner::new();
        let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
            .expect("restore catcode-dependent pending source");
        let resumed = restored
            .resume_continuation()
            .expect("resume catcode-dependent pending source");
        assert!(resumed.diagnostics.is_empty(), "{:#?}", resumed.diagnostics);
        let state = restored
            .snapshot()
            .layout_integer_parameter_state
            .expect("executed layout assignment");
        assert!(
            state.layers[0]
                .iter()
                .any(|assignment| assignment.parameter == parameter && assignment.value == 7)
        );
    }
}

#[test]
fn superscript_encoded_layout_spelling_remains_unreachable_through_current_mouth() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\catcode`\!=7");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let mut snapshot = vm.snapshot();
    assert_eq!(snapshot.catcodes.get(&'!'), Some(&CatCode::Superscript));
    snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\hang!!61fter=7").snapshot(),
        }],
        source_stack: vec![VmActiveSourceFrameSnapshot {
            path: Utf8PathBuf::from("superscript-encoding.tex"),
            output_start_utf8: 0,
            execution_anchor: None,
            return_to_parent: None,
            global_definition_base_scope: None,
            module_kind: None,
            catcode_overrides: BTreeMap::new(),
            suppressed_catcode_overrides: BTreeMap::new(),
            end_hooks: Vec::new(),
            module_options: None,
        }],
        last_token_end_utf8: 0,
    });

    assert!(!snapshot.required_capabilities().iter().any(|capability| {
        matches!(
            capability.as_str(),
            VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
                | VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
        )
    }));
    let bytes = serde_json::to_vec(&snapshot)
        .expect("the current mouth does not preprocess superscript encoding");
    drop(vm);

    let decoded: tex_vm::VmSnapshot =
        serde_json::from_slice(&bytes).expect("decode legacy-compatible snapshot");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &decoded)
        .expect("restore superscript-encoded source");
    let resumed = restored
        .resume_continuation()
        .expect("resume superscript-encoded source");
    assert!(
        resumed
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence })
    );
    assert!(restored.snapshot().layout_integer_parameter_state.is_none());
    let queried = restored.run_plain(r"[\number\hangafter]");
    assert_eq!(queried.output, "[1]");
    assert!(queried.diagnostics.is_empty(), "{:#?}", queried.diagnostics);
}

#[test]
fn source_layout_state_survives_restore_group_unwind_and_global_cancellation() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(r"\pretolerance=100{\pretolerance=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(source.snapshot()))
        .expect("encode source-created layout state");
    drop(source);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore source-created layout document");
    let outcome =
        restored.run_plain(r"[\the\pretolerance]\pretolerance=102}[\number\pretolerance]");
    assert_eq!(outcome.output, "[101][100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    drop(restored);
    let mut global_interner = ControlSequenceInterner::new();
    let mut global = Vm::try_restore_document(&mut global_interner, &document)
        .expect("restore layout document for global reassignment");
    let outcome = global.run_plain(r"\global\pretolerance=103}[\number\pretolerance]");
    assert_eq!(outcome.output, "[103]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn mixed_source_state_round_trip_preserves_local_defaults_and_family_cancellation() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(
        r"\tolerance=100\hangafter=2\pretolerance=10{\pretolerance=0\tolerance=101{\advance\hangafter by3",
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let source_snapshot = source.snapshot();
    assert_eq!(
        source_snapshot
            .layout_integer_parameter_state
            .as_ref()
            .expect("source-created layout owners")
            .layers
            .len(),
        3
    );
    assert_eq!(
        source_snapshot
            .integer_parameter_state
            .as_ref()
            .expect("source-created tolerance owners")
            .layers
            .len(),
        3
    );
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(source_snapshot.clone()))
        .expect("encode mixed source-created state");
    drop(source);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore mixed source-created state");
    let restored_snapshot = restored.snapshot();
    assert_eq!(
        restored_snapshot.layout_integer_parameter_state,
        source_snapshot.layout_integer_parameter_state
    );
    assert_eq!(
        restored_snapshot.integer_parameter_state,
        source_snapshot.integer_parameter_state
    );
    let outcome = restored.run_plain(
        r"[\the\hangafter,\the\pretolerance,\the\tolerance]\pretolerance=4}[\the\hangafter,\the\pretolerance,\the\tolerance]\global\pretolerance=9}[\the\hangafter,\the\pretolerance,\the\tolerance]",
    );
    assert_eq!(outcome.output, "[5,0,101][2,0,101][2,9,100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    drop(restored);
    let mut global_interner = ControlSequenceInterner::new();
    let mut global = Vm::try_restore_document(&mut global_interner, &document)
        .expect("restore for family-specific global cancellation");
    let outcome =
        global.run_plain(r"\global\hangafter=8}}[\the\hangafter,\the\pretolerance,\the\tolerance]");
    assert_eq!(outcome.output, "[8,10,100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn hidden_outer_scope_aliases_and_macros_keep_the_layout_capability() {
    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    let outcome = alias_vm.run_plain(r"\let\danger\pretolerance{\let\danger\relax");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let alias_document =
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(alias_vm.snapshot()))
            .expect("encode hidden layout alias");
    drop(alias_vm);
    let mut restored_alias_interner = ControlSequenceInterner::new();
    let mut restored_alias =
        Vm::try_restore_document(&mut restored_alias_interner, &alias_document)
            .expect("restore hidden layout alias");
    let outcome = restored_alias.run_plain(r"}\danger=444[\number\pretolerance]");
    assert_eq!(outcome.output, "[444]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut macro_interner = ControlSequenceInterner::new();
    let mut macro_vm = Vm::new(&mut macro_interner);
    let outcome = macro_vm.run_plain(r"\def\danger{\pretolerance=445}{\def\danger{}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let macro_document =
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(macro_vm.snapshot()))
            .expect("encode hidden layout macro");
    drop(macro_vm);
    let mut restored_macro_interner = ControlSequenceInterner::new();
    let mut restored_macro =
        Vm::try_restore_document(&mut restored_macro_interner, &macro_document)
            .expect("restore hidden layout macro");
    let outcome = restored_macro.run_plain(r"}\danger[\number\pretolerance]");
    assert_eq!(outcome.output, "[445]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn module_exit_checkpoint_is_not_captured_mid_layout_assignment_or_query() {
    let mut assignment_interner = ControlSequenceInterner::new();
    let mut assignment_vm = Vm::new(&mut assignment_interner);
    assignment_vm.set_entry_source_path("main.tex");
    assignment_vm.mount_file("split.tex", r"\pretolerance=");
    let assignment = assignment_vm.run_plain(r"\input{split}123[\number\pretolerance]");

    assert_eq!(assignment.output, "[123]");
    assert!(
        assignment.diagnostics.is_empty(),
        "{:#?}",
        assignment.diagnostics
    );
    assert!(!assignment.module_checkpoints.iter().any(|checkpoint| {
        checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
            && checkpoint.module_path == Utf8PathBuf::from("split.tex")
    }));

    let mut query_interner = ControlSequenceInterner::new();
    let mut query_vm = Vm::new(&mut query_interner);
    query_vm.set_entry_source_path("main.tex");
    query_vm.mount_file("split.tex", r"[\number");
    let query = query_vm.run_plain(r"\input{split}\pretolerance]");

    assert_eq!(query.output, "[0]");
    assert!(query.diagnostics.is_empty(), "{:#?}", query.diagnostics);
    assert!(!query.module_checkpoints.iter().any(|checkpoint| {
        checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
            && checkpoint.module_path == Utf8PathBuf::from("split.tex")
    }));
}
