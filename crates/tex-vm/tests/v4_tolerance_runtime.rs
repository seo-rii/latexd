use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use tex_tokens::{CatCode, ControlSequenceInterner};
use tex_vm::{
    SnapshotMeaning, SnapshotToken, SnapshotTokenKind, VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY, Vm, VmActiveModuleOptionsSnapshot,
    VmActiveSourceFrameSnapshot, VmDiagnosticKind, VmInputContinuationSnapshot,
    VmQueueItemSnapshot, VmSnapshot, VmSnapshotDocument,
};

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    Vm::new(&mut interner).run_plain(source)
}

#[test]
fn tolerance_defaults_assignments_queries_and_numeric_forms_match_tex82() {
    let outcome = run(
        r#"[\the\tolerance]\tolerance '123[\number\tolerance]\tolerance="1234[\the\tolerance]\tolerance=`A[\number\tolerance]\tolerance --+17[\the\tolerance]"#,
    );

    assert_eq!(outcome.output, "[10000][83][4660][65][17]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn tolerance_obeys_group_global_globaldefs_and_afterassignment_scope() {
    let outcome = run(
        r"\tolerance=100{\tolerance=101[\the\tolerance]}[\the\tolerance]{\global\tolerance=102}[\the\tolerance]{\globaldefs=-1\global\tolerance=103}[\the\tolerance]{\globaldefs=1\tolerance=104}[\the\tolerance]\def\mark{M}\afterassignment\mark\tolerance=105[\the\tolerance]",
    );

    assert_eq!(outcome.output, "[101][100][102][102][104]M[105]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn source_default_assignment_is_root_quiescent_but_local_default_is_materialized() {
    let mut root_interner = ControlSequenceInterner::new();
    let mut root = Vm::new(&mut root_interner);
    let outcome = root.run_plain(r"\tolerance=10000");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let root_snapshot = root.snapshot();
    assert!(root_snapshot.integer_parameter_state.is_none());
    assert!(
        !root_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            })
    );

    let mut local_interner = ControlSequenceInterner::new();
    let mut local = Vm::new(&mut local_interner);
    let outcome = local.run_plain(r"{\tolerance=10000");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let local_snapshot = local.snapshot();
    assert_eq!(
        local_snapshot
            .integer_parameter_state
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
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            })
    );
    let outcome = local.run_plain(r"}[\the\tolerance]");
    assert_eq!(outcome.output, "[10000]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn tolerance_is_an_integer_expression_and_arithmetic_lvalue() {
    let outcome = run(
        r"\tolerance=12\advance\tolerance by 5[\the\tolerance]\multiply\tolerance by -3[\number\tolerance]\divide\tolerance by 2[\the\tolerance]\ifnum\tolerance=-25T\else F\fi\tolerance=2147483647\advance\tolerance by1[\number\tolerance]\ifnum\tolerance<0T\else F\fi",
    );

    assert_eq!(outcome.output, "[17][-51][-25]T[-2147483648]T");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn tolerance_rejects_bad_numbers_and_arithmetic_without_losing_state_or_hooks() {
    let outcome = run(
        r"\def\mark{M}\afterassignment\mark\tolerance=2147483648[\the\tolerance]\tolerance=-2147483648[\the\tolerance]\afterassignment\mark\tolerance=\relax[\the\tolerance]\tolerance=1073741824\afterassignment\mark\multiply\tolerance by2[\the\tolerance]\tolerance=17\afterassignment\mark\divide\tolerance by0[\the\tolerance]\tolerance=2147483647\advance\tolerance by1\afterassignment\mark\divide\tolerance by-1[\the\tolerance]",
    );

    assert_eq!(
        outcome.output,
        "M[2147483647][-2147483647]M[0]M[1073741824]M[17]M[-2147483648]"
    );
    assert_eq!(outcome.diagnostics.len(), 6, "{:#?}", outcome.diagnostics);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.kind == VmDiagnosticKind::ExplicitError)
    );
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
fn tolerance_signs_internal_i32_min_without_panicking_or_losing_token_order() {
    let outcome = run(
        r"\def\mark{M}\let\savedtolerance\tolerance\tolerance=-1073741824\multiply\tolerance by2\afterassignment\mark\tolerance=-\savedtolerance X[\the\tolerance]\tolerance=-1073741824\multiply\tolerance by2\afterassignment\mark\tolerance=--\savedtolerance Y[\the\tolerance]\tolerance=-1073741824\multiply\tolerance by2\afterassignment\mark\advance\tolerance by-\savedtolerance Z[\the\tolerance]",
    );

    assert_eq!(outcome.output, "MX[2147483647]MY[-2147483648]MZ[-1]");
    assert_eq!(outcome.diagnostics.len(), 2, "{:#?}", outcome.diagnostics);
    assert!(outcome.diagnostics.iter().all(|diagnostic| {
        diagnostic.kind == VmDiagnosticKind::ExplicitError
            && diagnostic.detail.contains("Number too big")
    }));
}

#[test]
fn failed_tolerance_arithmetic_preserves_sparse_owner_and_unwind_state() {
    let mut virtual_interner = ControlSequenceInterner::new();
    let mut virtual_default = Vm::new(&mut virtual_interner);
    let before = virtual_default.snapshot();
    let outcome = virtual_default
        .run_plain(r"\def\mark{M}\afterassignment\mark\multiply\tolerance by2147483647");
    assert_eq!(outcome.output, "M");
    assert_eq!(outcome.diagnostics.len(), 1);
    let after = virtual_default.snapshot();
    assert_eq!(
        after.integer_parameter_state,
        before.integer_parameter_state
    );
    assert_eq!(
        after.required_capabilities(),
        before.required_capabilities()
    );

    let mut local_interner = ControlSequenceInterner::new();
    let mut local_default = Vm::new(&mut local_interner);
    let outcome = local_default.run_plain(r"{\tolerance=10000");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let before = local_default.snapshot();
    let outcome =
        local_default.run_plain(r"\def\mark{M}\afterassignment\mark\divide\tolerance by0");
    assert_eq!(outcome.output, "M");
    assert_eq!(outcome.diagnostics.len(), 1);
    let after = local_default.snapshot();
    assert_eq!(
        after.integer_parameter_state,
        before.integer_parameter_state
    );
    assert_eq!(
        after.required_capabilities(),
        before.required_capabilities()
    );
    let outcome = local_default.run_plain(r"}[\the\tolerance]");
    assert_eq!(outcome.output, "[10000]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn tolerance_alias_has_a_stable_wire_name_and_survives_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome =
        vm.run_plain(r"\let\savedtolerance\tolerance\savedtolerance=222[\the\savedtolerance]");

    assert_eq!(outcome.output, "[222]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot.scopes[0].get("savedtolerance"),
        Some(&SnapshotMeaning::Primitive {
            name: "tolerance".to_string(),
        })
    );
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    let document = VmSnapshotDocument::from_snapshot(snapshot);
    let wire = serde_json::to_value(&document).expect("encode tolerance primitive alias");
    assert_eq!(
        wire["state"]["scopes"][0]["savedtolerance"]["name"],
        "tolerance"
    );

    drop(vm);
    let bytes = serde_json::to_vec(&document).expect("encode tolerance alias document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &bytes)
        .expect("restore tolerance primitive alias");
    let outcome = restored.run_plain(r"\savedtolerance=223[\number\tolerance]");
    assert_eq!(outcome.output, "[223]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn explicit_redefinition_of_tolerance_overrides_the_builtin_name() {
    let outcome = run(
        r"\let\savedtolerance\tolerance{\def\tolerance{46}[\tolerance]}[\the\tolerance]\savedtolerance=45[\number\savedtolerance]",
    );

    assert_eq!(outcome.output, "[46][10000][45]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn tolerance_builtin_is_visible_to_existence_conditionals() {
    let outcome = run(
        r"\ifcsname tolerance\endcsname T\else F\fi\ifdefined\tolerance T\else F\fi{\def\tolerance{shadow}\ifcsname tolerance\endcsname T\else F\fi\ifdefined\tolerance T\else F\fi}\ifdefined\tolerance T\else F\fi",
    );

    assert_eq!(outcome.output, "TTTTT");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn source_tolerance_state_and_latent_tokens_require_the_capability() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\tolerance=100{\tolerance=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot
            .integer_parameter_state
            .as_ref()
            .expect("source-created tolerance state")
            .layers
            .len(),
        2
    );
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&snapshot).is_err());

    let mut latent_interner = ControlSequenceInterner::new();
    let mut latent_vm = Vm::new(&mut latent_interner);
    let outcome = latent_vm.run_plain(r"\def\deferred{\tolerance=200}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let latent = latent_vm.snapshot();
    assert!(latent.integer_parameter_state.is_none());
    assert!(latent.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&latent).is_err());
}

#[test]
fn every_serialized_pending_owner_derives_the_tolerance_capability() {
    let mut interner = ControlSequenceInterner::new();
    let base = Vm::new(&mut interner).snapshot();
    let tolerance_token = SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "tolerance".to_string(),
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
        .insert(17, vec![tolerance_token.clone()]);
    cases.push(("token register", token_register));

    let mut aftergroup = base.clone();
    aftergroup.aftergroup_tokens = vec![vec![tolerance_token.clone()]];
    cases.push(("aftergroup", aftergroup));

    let mut after_assignment = base.clone();
    after_assignment.after_assignment_token = Some(tolerance_token.clone());
    cases.push(("after-assignment", after_assignment));

    let mut end_document = base.clone();
    end_document.at_end_document_hooks = vec![vec![tolerance_token.clone()]];
    cases.push(("end-document hook", end_document));

    let mut continuation_queue = base.clone();
    continuation_queue.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::Token {
            token: tolerance_token.clone(),
        }],
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    cases.push(("continuation queue", continuation_queue));

    let mut character_source = base.clone();
    character_source.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\tolerance=200").snapshot(),
        }],
        source_stack: vec![source_frame(Vec::new(), None)],
        last_token_end_utf8: 0,
    });
    cases.push(("continuation character source", character_source));

    let mut source_end_hook = base.clone();
    source_end_hook.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(vec![vec![tolerance_token.clone()]], None)],
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
                    vec![tolerance_token.clone()],
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
                default_option_body: Some(vec![tolerance_token]),
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("default option", default_option));

    for (owner, snapshot) in cases {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            "{owner} omitted the tolerance capability"
        );
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &snapshot)
            .expect_err("pending tolerance command must not enter the legacy wire shape");
        assert!(bytes.is_empty(), "{owner} wrote legacy bytes: {bytes:?}");
    }
}

#[test]
fn aliased_dynamic_builder_and_split_name_tokens_require_tolerance_before_execution() {
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
    queue.extend("tolerance".chars().map(|ch| SnapshotToken {
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

    assert!(snapshot.integer_parameter_state.is_none());
    assert!(snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    assert!(serde_json::to_vec(&snapshot).is_err());
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("encode dynamically constructed tolerance continuation");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore dynamically constructed tolerance continuation");
    let resumed = restored
        .resume_continuation()
        .expect("resume dynamically constructed tolerance continuation");
    assert!(resumed.diagnostics.is_empty(), "{:#?}", resumed.diagnostics);
    let queried = restored.run_plain(r"[\number\tolerance]");
    assert_eq!(queried.output, "[456]");
    assert!(queried.diagnostics.is_empty(), "{:#?}", queried.diagnostics);
}

#[test]
fn caret_encoded_spellings_remain_unreachable_through_the_current_mouth() {
    for (source, requires_capability) in [
        (r"\^^74olerance=123", false),
        (r"\csna^^6de toler^^61nce\endcsname=123", true),
    ] {
        let mut interner = ControlSequenceInterner::new();
        let vm = Vm::new(&mut interner);
        let mut snapshot = vm.snapshot();
        snapshot.input_continuation = Some(VmInputContinuationSnapshot {
            queue: vec![VmQueueItemSnapshot::CharacterSource {
                mouth: tex_lexer::Mouth::new(source).snapshot(),
            }],
            source_stack: vec![VmActiveSourceFrameSnapshot {
                path: Utf8PathBuf::from("encoded.tex"),
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

        assert_eq!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
            }),
            requires_capability
        );
        let encoded = if requires_capability {
            serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
                .expect("encode conservatively classified dynamic source")
        } else {
            serde_json::to_vec(&snapshot)
                .expect("the current mouth does not preprocess direct caret encoding")
        };
        drop(vm);

        let mut restored_interner = ControlSequenceInterner::new();
        let mut restored = if requires_capability {
            Vm::try_restore_document(&mut restored_interner, &encoded)
                .expect("restore conservatively classified dynamic source")
        } else {
            let decoded: VmSnapshot =
                serde_json::from_slice(&encoded).expect("decode legacy snapshot");
            Vm::try_restore(&mut restored_interner, &decoded).expect("restore encoded source")
        };
        let outcome = restored
            .resume_continuation()
            .expect("resume encoded source");
        assert!(
            outcome.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
            })
        );
        assert!(restored.snapshot().integer_parameter_state.is_none());
    }
}

#[test]
fn standalone_vm_documents_keep_the_legacy_delcode_scanner_contract() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\delcode65=-2147483648[\the\delcode65]").snapshot(),
        }],
        source_stack: vec![VmActiveSourceFrameSnapshot {
            path: Utf8PathBuf::from("legacy-delcode.tex"),
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
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    assert!(!snapshot.required_capabilities().iter().any(|capability| {
        capability.as_str() == VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY
    }));
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("encode pending legacy delcode source");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore pending legacy delcode source");
    let outcome = restored
        .resume_continuation()
        .expect("resume pending legacy delcode source");
    assert_eq!(outcome.output, "[0]");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert!(outcome.diagnostics[0].detail.contains("delcode value"));
}

#[test]
fn source_tolerance_state_survives_restore_and_group_unwind() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(r"\tolerance=100{\tolerance=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(source.snapshot()))
        .expect("encode source-created tolerance state");
    drop(source);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore source-created tolerance document");
    let outcome = restored.run_plain(r"[\the\tolerance]\tolerance=102}[\number\tolerance]");
    assert_eq!(outcome.output, "[101][100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    drop(restored);
    let mut global_interner = ControlSequenceInterner::new();
    let mut global = Vm::try_restore_document(&mut global_interner, &document)
        .expect("restore tolerance document for global reassignment");
    let outcome = global.run_plain(r"\global\tolerance=103}[\number\tolerance]");
    assert_eq!(outcome.output, "[103]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn hidden_outer_scope_aliases_and_macros_keep_the_tolerance_capability() {
    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    let outcome = alias_vm.run_plain(r"\let\danger\tolerance{\let\danger\relax");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let alias_document =
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(alias_vm.snapshot()))
            .expect("encode hidden tolerance alias");
    drop(alias_vm);
    let mut restored_alias_interner = ControlSequenceInterner::new();
    let mut restored_alias =
        Vm::try_restore_document(&mut restored_alias_interner, &alias_document)
            .expect("restore hidden tolerance alias");
    let outcome = restored_alias.run_plain(r"}\danger=444[\number\tolerance]");
    assert_eq!(outcome.output, "[444]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut macro_interner = ControlSequenceInterner::new();
    let mut macro_vm = Vm::new(&mut macro_interner);
    let outcome = macro_vm.run_plain(r"\def\danger{\tolerance=445}{\def\danger{}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let macro_document =
        serde_json::to_vec(&VmSnapshotDocument::from_snapshot(macro_vm.snapshot()))
            .expect("encode hidden tolerance macro");
    drop(macro_vm);
    let mut restored_macro_interner = ControlSequenceInterner::new();
    let mut restored_macro =
        Vm::try_restore_document(&mut restored_macro_interner, &macro_document)
            .expect("restore hidden tolerance macro");
    let outcome = restored_macro.run_plain(r"}\danger[\number\tolerance]");
    assert_eq!(outcome.output, "[445]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn module_exit_checkpoint_is_not_captured_mid_tolerance_assignment_or_query() {
    let mut assignment_interner = ControlSequenceInterner::new();
    let mut assignment_vm = Vm::new(&mut assignment_interner);
    assignment_vm.set_entry_source_path("main.tex");
    assignment_vm.mount_file("split.tex", r"\tolerance=");
    let assignment = assignment_vm.run_plain(r"\input{split}123[\number\tolerance]");

    assert_eq!(assignment.output, "[123]");
    assert!(
        assignment.diagnostics.is_empty(),
        "{:#?}",
        assignment.diagnostics
    );
    assert!(
        !assignment.module_checkpoints.iter().any(|checkpoint| {
            checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                && checkpoint.module_path == Utf8PathBuf::from("split.tex")
        }),
        "an exit snapshot cannot serialize the active tolerance assignment"
    );

    let mut query_interner = ControlSequenceInterner::new();
    let mut query_vm = Vm::new(&mut query_interner);
    query_vm.set_entry_source_path("main.tex");
    query_vm.mount_file("split.tex", r"[\number");
    let query = query_vm.run_plain(r"\input{split}\tolerance]");

    assert_eq!(query.output, "[10000]");
    assert!(query.diagnostics.is_empty(), "{:#?}", query.diagnostics);
    assert!(
        !query.module_checkpoints.iter().any(|checkpoint| {
            checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                && checkpoint.module_path == Utf8PathBuf::from("split.tex")
        }),
        "an exit snapshot cannot serialize the active tolerance query"
    );
}

#[test]
fn tolerance_assignment_does_not_activate_paragraph_line_breaking() {
    let baseline = run("one two three four five six seven eight nine ten");
    let assigned = run(r"\tolerance=0 one two three four five six seven eight nine ten");

    assert_eq!(assigned.output, baseline.output);
    assert_eq!(assigned.render_events, baseline.render_events);
    assert!(
        assigned.diagnostics.is_empty(),
        "{:#?}",
        assigned.diagnostics
    );
}
