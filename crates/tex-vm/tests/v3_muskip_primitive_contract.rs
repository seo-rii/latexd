use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotMeaning, SnapshotToken, SnapshotTokenKind, Vm, VmActiveModuleOptionsSnapshot,
    VmActiveSourceFrameSnapshot, VmInputContinuationSnapshot, VmQueueItemSnapshot, VmSnapshot,
};

const MUSKIP_SCALAR_V1_CAPABILITY: &str = "eqtb.muskip.scalar-v1";
const MUSKIP_ALIAS_V1_CAPABILITY: &str = "eqtb.muskip.alias-v1";

fn assert_muskip_alias(meaning: Option<&SnapshotMeaning>, expected_index: u32) {
    let Some(SnapshotMeaning::Macro {
        parameter_count: 0,
        parameter_text,
        optional_first_argument_default: None,
        body,
        ..
    }) = meaning
    else {
        panic!("expected a markerless muskip register alias, got {meaning:?}");
    };
    assert!(parameter_text.is_empty());
    assert!(matches!(
        body.first().map(|token| &token.kind),
        Some(SnapshotTokenKind::ControlSequence { name }) if name == "muskip"
    ));
    let digits = body[1..]
        .iter()
        .map(|token| match token.kind {
            SnapshotTokenKind::Character { ch, .. } => ch,
            SnapshotTokenKind::ControlSequence { .. } => {
                panic!("muskip register index must contain only digits")
            }
        })
        .collect::<String>();
    assert_eq!(digits, expected_index.to_string());
}

#[test]
fn source_muskip_primitives_allocate_alias_assign_and_render_mu_units() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);

    let outcome = vm.run_plain(
        r"\newmuskip\dynamic\dynamic=1.5mu\muskipdef\fixed=17\fixed=2mu\let\copied\fixed\copied=3mu[\the\dynamic][\the\fixed][\the\copied]",
    );

    assert_eq!(outcome.output, "[1.5mu][3mu][3mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(snapshot.muskip_registers.get(&17), Some(&(3 * 65_536)));
    assert_eq!(snapshot.muskip_registers.get(&256), Some(&(98_304)));
    assert_eq!(snapshot.next_muskip_register, 257);
    assert_muskip_alias(snapshot.scopes[0].get("dynamic"), 256);
    assert_muskip_alias(snapshot.scopes[0].get("fixed"), 17);
    assert_muskip_alias(snapshot.scopes[0].get("copied"), 17);
}

#[test]
fn render_event_capture_preserves_muskip_primitive_assignment_syntax() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.enable_render_event_capture();

    let outcome = vm.run_plain(
        r"\newmuskip\dynamic\dynamic=2mu\muskipdef\fixed=17\fixed=3mu[\the\dynamic][\the\fixed]",
    );

    assert_eq!(
        outcome.output, "[2mu][3mu]",
        "diagnostics={:#?}, transcript={:#?}",
        outcome.diagnostics, outcome.transcript
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn muskip_assignment_obeys_local_global_and_globaldefs_scope() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);

    let outcome = vm.run_plain(
        r"\muskipdef\slot=17\slot=1mu{\slot=2mu[\the\slot]}[\the\slot]{\global\slot=3mu}[\the\slot]{\globaldefs=-1\global\slot=4mu}[\the\slot]",
    );

    assert_eq!(outcome.output, "[2mu][1mu][3mu][3mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn muskip_arithmetic_uses_math_units_through_register_aliases() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);

    let outcome = vm.run_plain(
        r"\newmuskip\value\value=1.5mu\advance\value by 0.5mu\multiply\value by 3\divide\value by 2[\the\value]",
    );

    assert_eq!(outcome.output, "[3mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn deferred_dynamic_muskip_names_require_alias_capability_and_replay() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut source_interner);
    let outcome = vm.run_plain(
        r"\def\literal{\csname muskip\endcsname0=9mu}\let\builder\csname\def\parameterized#1{\builder #1\endcsname1=8mu}",
    );
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();

    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
    let mut bytes = Vec::new();
    serde_json::to_writer(&mut bytes, &snapshot)
        .expect_err("dynamic muskip names must not enter the legacy wire shape");
    assert!(bytes.is_empty(), "legacy serializer wrote {bytes:?}");
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &snapshot)
        .expect("restore dynamic muskip program in memory");
    let replay = restored.run_plain(r"\literal\parameterized{muskip}[\the\muskip0][\the\muskip1]");
    assert_eq!(replay.output, "[9mu][8mu]");
    assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
}

#[test]
fn deferred_nameuse_muskip_lookup_requires_alias_capability() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(r"\makeatletter\def\later{\@nameuse{muskip}2=7mu}\makeatother");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();

    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
    let mut bytes = Vec::new();
    serde_json::to_writer(&mut bytes, &snapshot)
        .expect_err("dynamic nameuse lookup must not enter the legacy wire shape");
    assert!(bytes.is_empty(), "legacy serializer wrote {bytes:?}");
}

#[test]
fn unsupported_muskip_components_are_consumed_without_mutation_or_prefix_leakage() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\def\marker{A}\muskipdef\slot=17\slot=7mu\afterassignment\marker\slot=1mu plus 2mu minus 3mu[\the\slot]\count0=0{\global\slot=4mu plus 5mu\count0=1}[\the\slot][\the\count0]",
    );

    assert_eq!(outcome.output, "A[7mu][7mu][0]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(vm.snapshot().muskip_registers.get(&17), Some(&(7 * 65_536)));
}

#[test]
fn rejected_muskip_operations_complete_afterassignment_and_division_saturates() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\def\marker{A}\muskipdef\slot=17\slot=7mu\afterassignment\marker\muskip-1=9mu[\the\slot]\afterassignment\marker\divide\slot by 0[\the\slot]\slot=-32768mu\divide\slot by -1[\the\slot]",
    );

    assert_eq!(outcome.output, "A[7mu]A[7mu][32767.99998mu]");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(vm.snapshot().muskip_registers.get(&17), Some(&i32::MAX));
}

#[test]
fn rejected_muskip_definitions_and_arithmetic_do_not_leak_completion_state() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome = vm.run_plain(
        r"\def\marker{A}\count0=0\afterassignment\marker\muskipdef\bad=-1[B]\afterassignment\marker\global\advance\muskip-1 by 2mu[C]\afterassignment\marker\multiply\muskip-2 by 3[D]\afterassignment\marker\divide\muskip-3 by 4[E]{\count0=1}[\the\count0]\ifdefined\bad T\else F\fi",
    );

    assert_eq!(outcome.output, "A[B]A[C]A[D]A[E][0]F");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert_eq!(snapshot.registers.get(&0), Some(&0));
    assert!(snapshot.after_assignment_token.is_none());
}

#[test]
fn exhausted_muskip_allocator_completes_afterassignment_without_defining_alias() {
    let mut source_interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut source_interner);
    let mut snapshot = vm.snapshot();
    snapshot.next_muskip_register = u32::MAX;
    drop(vm);
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &snapshot)
        .expect("restore exhausted muskip cursor");

    let outcome = restored.run_plain(
        r"\def\marker{A}\afterassignment\marker\global\newmuskip\overflow[B]\ifdefined\overflow T\else F\fi",
    );

    assert_eq!(outcome.output, "A[B]F");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert!(restored.snapshot().after_assignment_token.is_none());
}

#[test]
fn negative_muskip_indices_do_not_alias_the_u32_max_register() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);

    let outcome =
        vm.run_plain(r"\muskipdef\negative=-1\muskip-1=9mu\ifdefined\negative T\else F\fi");

    assert_eq!(outcome.output, "F");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();
    assert!(!snapshot.muskip_registers.contains_key(&u32::MAX));
    assert!(snapshot.required_capabilities().is_empty());
}

#[test]
fn raw_muskip_register_and_alias_survive_complete_snapshot_restore() {
    let mut source_interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut source_interner);
    let first = vm.run_plain(r"\newmuskip\first\first=2.25mu\muskip18=4mu");
    assert!(first.diagnostics.is_empty(), "{:#?}", first.diagnostics);
    let snapshot = vm.snapshot();
    drop(vm);

    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &snapshot)
        .expect("restore source-created muskip state");
    let outcome = restored.run_plain(
        r"\newmuskip\second\second=5mu[\the\first][\the\muskip18][\the\second]\let\copy\first\ifx\copy\first T\else F\fi",
    );

    assert_eq!(outcome.output, "[2.25mu][4mu][5mu]T");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let round_trip = restored.snapshot();
    assert_eq!(round_trip.next_muskip_register, 258);
    assert_muskip_alias(round_trip.scopes[0].get("first"), 256);
    assert_muskip_alias(round_trip.scopes[0].get("second"), 257);
}

#[test]
fn alias_only_and_deferred_muskip_meanings_require_structural_capability() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    let outcome =
        vm.run_plain(r"\muskipdef\fixed=17\def\deferred{\muskip18}\let\allocator\newmuskip");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let snapshot = vm.snapshot();

    assert!(snapshot.muskip_registers.is_empty());
    assert_eq!(snapshot.next_muskip_register, 256);
    let capabilities = snapshot.required_capabilities();
    assert_eq!(capabilities.len(), 1);
    assert!(
        capabilities
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
    assert!(
        capabilities
            .iter()
            .all(|capability| capability.as_str() != MUSKIP_SCALAR_V1_CAPABILITY)
    );
    let mut bytes = Vec::new();
    let error = serde_json::to_writer(&mut bytes, &snapshot)
        .expect_err("alias-only muskip state must not enter the legacy wire shape");
    assert!(error.to_string().contains(MUSKIP_ALIAS_V1_CAPABILITY));
    assert!(bytes.is_empty(), "legacy serializer wrote {bytes:?}");
    let explicit_legacy_projection =
        serde_json::to_vec(&*snapshot).expect("serialize explicit legacy projection");
    let decode_error = serde_json::from_slice::<VmSnapshot>(&explicit_legacy_projection)
        .expect_err("legacy reader must reject muskip alias meanings");
    assert!(
        decode_error
            .to_string()
            .contains(MUSKIP_ALIAS_V1_CAPABILITY)
    );
}

#[test]
fn nested_alias_history_is_included_in_the_capability_gate() {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\muskipdef\slot=17{\muskipdef\slot=18");
    let snapshot = vm.snapshot();

    assert_eq!(snapshot.scopes.len(), 2);
    assert_muskip_alias(snapshot.scopes[0].get("slot"), 17);
    assert_muskip_alias(snapshot.scopes[1].get("slot"), 18);
    assert!(
        snapshot
            .required_capabilities()
            .iter()
            .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY)
    );
}

#[test]
fn every_serialized_pending_token_owner_participates_in_alias_capability_derivation() {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let base = vm.snapshot();
    let muskip_token = SnapshotToken {
        kind: SnapshotTokenKind::ControlSequence {
            name: "muskip".to_string(),
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
        .insert(17, vec![muskip_token.clone()]);
    cases.push(("token register", token_register));

    let mut aftergroup = base.clone();
    aftergroup.aftergroup_tokens = vec![vec![muskip_token.clone()]];
    cases.push(("aftergroup", aftergroup));

    let mut after_assignment = base.clone();
    after_assignment.after_assignment_token = Some(muskip_token.clone());
    cases.push(("after-assignment", after_assignment));

    let mut end_document = base.clone();
    end_document.at_end_document_hooks = vec![vec![muskip_token.clone()]];
    cases.push(("end-document hook", end_document));

    let mut continuation_queue = base.clone();
    continuation_queue.input_continuation = Some(VmInputContinuationSnapshot {
        queue: vec![VmQueueItemSnapshot::Token {
            token: muskip_token.clone(),
        }],
        source_stack: Vec::new(),
        last_token_end_utf8: 0,
    });
    cases.push(("continuation queue", continuation_queue));

    let mut source_end_hook = base.clone();
    source_end_hook.input_continuation = Some(VmInputContinuationSnapshot {
        queue: Vec::new(),
        source_stack: vec![source_frame(vec![vec![muskip_token.clone()]], None)],
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
                    vec![muskip_token.clone()],
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
                default_option_body: Some(vec![muskip_token]),
            }),
        )],
        last_token_end_utf8: 0,
    });
    cases.push(("default option", default_option));

    for (owner, snapshot) in cases {
        assert!(
            snapshot
                .required_capabilities()
                .iter()
                .any(|capability| capability.as_str() == MUSKIP_ALIAS_V1_CAPABILITY),
            "{owner} omitted the muskip alias capability"
        );
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &snapshot)
            .expect_err("pending muskip command must not enter the legacy wire shape");
        assert!(bytes.is_empty(), "{owner} wrote legacy bytes: {bytes:?}");
    }
}

#[test]
fn exhausted_muskip_allocator_does_not_wrap_or_define_an_alias() {
    let mut source_interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut source_interner);
    let mut snapshot = vm.snapshot();
    snapshot.next_muskip_register = u32::MAX;
    drop(vm);
    let mut restored_interner = ControlSequenceInterner::new();
    let mut restored = Vm::try_restore(&mut restored_interner, &snapshot)
        .expect("restore exhausted muskip cursor");

    let outcome = restored.run_plain(r"\newmuskip\overflow\ifdefined\overflow T\else F\fi");

    assert_eq!(outcome.output, "F");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let after = restored.snapshot();
    assert_eq!(after.next_muskip_register, u32::MAX);
    assert!(!after.scopes[0].contains_key("overflow"));
}
