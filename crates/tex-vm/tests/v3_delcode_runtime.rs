use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotMeaning, SnapshotToken, SnapshotTokenKind, VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY, Vm, VmActiveModuleOptionsSnapshot,
    VmActiveSourceFrameSnapshot, VmDiagnosticKind, VmInputContinuationSnapshot,
    VmQueueItemSnapshot, VmSnapshot, VmSnapshotDocument,
};

fn run(source: &str) -> tex_vm::VmOutcome {
    let mut interner = ControlSequenceInterner::new();
    Vm::new(&mut interner).run_plain(source)
}

#[test]
fn delcode_defaults_assignments_and_queries_match_the_v1_contract() {
    let outcome = run(
        r#"[\number\delcode`A][\the\delcode`.]\delcode65=123[\the\delcode`A]\delcode'101="1234[\number\delcode65]\delcode"42=-2147483647[\the\delcode66]\delcode67 16777215[\number\delcode67]"#,
    );

    assert_eq!(outcome.output, "[-1][0][123][4660][-2147483647][16777215]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn delcode_assignments_obey_group_global_and_globaldefs_scope() {
    let outcome = run(
        r"\delcode65=100{\delcode65=101[\the\delcode65]}[\the\delcode65]{\global\delcode65=102}[\the\delcode65]{\globaldefs=-1\global\delcode65=103}[\the\delcode65]{\globaldefs=1\delcode65=104}[\the\delcode65]",
    );

    assert_eq!(outcome.output, "[101][100][102][102][104]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn delcode_primitive_alias_has_a_stable_wire_name_and_survives_restore() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\let\code\delcode\code65=222[\the\code65]");

    assert_eq!(outcome.output, "[222]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot.scopes[0].get("code"),
        Some(&SnapshotMeaning::Primitive {
            name: "delcode".to_string(),
        })
    );
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    let document = VmSnapshotDocument::from_snapshot(snapshot);
    let wire = serde_json::to_value(&document).expect("encode delcode primitive alias");
    assert_eq!(wire["state"]["scopes"][0]["code"]["name"], "delcode");

    drop(vm);
    let bytes = serde_json::to_vec(&document).expect("encode delcode alias document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &bytes)
        .expect("restore delcode primitive alias");
    let outcome = restored.run_plain(r"\code66=223[\number\code66]");
    assert_eq!(outcome.output, "[223]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn explicit_redefinition_of_delcode_overrides_the_builtin_name() {
    let outcome = run(
        r"\let\saveddelcode\delcode\let\delcode\count\delcode0=17[\number\delcode0]\saveddelcode65=1234[\number\saveddelcode65]",
    );

    assert_eq!(outcome.output, "[17][1234]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn delcode_invalid_operands_recover_to_zero_and_report_diagnostics() {
    let outcome = run(
        r"\delcode256=123\delcode65=-2147483648[\the\delcode65]\delcode65=16777216[\the\delcode0][\the\delcode65]",
    );

    assert_eq!(outcome.output, "[0][123][0]");
    assert_eq!(outcome.diagnostics.len(), 3);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.kind == VmDiagnosticKind::ExplicitError)
    );
    assert!(outcome.diagnostics[0].detail.contains("character code"));
    assert!(outcome.diagnostics[1].detail.contains("delcode value"));
    assert!(outcome.diagnostics[2].detail.contains("delcode value"));
}

#[test]
fn invalid_delcode_recovery_consumes_input_and_completes_afterassignment_once() {
    let outcome = run(
        r"\def\mark{M}\delcode65=9\afterassignment\mark\delcode65=16777216X[\number\delcode65]\afterassignment\mark\delcode256=7Y[\number\delcode0]",
    );

    assert_eq!(outcome.output, "MX[0]MY[7]");
    assert_eq!(outcome.diagnostics.len(), 2);
    assert!(outcome.transcript.iter().any(|line| line == "delcode65=0"));
    assert!(outcome.transcript.iter().any(|line| line == "delcode0=7"));
}

#[test]
fn invalid_delcode_query_reports_an_error_without_materializing_state() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"[\number\delcode256]");

    assert_eq!(outcome.output, "[-1]");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].kind, VmDiagnosticKind::ExplicitError);
    assert!(outcome.diagnostics[0].detail.contains("character code"));
    assert!(vm.snapshot().delcode_state.is_none());
}

#[test]
fn source_delcode_state_and_latent_tokens_require_the_capability() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\delcode65=100{\delcode65=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot
            .delcode_state
            .as_ref()
            .expect("source-created delcode state")
            .layers
            .len(),
        2
    );
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    assert!(serde_json::to_vec(&snapshot).is_err());

    let mut latent_interner = ControlSequenceInterner::new();
    let mut latent_vm = Vm::new(&mut latent_interner);
    let outcome = latent_vm.run_plain(r"\def\deferred{\delcode65=200}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let latent = latent_vm.snapshot();
    assert!(latent.delcode_state.is_none());
    assert!(
        latent
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    assert!(serde_json::to_vec(&latent).is_err());
}

#[test]
fn every_serialized_pending_owner_derives_the_delcode_capability() {
    let mut interner = ControlSequenceInterner::new();
    let base = Vm::new(&mut interner).snapshot();
    let delcode_token = SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "delcode".to_string(),
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
        .insert(17, vec![delcode_token.clone()]);
    cases.push(("token register", token_register));

    let mut aftergroup = base.clone();
    aftergroup.aftergroup_tokens = vec![vec![delcode_token.clone()]];
    cases.push(("aftergroup", aftergroup));

    let mut after_assignment = base.clone();
    after_assignment.after_assignment_token = Some(delcode_token.clone());
    cases.push(("after-assignment", after_assignment));

    let mut end_document = base.clone();
    end_document.at_end_document_hooks = vec![vec![delcode_token.clone()]];
    cases.push(("end-document hook", end_document));

    let mut continuation_queue = base.clone();
    continuation_queue.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::Token {
            token: delcode_token.clone(),
        }],
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    cases.push(("continuation queue", continuation_queue));

    let mut character_source = base.clone();
    character_source.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\delcode65=200").snapshot(),
        }],
        source_stack: vec![source_frame(Vec::new(), None)],
        last_token_end_utf8: 0,
    });
    cases.push(("continuation character source", character_source));

    let mut source_end_hook = base.clone();
    source_end_hook.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(vec![vec![delcode_token.clone()]], None)],
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
                    vec![delcode_token.clone()],
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
                default_option_body: Some(vec![delcode_token]),
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("default option", default_option));

    for (owner, snapshot) in cases {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY
            }),
            "{owner} omitted the delcode capability"
        );
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &snapshot)
            .expect_err("pending delcode command must not enter the legacy wire shape");
        assert!(bytes.is_empty(), "{owner} wrote legacy bytes: {bytes:?}");
    }
}

#[test]
fn aliased_dynamic_builder_and_split_name_tokens_require_delcode_before_execution() {
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
    queue.extend("delcode".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: tex_tokens::CatCode::Letter,
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
    queue.extend("65=456".chars().map(|ch| SnapshotToken {
        kind: SnapshotTokenKind::Character {
            ch,
            catcode: tex_tokens::CatCode::Other,
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

    assert!(snapshot.delcode_state.is_none());
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY })
    );
    assert!(serde_json::to_vec(&snapshot).is_err());
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("encode dynamically constructed delcode continuation");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore dynamically constructed delcode continuation");
    let resumed = restored
        .resume_continuation()
        .expect("resume dynamically constructed delcode continuation");
    assert!(resumed.diagnostics.is_empty(), "{:#?}", resumed.diagnostics);
    let queried = restored.run_plain(r"[\number\delcode65]");
    assert_eq!(queried.output, "[456]");
    assert!(queried.diagnostics.is_empty(), "{:#?}", queried.diagnostics);
}

#[test]
fn module_exit_checkpoint_is_not_captured_mid_delcode_assignment_or_query() {
    let mut assignment_interner = ControlSequenceInterner::new();
    let mut assignment_vm = Vm::new(&mut assignment_interner);
    assignment_vm.set_entry_source_path("main.tex");
    assignment_vm.mount_file("split.tex", r"\delcode65=");
    let assignment = assignment_vm.run_plain(r"\input{split}123[\number\delcode65]");

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
        "an exit snapshot cannot serialize the active delcode assignment"
    );

    let mut query_interner = ControlSequenceInterner::new();
    let mut query_vm = Vm::new(&mut query_interner);
    query_vm.set_entry_source_path("main.tex");
    query_vm.mount_file("split.tex", r"[\number\delcode");
    let query = query_vm.run_plain(r"\input{split}65]");

    assert_eq!(query.output, "[-1]");
    assert!(query.diagnostics.is_empty(), "{:#?}", query.diagnostics);
    assert!(
        !query.module_checkpoints.iter().any(|checkpoint| {
            checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                && checkpoint.module_path == Utf8PathBuf::from("split.tex")
        }),
        "an exit snapshot cannot serialize the active delcode query"
    );
}

#[test]
fn encoded_delcode_spelling_is_not_reachable_through_the_current_mouth() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\del^^63ode65=200").snapshot(),
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

    assert!(snapshot.required_capabilities().is_empty());
    let legacy = serde_json::to_vec(&snapshot)
        .expect("the current mouth cannot turn the encoded spelling into delcode");
    let decoded: VmSnapshot = serde_json::from_slice(&legacy).expect("decode legacy snapshot");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored =
        Vm::try_restore(&mut restored_interner, &decoded).expect("restore encoded source");
    let outcome = restored
        .resume_continuation()
        .expect("resume encoded character source");

    assert!(outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == VmDiagnosticKind::UndefinedControlSequence
            && diagnostic.detail.contains("del")
    }));
    assert!(restored.snapshot().delcode_state.is_none());
}

#[test]
fn source_delcode_state_survives_snapshot_restore_and_group_unwind() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(r"\delcode65=100{\delcode65=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(source.snapshot()))
        .expect("encode source-created delcode state");
    drop(source);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore source-created delcode document");
    let outcome = restored.run_plain(r"[\the\delcode65]\delcode65=102}[\number\delcode65]");
    assert_eq!(outcome.output, "[101][100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    drop(restored);
    let mut global_interner = ControlSequenceInterner::new();
    let mut global = Vm::try_restore_document(&mut global_interner, &document)
        .expect("restore delcode document for global reassignment");
    let outcome = global.run_plain(r"\global\delcode65=103}[\number\delcode65]");
    assert_eq!(outcome.output, "[103]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn hidden_outer_scope_aliases_and_macros_keep_the_delcode_capability() {
    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    let outcome = alias_vm.run_plain(r"\let\danger\delcode{\let\danger\relax");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let alias_snapshot = alias_vm.snapshot();
    assert!(
        alias_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    let alias_document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(alias_snapshot))
        .expect("encode hidden delcode alias");
    drop(alias_vm);
    let mut restored_alias_interner = ControlSequenceInterner::new();
    let mut restored_alias =
        Vm::try_restore_document(&mut restored_alias_interner, &alias_document)
            .expect("restore hidden delcode alias");
    let outcome = restored_alias.run_plain(r"}\danger65=444[\number\danger65]");
    assert_eq!(outcome.output, "[444]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut macro_interner = ControlSequenceInterner::new();
    let mut macro_vm = Vm::new(&mut macro_interner);
    let outcome = macro_vm.run_plain(r"\def\danger{\delcode65=445}{\def\danger{}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let macro_snapshot = macro_vm.snapshot();
    assert!(
        macro_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
    let macro_document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(macro_snapshot))
        .expect("encode hidden delcode macro");
    drop(macro_vm);
    let mut restored_macro_interner = ControlSequenceInterner::new();
    let mut restored_macro =
        Vm::try_restore_document(&mut restored_macro_interner, &macro_document)
            .expect("restore hidden delcode macro");
    let outcome = restored_macro.run_plain(r"}\danger[\number\delcode65]");
    assert_eq!(outcome.output, "[445]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn delcode_assignment_does_not_activate_rendering_or_mathcode() {
    let baseline = run("$A$");
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let assigned = vm.run_plain(r"\delcode65=123$A$");

    assert_eq!(assigned.output, baseline.output);
    assert!(
        assigned.diagnostics.is_empty(),
        "{:#?}",
        assigned.diagnostics
    );
    let snapshot = vm.snapshot();
    assert!(snapshot.delcode_state.is_some());
    assert!(snapshot.mathcode_state.is_none());
    assert!(
        !snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY })
    );
}
