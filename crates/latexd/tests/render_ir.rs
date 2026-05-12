use std::{env, fs, path::Path};

use camino::Utf8PathBuf;
use latexd::compiler::capture_internal_render_ir;
use tex_aux::SemanticAux;
use tex_render_model::DrawOp;
use tex_render_model::{InlineNode, IrBlock, ProvenanceSpan, SourceSpanRole};

#[test]
fn compact_render_ir_capture_matches_goldens() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());

    let event_json = serde_json::to_string_pretty(&capture.events).expect("event json");
    let ir_json = serde_json::to_string_pretty(&capture.document_ir).expect("ir json");
    let display_list_json =
        serde_json::to_string_pretty(&capture.page_display_lists).expect("display list json");

    assert_or_update_golden("tests/goldens/render_ir/compact.events.json", &event_json);
    assert_or_update_golden("tests/goldens/render_ir/compact.ir.json", &ir_json);
    assert_or_update_golden(
        "tests/goldens/render_ir/compact.display-list.json",
        &display_list_json,
    );

    assert!(capture.document_ir.extracted_text().contains("A Paper"));
    assert!(
        capture
            .document_ir
            .extracted_text()
            .contains("Short abstract.")
    );
    assert!(capture.document_ir.extracted_text().contains("Intro"));
    assert!(
        capture
            .document_ir
            .extracted_text()
            .contains("Author. Title.")
    );
    assert!(!capture.document_ir.extracted_text().contains("key."));
    assert_eq!(capture.page_display_lists.len(), 1);
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run) if run.text == "A Paper" && run.glyphs.is_none()
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.starts_with("%PDF-1.4"));
    assert!(pdf_text.contains("(A Paper) Tj"));
}

#[test]
fn compact_title_ir_preserves_emit_and_metadata_provenance() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let title = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::TitleBlock(title) => Some(title),
            _ => None,
        })
        .expect("title block");

    assert!(matches!(
        &title.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "\\maketitle"
    ));
    assert!(title.source.related.iter().any(|related| {
        related.role == SourceSpanRole::MetadataDefinition
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "A Paper"
            )
    }));
}

#[test]
fn compact_ir_contains_expected_first_batch_structures() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());

    assert!(matches!(
        capture.document_ir.blocks.as_slice(),
        [
            IrBlock::TitleBlock(_),
            IrBlock::Abstract(_),
            IrBlock::Heading(_),
            IrBlock::Paragraph(_),
            IrBlock::DisplayMath(_),
            IrBlock::Bibliography(_),
            IrBlock::RawFallback(_)
        ]
    ));
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys.len() == 1
                    && citation.keys[0] == "key"
                    && citation.display_text == "[?]"
        )
    }));
}

#[test]
fn compact_render_ir_capture_writes_debug_artifacts() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let tempdir = tempfile::tempdir().expect("tempdir");
    let output_dir = Utf8PathBuf::from_path_buf(tempdir.path().join("render-artifacts"))
        .expect("utf8 temp path");

    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");

    assert!(
        fs::read_to_string(paths.legacy_output)
            .expect("legacy output")
            .contains("A Paper")
    );
    assert!(
        fs::read_to_string(paths.events)
            .expect("events json")
            .contains("\"schema_version\": 1")
    );
    assert!(
        fs::read_to_string(paths.document_ir)
            .expect("document ir json")
            .contains("\"kind\": \"title_block\"")
    );
    assert!(
        fs::read_to_string(paths.page_display_list)
            .expect("display list json")
            .contains("\"kind\": \"text_run\"")
    );
    assert_eq!(paths.display_list_svgs.len(), 1);
    assert!(
        fs::read_to_string(&paths.display_list_svgs[0])
            .expect("display list svg")
            .contains("data-source-path=\"main.tex\"")
    );
    assert!(
        fs::read(paths.display_list_pdf)
            .expect("display list pdf")
            .starts_with(b"%PDF-1.4")
    );
}

const COMPACT_SOURCE: &str = r"\title{A Paper}\author{Ada Lovelace}\date{May 1843}\begin{document}\maketitle\begin{abstract}Short abstract.\end{abstract}\section{Intro}Hello \cite{key}.\[x^2\]\begin{thebibliography}{1}\bibitem{key} Author. Title.\end{thebibliography}\begin{unknownenv}Fallback text.\end{unknownenv}\end{document}";

fn assert_or_update_golden(relative_path: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    if env::var_os("LATEXD_UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().expect("golden parent")).expect("create golden dir");
        fs::write(&path, format!("{actual}\n")).expect("write golden");
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read golden {}: {error}", path.display());
    });
    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "golden {relative_path}"
    );
}
