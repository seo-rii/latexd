use std::{
    fs,
    path::{Path, PathBuf},
};

use syn::{
    Expr, ExprAssign, ExprCall, ExprField, ImplItemFn, ItemFn, ItemImpl, ItemMod, Member,
    visit::{self, Visit},
};

#[derive(Default)]
struct LegacyConstructorVisitor {
    calls: Vec<String>,
    assignments: Vec<String>,
    constructor_surface_violations: Vec<String>,
    in_render_event_envelope_impl: bool,
    current_impl_function: Option<String>,
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
    let syntax = syn::parse_file(source).expect("test input must be valid Rust syntax");
    let mut visitor = LegacyConstructorVisitor::default();
    visitor.visit_file(&syntax);
    visitor.calls
}

fn direct_origin_metadata_assignments(source: &str) -> Vec<String> {
    let syntax = syn::parse_file(source).expect("test input must be valid Rust syntax");
    let mut visitor = LegacyConstructorVisitor::default();
    visitor.visit_file(&syntax);
    visitor.assignments
}

fn envelope_constructor_surface_violations(source: &str) -> Vec<String> {
    let syntax = syn::parse_file(source).expect("test input must be valid Rust syntax");
    let mut visitor = LegacyConstructorVisitor::default();
    visitor.visit_file(&syntax);
    visitor.constructor_surface_violations
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
}

#[cfg(test)]
mod tests {
    fn fixture() {
        RenderEventEnvelope::new(sequence, event, source);
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
        for constructor in legacy_constructor_calls(&source) {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: RenderEventEnvelope::{constructor}",
                relative.display()
            ));
        }
        for assignment in direct_origin_metadata_assignments(&source) {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!(
                "{}: direct {assignment} assignment",
                relative.display()
            ));
        }
        for violation in envelope_constructor_surface_violations(&source) {
            let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
            violations.push(format!("{}: {violation}", relative.display()));
        }
    }

    assert!(
        violations.is_empty(),
        "production code must use typed event-origin constructors:\n{}",
        violations.join("\n")
    );
}
