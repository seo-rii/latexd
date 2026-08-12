use std::{
    fs,
    path::{Path, PathBuf},
};

use syn::{
    Expr, ExprAssign, ExprCall, ExprField, ExprPath, GenericArgument, ImplItemFn, ItemFn, ItemImpl,
    ItemMod, Member, Pat, PathArguments, Type,
    visit::{self, Visit},
};

#[derive(Default)]
struct LegacyConstructorVisitor {
    calls: Vec<String>,
    assignments: Vec<String>,
    constructor_surface_violations: Vec<String>,
    compatibility_producer_constructions: Vec<String>,
    provenance_authority_conversion_violations: Vec<String>,
    in_render_event_envelope_impl: bool,
    current_impl_function: Option<String>,
    pattern_depth: usize,
}

impl<'ast> Visit<'ast> for LegacyConstructorVisitor {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !is_cfg_test(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if !is_cfg_test(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if !is_cfg_test(&item.attrs) {
            let converts_generated_by_to_event_producer =
                matches!(item.self_ty.as_ref(),
                    Type::Path(path) if path.path.segments.last().is_some_and(|segment| {
                        segment.ident == "EventProducer"
                    })
                ) && item.trait_.as_ref().is_some_and(|(_, path, _)| {
                    path.segments.last().is_some_and(|segment| {
                        segment.ident == "From"
                            && matches!(&segment.arguments,
                                PathArguments::AngleBracketed(arguments)
                                    if arguments.args.iter().any(|argument| matches!(argument,
                                        GenericArgument::Type(Type::Path(path))
                                            if path.path.segments.last().is_some_and(|segment| {
                                                segment.ident == "GeneratedBy"
                                            })
                                    ))
                            )
                    })
                });
            if converts_generated_by_to_event_producer {
                self.provenance_authority_conversion_violations
                    .push("From<GeneratedBy> for EventProducer".to_string());
            }
            let previous = self.in_render_event_envelope_impl;
            self.in_render_event_envelope_impl = matches!(
                item.self_ty.as_ref(),
                syn::Type::Path(path)
                    if path.path.segments.last().is_some_and(|segment| {
                        segment.ident == "RenderEventEnvelope"
                    })
            );
            visit::visit_item_impl(self, item);
            self.in_render_event_envelope_impl = previous;
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if !is_cfg_test(&item.attrs) {
            let previous = self
                .current_impl_function
                .replace(item.sig.ident.to_string());
            if self.in_render_event_envelope_impl
                && item.sig.receiver().is_none()
                && matches!(item.vis, syn::Visibility::Public(_))
                && !matches!(
                    item.sig.ident.to_string().as_str(),
                    "try_from_origin" | "from_scanner_recovery"
                )
            {
                self.constructor_surface_violations.push(format!(
                    "public RenderEventEnvelope associated function {}",
                    item.sig.ident
                ));
            }
            visit::visit_impl_item_fn(self, item);
            self.current_impl_function = previous;
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(function) = call.func.as_ref() {
            let mut segments = function.path.segments.iter().rev();
            let constructor = segments.next().map(|segment| segment.ident.to_string());
            let owner = segments.next().map(|segment| segment.ident.to_string());
            if owner.as_deref() == Some("RenderEventEnvelope")
                && matches!(constructor.as_deref(), Some("new" | "with_origin"))
            {
                self.calls.push(
                    constructor
                        .as_deref()
                        .expect("matched constructor")
                        .to_string(),
                );
            }
            if self.in_render_event_envelope_impl
                && constructor.as_deref() == Some("from_metadata")
                && matches!(owner.as_deref(), Some("Self" | "RenderEventEnvelope"))
                && self.current_impl_function.as_deref() != Some("try_from_origin")
            {
                self.constructor_surface_violations.push(format!(
                    "RenderEventEnvelope::{} calls private from_metadata",
                    self.current_impl_function.as_deref().unwrap_or("<unknown>")
                ));
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_assign(&mut self, assignment: &'ast ExprAssign) {
        let mut fields = Vec::new();
        collect_field_chain(assignment.left.as_ref(), &mut fields);
        let field = fields.join(".");
        if matches!(
            field.as_str(),
            "meta.producer" | "meta.confidence" | "meta.source.generated_by"
        ) {
            self.assignments.push(field);
        }
        visit::visit_expr_assign(self, assignment);
    }

    fn visit_expr_path(&mut self, path: &'ast ExprPath) {
        let mut segments = path.path.segments.iter().rev();
        let producer = segments.next().map(|segment| segment.ident.to_string());
        let owner = segments.next().map(|segment| segment.ident.to_string());
        if self.pattern_depth == 0
            && owner.as_deref() == Some("EventProducer")
            && matches!(producer.as_deref(), Some("Command" | "Shim" | "BblParser"))
        {
            self.compatibility_producer_constructions.push(format!(
                "EventProducer::{}",
                producer.as_deref().expect("matched producer")
            ));
        }
        visit::visit_expr_path(self, path);
    }

    fn visit_pat(&mut self, pattern: &'ast Pat) {
        self.pattern_depth += 1;
        visit::visit_pat(self, pattern);
        self.pattern_depth -= 1;
    }
}

fn collect_field_chain(expression: &Expr, fields: &mut Vec<String>) {
    let Expr::Field(ExprField { base, member, .. }) = expression else {
        return;
    };
    collect_field_chain(base, fields);
    if let Member::Named(field) = member {
        fields.push(field.to_string());
    }
}

fn is_cfg_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<syn::Ident>()
                .is_ok_and(|condition| condition == "test")
    })
}

fn legacy_constructor_calls(source: &str) -> Vec<String> {
    event_origin_policy(source).calls
}

fn direct_origin_metadata_assignments(source: &str) -> Vec<String> {
    event_origin_policy(source).assignments
}

fn envelope_constructor_surface_violations(source: &str) -> Vec<String> {
    event_origin_policy(source).constructor_surface_violations
}

fn compatibility_producer_constructions(source: &str) -> Vec<String> {
    event_origin_policy(source).compatibility_producer_constructions
}

fn provenance_authority_conversion_violations(source: &str) -> Vec<String> {
    event_origin_policy(source).provenance_authority_conversion_violations
}

fn event_origin_policy(source: &str) -> LegacyConstructorVisitor {
    let syntax = syn::parse_file(source).expect("test input must be valid Rust syntax");
    let mut visitor = LegacyConstructorVisitor::default();
    visitor.visit_file(&syntax);
    visitor
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    if !directory.is_dir() {
        return;
    }
    for entry in fs::read_dir(directory).expect("read Rust source directory") {
        let path = entry.expect("read Rust source entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

#[test]
fn origin_policy_finds_production_calls_and_ignores_test_modules() {
    let source = r#"
struct RenderEventEnvelope;

impl From<GeneratedBy> for EventProducer {
    fn from(_: GeneratedBy) -> Self {
        Self::Shim
    }
}

impl RenderEventEnvelope {
    pub fn try_from_origin() {}
    pub fn from_scanner_recovery() {}

    pub fn new() {
        Self::from_metadata();
    }

    pub fn with_origin() {
        Self::from_metadata();
    }

    pub fn from_raw() {}

    fn from_metadata() {}
}

fn emit() {
    RenderEventEnvelope::new(sequence, event, source);
    tex_render_model::RenderEventEnvelope::with_origin(
        sequence,
        event,
        source,
        producer,
        confidence,
    );
    event.meta.producer = producer;
    event.meta.confidence = confidence;
    event.meta.source.generated_by = generated_by;
    let _command = EventProducer::Command;
    let _shim = EventProducer::Shim;
    let _bbl_parser = EventProducer::BblParser;
}

fn reject_decoded_compatibility_producer(producer: EventProducer) {
    match producer {
        EventProducer::Command | EventProducer::Shim | EventProducer::BblParser => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    fn fixture() {
        RenderEventEnvelope::new(sequence, event, source);
        let _allowed_test_fixture = EventProducer::Command;
    }
}
"#;

    assert_eq!(
        legacy_constructor_calls(source),
        vec!["new".to_string(), "with_origin".to_string()]
    );
    assert_eq!(
        direct_origin_metadata_assignments(source),
        vec![
            "meta.producer".to_string(),
            "meta.confidence".to_string(),
            "meta.source.generated_by".to_string(),
        ]
    );
    assert_eq!(
        envelope_constructor_surface_violations(source),
        vec![
            "public RenderEventEnvelope associated function new".to_string(),
            "RenderEventEnvelope::new calls private from_metadata".to_string(),
            "public RenderEventEnvelope associated function with_origin".to_string(),
            "RenderEventEnvelope::with_origin calls private from_metadata".to_string(),
            "public RenderEventEnvelope associated function from_raw".to_string(),
        ]
    );
    assert_eq!(
        compatibility_producer_constructions(source),
        vec![
            "EventProducer::Command".to_string(),
            "EventProducer::Shim".to_string(),
            "EventProducer::BblParser".to_string(),
        ]
    );
    assert_eq!(
        provenance_authority_conversion_violations(source),
        vec!["From<GeneratedBy> for EventProducer".to_string()]
    );
}

#[test]
fn production_sources_use_typed_event_origin_constructors() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("render-model crate must live under the workspace crates directory");
    let mut sources = Vec::new();
    for entry in fs::read_dir(workspace_root.join("crates")).expect("read workspace crates") {
        collect_rust_sources(
            &entry.expect("read workspace crate").path().join("src"),
            &mut sources,
        );
    }
    sources.sort();

    let mut violations = Vec::new();
    for path in sources {
        let source = fs::read_to_string(&path).expect("read Rust source");
        let policy = event_origin_policy(&source);
        for constructor in policy.calls {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: RenderEventEnvelope::{constructor}",
                relative.display()
            ));
        }
        for assignment in policy.assignments {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: direct {assignment} assignment",
                relative.display()
            ));
        }
        for violation in policy.constructor_surface_violations {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!("{}: {violation}", relative.display()));
        }
        for producer in policy.compatibility_producer_constructions {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: direct {producer} construction",
                relative.display()
            ));
        }
        for conversion in policy.provenance_authority_conversion_violations {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: direct {conversion} conversion",
                relative.display()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production code must use typed event-origin constructors:\n{}",
        violations.join("\n")
    );
}
