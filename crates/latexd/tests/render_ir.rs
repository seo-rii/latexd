use latexd::compiler::capture_internal_render_ir;
use tex_aux::SemanticAux;

#[test]
fn compact_render_ir_capture_has_reviewable_json_artifacts() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\title{A Paper}\author{Ada Lovelace}\begin{document}\maketitle\section{Intro}Hello \cite{key}.\[\alpha\]\begin{thebibliography}{1}\bibitem{key} Author. Title.\end{thebibliography}\end{document}",
        &SemanticAux::default(),
    );

    let event_json = serde_json::to_string_pretty(&capture.events).expect("event json");
    let ir_json = serde_json::to_string_pretty(&capture.document_ir).expect("ir json");

    assert!(event_json.contains("\"schema_version\": 1"));
    assert!(event_json.contains("\"kind\": \"inline_citation\""));
    assert!(ir_json.contains("\"kind\": \"title_block\""));
    assert!(ir_json.contains("\"display_text\": \"[?]\""));
    assert!(capture.document_ir.extracted_text().contains("A Paper"));
    assert!(
        capture
            .document_ir
            .extracted_text()
            .contains("Author. Title.")
    );
    assert!(!capture.document_ir.extracted_text().contains("key."));
}
