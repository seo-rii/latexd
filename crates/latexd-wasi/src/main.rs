use std::{collections::BTreeMap, fs, path::Path};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tex_render_model::{BrowserBuildMetadata, BrowserPagesArtifact, RenderEventStream};
use tex_tokens::ControlSequenceInterner;

const WORKSPACE: &str = "/workspace";

#[derive(Debug, Deserialize)]
struct CompileRequest {
    #[serde(default)]
    revision: u64,
    entry: String,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CompileResponse {
    schema_version: u32,
    success: bool,
    event_count: usize,
    page_count: usize,
    extracted_text: String,
    diagnostics: Vec<String>,
    error: Option<String>,
}

fn main() {
    let response = compile().unwrap_or_else(|error| CompileResponse {
        schema_version: 1,
        success: false,
        event_count: 0,
        page_count: 0,
        extracted_text: String::new(),
        diagnostics: Vec::new(),
        error: Some(error),
    });
    let bytes = serde_json::to_vec(&response).expect("compile response should serialize");
    fs::write(Path::new(WORKSPACE).join("output.json"), bytes)
        .expect("WASI memfs should accept output.json");
}

fn explicit_error_message(diagnostics: &[tex_vm::VmDiagnostic]) -> Option<String> {
    diagnostics
        .iter()
        .find(|diagnostic| matches!(&diagnostic.kind, tex_vm::VmDiagnosticKind::ExplicitError))
        .map(|diagnostic| format!("{:?}: {}", diagnostic.kind, diagnostic.detail))
}

fn compile() -> Result<CompileResponse, String> {
    let request: CompileRequest = serde_json::from_slice(
        &fs::read(Path::new(WORKSPACE).join("request.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let entry = normalize_path(&request.entry)?;
    let mut sources = BTreeMap::new();
    for file in request.files {
        let path = normalize_path(&file)?;
        if !is_tex_source(&path) {
            continue;
        }
        let source = fs::read_to_string(Path::new(WORKSPACE).join(&path))
            .map_err(|error| format!("failed to read {path}: {error}"))?;
        sources.insert(path, source);
    }
    let source = sources
        .get(&entry)
        .ok_or_else(|| format!("entry source `{entry}` is missing"))?;

    let mut interner = ControlSequenceInterner::new();
    let mut vm = tex_vm::Vm::new(&mut interner);
    vm.set_file_root(WORKSPACE);
    vm.set_entry_source_path(entry.clone());
    for (path, mounted_source) in &sources {
        if path != &entry {
            vm.mount_file(path, mounted_source);
        }
    }
    vm.enable_render_event_capture();
    vm.enable_structured_table_events();
    let outcome = vm.run_plain(source);
    let explicit_error = explicit_error_message(&outcome.diagnostics);
    let diagnostics = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| format!("{:?}: {}", diagnostic.kind, diagnostic.detail))
        .collect();
    if let Some(error) = explicit_error {
        return Ok(CompileResponse {
            schema_version: 1,
            success: false,
            event_count: 0,
            page_count: 0,
            extracted_text: String::new(),
            diagnostics,
            error: Some(error),
        });
    }
    let stream = RenderEventStream::new(Some(entry), outcome.render_events);
    let event_count = stream.events.len();
    let document = tex_layout::build_document_ir(&stream, &());
    let extracted_text = document.extracted_text();
    let pages = tex_layout::build_page_display_lists(
        &document,
        tex_layout::PageDisplayListOptions::for_document_ir(&document),
    );
    let pages_artifact = BrowserPagesArtifact::one_shot(request.revision, pages);
    let build_metadata = BrowserBuildMetadata::one_shot(
        request.revision,
        event_count as u64,
        diagnostics.len() as u64,
        &pages_artifact,
    );
    let pdf = tex_pdf::render_display_list_pdf_with_assets(&pages_artifact.pages, |asset_ref| {
        let path = normalize_path(asset_ref).ok()?;
        fs::read(Path::new(WORKSPACE).join(path)).ok()
    });
    let pages_json = serde_json::to_vec(&pages_artifact)
        .map_err(|error| format!("failed to serialize pages.json: {error}"))?;
    let build_meta_json = serde_json::to_vec(&build_metadata)
        .map_err(|error| format!("failed to serialize build-meta.json: {error}"))?;
    fs::write(Path::new(WORKSPACE).join("output.pdf"), pdf).map_err(|error| error.to_string())?;
    fs::write(Path::new(WORKSPACE).join("pages.json"), pages_json)
        .map_err(|error| error.to_string())?;
    fs::write(
        Path::new(WORKSPACE).join("build-meta.json"),
        build_meta_json,
    )
    .map_err(|error| error.to_string())?;
    Ok(CompileResponse {
        schema_version: 1,
        success: true,
        event_count,
        page_count: pages_artifact.pages.len(),
        extracted_text,
        diagnostics,
        error: None,
    })
}

fn normalize_path(path: &str) -> Result<String, String> {
    let path = Utf8PathBuf::from(path.replace('\\', "/"));
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                camino::Utf8Component::ParentDir | camino::Utf8Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe project path `{path}`"));
    }
    Ok(path.as_str().trim_start_matches("./").to_string())
}

fn is_tex_source(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|value| value.to_str()),
        Some("tex" | "sty" | "cls" | "cfg" | "def" | "bbl")
    )
}

#[cfg(test)]
mod tests {
    use tex_tokens::ControlSequenceInterner;
    use tex_vm::Vm;

    use super::{explicit_error_message, normalize_path};

    #[test]
    fn project_paths_are_relative_and_sandboxed() {
        assert_eq!(
            normalize_path("./sections/intro.tex").unwrap(),
            "sections/intro.tex"
        );
        assert!(normalize_path("../secret.tex").is_err());
        assert!(normalize_path("/etc/passwd").is_err());
    }

    #[test]
    fn explicit_tex_errors_fail_the_browser_build() {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        let outcome = vm.run_plain(r"\errmessage{browser failure}");

        assert_eq!(
            explicit_error_message(&outcome.diagnostics).as_deref(),
            Some("ExplicitError: errmessage: browser failure")
        );
    }

    #[test]
    fn recoverable_vm_diagnostics_keep_the_browser_build_available() {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        let outcome = vm.run_plain(r"\undefinedcommand");

        assert!(explicit_error_message(&outcome.diagnostics).is_none());
    }
}
