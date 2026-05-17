use std::{env, fs, path::Path};

use camino::Utf8PathBuf;
use latexd::compiler::capture_internal_render_ir;
use tex_aux::{BibliographyEntry, SemanticAux, SemanticLabel};
use tex_render_model::{CitationStyleHint, DrawOp, ListKind, to_pretty_json};
use tex_render_model::{InlineNode, IrBlock, ProvenanceSpan, SourceSpanRole};

#[test]
fn compact_render_ir_capture_matches_goldens() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());

    let event_json = to_pretty_json(&capture.events).expect("event json");
    let ir_json = to_pretty_json(&capture.document_ir).expect("ir json");
    let display_list_json = to_pretty_json(&capture.page_display_lists).expect("display list json");

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
    assert!(matches!(
        title.title_source.as_ref().map(|source| &source.primary),
        Some(ProvenanceSpan::File(span))
            if span.path.as_str() == "main.tex"
                && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "A Paper"
    ));
    assert!(
        title
            .title_source
            .as_ref()
            .is_some_and(|source| source.related.iter().any(|related| {
                related.role == SourceSpanRole::EmitSite
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "\\maketitle"
                    )
            }))
    );
    assert!(matches!(
        title.author_sources.first().map(|source| &source.primary),
        Some(ProvenanceSpan::File(span))
            if span.path.as_str() == "main.tex"
                && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                    == "Ada Lovelace"
    ));
    assert!(matches!(
        title.date_source.as_ref().map(|source| &source.primary),
        Some(ProvenanceSpan::File(span))
            if span.path.as_str() == "main.tex"
                && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "May 1843"
    ));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "A Paper"
                    && matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "A Paper"
                    )
        )
    }));
}

#[test]
fn title_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", TITLE_INLINE_KEY_SOURCE, &SemanticAux::default());
    let title = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::TitleBlock(title) => Some(title),
            _ => None,
        })
        .expect("title block");

    assert_eq!(title.title.as_deref(), Some("See [?] and [?]."));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("See [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
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
fn bibliography_item_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        BIBLIOGRAPHY_ITEM_INLINE_KEY_SOURCE,
        &SemanticAux::default(),
    );
    let bibliography = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Bibliography(bibliography) => Some(bibliography),
            _ => None,
        })
        .expect("bibliography block");

    assert_eq!(bibliography.items[0].content, "See [?] and [?].");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("cited"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("See [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("cited"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
}

#[test]
fn graphic_render_ir_capture_derives_display_list_image() {
    let capture = capture_internal_render_ir("main.tex", GRAPHIC_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.pdf"
                    && graphic.caption.as_deref() == Some("Plot caption.")
                    && graphic.caption_source.is_some()
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf"
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run) if run.text == "Plot caption."
                && matches!(
                    &run.source.primary,
                    ProvenanceSpan::File(span)
                        if &GRAPHIC_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == "Plot caption."
                )
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.pdf]"));
}

#[test]
fn float_label_definitions_survive_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        FIGURE_TABLE_LABEL_SOURCE,
        &SemanticAux::default(),
    );
    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();

    assert!(label_keys.contains(&"fig:plot"));
    assert!(label_keys.contains(&"tab:data"));
    assert!(capture.document_ir.labels.iter().any(|label| {
        matches!(
            &label.source.primary,
            ProvenanceSpan::File(span)
                if &FIGURE_TABLE_LABEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                    == "fig:plot"
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Plot caption."));
    assert!(extracted_text.contains("Table caption."));
    assert!(!extracted_text.contains("fig:plot"));
    assert!(!extracted_text.contains("tab:data"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Plot caption."));
    assert!(display_list_text.contains("Table caption."));
    assert!(!display_list_text.contains("fig:plot"));
    assert!(!display_list_text.contains("tab:data"));
}

#[test]
fn caption_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CAPTION_INLINE_KEY_SOURCE,
        &SemanticAux::default(),
    );
    let extracted_text = capture.document_ir.extracted_text();

    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("See [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
}

#[test]
fn caption_href_targets_are_hidden_in_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", CAPTION_HREF_SOURCE, &SemanticAux::default());
    let extracted_text = capture.document_ir.extracted_text();

    assert!(
        extracted_text.contains("Read paper and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains(r"\href"));
    assert!(!extracted_text.contains("key"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Read paper and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains(r"\href"));
    assert!(!display_list_text.contains("key"));
}

#[test]
fn caption_url_like_wrappers_are_normalized_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CAPTION_URL_LIKE_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let expected =
        r"Visit https://shown.test/path at /tmp/archive via https://visible.test/raw and \foo+*.";
    let extracted_text = capture.document_ir.extracted_text();

    assert!(extracted_text.contains(expected), "{extracted_text}");
    assert!(!extracted_text.contains('|'));
    assert!(!extracted_text.contains(r"\url"));
    assert!(!extracted_text.contains(r"\path"));
    assert!(!extracted_text.contains(r"\nolinkurl"));
    assert!(!extracted_text.contains(r"\detokenize"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(expected), "{display_list_text}");
    assert!(!display_list_text.contains('|'));
    assert!(!display_list_text.contains(r"\url"));
    assert!(!display_list_text.contains(r"\path"));
    assert!(!display_list_text.contains(r"\nolinkurl"));
    assert!(!display_list_text.contains(r"\detokenize"));
}

#[test]
fn inline_math_capture_survives_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", INLINE_MATH_SOURCE, &SemanticAux::default());
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
            InlineNode::InlineMath { raw_source, source, .. }
                if raw_source == "x^2 + y^2"
                    && matches!(
                        &source.primary,
                        ProvenanceSpan::File(span)
                            if &INLINE_MATH_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "x^2 + y^2"
                    )
        )
    }));
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("Area"));
    assert!(display_list_text.contains("x^2 + y^2"));
}

#[test]
fn dollar_math_capture_survives_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", DOLLAR_MATH_SOURCE, &SemanticAux::default());
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
            InlineNode::InlineMath { raw_source, source, .. }
                if raw_source == "x^2 + y^2"
                    && matches!(
                        &source.primary,
                        ProvenanceSpan::File(span)
                            if &DOLLAR_MATH_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "x^2 + y^2"
                    )
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display)
                if display.raw_source == "z^2"
                    && matches!(
                        &display.source.primary,
                        ProvenanceSpan::File(span)
                            if &DOLLAR_MATH_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "z^2"
                    )
        )
    }));
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("x^2 + y^2"));
    assert!(display_list_text.contains("z^2"));
}

#[test]
fn math_environment_capture_survives_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", MATH_ENVIRONMENT_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display)
                if display.raw_source == r"\frac{a}{b}"
                    && matches!(
                        &display.source.primary,
                        ProvenanceSpan::File(span)
                            if &MATH_ENVIRONMENT_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\frac{a}{b}"
                    )
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("equation")
        )
    }));
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains(r"\frac{a}{b}"));
}

#[test]
fn heading_level_capture_survives_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", HEADING_LEVEL_SOURCE, &SemanticAux::default());
    let headings = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Heading(heading) => Some(heading),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(headings.len(), 4);
    assert_eq!(headings[0].level, 1);
    assert!(matches!(
        &headings[0].content[0],
        InlineNode::Text { text, .. } if text == "Long Section"
    ));
    assert!(matches!(
        &headings[0].source.primary,
        ProvenanceSpan::File(span)
            if &HEADING_LEVEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                == "Long Section"
    ));
    assert_eq!(headings[1].level, 2);
    assert!(matches!(
        &headings[1].content[0],
        InlineNode::Text { text, .. } if text == "Methods"
    ));
    assert_eq!(headings[2].level, 3);
    assert!(matches!(
        &headings[2].content[0],
        InlineNode::Text { text, .. } if text == "Details"
    ));
    assert_eq!(headings[3].level, 4);
    assert!(matches!(
        &headings[3].content[0],
        InlineNode::Text { text, .. } if text == "Sketch"
    ));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("Long Section"));
    assert!(display_list_text.contains("Methods"));
    assert!(display_list_text.contains("Details"));
    assert!(display_list_text.contains("Sketch"));
}

#[test]
fn heading_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        HEADING_INLINE_KEY_SOURCE,
        &SemanticAux::default(),
    );
    let heading = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Heading(heading) => Some(heading),
            _ => None,
        })
        .expect("heading");

    assert!(matches!(
        &heading.content[0],
        InlineNode::Text { text, .. } if text == "See [?] and [?]."
    ));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("See [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
}

#[test]
fn citation_variant_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CITATION_VARIANTS_SOURCE,
        &SemanticAux::default(),
    );
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let citations = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Citation(citation) => Some(citation),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(citations.len(), 4);
    assert_eq!(
        citations[0].keys,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(citations[0].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[0].display_text, "[?]");
    assert!(matches!(
        &citations[0].source.primary,
        ProvenanceSpan::File(span)
            if &CITATION_VARIANTS_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                == "alpha,beta"
    ));
    assert_eq!(citations[1].keys, vec!["gamma".to_string()]);
    assert_eq!(citations[1].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[2].keys, vec!["delta".to_string()]);
    assert_eq!(citations[2].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[3].keys, vec!["epsilon".to_string()]);
    assert_eq!(citations[3].style_hint, CitationStyleHint::Textual);

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("[?]"));
    assert!(!display_list_text.contains("alpha"));
    assert!(!display_list_text.contains("epsilon"));
}

#[test]
fn reference_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir("main.tex", REFERENCE_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 3);
    assert_eq!(references[0].command, "ref");
    assert_eq!(references[0].keys, vec!["sec:intro".to_string()]);
    assert_eq!(references[0].display_text, "[?]");
    assert!(matches!(
        &references[0].source.primary,
        ProvenanceSpan::File(span)
            if &REFERENCE_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                == "sec:intro"
    ));
    assert_eq!(references[1].command, "eqref");
    assert_eq!(references[1].keys, vec!["eq:main".to_string()]);
    assert_eq!(references[1].display_text, "(?)");
    assert_eq!(references[2].command, "cref");
    assert_eq!(
        references[2].keys,
        vec!["fig:a".to_string(), "tab:b".to_string()]
    );

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("[?]"));
    assert!(display_list_text.contains("(?)"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("eq:main"));
}

#[test]
fn starred_reference_capture_survives_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        STARRED_REFERENCE_SOURCE,
        &SemanticAux::default(),
    );
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 3);
    assert_eq!(references[0].command, "ref");
    assert_eq!(references[0].keys, vec!["sec:intro".to_string()]);
    assert_eq!(references[1].command, "autoref");
    assert_eq!(references[1].keys, vec!["fig:plot".to_string()]);
    assert_eq!(references[2].command, "Cref");
    assert_eq!(references[2].keys, vec!["tab:data".to_string()]);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [?], [?], and [?]."));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("fig:plot"));
    assert!(!extracted_text.contains("tab:data"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("See [?], [?], and [?]."));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("fig:plot"));
    assert!(!display_list_text.contains("tab:data"));
}

#[test]
fn reference_alias_capture_survives_ir_without_visible_keys() {
    let capture =
        capture_internal_render_ir("main.tex", REFERENCE_ALIAS_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 7);
    assert_eq!(references[0].command, "subref");
    assert_eq!(references[0].keys, vec!["sub:a".to_string()]);
    assert_eq!(references[1].command, "vref");
    assert_eq!(references[1].keys, vec!["sec:intro".to_string()]);
    assert_eq!(references[2].command, "Vref");
    assert_eq!(references[2].keys, vec!["chap:main".to_string()]);
    assert_eq!(references[3].command, "vpageref");
    assert_eq!(references[3].keys, vec!["page:two".to_string()]);
    assert_eq!(references[4].command, "fullref");
    assert_eq!(references[4].keys, vec!["sec:full".to_string()]);
    assert_eq!(references[5].command, "namecref");
    assert_eq!(references[5].keys, vec!["thm:one".to_string()]);
    assert_eq!(references[6].command, "labelcref");
    assert_eq!(references[6].keys, vec!["item:x".to_string()]);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [?], [?], [?], [?], [?], [?], and [?]."));
    assert!(!extracted_text.contains("sub:a"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("chap:main"));
    assert!(!extracted_text.contains("page:two"));
    assert!(!extracted_text.contains("sec:full"));
    assert!(!extracted_text.contains("thm:one"));
    assert!(!extracted_text.contains("item:x"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("See [?], [?], [?], [?], [?], [?], and [?]."));
    assert!(!display_list_text.contains("sub:a"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("chap:main"));
    assert!(!display_list_text.contains("page:two"));
    assert!(!display_list_text.contains("sec:full"));
    assert!(!display_list_text.contains("thm:one"));
    assert!(!display_list_text.contains("item:x"));
}

#[test]
fn reference_page_name_alias_capture_survives_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        REFERENCE_PAGE_NAME_ALIAS_SOURCE,
        &SemanticAux::default(),
    );
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();

    let expected = [
        ("cpageref", vec!["page:intro"]),
        ("Cpageref", vec!["sub:scope"]),
        ("autopageref", vec!["sec:auto"]),
        ("labelcpageref", vec!["eq:main"]),
        ("Fullref", vec!["sec:full"]),
        ("titleref", vec!["sec:title"]),
        ("Titleref", vec!["chap:title"]),
        ("nameCref", vec!["thm:upper"]),
        ("lcnamecref", vec!["sub:lower"]),
        ("namecrefs", vec!["thm:a", "thm:b"]),
        ("nameCrefs", vec!["lem:a", "lem:b"]),
        ("lcnamecrefs", vec!["def:a", "def:b"]),
    ];
    assert_eq!(references.len(), expected.len());
    for (reference, (command, keys)) in references.iter().zip(expected.iter()) {
        assert_eq!(reference.command, *command);
        assert_eq!(
            reference.keys,
            keys.iter().map(|key| key.to_string()).collect::<Vec<_>>()
        );
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert_eq!(extracted_text.matches("[?]").count(), expected.len());
    for label in [
        "page:intro",
        "sub:scope",
        "sec:auto",
        "eq:main",
        "sec:full",
        "sec:title",
        "chap:title",
        "thm:upper",
        "sub:lower",
        "thm:a",
        "thm:b",
        "lem:a",
        "lem:b",
        "def:a",
        "def:b",
    ] {
        assert!(!extracted_text.contains(label));
    }

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(display_list_text.matches("[?]").count(), expected.len());
    for label in [
        "page:intro",
        "sub:scope",
        "sec:auto",
        "eq:main",
        "sec:full",
        "sec:title",
        "chap:title",
        "thm:upper",
        "sub:lower",
        "thm:a",
        "thm:b",
        "lem:a",
        "lem:b",
        "def:a",
        "def:b",
    ] {
        assert!(!display_list_text.contains(label));
    }
}

#[test]
fn reference_range_capture_survives_ir_without_visible_keys() {
    let capture =
        capture_internal_render_ir("main.tex", REFERENCE_RANGE_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 4);
    assert_eq!(references[0].command, "crefrange");
    assert_eq!(
        references[0].keys,
        vec!["fig:a".to_string(), "fig:b".to_string()]
    );
    assert_eq!(references[1].command, "Crefrange");
    assert_eq!(
        references[1].keys,
        vec!["sec:a".to_string(), "sec:b".to_string()]
    );
    assert_eq!(references[2].command, "cpagerefrange");
    assert_eq!(
        references[2].keys,
        vec!["p:a".to_string(), "p:b".to_string()]
    );
    assert_eq!(references[3].command, "Cpagerefrange");
    assert_eq!(
        references[3].keys,
        vec!["app:a".to_string(), "app:b".to_string()]
    );

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [?], [?], [?], and [?]."));
    assert!(!extracted_text.contains("fig:a"));
    assert!(!extracted_text.contains("fig:b"));
    assert!(!extracted_text.contains("sec:a"));
    assert!(!extracted_text.contains("sec:b"));
    assert!(!extracted_text.contains("p:a"));
    assert!(!extracted_text.contains("p:b"));
    assert!(!extracted_text.contains("app:a"));
    assert!(!extracted_text.contains("app:b"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("See [?], [?], [?], and [?]."));
    assert!(!display_list_text.contains("fig:a"));
    assert!(!display_list_text.contains("fig:b"));
    assert!(!display_list_text.contains("sec:a"));
    assert!(!display_list_text.contains("sec:b"));
    assert!(!display_list_text.contains("p:a"));
    assert!(!display_list_text.contains("p:b"));
    assert!(!display_list_text.contains("app:a"));
    assert!(!display_list_text.contains("app:b"));
}

#[test]
fn link_capture_survives_ir_and_display_list_annotations() {
    let capture = capture_internal_render_ir("main.tex", LINK_SOURCE, &SemanticAux::default());
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
            InlineNode::Link(link)
                if link.target == "https://example.test/paper"
                    && link.display_text == "paper link"
                    && matches!(
                        &link.source.primary,
                        ProvenanceSpan::File(span)
                            if &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "paper link"
                    )
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://example.test/raw"
                    && link.display_text == "https://example.test/raw"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://example.test/delimited"
                    && link.display_text == "https://example.test/delimited"
                    && matches!(
                        &link.source.primary,
                        ProvenanceSpan::File(span)
                            if &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "https://example.test/delimited"
                    )
        )
    }));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(
        "Read paper link, https://example.test/raw, and https://example.test/delimited."
    ));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://example.test/paper" && link.rect.width > 0.0
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://example.test/raw" && link.rect.width > 0.0
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://example.test/delimited" && link.rect.width > 0.0
        )
    }));
    assert!(!display_list_text.contains("https://example.test/paper"));
}

#[test]
fn link_text_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LINK_TEXT_INLINE_KEY_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Link(link)
                if link.target == "https://hidden.test"
                    && link.display_text == "see [?] and [?]"
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Read see [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("cited"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));
    assert!(!extracted_text.contains("https://hidden.test"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Read see [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("cited"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://hidden.test"
        )
    }));
}

#[test]
fn hyperref_visible_text_survives_ir_without_targets() {
    let capture = capture_internal_render_ir(
        "main.tex",
        HYPERREF_VISIBLE_TEXT_SOURCE,
        &SemanticAux::default(),
    );
    let expected = "Read intro, anchor text, and target text.";
    let extracted_text = capture.document_ir.extracted_text();

    assert!(extracted_text.contains(expected), "{extracted_text}");
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("hidden-anchor"));
    assert!(!extracted_text.contains("target-id"));
    assert!(!extracted_text.contains(r"\hyperref"));
    assert!(!extracted_text.contains(r"\hyperlink"));
    assert!(!extracted_text.contains(r"\hypertarget"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(expected), "{display_list_text}");
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("hidden-anchor"));
    assert!(!display_list_text.contains("target-id"));
    assert!(!display_list_text.contains(r"\hyperref"));
    assert!(!display_list_text.contains(r"\hyperlink"));
    assert!(!display_list_text.contains(r"\hypertarget"));
}

#[test]
fn url_text_wrapper_capture_survives_ir_without_link_annotations() {
    let capture =
        capture_internal_render_ir("main.tex", URL_TEXT_WRAPPER_SOURCE, &SemanticAux::default());
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
            InlineNode::Text { text, source }
                if text == "https://example.test/paper"
                    && matches!(
                        &source.primary,
                        ProvenanceSpan::File(span)
                            if &URL_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "https://example.test/paper"
                    )
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "/tmp/archive"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, source }
                if text == "https://example.test/delimited"
                    && matches!(
                        &source.primary,
                        ProvenanceSpan::File(span)
                            if &URL_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "https://example.test/delimited"
                    )
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "/var/tmp"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == r"\foo+*"
        )
    }));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(
        r"Use https://example.test/paper, https://example.test/delimited, at /tmp/archive and /var/tmp via \foo+*."
    ));
    assert!(
        !capture.page_display_lists[0]
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::LinkAnnotation(_)))
    );
}

#[test]
fn text_wrapper_capture_survives_ir_without_raw_braces() {
    let capture =
        capture_internal_render_ir("main.tex", TEXT_WRAPPER_SOURCE, &SemanticAux::default());
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
            InlineNode::Text { text, source }
                if text == "important"
                    && matches!(
                        &source.primary,
                        ProvenanceSpan::File(span)
                            if &TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "important"
                    )
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "bold text"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "code_path"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Styled important and bold text with code_path."));
    assert!(!extracted_text.contains("{important}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Styled important and bold text with code_path."));
    assert!(!display_list_text.contains("{important}"));
}

#[test]
fn nested_text_wrapper_capture_survives_ir_without_raw_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Text { text, .. } if text == "important"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation) if citation.keys == vec!["key".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested important [?] and [?] text."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("{important"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested important [?] and [?] text."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("{important"));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
}

#[test]
fn nested_text_wrapper_link_capture_survives_ir_without_hidden_targets() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_LINK_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Link(link)
                if link.target == "https://hidden.test" && link.display_text == "paper"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://shown.test"
                    && link.display_text == "https://shown.test"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested read paper and https://shown.test."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains(r"\href"));
    assert!(!extracted_text.contains("{paper}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested read paper and https://shown.test."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains(r"\href"));
    assert!(!display_list_text.contains("{paper}"));

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://hidden.test"
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://shown.test"
        )
    }));
}

#[test]
fn nested_text_wrapper_label_definition_survives_ir_without_visible_key() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_LABEL_SOURCE,
        &SemanticAux::default(),
    );

    assert_eq!(capture.document_ir.labels.len(), 1);
    assert_eq!(capture.document_ir.labels[0].key, "sec:intro");
    assert!(matches!(
        &capture.document_ir.labels[0].source.primary,
        ProvenanceSpan::File(span)
            if &NESTED_TEXT_WRAPPER_LABEL_SOURCE
                [span.start_utf8 as usize..span.end_utf8 as usize]
                == "sec:intro"
    ));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Nested Intro text."));
    assert!(!extracted_text.contains("label"));
    assert!(!extracted_text.contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Nested Intro text."));
    assert!(!display_list_text.contains("label"));
    assert!(!display_list_text.contains("sec:intro"));
}

#[test]
fn nested_text_wrapper_math_capture_survives_ir_without_raw_delimiters() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_MATH_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::InlineMath { raw_source, .. } if raw_source == "x^2"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::InlineMath { raw_source, .. } if raw_source == "y^2"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested area x^2 and y^2 text."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("$x^2$"));
    assert!(!extracted_text.contains(r"\(y^2\)"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested area x^2 and y^2 text."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("$x^2$"));
    assert!(!display_list_text.contains(r"\(y^2\)"));
}

#[test]
fn nested_text_wrapper_inside_wrapper_survives_ir_without_raw_braces() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested outer inner text done."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("{inner text}"));
    assert!(!extracted_text.contains("textbf"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested outer inner text done."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("{inner text}"));
    assert!(!display_list_text.contains("textbf"));
}

#[test]
fn nested_text_wrapper_unknown_command_survives_ir_without_raw_braces() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE,
        &SemanticAux::default(),
    );

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested before visible text after."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("{visible text}"));
    assert!(!extracted_text.contains("unknowntext"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested before visible text after."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("{visible text}"));
    assert!(!display_list_text.contains("unknowntext"));
}

#[test]
fn nested_text_wrapper_unknown_command_inline_events_survive_ir_without_raw_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Citation(citation) if citation.keys == vec!["key".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested before see [?] and [?] after."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("{see"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("Nested before see [?] and [?] after."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("{see"));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
}

#[test]
fn nested_text_wrapper_unknown_command_links_and_math_survive_ir_without_raw_syntax() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Link(link)
                if link.target == "https://hidden.test" && link.display_text == "paper"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://shown.test"
                    && link.display_text == "https://shown.test"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::InlineMath { raw_source, .. } if raw_source == "x^2"
        )
    }));

    let expected_text = "Nested before see paper, https://shown.test, and x^2 after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains("{paper}"));
    assert!(!extracted_text.contains("$x^2$"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains("{paper}"));
    assert!(!display_list_text.contains("$x^2$"));

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://hidden.test"
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://shown.test"
        )
    }));
}

#[test]
fn nested_text_wrapper_unknown_command_escaped_visible_chars_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_ESCAPED_VISIBLE_SOURCE,
        &SemanticAux::default(),
    );
    let expected_text = "Nested before 50% A&B costs $5_0 #1 {x} after.";

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains(r"\%"));
    assert!(!extracted_text.contains(r"\&"));
    assert!(!extracted_text.contains(r"\{x\}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains(r"\%"));
    assert!(!display_list_text.contains(r"\&"));
    assert!(!display_list_text.contains(r"\{x\}"));
}

#[test]
fn nested_text_wrapper_unknown_command_text_wrappers_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_TEXT_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let expected_text = "Nested before outer inner text done after.";

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("textbf"));
    assert!(!extracted_text.contains("{inner text}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("textbf"));
    assert!(!display_list_text.contains("{inner text}"));
}

#[test]
fn nested_text_wrapper_unknown_command_nested_unknown_commands_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_SOURCE,
        &SemanticAux::default(),
    );
    let expected_text = "Nested before outer inner text done after.";

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("{inner text}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("innerunknown"));
    assert!(!display_list_text.contains("{inner text}"));
}

#[test]
fn nested_text_wrapper_unknown_command_nested_unknown_inline_events_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_INLINE_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Citation(citation) if citation.keys == vec!["key".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));

    let expected_text = "Nested before outer see [?] and [?] done after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("innerunknown"));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("sec:intro"));
}

#[test]
fn nested_text_wrapper_unknown_command_nested_unknown_links_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE,
        &SemanticAux::default(),
    );
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
            InlineNode::Link(link)
                if link.target == "https://hidden.test" && link.display_text == "paper"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://shown.test"
                    && link.display_text == "https://shown.test"
        )
    }));

    let expected_text = "Nested before outer see paper and https://shown.test done after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains("{paper}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("innerunknown"));
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains("{paper}"));

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://hidden.test"
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://shown.test"
        )
    }));
}

#[test]
fn nested_text_wrapper_unknown_command_nested_unknown_url_text_wrappers_survive_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE,
        &SemanticAux::default(),
    );
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");

    assert!(
        !paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::Link(_)))
    );

    let expected_text =
        r"Nested before outer use https://visible.test/path, /tmp/archive, and \foo+* done after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("nolinkurl"));
    assert!(!extracted_text.contains("detokenize"));
    assert!(!extracted_text.contains("{https://visible.test/path}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("innerunknown"));
    assert!(!display_list_text.contains("nolinkurl"));
    assert!(!display_list_text.contains("detokenize"));
    assert!(!display_list_text.contains("{https://visible.test/path}"));
    assert!(
        !capture.page_display_lists[0]
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::LinkAnnotation(_)))
    );
}

#[test]
fn nested_text_wrapper_unknown_command_nested_unknown_label_survives_ir_without_visible_key() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LABEL_SOURCE,
        &SemanticAux::default(),
    );

    assert_eq!(capture.document_ir.labels.len(), 1);
    assert_eq!(capture.document_ir.labels[0].key, "sec:intro");
    assert!(matches!(
        &capture.document_ir.labels[0].source.primary,
        ProvenanceSpan::File(span)
            if &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LABEL_SOURCE
                [span.start_utf8 as usize..span.end_utf8 as usize]
                == "sec:intro"
    ));

    let expected_text = "Nested before outer Intro text done after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("label"));
    assert!(!extracted_text.contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("innerunknown"));
    assert!(!display_list_text.contains("label"));
    assert!(!display_list_text.contains("sec:intro"));
}

#[test]
fn escaped_visible_character_capture_survives_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", ESCAPED_VISIBLE_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");

    for expected in ["%", "&", "$", "_", "#", "{", "}"] {
        assert!(
            paragraph.content.iter().any(|node| {
                matches!(
                    node,
                    InlineNode::Text { text, .. } if text == expected
                )
            }),
            "missing escaped visible IR text node for {expected}"
        );
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("50% A&B costs $5_0 #1 {x} A B."));
    assert!(!extracted_text.contains(r"\%"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("50% A&B costs $5_0 #1 {x} A B."));
    assert!(!display_list_text.contains(r"\%"));
}

#[test]
fn nonbreaking_tilde_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NONBREAKING_TILDE_SOURCE,
        &SemanticAux::default(),
    );

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Figure 1 references Related Work."));
    assert!(extracted_text.contains("Related Work"));
    assert!(!extracted_text.contains('~'));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Figure 1 references Related Work."));
    assert!(display_list_text.contains("Related Work"));
    assert!(!display_list_text.contains('~'));
}

#[test]
fn linebreak_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir("main.tex", LINEBREAK_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");

    assert!(
        paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::LineBreak { .. })),
        "missing explicit line break IR node"
    );
    assert!(
        capture
            .document_ir
            .extracted_text()
            .contains("First line\nSecond line.")
    );

    let text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) if !run.text.is_empty() => Some(run),
            _ => None,
        })
        .collect::<Vec<_>>();
    let first_y = text_runs
        .iter()
        .find(|run| run.text == "First")
        .expect("first line run")
        .origin
        .y;
    let second_y = text_runs
        .iter()
        .find(|run| run.text == "Second")
        .expect("second line run")
        .origin
        .y;
    assert!(second_y > first_y);
}

#[test]
fn tabular_fallback_capture_uses_normalized_visible_text() {
    let capture =
        capture_internal_render_ir("main.tex", TABULAR_FALLBACK_SOURCE, &SemanticAux::default());
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tabular") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("tabular fallback");

    assert_eq!(
        fallback.normalized_visible_text.as_deref(),
        Some("Alpha | Beta ; Gamma | Delta")
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha | Beta ; Gamma | Delta"));
    assert!(!extracted_text.contains("&"));
    assert!(!extracted_text.contains("ll"));
    assert!(!extracted_text.contains("hline"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Alpha | Beta ; Gamma | Delta"));
    assert!(!display_list_text.contains("&"));
    assert!(!display_list_text.contains("hline"));
}

#[test]
fn raw_fallback_inline_keys_are_redacted_in_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        RAW_FALLBACK_INLINE_KEY_SOURCE,
        &SemanticAux::default(),
    );
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("unknownenv") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("unknownenv fallback");

    assert_eq!(
        fallback.normalized_visible_text.as_deref(),
        Some("See [?] and [?].")
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("cited"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains(r"\ref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        display_list_text.contains("See [?] and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("cited"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\ref"));
}

#[test]
fn list_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir("main.tex", LIST_SOURCE, &SemanticAux::default());
    let lists = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::List(list) => Some(list),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(lists.len(), 3);
    assert_eq!(lists[0].kind, ListKind::Unordered);
    assert_eq!(lists[0].items.len(), 2);
    assert_eq!(lists[0].items[0].marker, "-");
    assert!(lists[0].items[0].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert_eq!(lists[0].items[1].marker, "Custom");
    assert!(matches!(
        &lists[0].items[1].source.primary,
        ProvenanceSpan::File(span)
            if &LIST_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == r"\item[Custom]"
    ));
    assert_eq!(lists[1].kind, ListKind::Ordered);
    assert_eq!(lists[1].items.len(), 2);
    assert_eq!(lists[1].items[0].marker, "1.");
    assert_eq!(lists[1].items[1].marker, "2.");
    assert_eq!(lists[2].kind, ListKind::Description);
    assert_eq!(lists[2].items.len(), 2);
    assert_eq!(lists[2].items[0].marker, "Term");
    assert!(lists[2].items[0].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("- First [?]"));
    assert!(extracted_text.contains("Custom Second"));
    assert!(extracted_text.contains("1. One"));
    assert!(extracted_text.contains("Term Meaning [?]"));
    assert!(!extracted_text.contains("key"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("- First [?]"));
    assert!(display_list_text.contains("Custom Second"));
    assert!(display_list_text.contains("1. One"));
    assert!(display_list_text.contains("2. Two"));
    assert!(display_list_text.contains("Term Meaning [?]"));
}

#[test]
fn simple_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        SIMPLE_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environments = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(environments.len(), 2);
    assert_eq!(environments[0].name, "quote");
    assert!(environments[0].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(matches!(
        &environments[0].source.primary,
        ProvenanceSpan::File(span)
            if &SIMPLE_ENVIRONMENT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                == r"\begin{quote}"
    ));
    assert_eq!(environments[1].name, "center");
    assert!(environments[1].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Centered"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("quote" | "center"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Quoted [?]."));
    assert!(extracted_text.contains("Centered text."));
    assert!(!extracted_text.contains("key"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Quoted [?]."));
    assert!(display_list_text.contains("Centered text."));
    assert!(!display_list_text.contains("key"));
}

#[test]
fn aux_resolved_references_and_citations_survive_ir_and_display_list() {
    let mut aux = SemanticAux::default();
    aux.labels.push(SemanticLabel {
        key: "sec:intro".to_string(),
        number: "1".to_string(),
        page: 1,
        file: Utf8PathBuf::from("main.tex"),
        offset_utf8: 0,
    });
    aux.bibliography.push(BibliographyEntry {
        key: "key".to_string(),
        text: "Author. Title.".to_string(),
        label: Some("7".to_string()),
        file: Utf8PathBuf::from("refs.bbl"),
    });

    let capture = capture_internal_render_ir("main.tex", AUX_RESOLUTION_SOURCE, &aux);
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
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
                    && reference.resolved_target.as_deref() == Some("1")
                    && reference.display_text == "1"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()]
                    && citation.resolved_label.as_deref() == Some("[7]")
                    && citation.display_text == "[7]"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See 1 and [7]."));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("key."));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("See 1 and [7]."));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "1"
                    && matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if &AUX_RESOLUTION_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "sec:intro"
                    )
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "[7]"
                    && matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if &AUX_RESOLUTION_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                    )
        )
    }));
}

#[test]
fn label_definition_capture_survives_ir_without_visible_key() {
    let capture = capture_internal_render_ir("main.tex", LABEL_SOURCE, &SemanticAux::default());

    assert_eq!(capture.document_ir.labels.len(), 1);
    assert_eq!(capture.document_ir.labels[0].key, "sec:intro");
    assert!(matches!(
        &capture.document_ir.labels[0].source.primary,
        ProvenanceSpan::File(span)
            if &LABEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
    ));
    assert!(!capture.document_ir.extracted_text().contains("sec:intro"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("Intro"));
    assert!(display_list_text.contains("[?]"));
    assert!(!display_list_text.contains("sec:intro"));
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
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("display list svg");
    assert!(display_list_svg.contains("data-source-path=\"main.tex\""));
    assert!(display_list_svg.contains("data-source-related-roles=\""));
    assert!(display_list_svg.contains("data-source-related-spans=\""));
    assert!(display_list_svg.contains("emit_site"));
    assert!(
        fs::read(paths.display_list_pdf)
            .expect("display list pdf")
            .starts_with(b"%PDF-1.4")
    );
}

#[test]
fn macro_heading_display_list_svg_preserves_expansion_provenance() {
    let capture =
        capture_internal_render_ir("main.tex", MACRO_SECTION_SOURCE, &SemanticAux::default());
    let tempdir = tempfile::tempdir().expect("tempdir");
    let output_dir = Utf8PathBuf::from_path_buf(tempdir.path().join("render-artifacts"))
        .expect("utf8 temp path");

    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("display list svg");

    assert!(display_list_svg.contains(">Intro</text>"));
    assert!(display_list_svg.contains("data-source-expansion-depth=\"1\""));
    assert!(display_list_svg.contains("data-source-expansion-commands=\"mysection\""));
    assert!(display_list_svg.contains("data-source-expansion-calls=\"file:main.tex:"));
    assert!(display_list_svg.contains("data-source-expansion-definitions=\"file:main.tex:"));
}

const COMPACT_SOURCE: &str = r"\title{A Paper}\author{Ada Lovelace}\date{May 1843}\begin{document}\maketitle\begin{abstract}Short abstract.\end{abstract}\section{Intro}Hello \cite{key}.\[x^2\]\begin{thebibliography}{1}\bibitem{key} Author. Title.\end{thebibliography}\begin{unknownenv}Fallback text.\end{unknownenv}\end{document}";

const TITLE_INLINE_KEY_SOURCE: &str =
    r"\title{See \cite{key} and \ref{sec:intro}.}\begin{document}\maketitle\end{document}";

const BIBLIOGRAPHY_ITEM_INLINE_KEY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{entry} See \cite{cited} and \ref{sec:intro}.\end{thebibliography}\end{document}";

const RAW_FALLBACK_INLINE_KEY_SOURCE: &str = r"\begin{document}\begin{unknownenv}See \cite{cited} and \ref{sec:intro}.\end{unknownenv}\end{document}";

const GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\end{figure}\end{document}";

const FIGURE_TABLE_LABEL_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\label{fig:plot}\end{figure}\begin{table}\caption{Table caption.}\label{tab:data}\end{table}\end{document}";

const CAPTION_INLINE_KEY_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{See \cite{key} and \ref{sec:intro}.}\end{figure}\end{document}";

const CAPTION_HREF_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Read \href{https://hidden.test}{paper} and \cite{key}.}\end{figure}\end{document}";

const CAPTION_URL_LIKE_WRAPPER_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Visit \url|https://shown.test/path| at \path|/tmp/archive| via \nolinkurl|https://visible.test/raw| and \detokenize{\foo+*}.}\end{figure}\end{document}";

const INLINE_MATH_SOURCE: &str = r"\begin{document}Area \(x^2 + y^2\).\end{document}";

const DOLLAR_MATH_SOURCE: &str = r"\begin{document}Area $x^2 + y^2$.$$z^2$$\end{document}";

const MATH_ENVIRONMENT_SOURCE: &str =
    r"\begin{document}\begin{equation}\frac{a}{b}\end{equation}\end{document}";

const HEADING_LEVEL_SOURCE: &str = r"\begin{document}\section[Short]{Long Section}\subsection*{Methods}\subsubsection{Details}\paragraph{Sketch}\end{document}";

const HEADING_INLINE_KEY_SOURCE: &str =
    r"\begin{document}\section{See \cite{key} and \ref{sec:intro}.}\end{document}";

const CITATION_VARIANTS_SOURCE: &str = r"\begin{document}\citep[see][p.~3]{alpha,beta}\citet*{gamma}\parencite{delta}\textcite{epsilon}\end{document}";

const REFERENCE_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \eqref{eq:main}; \cref{fig:a,tab:b}.\end{document}";

const STARRED_REFERENCE_SOURCE: &str = r"\begin{document}See \ref*{sec:intro}, \autoref*{fig:plot}, and \Cref*{tab:data}.\end{document}";

const REFERENCE_ALIAS_SOURCE: &str = r"\begin{document}See \subref{sub:a}, \vref{sec:intro}, \Vref{chap:main}, \vpageref{page:two}, \fullref{sec:full}, \namecref{thm:one}, and \labelcref{item:x}.\end{document}";

const REFERENCE_PAGE_NAME_ALIAS_SOURCE: &str = r"\begin{document}See \cpageref{page:intro}, \Cpageref{sub:scope}, \autopageref{sec:auto}, \labelcpageref{eq:main}, \Fullref{sec:full}, \titleref{sec:title}, \Titleref{chap:title}, \nameCref{thm:upper}, \lcnamecref{sub:lower}, \namecrefs{thm:a,thm:b}, \nameCrefs{lem:a,lem:b}, and \lcnamecrefs{def:a,def:b}.\end{document}";

const REFERENCE_RANGE_SOURCE: &str = r"\begin{document}See \crefrange{fig:a}{fig:b}, \Crefrange{sec:a}{sec:b}, \cpagerefrange{p:a}{p:b}, and \Cpagerefrange{app:a}{app:b}.\end{document}";

const LINK_SOURCE: &str = r"\begin{document}Read \href{https://example.test/paper}{paper link}, \url{https://example.test/raw}, and \url|https://example.test/delimited|.\end{document}";

const LINK_TEXT_INLINE_KEY_SOURCE: &str = r"\begin{document}Read \href{https://hidden.test}{see \cite{cited} and \ref{sec:intro}}.\end{document}";

const HYPERREF_VISIBLE_TEXT_SOURCE: &str = r"\begin{document}Read \hyperref[sec:intro]{intro}, \hyperlink{hidden-anchor}{anchor text}, and \hypertarget{target-id}{target text}.\end{document}";

const URL_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Use \nolinkurl{https://example.test/paper}, \nolinkurl|https://example.test/delimited|, at \path{/tmp/archive} and \path|/var/tmp| via \detokenize{\foo+*}.\end{document}";

const TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Styled \emph{important} and \textbf{bold text} with \texttt{code_path}.\end{document}";

const NESTED_TEXT_WRAPPER_SOURCE: &str =
    r"\begin{document}Nested \emph{important \cite{key} and \ref{sec:intro}} text.\end{document}";

const NESTED_TEXT_WRAPPER_LINK_SOURCE: &str = r"\begin{document}Nested \emph{read \href{https://hidden.test}{paper} and \url{https://shown.test}}.\end{document}";

const NESTED_TEXT_WRAPPER_LABEL_SOURCE: &str =
    r"\begin{document}Nested \emph{Intro\label{sec:intro} text}.\end{document}";

const NESTED_TEXT_WRAPPER_MATH_SOURCE: &str =
    r"\begin{document}Nested \emph{area $x^2$ and \(y^2\)} text.\end{document}";

const NESTED_TEXT_WRAPPER_WRAPPER_SOURCE: &str =
    r"\begin{document}Nested \emph{outer \textbf{inner text} done}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE: &str =
    r"\begin{document}Nested \emph{before \unknowntext{visible text} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{see \cite{key} and \ref{sec:intro}} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{see \href{https://hidden.test}{paper}, \url{https://shown.test}, and $x^2$} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_ESCAPED_VISIBLE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{50\% A\&B costs \$5\_0 \#1 \{x\}} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \textbf{inner text} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{inner text} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_INLINE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{see \cite{key} and \ref{sec:intro}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{see \href{https://hidden.test}{paper} and \url{https://shown.test}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{use \nolinkurl{https://visible.test/path}, \path|/tmp/archive|, and \detokenize{\foo+*}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LABEL_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{Intro\label{sec:intro} text} done} after}.\end{document}";

const ESCAPED_VISIBLE_SOURCE: &str =
    r"\begin{document}50\% A\&B costs \$5\_0 \#1 \{x\} A\ B.\end{document}";

const NONBREAKING_TILDE_SOURCE: &str =
    r"\begin{document}Figure~1 references Related~Work.\section{Related~Work}\end{document}";

const LINEBREAK_SOURCE: &str = r"\begin{document}First line\\Second line.\end{document}";

const TABULAR_FALLBACK_SOURCE: &str = r"\begin{document}\begin{tabular}{ll}Alpha & Beta \\ Gamma & \textbf{Delta} \\\hline\end{tabular}\end{document}";

const LIST_SOURCE: &str = r"\begin{document}\begin{itemize}\item First \cite{key}\item[Custom] Second\end{itemize}\begin{enumerate}\item One\item Two\end{enumerate}\begin{description}\item[Term] Meaning \cite{key}\item[Other] More\end{description}\end{document}";

const SIMPLE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{quote}Quoted \cite{key}.\end{quote}\begin{center}Centered text.\end{center}\end{document}";

const AUX_RESOLUTION_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \cite{key}.\end{document}";

const LABEL_SOURCE: &str =
    r"\begin{document}\section{Intro}\label{sec:intro}See \ref{sec:intro}.\end{document}";

const MACRO_SECTION_SOURCE: &str =
    r"\newcommand{\mysection}[1]{\section{#1}}\begin{document}\mysection{Intro}\end{document}";

fn assert_or_update_golden(relative_path: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    if env::var_os("LATEXD_UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().expect("golden parent")).expect("create golden dir");
        fs::write(&path, actual).expect("write golden");
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
