use std::{collections::BTreeMap, fs, path::Path};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tex_render_model::RenderEventStream;
use tex_tokens::ControlSequenceInterner;

const WORKSPACE: &str = "/workspace";

#[derive(Debug, Deserialize)]
struct CompileRequest {
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
    let outcome = vm.run_plain(source);
    let stream = RenderEventStream::new(Some(entry), outcome.render_events);
    let event_count = stream.events.len();
    let document = tex_layout::build_document_ir(&stream, &());
    let extracted_text = document.extracted_text();
    let pages = tex_layout::build_page_display_lists(
        &document,
        tex_layout::PageDisplayListOptions::for_document_ir(&document),
    );
    let pdf = tex_pdf::render_display_list_pdf_with_assets(&pages, |asset_ref| {
        let path = normalize_path(asset_ref).ok()?;
        fs::read(Path::new(WORKSPACE).join(path)).ok()
    });
    fs::write(Path::new(WORKSPACE).join("output.pdf"), pdf).map_err(|error| error.to_string())?;
    let diagnostics = outcome
        .diagnostics
        .into_iter()
        .map(|diagnostic| format!("{:?}: {}", diagnostic.kind, diagnostic.detail))
        .collect();

    Ok(CompileResponse {
        schema_version: 1,
        success: true,
        event_count,
        page_count: pages.len(),
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
    use super::normalize_path;

    #[test]
    fn project_paths_are_relative_and_sandboxed() {
        assert_eq!(
            normalize_path("./sections/intro.tex").unwrap(),
            "sections/intro.tex"
        );
        assert!(normalize_path("../secret.tex").is_err());
        assert!(normalize_path("/etc/passwd").is_err());
    }
}
