use serde::Serialize;
use tex_layout::{LayoutOptions, build_document_ir, layout_text};
use tex_render_model::RenderEventStream;
use tex_tokens::ControlSequenceInterner;
use wasm_bindgen::prelude::*;

#[derive(Debug, Serialize)]
struct BrowserCompileResult {
    schema_version: u32,
    extracted_text: String,
    event_count: usize,
    pages: Vec<BrowserPage>,
    diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BrowserPage {
    page_id: String,
    width_pt: f32,
    height_pt: f32,
    lines: Vec<String>,
}

#[wasm_bindgen]
pub fn compile_source(source: &str) -> Result<String, JsValue> {
    compile_source_json(source).map_err(|error| JsValue::from_str(&error))
}

fn compile_source_json(source: &str) -> Result<String, String> {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = tex_vm::Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let stream = RenderEventStream::new(Some("main.tex".to_string()), outcome.render_events);
    let event_count = stream.events.len();
    let document = build_document_ir(&stream, &());
    let extracted_text = document.extracted_text();
    let layout = layout_text(&extracted_text, LayoutOptions::default());
    let pages = layout
        .pages
        .into_iter()
        .map(|page| BrowserPage {
            page_id: page.page_id,
            width_pt: page.width_pt,
            height_pt: page.height_pt,
            lines: page.lines,
        })
        .collect();
    let diagnostics = outcome
        .diagnostics
        .into_iter()
        .map(|diagnostic| format!("{:?}: {}", diagnostic.kind, diagnostic.detail))
        .collect();

    serde_json::to_string(&BrowserCompileResult {
        schema_version: 1,
        extracted_text,
        event_count,
        pages,
        diagnostics,
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::compile_source_json;

    #[test]
    fn compiles_a_document_into_browser_pages() {
        let json = compile_source_json(
            r"\documentclass{article}
\title{WASM Paper}
\author{Ada Lovelace}
\begin{document}
\maketitle
\section{Introduction}
Hello from the browser.
\end{document}",
        )
        .expect("browser compilation should succeed");
        let result: Value = serde_json::from_str(&json).expect("result should be JSON");

        assert_eq!(result["schema_version"], 1);
        assert!(result["event_count"].as_u64().unwrap_or_default() > 0);
        assert!(
            result["extracted_text"]
                .as_str()
                .unwrap()
                .contains("WASM Paper")
        );
        assert!(!result["pages"].as_array().unwrap().is_empty());
    }
}
