use tex_tfm_metrics::dimension_subset::{ExactTfmDimensions, ExtractError, extract_exact_frame};

use syn::{
    Expr, ExprStruct, ExprUnsafe, Field, Fields, FnArg, ForeignItem, ImplItem, Item, ItemFn,
    Member, Meta, Pat, Path as SynPath, ReturnType, Stmt, Type, UseTree, Visibility,
    visit::{self, Visit},
};

const CMR10: &[u8] = include_bytes!("../../tex-fonts/assets/classic/tfm/cmr10.tfm");

#[test]
fn public_api_names_the_exact_frame_dimension_subset_and_removes_broad_aliases() {
    let source = include_str!("../src/lib.rs");

    assert!(source.contains("pub mod dimension_subset"));
    assert!(source.contains("pub fn extract_exact_frame"));
    assert!(source.contains("Success does not imply"));
    assert!(!source.contains("pub fn parse_tfm"));
    assert!(!source.contains("pub enum TfmParseError"));
}

#[test]
fn staged_validator_module_exists_without_crate_or_public_visibility() {
    let source = include_str!("../src/lib.rs");

    assert!(source.contains("\nmod tfm_validation;"));
    assert!(!source.contains("pub mod tfm_validation"));
    assert!(!source.contains("pub(crate) mod tfm_validation"));
}

#[test]
fn staged_validator_states_and_entrypoints_remain_private_and_uncalled() {
    let source = include_str!("../src/tfm_validation.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();

    assert!(source.contains("#![forbid(unsafe_code)]"));
    assert!(!production.contains("\npub "));
    assert!(!production.contains("\npub("));
    assert_eq!(production.matches("check_preamble_header(").count(), 1);
    assert_eq!(production.matches("check_characters(").count(), 1);
    assert_eq!(production.matches("check_boxes(").count(), 1);
    assert_eq!(production.matches("check_lig_kern(").count(), 1);
    assert_eq!(production.matches("check_kerns(").count(), 1);
    assert_eq!(production.matches("check_extensibles(").count(), 1);
    assert_eq!(production.matches("check_parameters(").count(), 1);
    assert_eq!(production.matches("finish_validation(").count(), 1);
    for forbidden in [
        "impl From<",
        "impl TryFrom<",
        "Serialize for HeaderCheckedTfm",
        "Deserialize for HeaderCheckedTfm",
        "Serialize for CharacterCheckedTfm",
        "Deserialize for CharacterCheckedTfm",
        "Serialize for BoxCheckedTfm",
        "Deserialize for BoxCheckedTfm",
        "Serialize for LigKernCheckedTfm",
        "Deserialize for LigKernCheckedTfm",
        "Serialize for KernCheckedTfm",
        "Deserialize for KernCheckedTfm",
        "Serialize for ExtensibleCheckedTfm",
        "Deserialize for ExtensibleCheckedTfm",
        "Serialize for ParameterCheckedTfm",
        "Deserialize for ParameterCheckedTfm",
        "Serialize for CompleteCheckedTfm",
        "Deserialize for CompleteCheckedTfm",
    ] {
        assert!(
            !production.contains(forbidden),
            "forbidden API: {forbidden}"
        );
    }
}

#[test]
fn staged_validator_ast_has_only_private_items_and_no_production_references() {
    let syntax = syn::parse_file(include_str!("../src/tfm_validation.rs")).unwrap();
    let mut policy = PrivateValidatorPolicy::default();
    policy.visit_file(&syntax);

    assert_eq!(
        policy.entrypoint_definitions,
        [
            "check_preamble_header",
            "check_characters",
            "check_boxes",
            "check_lig_kern",
            "check_kerns",
            "check_extensibles",
            "check_parameters",
            "finish_validation",
        ]
    );
    assert!(
        policy.entrypoint_references.is_empty(),
        "production entrypoint references: {:?}",
        policy.entrypoint_references
    );
    assert_eq!(
        policy.proof_state_returners,
        [
            ("HeaderCheckedTfm".into(), "check_preamble_header".into()),
            ("CharacterCheckedTfm".into(), "check_characters".into()),
            ("BoxCheckedTfm".into(), "check_boxes".into()),
            ("LigKernCheckedTfm".into(), "check_lig_kern".into()),
            ("KernCheckedTfm".into(), "check_kerns".into()),
            ("ExtensibleCheckedTfm".into(), "check_extensibles".into(),),
            ("ParameterCheckedTfm".into(), "check_parameters".into(),),
            ("CompleteCheckedTfm".into(), "finish_validation".into(),),
        ]
    );
    assert_eq!(
        policy.proof_state_constructions,
        policy.proof_state_returners
    );
}

#[test]
fn structural_policy_rejects_external_module_alias_wrapper_macro_and_visibility_mutants() {
    for source in [
        "fn check_characters() {} const ALIAS: fn() = check_characters;",
        "fn check_characters() {} fn wrapper() { check_characters(); }",
        "fn check_characters() {} use self::check_characters as run;",
        "fn check_characters() {} delegate!(check_characters);",
    ] {
        let syntax = syn::parse_file(source).unwrap();
        let mut policy = PrivateValidatorPolicy::default();
        policy.visit_file(&syntax);
        assert_eq!(
            policy.entrypoint_references,
            ["check_characters"],
            "missed production reference in {source}"
        );
    }

    for source in [
        "mod bypass;",
        "#[path = \"bypass.rs\"] mod bypass;",
        "pub(crate) fn check_characters() {}",
        "struct Proof { pub(crate) raw: () }",
        "struct Proof; impl Proof { pub(crate) fn leak() {} }",
        "extern \"C\" { pub(crate) fn leak(); }",
        "#[derive(Clone)] struct BoxCheckedTfm;",
        "struct BoxCheckedTfm; impl Clone for BoxCheckedTfm { fn clone(&self) -> Self { loop {} } }",
        "struct BoxCheckedTfm; fn forge_box() -> BoxCheckedTfm { loop {} }",
        "struct BoxCheckedTfm; type ForgedBox = BoxCheckedTfm;",
        "struct BoxCheckedTfm; struct Factory; impl Factory { fn forge() -> BoxCheckedTfm { loop {} } }",
        "struct BoxCheckedTfm; macro_rules! forge { () => { BoxCheckedTfm } }",
        "#[derive(Clone)] struct LigKernCheckedTfm;",
        "struct LigKernCheckedTfm; fn forge_lig_kern() -> LigKernCheckedTfm { loop {} }",
        "struct LigKernCheckedTfm; struct HiddenDuplicate(std::mem::ManuallyDrop<LigKernCheckedTfm>); fn duplicate(state: &LigKernCheckedTfm) -> HiddenDuplicate { HiddenDuplicate(std::mem::ManuallyDrop::new(unsafe { std::ptr::read(state) })) }",
        "#[derive(Clone)] struct KernCheckedTfm;",
        "struct KernCheckedTfm; fn forge_kerns() -> KernCheckedTfm { loop {} }",
        "#[forge] struct KernCheckedTfm;",
        "#[derive(Debug)] struct KernCheckedTfm;",
        "struct KernCheckedTfm; include!(\"tfm_validation_forge.rs\");",
        "#[derive(Clone)] struct ExtensibleCheckedTfm;",
        "struct ExtensibleCheckedTfm; fn forge_extensibles() -> ExtensibleCheckedTfm { loop {} }",
        "#[forge] struct ExtensibleCheckedTfm;",
        "#[derive(Debug)] struct ExtensibleCheckedTfm;",
        "struct ExtensibleCheckedTfm; include!(\"tfm_validation_forge.rs\");",
        "#[derive(Clone)] struct ParameterCheckedTfm;",
        "struct ParameterCheckedTfm; fn forge_parameters() -> ParameterCheckedTfm { loop {} }",
        "#[forge] struct ParameterCheckedTfm;",
        "#[derive(Debug)] struct ParameterCheckedTfm;",
        "struct ParameterCheckedTfm; include!(\"tfm_validation_forge.rs\");",
        "#[derive(Clone)] struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } impl CompleteCheckedTfm { fn inspect(&self) {} }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm, extra: () }",
        "struct CompleteCheckedTfm(ParameterCheckedTfm);",
        "struct CompleteCheckedTfm { predecessor: () }",
        "struct CompleteCheckedTfm { pub(crate) predecessor: ParameterCheckedTfm }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } fn forge_completion() -> CompleteCheckedTfm { loop {} }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } fn finish_validation(predecessor: ParameterCheckedTfm) -> CompleteCheckedTfm { CompleteCheckedTfm { predecessor } } fn alternate(predecessor: ParameterCheckedTfm) -> CompleteCheckedTfm { CompleteCheckedTfm { predecessor } }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } fn finish_validation(predecessor: ParameterCheckedTfm) -> CompleteCheckedTfm { let _ = &predecessor.predecessor; CompleteCheckedTfm { predecessor } }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } fn finish_validation(predecessor: &ParameterCheckedTfm) -> CompleteCheckedTfm { loop {} }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } fn finish_validation(state: ParameterCheckedTfm) -> CompleteCheckedTfm { CompleteCheckedTfm { predecessor: state } }",
        "struct CompleteCheckedTfm { predecessor: ParameterCheckedTfm } #[inline] fn finish_validation(predecessor: ParameterCheckedTfm) -> CompleteCheckedTfm { CompleteCheckedTfm { predecessor } }",
    ] {
        let syntax = syn::parse_file(source).unwrap();
        let rejected = std::panic::catch_unwind(|| {
            let mut policy = PrivateValidatorPolicy::default();
            policy.visit_file(&syntax);
        });
        assert!(rejected.is_err(), "missed non-private syntax in {source}");
    }
}

#[test]
fn exact_frame_policy_rejects_native_accepted_trailing_data_without_claiming_invalidity() {
    let mut trailing_word = CMR10.to_vec();
    trailing_word.extend_from_slice(&[0; 4]);

    assert!(matches!(
        extract_exact_frame(&trailing_word),
        Err(ExtractError::ExactFrameLengthMismatch { .. })
    ));
}

#[test]
fn unrelated_native_invalid_table_data_can_still_expose_the_dimension_subset() {
    let control = extract_exact_frame(CMR10).unwrap();
    let mut invalid_fontdimen2 = CMR10.to_vec();
    let parameter_start = parameter_start(CMR10);
    invalid_fontdimen2[parameter_start + 4..parameter_start + 8]
        .copy_from_slice(&(1i32 << 24).to_be_bytes());

    let subset = extract_exact_frame(&invalid_fontdimen2)
        .expect("unselected fontdimen validity is outside this subset contract");
    assert_eq!(subset.design_size_sp(), control.design_size_sp());
    assert_eq!(
        subset.at_size_sp(subset.design_size_sp()).unwrap(),
        control.at_size_sp(control.design_size_sp()).unwrap()
    );
}

#[test]
fn empty_parameter_table_zero_fills_both_selected_dimensions() {
    let mut missing_parameters = CMR10[..CMR10.len() - 28].to_vec();
    missing_parameters[0..2].copy_from_slice(&317u16.to_be_bytes());
    missing_parameters[22..24].copy_from_slice(&0u16.to_be_bytes());

    let metrics = extract_exact_frame(&missing_parameters)
        .expect("np=0 exposes a valid zero-filled dimension subset");
    assert_eq!(
        metrics.at_size_sp(metrics.design_size_sp()).unwrap(),
        ExactTfmDimensions {
            quad_sp: 0,
            x_height_sp: 0,
        }
    );
}

fn parameter_start(bytes: &[u8]) -> usize {
    let counts = (0..12)
        .map(|index| u16::from_be_bytes([bytes[index * 2], bytes[index * 2 + 1]]) as usize)
        .collect::<Vec<_>>();
    let [_, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, _] = counts.as_slice() else {
        unreachable!()
    };
    let character_count = ec - bc + 1;
    4 * (6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne)
}

#[derive(Default)]
struct PrivateValidatorPolicy {
    entrypoint_definitions: Vec<String>,
    entrypoint_references: Vec<String>,
    proof_state_returners: Vec<(String, String)>,
    proof_state_constructions: Vec<(String, String)>,
    current_function: Option<String>,
}

impl<'ast> Visit<'ast> for PrivateValidatorPolicy {
    fn visit_item(&mut self, item: &'ast Item) {
        if let Item::Mod(module) = item
            && module.attrs.iter().any(|attribute| {
                attribute.path().is_ident("cfg")
                    && matches!(
                        &attribute.meta,
                        Meta::List(arguments) if arguments.tokens.to_string() == "test"
                    )
            })
        {
            return;
        }

        if let Item::Mod(module) = item {
            assert!(
                module.content.is_some(),
                "production validator must not use out-of-line child modules"
            );
        }

        if let Item::Struct(structure) = item
            && is_proof_state(&structure.ident.to_string())
        {
            assert!(
                structure.attrs.is_empty(),
                "proof states must not have attributes or derives"
            );
        }

        if let Item::Struct(structure) = item
            && structure.ident == "CompleteCheckedTfm"
        {
            let Fields::Named(fields) = &structure.fields else {
                panic!("completion proof state must use one named predecessor field");
            };
            assert_eq!(
                fields.named.len(),
                1,
                "completion proof state must have exactly one field"
            );
            let field = fields.named.first().unwrap();
            assert_eq!(
                field.ident.as_ref().map(ToString::to_string).as_deref(),
                Some("predecessor"),
                "completion proof state field must be named predecessor"
            );
            let Type::Path(field_type) = &field.ty else {
                panic!("completion predecessor must be ParameterCheckedTfm");
            };
            assert!(
                field_type.qself.is_none() && field_type.path.is_ident("ParameterCheckedTfm"),
                "completion predecessor must be ParameterCheckedTfm"
            );
        }

        if let Item::Impl(implementation) = item {
            let mut proof_states = ProofStatePathCollector::default();
            proof_states.visit_type(&implementation.self_ty);
            assert!(
                proof_states.names.is_empty(),
                "proof states must not have manual or inherent impls"
            );
        }

        if let Item::Type(alias) = item {
            let mut proof_states = ProofStatePathCollector::default();
            proof_states.visit_type(&alias.ty);
            assert!(
                proof_states.names.is_empty(),
                "proof states must not have type aliases"
            );
        }

        let visibility = match item {
            Item::Const(item) => Some(&item.vis),
            Item::Enum(item) => Some(&item.vis),
            Item::ExternCrate(item) => Some(&item.vis),
            Item::Fn(item) => Some(&item.vis),
            Item::Mod(item) => Some(&item.vis),
            Item::Static(item) => Some(&item.vis),
            Item::Struct(item) => Some(&item.vis),
            Item::Trait(item) => Some(&item.vis),
            Item::TraitAlias(item) => Some(&item.vis),
            Item::Type(item) => Some(&item.vis),
            Item::Union(item) => Some(&item.vis),
            Item::Use(item) => Some(&item.vis),
            Item::ForeignMod(_) | Item::Impl(_) | Item::Macro(_) | Item::Verbatim(_) | _ => None,
        };
        if let Some(visibility) = visibility {
            assert!(
                matches!(visibility, Visibility::Inherited),
                "production validator item has non-private visibility"
            );
        }
        if let Item::Fn(function) = item {
            let name = function.sig.ident.to_string();
            if is_validator_entrypoint(&name) {
                self.entrypoint_definitions.push(name);
            }
        }
        visit::visit_item(self, item);
    }

    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        let function_name = function.sig.ident.to_string();
        if function_name == "finish_validation" {
            assert!(
                function.attrs.is_empty()
                    && function.sig.constness.is_none()
                    && function.sig.asyncness.is_none()
                    && function.sig.unsafety.is_none()
                    && function.sig.abi.is_none()
                    && function.sig.variadic.is_none()
                    && function.sig.generics.params.is_empty()
                    && function.sig.generics.where_clause.is_none(),
                "completion constructor must have the exact plain function signature"
            );
            assert_eq!(
                function.sig.inputs.len(),
                1,
                "completion constructor must have one by-value predecessor"
            );
            let Some(FnArg::Typed(argument)) = function.sig.inputs.first() else {
                panic!("completion constructor must have one typed predecessor");
            };
            assert!(
                argument.attrs.is_empty(),
                "completion argument must be plain"
            );
            let Pat::Ident(pattern) = argument.pat.as_ref() else {
                panic!("completion argument must be named predecessor");
            };
            assert!(
                pattern.attrs.is_empty()
                    && pattern.by_ref.is_none()
                    && pattern.mutability.is_none()
                    && pattern.subpat.is_none()
                    && pattern.ident == "predecessor",
                "completion argument must be the plain predecessor binding"
            );
            let Type::Path(argument_type) = argument.ty.as_ref() else {
                panic!("completion argument must consume ParameterCheckedTfm");
            };
            assert!(
                argument_type.qself.is_none() && argument_type.path.is_ident("ParameterCheckedTfm"),
                "completion argument must consume ParameterCheckedTfm"
            );
            let ReturnType::Type(_, return_type) = &function.sig.output else {
                panic!("completion constructor must return CompleteCheckedTfm");
            };
            let Type::Path(return_type) = return_type.as_ref() else {
                panic!("completion constructor must return CompleteCheckedTfm");
            };
            assert!(
                return_type.qself.is_none() && return_type.path.is_ident("CompleteCheckedTfm"),
                "completion constructor must return CompleteCheckedTfm"
            );
            assert_eq!(
                function.block.stmts.len(),
                1,
                "completion constructor must contain one read-free expression"
            );
            let Some(Stmt::Expr(Expr::Struct(construction), None)) = function.block.stmts.first()
            else {
                panic!("completion constructor must contain only its struct construction");
            };
            assert!(
                construction.attrs.is_empty()
                    && construction.qself.is_none()
                    && construction.path.is_ident("CompleteCheckedTfm")
                    && construction.rest.is_none()
                    && construction.fields.len() == 1,
                "completion constructor must construct exactly one predecessor field"
            );
            let field = construction.fields.first().unwrap();
            assert!(
                field.attrs.is_empty()
                    && matches!(&field.member, Member::Named(name) if name == "predecessor")
                    && field.colon_token.is_none()
                    && matches!(&field.expr, Expr::Path(path) if path.qself.is_none() && path.path.is_ident("predecessor")),
                "completion constructor must use predecessor field shorthand"
            );
        }
        let mut proof_states = ProofStatePathCollector::default();
        proof_states.visit_return_type(&function.sig.output);
        for proof_state in proof_states.names {
            assert_eq!(
                authorized_proof_constructor(&proof_state),
                Some(function_name.as_str()),
                "unauthorized function returns a proof state"
            );
            self.proof_state_returners
                .push((proof_state, function_name.clone()));
        }

        let previous_function = self.current_function.replace(function_name);
        visit::visit_item_fn(self, function);
        self.current_function = previous_function;
    }

    fn visit_expr_struct(&mut self, expression: &'ast ExprStruct) {
        if let Some(segment) = expression.path.segments.last() {
            let proof_state = segment.ident.to_string();
            if is_proof_state(&proof_state) {
                let function_name = self
                    .current_function
                    .as_deref()
                    .expect("proof state constructed outside a function");
                assert_eq!(
                    authorized_proof_constructor(&proof_state),
                    Some(function_name),
                    "proof state constructed outside its authorized entrypoint"
                );
                self.proof_state_constructions
                    .push((proof_state, function_name.to_owned()));
            }
        }
        visit::visit_expr_struct(self, expression);
    }

    fn visit_expr_unsafe(&mut self, _expression: &'ast ExprUnsafe) {
        panic!("production validator must not contain unsafe blocks");
    }

    fn visit_field(&mut self, field: &'ast Field) {
        assert!(
            matches!(field.vis, Visibility::Inherited),
            "production validator field has non-private visibility"
        );
        visit::visit_field(self, field);
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let visibility = match item {
            ImplItem::Const(item) => Some(&item.vis),
            ImplItem::Fn(item) => Some(&item.vis),
            ImplItem::Type(item) => Some(&item.vis),
            ImplItem::Macro(_) | ImplItem::Verbatim(_) | _ => None,
        };
        if let Some(visibility) = visibility {
            assert!(
                matches!(visibility, Visibility::Inherited),
                "production validator associated item has non-private visibility"
            );
        }
        if let ImplItem::Fn(function) = item {
            let mut proof_states = ProofStatePathCollector::default();
            proof_states.visit_return_type(&function.sig.output);
            assert!(
                proof_states.names.is_empty(),
                "associated functions must not return proof states"
            );
        }
        visit::visit_impl_item(self, item);
    }

    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        let visibility = match item {
            ForeignItem::Fn(item) => Some(&item.vis),
            ForeignItem::Static(item) => Some(&item.vis),
            ForeignItem::Type(item) => Some(&item.vis),
            ForeignItem::Macro(_) | ForeignItem::Verbatim(_) | _ => None,
        };
        if let Some(visibility) = visibility {
            assert!(
                matches!(visibility, Visibility::Inherited),
                "production validator foreign item has non-private visibility"
            );
        }
        visit::visit_foreign_item(self, item);
    }

    fn visit_path(&mut self, path: &'ast SynPath) {
        if let Some(segment) = path.segments.last() {
            let name = segment.ident.to_string();
            if is_validator_entrypoint(&name) {
                self.entrypoint_references.push(name);
            }
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, macro_invocation: &'ast syn::Macro) {
        assert!(
            !macro_invocation.path.is_ident("include"),
            "production validator must not include generated source"
        );
        for token in macro_invocation
            .tokens
            .to_string()
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        {
            if is_validator_entrypoint(token) {
                self.entrypoint_references.push(token.to_owned());
            }
            assert!(!is_proof_state(token), "macros must not name proof states");
        }
        visit::visit_macro(self, macro_invocation);
    }

    fn visit_use_tree(&mut self, use_tree: &'ast UseTree) {
        let imported_name = match use_tree {
            UseTree::Name(name) => Some(name.ident.to_string()),
            UseTree::Rename(rename) => Some(rename.ident.to_string()),
            UseTree::Glob(_) | UseTree::Group(_) | UseTree::Path(_) => None,
        };
        if let Some(name) = imported_name
            && is_validator_entrypoint(&name)
        {
            self.entrypoint_references.push(name);
        }
        visit::visit_use_tree(self, use_tree);
    }
}

#[derive(Default)]
struct ProofStatePathCollector {
    names: Vec<String>,
}

impl<'ast> Visit<'ast> for ProofStatePathCollector {
    fn visit_path(&mut self, path: &'ast SynPath) {
        if let Some(segment) = path.segments.last() {
            let name = segment.ident.to_string();
            if is_proof_state(&name) && !self.names.contains(&name) {
                self.names.push(name);
            }
        }
        visit::visit_path(self, path);
    }
}

fn is_validator_entrypoint(name: &str) -> bool {
    matches!(
        name,
        "check_preamble_header"
            | "check_characters"
            | "check_boxes"
            | "check_lig_kern"
            | "check_kerns"
            | "check_extensibles"
            | "check_parameters"
            | "finish_validation"
    )
}

fn is_proof_state(name: &str) -> bool {
    matches!(
        name,
        "HeaderCheckedTfm"
            | "CharacterCheckedTfm"
            | "BoxCheckedTfm"
            | "LigKernCheckedTfm"
            | "KernCheckedTfm"
            | "ExtensibleCheckedTfm"
            | "ParameterCheckedTfm"
            | "CompleteCheckedTfm"
    )
}

fn authorized_proof_constructor(proof_state: &str) -> Option<&'static str> {
    match proof_state {
        "HeaderCheckedTfm" => Some("check_preamble_header"),
        "CharacterCheckedTfm" => Some("check_characters"),
        "BoxCheckedTfm" => Some("check_boxes"),
        "LigKernCheckedTfm" => Some("check_lig_kern"),
        "KernCheckedTfm" => Some("check_kerns"),
        "ExtensibleCheckedTfm" => Some("check_extensibles"),
        "ParameterCheckedTfm" => Some("check_parameters"),
        "CompleteCheckedTfm" => Some("finish_validation"),
        _ => None,
    }
}
