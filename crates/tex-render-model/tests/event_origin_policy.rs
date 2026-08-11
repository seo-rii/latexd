use std::{
    fs,
    path::{Path, PathBuf},
};

use syn::{
    Expr, ExprCall, ImplItemFn, ItemFn, ItemImpl, ItemMod,
    visit::{self, Visit},
};

#[derive(Default)]
struct LegacyConstructorVisitor {
    calls: Vec<String>,
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
            visit::visit_item_impl(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if !is_cfg_test(&item.attrs) {
            visit::visit_impl_item_fn(self, item);
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
                self.calls.push(constructor.expect("matched constructor"));
            }
        }
        visit::visit_expr_call(self, call);
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
fn emit() {
    RenderEventEnvelope::new(sequence, event, source);
    tex_render_model::RenderEventEnvelope::with_origin(
        sequence,
        event,
        source,
        producer,
        confidence,
    );
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
    }

    assert!(
        violations.is_empty(),
        "production code must use typed event-origin constructors:\n{}",
        violations.join("\n")
    );
}
