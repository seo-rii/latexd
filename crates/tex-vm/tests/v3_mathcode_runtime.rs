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
fn mathcode_defaults_assignments_and_queries_match_the_v1_contract() {
    let outcome = run(
        r#"[\number\mathcode`A][\the\mathcode`0][\number\mathcode`+]\mathcode65=123[\the\mathcode`A]\mathcode'101="7FFF[\number\mathcode65]\mathcode"42=32768[\the\mathcode66]\mathcode67 321[\number\mathcode67]"#,
    );

    assert_eq!(outcome.output, "[28993][28720][43][123][32767][32768][321]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn mathcode_assignments_obey_group_global_and_globaldefs_scope() {
    let outcome = run(
        r"\mathcode65=100{\mathcode65=101[\the\mathcode65]}[\the\mathcode65]{\global\mathcode65=102}[\the\mathcode65]{\globaldefs=-1\global\mathcode65=103}[\the\mathcode65]{\globaldefs=1\mathcode65=104}[\the\mathcode65]",
    );

    assert_eq!(outcome.output, "[101][100][102][102][104]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn mathcode_primitive_alias_supports_assignment_and_internal_query() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\let\code\mathcode\code65=222[\the\code65]");

    assert_eq!(outcome.output, "[222]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot.scopes[0].get("code"),
        Some(&SnapshotMeaning::Primitive {
            name: "mathcode".to_string(),
        })
    );
    let document = VmSnapshotDocument::from_snapshot(snapshot.clone());
    let wire = serde_json::to_value(&document).expect("encode mathcode primitive alias");
    assert_eq!(wire["state"]["scopes"][0]["code"]["name"], "mathcode");
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY })
    );

    drop(vm);
    let bytes = serde_json::to_vec(&document).expect("encode mathcode alias document");
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &bytes)
        .expect("restore mathcode primitive alias");
    let outcome = restored.run_plain(r"\code66=223[\number\code66]");
    assert_eq!(outcome.output, "[223]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn radix_number_syntax_remains_available_to_existing_numeric_primitives() {
    let outcome = run(
        r#"\count0='10\count1="FF[\number\count0][\the\count1]\count2=-'10[\number\count2]\count3=+"aF[\number\count3]\count4='18[\number\count4]\mathcode65=123\count5=\mathcode65[\number\count5]\ifnum\mathcode65=123[T]\else[F]\fi"#,
    );

    assert_eq!(outcome.output, "[8][255][-8][175]8[1][123][T]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn explicit_redefinition_of_mathcode_overrides_the_builtin_name() {
    let outcome = run(
        r"\let\savedmathcode\mathcode\let\mathcode\count\mathcode0=17[\number\mathcode0]\savedmathcode65=1234[\number\savedmathcode65]",
    );

    assert_eq!(outcome.output, "[17][1234]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn mathcode_invalid_operands_recover_to_zero_and_report_diagnostics() {
    let outcome = run(r"\mathcode256=123\mathcode65=32769[\the\mathcode0][\the\mathcode65]");

    assert_eq!(outcome.output, "[123][0]");
    assert_eq!(outcome.diagnostics.len(), 2);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.kind == VmDiagnosticKind::ExplicitError)
    );
    assert!(outcome.diagnostics[0].detail.contains("character code"));
    assert!(outcome.diagnostics[1].detail.contains("mathcode value"));
}

#[test]
fn invalid_mathcode_query_reports_an_error_without_materializing_state() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"[\number\mathcode256]");

    assert_eq!(outcome.output, "[0]");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].kind, VmDiagnosticKind::ExplicitError);
    assert!(outcome.diagnostics[0].detail.contains("character code"));
    assert!(vm.snapshot().mathcode_state.is_none());
}

#[test]
fn source_mathcode_state_and_latent_tokens_require_the_capability() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\mathcode65=100{\mathcode65=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(
        snapshot
            .mathcode_state
            .as_ref()
            .expect("source-created mathcode state")
            .layers
            .len(),
        2
    );
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY })
    );
    assert!(serde_json::to_vec(&snapshot).is_err());

    let mut latent_interner = ControlSequenceInterner::new();
    let mut latent_vm = Vm::new(&mut latent_interner);
    let outcome = latent_vm.run_plain(r"\def\deferred{\mathcode65=200}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let latent = latent_vm.snapshot();
    assert!(latent.mathcode_state.is_none());
    assert!(
        latent
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY })
    );
    assert!(serde_json::to_vec(&latent).is_err());
}

#[test]
fn every_serialized_pending_owner_derives_the_mathcode_capability() {
    let mut interner = ControlSequenceInterner::new();
    let base = Vm::new(&mut interner).snapshot();
    let mathcode_token = SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "mathcode".to_string(),
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
        .insert(17, vec![mathcode_token.clone()]);
    cases.push(("token register", token_register));

    let mut aftergroup = base.clone();
    aftergroup.aftergroup_tokens = vec![vec![mathcode_token.clone()]];
    cases.push(("aftergroup", aftergroup));

    let mut after_assignment = base.clone();
    after_assignment.after_assignment_token = Some(mathcode_token.clone());
    cases.push(("after-assignment", after_assignment));

    let mut end_document = base.clone();
    end_document.at_end_document_hooks = vec![vec![mathcode_token.clone()]];
    cases.push(("end-document hook", end_document));

    let mut continuation_queue = base.clone();
    continuation_queue.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::Token {
            token: mathcode_token.clone(),
        }],
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    cases.push(("continuation queue", continuation_queue));

    let mut character_source = base.clone();
    character_source.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\mathcode65=200").snapshot(),
        }],
        source_stack: vec![source_frame(Vec::new(), None)],
        last_token_end_utf8: 0,
    });
    cases.push(("continuation character source", character_source));

    let mut source_end_hook = base.clone();
    source_end_hook.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(vec![vec![mathcode_token.clone()]], None)],
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
                    vec![mathcode_token.clone()],
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
                default_option_body: Some(vec![mathcode_token]),
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("default option", default_option));

    for (owner, snapshot) in cases {
        assert!(
            snapshot.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY
            }),
            "{owner} omitted the mathcode capability"
        );
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &snapshot)
            .expect_err("pending mathcode command must not enter the legacy wire shape");
        assert!(bytes.is_empty(), "{owner} wrote legacy bytes: {bytes:?}");
    }
}

#[test]
fn encoded_mathcode_spelling_is_not_reachable_through_the_current_mouth() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::CharacterSource {
            mouth: tex_lexer::Mouth::new(r"\math^^63ode65=200").snapshot(),
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
        .expect("the current mouth cannot turn the encoded spelling into mathcode");
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
            && diagnostic.detail.contains("math")
    }));
    assert!(restored.snapshot().mathcode_state.is_none());
}

#[test]
fn source_mathcode_state_survives_snapshot_restore_and_group_unwind() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut source = Vm::new(&mut source_interner);
    let outcome = source.run_plain(r"\mathcode65=100{\mathcode65=101");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = source.snapshot();
    let document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(snapshot))
        .expect("encode source-created mathcode state");
    drop(source);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore_document(&mut restored_interner, &document)
        .expect("restore source-created mathcode document");
    let outcome = restored.run_plain(r"[\the\mathcode65]\mathcode65=102}[\number\mathcode65]");

    assert_eq!(outcome.output, "[101][100]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    drop(restored);
    let mut global_interner = ControlSequenceInterner::new();
    let mut global = Vm::try_restore_document(&mut global_interner, &document)
        .expect("restore mathcode document for global reassignment");
    let outcome = global.run_plain(r"\global\mathcode65=103}[\number\mathcode65]");

    assert_eq!(outcome.output, "[103]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn hidden_outer_scope_aliases_and_macros_keep_the_mathcode_capability() {
    let mut alias_interner = ControlSequenceInterner::new();
    let mut alias_vm = Vm::new(&mut alias_interner);
    let outcome = alias_vm.run_plain(r"\let\danger\mathcode{\let\danger\relax");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let alias_snapshot = alias_vm.snapshot();
    assert!(
        alias_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY)
    );
    assert!(serde_json::to_vec(&alias_snapshot).is_err());
    let alias_document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(alias_snapshot))
        .expect("encode hidden mathcode alias");
    drop(alias_vm);
    let mut restored_alias_interner = ControlSequenceInterner::new();
    let mut restored_alias =
        Vm::try_restore_document(&mut restored_alias_interner, &alias_document)
            .expect("restore hidden mathcode alias");
    let outcome = restored_alias.run_plain(r"}\danger65=444[\number\danger65]");
    assert_eq!(outcome.output, "[444]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut macro_interner = ControlSequenceInterner::new();
    let mut macro_vm = Vm::new(&mut macro_interner);
    let outcome = macro_vm.run_plain(r"\def\danger{\mathcode65=445}{\def\danger{}");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let macro_snapshot = macro_vm.snapshot();
    assert!(
        macro_snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY)
    );
    assert!(serde_json::to_vec(&macro_snapshot).is_err());
    let macro_document = serde_json::to_vec(&VmSnapshotDocument::from_snapshot(macro_snapshot))
        .expect("encode hidden mathcode macro");
    drop(macro_vm);
    let mut restored_macro_interner = ControlSequenceInterner::new();
    let mut restored_macro =
        Vm::try_restore_document(&mut restored_macro_interner, &macro_document)
            .expect("restore hidden mathcode macro");
    let outcome = restored_macro.run_plain(r"}\danger[\number\mathcode65]");
    assert_eq!(outcome.output, "[445]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn mathcode_assignment_does_not_activate_rendering_or_delcode() {
    let baseline = run("$A$");
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let assigned = vm.run_plain(r"\mathcode65=123$A$");

    assert_eq!(assigned.output, baseline.output);
    assert!(
        assigned.diagnostics.is_empty(),
        "{:#?}",
        assigned.diagnostics
    );
    let snapshot = vm.snapshot();
    assert!(snapshot.mathcode_state.is_some());
    assert!(snapshot.delcode_state.is_none());
    assert!(
        !snapshot
            .required_capabilities()
            .iter()
            .any(|capability| { capability.as_str() == VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY })
    );
}
