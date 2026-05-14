use std::{env, fs, path::Path};

use camino::Utf8PathBuf;
use latexd::compiler::capture_internal_render_ir;
use tex_aux::{BibliographyEntry, SemanticAux, SemanticLabel};
use tex_render_model::{CitationStyleHint, DrawOp, to_pretty_json};
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

const GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\end{figure}\end{document}";

const INLINE_MATH_SOURCE: &str = r"\begin{document}Area \(x^2 + y^2\).\end{document}";

const DOLLAR_MATH_SOURCE: &str = r"\begin{document}Area $x^2 + y^2$.$$z^2$$\end{document}";

const MATH_ENVIRONMENT_SOURCE: &str =
    r"\begin{document}\begin{equation}\frac{a}{b}\end{equation}\end{document}";

const HEADING_LEVEL_SOURCE: &str = r"\begin{document}\section[Short]{Long Section}\subsection*{Methods}\subsubsection{Details}\paragraph{Sketch}\end{document}";

const CITATION_VARIANTS_SOURCE: &str = r"\begin{document}\citep[see][p.~3]{alpha,beta}\citet*{gamma}\parencite{delta}\textcite{epsilon}\end{document}";

const REFERENCE_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \eqref{eq:main}; \cref{fig:a,tab:b}.\end{document}";

const LINK_SOURCE: &str = r"\begin{document}Read \href{https://example.test/paper}{paper link}, \url{https://example.test/raw}, and \url|https://example.test/delimited|.\end{document}";

const URL_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Use \nolinkurl{https://example.test/paper}, \nolinkurl|https://example.test/delimited|, at \path{/tmp/archive} and \path|/var/tmp| via \detokenize{\foo+*}.\end{document}";

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
