use tex_tfm_metrics::dimension_subset::{ExactTfmDimensions, ExtractError, extract_exact_frame};

use syn::{
    ExprStruct, ExprUnsafe, Field, ForeignItem, ImplItem, Item, ItemFn, Meta, Path as SynPath,
    UseTree, Visibility,
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
        ]
    );
    assert_eq!(
        policy.proof_state_constructions,
        policy.proof_state_returners
    );
}

#[test]
fn structural_policy_rejects_alias_wrapper_reexport_macro_and_visibility_mutants() {
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

        if let Item::Struct(structure) = item
            && is_proof_state(&structure.ident.to_string())
        {
            for attribute in &structure.attrs {
                if let Meta::List(arguments) = &attribute.meta
                    && attribute.path().is_ident("derive")
                {
                    assert!(
                        !arguments
                            .tokens
                            .to_string()
                            .split(|character: char| {
                                !(character.is_ascii_alphanumeric() || character == '_')
                            })
                            .any(|token| token == "Clone"),
                        "proof states must not derive Clone"
                    );
                }
            }
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
        "check_preamble_header" | "check_characters" | "check_boxes" | "check_lig_kern"
    )
}

fn is_proof_state(name: &str) -> bool {
    matches!(
        name,
        "HeaderCheckedTfm" | "CharacterCheckedTfm" | "BoxCheckedTfm" | "LigKernCheckedTfm"
    )
}

fn authorized_proof_constructor(proof_state: &str) -> Option<&'static str> {
    match proof_state {
        "HeaderCheckedTfm" => Some("check_preamble_header"),
        "CharacterCheckedTfm" => Some("check_characters"),
        "BoxCheckedTfm" => Some("check_boxes"),
        "LigKernCheckedTfm" => Some("check_lig_kern"),
        _ => None,
    }
}
