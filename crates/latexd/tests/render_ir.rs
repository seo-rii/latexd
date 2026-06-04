use std::{env, fs, path::Path, process::Command};

use camino::Utf8PathBuf;
use latexd::compiler::{
    InternalRenderIrCapture, capture_internal_render_ir,
    capture_internal_render_ir_from_project_root, capture_internal_render_ir_with_mounted_files,
};
use tex_aux::{BibliographyEntry, SemanticAux, SemanticLabel};
use tex_render_model::{
    BlockKind, CitationStyleHint, DrawOp, EventProducer, GeneratedBy, GraphicAssetDensity,
    GraphicAssetDensityUnit, GraphicAssetFormat, ImageCrop, ImageRotation, ImageScale, ImageTrim,
    ImageViewport, ListKind, MetadataField, ModeHint, RenderEvent, SemanticConfidence, SpaceKind,
    TableColumnAlignment, to_pretty_json, to_semantic_pretty_json,
};
use tex_render_model::{InlineNode, IrBlock, ProvenanceSpan, SourceSpanRole, TableRuleSpan};

fn tiny_png_bytes() -> Vec<u8> {
    use image::ImageEncoder;

    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &[
                255, 0, 0, 0, 255, 0, //
                0, 0, 255, 255, 255, 0,
            ],
            2,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .expect("encode png");
    bytes
}

fn tiny_jpeg_bytes() -> Vec<u8> {
    use image::ImageEncoder;

    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .write_image(
            &[
                255, 0, 0, 0, 255, 0, //
                0, 0, 255, 255, 255, 0,
            ],
            2,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .expect("encode jpeg");
    bytes
}

#[test]
fn compact_render_ir_capture_matches_goldens() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());

    let event_json = to_pretty_json(&capture.events).expect("event json");
    let semantic_event_json =
        to_semantic_pretty_json(&capture.events).expect("semantic event json");
    let ir_json = to_pretty_json(&capture.document_ir).expect("ir json");
    let semantic_ir_json = to_semantic_pretty_json(&capture.document_ir).expect("semantic ir json");
    let display_list_json = to_pretty_json(&capture.page_display_lists).expect("display list json");
    let semantic_display_list_json =
        to_semantic_pretty_json(&capture.page_display_lists).expect("semantic display-list json");

    assert_or_update_golden("tests/goldens/render_ir/compact.events.json", &event_json);
    assert_or_update_golden(
        "tests/goldens/render_ir/compact.semantic-events.json",
        &semantic_event_json,
    );
    assert_or_update_golden("tests/goldens/render_ir/compact.ir.json", &ir_json);
    assert_or_update_golden(
        "tests/goldens/render_ir/compact.semantic-ir.json",
        &semantic_ir_json,
    );
    assert_or_update_golden(
        "tests/goldens/render_ir/compact.display-list.json",
        &display_list_json,
    );
    assert_or_update_golden(
        "tests/goldens/render_ir/compact.semantic-display-list.json",
        &semantic_display_list_json,
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
    assert!(pdf_text.contains("([?]) Tj"));
    assert!(!pdf_text.contains("key"));
}

#[test]
fn compact_display_list_pdf_text_is_extractable_when_pdftotext_is_available() {
    let pdftotext = match which::which("pdftotext") {
        Ok(path) => path,
        Err(_) => return,
    };
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let tempdir = tempfile::tempdir().expect("tempdir");
    let pdf_path = tempdir.path().join("display-list.pdf");
    fs::write(&pdf_path, &capture.display_list_pdf).expect("write display-list pdf");

    let output = Command::new(pdftotext)
        .args(["-layout", "-enc", "UTF-8"])
        .arg(&pdf_path)
        .arg("-")
        .output()
        .expect("run pdftotext");

    assert!(
        output.status.success(),
        "pdftotext failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let extracted_text = String::from_utf8_lossy(&output.stdout);
    assert!(extracted_text.contains("A Paper"), "{extracted_text}");
    assert!(extracted_text.contains("[?]"), "{extracted_text}");
    assert!(!extracted_text.contains("key"), "{extracted_text}");
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
fn compact_title_provenance_matches_golden() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let title_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::SetDocumentMetadata(event) if event.field == MetadataField::Title
            )
        })
        .expect("title metadata event");
    let author_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::SetDocumentMetadata(event) if event.field == MetadataField::Author
            )
        })
        .expect("author metadata event");
    let date_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::SetDocumentMetadata(event) if event.field == MetadataField::Date
            )
        })
        .expect("date metadata event");
    let flush_event = capture
        .events
        .events
        .iter()
        .find(|envelope| matches!(&envelope.event, RenderEvent::FlushTitleBlock(_)))
        .expect("flush title event");

    assert_eq!(title_event.meta.mode_hint, ModeHint::Preamble);
    assert_eq!(author_event.meta.mode_hint, ModeHint::Preamble);
    assert_eq!(date_event.meta.mode_hint, ModeHint::Preamble);
    assert_eq!(flush_event.meta.mode_hint, ModeHint::Vertical);
    let title = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::TitleBlock(title) => Some(title),
            _ => None,
        })
        .expect("title block");
    let title_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "A Paper" => Some(run),
            _ => None,
        })
        .expect("title text run");
    let provenance_snapshot = serde_json::json!({
        "source": COMPACT_SOURCE,
        "events": {
            "title": {
                "event": title_event.event,
                "meta": title_event.meta,
            },
            "author": {
                "event": author_event.event,
                "meta": author_event.meta,
            },
            "date": {
                "event": date_event.event,
                "meta": date_event.meta,
            },
            "flush": {
                "event": flush_event.event,
                "meta": flush_event.meta,
            },
        },
        "ir": {
            "title": title.title,
            "title_source": title.title_source,
            "authors": title.authors,
            "author_sources": title.author_sources,
            "date": title.date,
            "date_source": title.date_source,
            "block_source": title.source,
        },
        "display_list": {
            "text": title_text_run.text,
            "source": title_text_run.source,
            "clusters": title_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/compact-title.provenance.json",
        &provenance_json,
    );
}

#[test]
fn compact_citation_provenance_preserves_invocation_and_key_spans() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let citation_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineCitation(citation)
                    if citation.keys.len() == 1 && citation.keys[0] == "key"
            )
        })
        .expect("citation event");
    let citation_inline = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => paragraph.content.iter().find_map(|node| match node {
                InlineNode::Citation(citation)
                    if citation.keys.len() == 1 && citation.keys[0] == "key" =>
                {
                    Some(citation)
                }
                _ => None,
            }),
            _ => None,
        })
        .expect("citation inline");
    let citation_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "[?]" => Some(run),
            _ => None,
        })
        .expect("citation text run");

    assert_eq!(citation_event.meta.mode_hint, ModeHint::Horizontal);
    for source in [
        &citation_event.meta.source,
        &citation_inline.source,
        &citation_text_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "\\cite{key}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::CitationKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": COMPACT_SOURCE,
        "event": {
            "event": citation_event.event,
            "meta": citation_event.meta,
        },
        "ir": {
            "keys": citation_inline.keys,
            "style_hint": citation_inline.style_hint,
            "resolved_label": citation_inline.resolved_label,
            "display_text": citation_inline.display_text,
            "source": citation_inline.source,
        },
        "display_list": {
            "text": citation_text_run.text,
            "source": citation_text_run.source,
            "clusters": citation_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/compact-citation.provenance.json",
        &provenance_json,
    );
}

#[test]
fn compact_heading_provenance_preserves_argument_and_invocation_spans() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let heading_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(&envelope.event, RenderEvent::Heading(heading) if heading.text == "Intro")
        })
        .expect("heading event");
    let heading_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Heading(heading) if heading.level == 1 => Some(heading),
            _ => None,
        })
        .expect("heading block");
    let heading_text_source = heading_block
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Text { text, source } if text == "Intro" => Some(source),
            _ => None,
        })
        .expect("heading text source");
    let heading_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "Intro" => Some(run),
            _ => None,
        })
        .expect("heading text run");

    assert_eq!(heading_event.meta.mode_hint, ModeHint::Vertical);
    for source in [
        &heading_event.meta.source,
        &heading_block.source,
        heading_text_source,
        &heading_text_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "Intro"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "\\section{Intro}"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": COMPACT_SOURCE,
        "event": {
            "event": heading_event.event,
            "meta": heading_event.meta,
        },
        "ir": {
            "level": heading_block.level,
            "number": heading_block.number,
            "source": heading_block.source,
            "text_source": heading_text_source,
        },
        "display_list": {
            "text": heading_text_run.text,
            "source": heading_text_run.source,
            "clusters": heading_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/compact-heading.provenance.json",
        &provenance_json,
    );
}

#[test]
fn compact_text_and_space_events_are_horizontal() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let hello_text = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Text(text) if text.text == "Hello"
            )
        })
        .expect("hello text event");
    let interword_space = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Space(space) if space.kind == SpaceKind::Interword
            )
        })
        .expect("interword space event");

    assert_eq!(hello_text.meta.mode_hint, ModeHint::Horizontal);
    assert_eq!(interword_space.meta.mode_hint, ModeHint::Horizontal);
}

#[test]
fn compact_block_boundary_events_are_vertical() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let abstract_begin = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::BeginBlock(block) if block.block == BlockKind::Abstract
            )
        })
        .expect("abstract begin block event");
    let abstract_end = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::EndBlock(block) if block.block == BlockKind::Abstract
            )
        })
        .expect("abstract end block event");

    assert_eq!(abstract_begin.meta.mode_hint, ModeHint::Vertical);
    assert_eq!(abstract_end.meta.mode_hint, ModeHint::Vertical);
}

#[test]
fn compact_display_math_provenance_preserves_body_and_delimiter_spans() {
    let capture = capture_internal_render_ir("main.tex", COMPACT_SOURCE, &SemanticAux::default());
    let display_math_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::DisplayMath(math) if math.raw_source == "x^2"
            )
        })
        .expect("display math event");
    let display_math_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::DisplayMath(math) if math.raw_source == "x^2" => Some(math),
            _ => None,
        })
        .expect("display math block");
    let display_math_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "x^2" => Some(run),
            _ => None,
        })
        .expect("display math text run");

    assert_eq!(display_math_event.meta.mode_hint, ModeHint::Math);
    for source in [
        &display_math_event.meta.source,
        &display_math_block.source,
        &display_math_text_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "x^2"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &COMPACT_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\[x^2\]"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": COMPACT_SOURCE,
        "event": {
            "event": display_math_event.event,
            "meta": display_math_event.meta,
        },
        "ir": {
            "raw_source": display_math_block.raw_source,
            "normalized_text": display_math_block.normalized_text,
            "source": display_math_block.source,
        },
        "display_list": {
            "text": display_math_text_run.text,
            "source": display_math_text_run.source,
            "clusters": display_math_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/display-math.provenance.json",
        &provenance_json,
    );
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
fn authblk_frontmatter_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        AUTHBLK_FRONTMATTER_SOURCE,
        &SemanticAux::default(),
    );
    let title = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::TitleBlock(title) => Some(title),
            _ => None,
        })
        .expect("title block");

    assert_eq!(title.title.as_deref(), Some("Quantum Paper"));
    assert_eq!(
        title.authors,
        vec![
            "Nai-Hui Chia nc67@rice.edu",
            "Atsuya Hasegawa",
            "Department of Computer Science",
            "Graduate School of Mathematics"
        ]
    );
    let extracted_text = capture.document_ir.extracted_text();
    for visible in [
        "Quantum Paper",
        "Nai-Hui Chia nc67@rice.edu",
        "Atsuya Hasegawa",
        "Department of Computer Science",
        "Graduate School of Mathematics",
    ] {
        assert!(extracted_text.contains(visible), "{extracted_text}");
    }
    for hidden in ["[1]", "[2]", "affil", "thanks"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text}");
    }

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    for visible in [
        "Quantum Paper",
        "Nai-Hui Chia nc67@rice.edu",
        "Atsuya Hasegawa",
        "Department of Computer Science",
        "Graduate School of Mathematics",
    ] {
        assert!(display_list_text.contains(visible), "{display_list_text}");
    }
}

#[test]
fn llncs_frontmatter_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LLNCS_FRONTMATTER_SOURCE,
        &SemanticAux::default(),
    );
    let title = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::TitleBlock(title) => Some(title),
            _ => None,
        })
        .expect("title block");

    assert_eq!(title.title.as_deref(), Some("LNCS Paper"));
    assert_eq!(
        title.authors,
        vec!["Alice and Bob", "Lab One alice@example.test and Lab Two"]
    );
    let extracted_text = capture.document_ir.extracted_text();
    for visible in [
        "LNCS Paper",
        "Alice and Bob",
        "Lab One alice@example.test and Lab Two",
    ] {
        assert!(extracted_text.contains(visible), "{extracted_text}");
    }
    for hidden in ["inst", "orcid", "0000", "institute"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text}");
    }
}

#[test]
fn class_frontmatter_shims_survive_ir_and_display_list() {
    let cases = [
        (
            "revtex",
            REVTEX_FRONTMATTER_SOURCE,
            "REVTeX Paper",
            vec!["Alice", "alice@example.test", "Quantum Lab"],
            vec!["affiliation", "email"],
        ),
        (
            "wacv",
            WACV_FRONTMATTER_SOURCE,
            "WACV Paper",
            vec!["Alice", "Vision Lab"],
            vec!["affiliation"],
        ),
        (
            "ieee",
            IEEE_FRONTMATTER_SOURCE,
            "IEEE Paper",
            vec!["Alice Smith and Bob Jones Vision Lab"],
            vec!["IEEEauthor", "IEEEauthorrefmark"],
        ),
    ];

    for (case, source, expected_title, expected_authors, hidden) in cases {
        let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
        let title = capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::TitleBlock(title) => Some(title),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{case} title block"));

        assert_eq!(title.title.as_deref(), Some(expected_title), "{case}");
        assert_eq!(
            title.authors.iter().map(String::as_str).collect::<Vec<_>>(),
            expected_authors,
            "{case}"
        );
        let extracted_text = capture.document_ir.extracted_text();
        assert!(
            extracted_text.contains(expected_title),
            "{case}: {extracted_text}"
        );
        for author in expected_authors {
            assert!(extracted_text.contains(author), "{case}: {extracted_text}");
        }
        for hidden in hidden {
            assert!(!extracted_text.contains(hidden), "{case}: {extracted_text}");
        }
    }
}

#[test]
fn footnote_bodies_survive_ir_and_display_list_without_raw_braces() {
    let capture =
        capture_internal_render_ir("main.tex", FOOTNOTE_BODY_SOURCE, &SemanticAux::default());
    let extracted_text = capture.document_ir.extracted_text();

    assert!(
        extracted_text.contains("Text Note [?] and [?]. after. Loose note."),
        "{extracted_text}"
    );
    for hidden in ["footnote", "footnotetext", "{", "}", "key", "sec:intro"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text}");
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
    assert!(
        display_list_text.contains("Text Note [?] and [?]. after. Loose note."),
        "{display_list_text}"
    );
    for hidden in ["footnote", "footnotetext", "{", "}", "key", "sec:intro"] {
        assert!(!display_list_text.contains(hidden), "{display_list_text}");
    }
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
fn starred_abstract_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", STARRED_ABSTRACT_SOURCE, &SemanticAux::default());
    let abstract_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Abstract(abstract_block) => Some(abstract_block),
            _ => None,
        })
        .expect("abstract block");

    assert!(abstract_block.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("abstract*")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Starred [?] abstract."));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("abstract*"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Starred [?] abstract."));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("abstract*"));
}

#[test]
fn onecolabstract_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", ONECOL_ABSTRACT_SOURCE, &SemanticAux::default());
    let abstract_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Abstract(abstract_block) => Some(abstract_block),
            _ => None,
        })
        .expect("abstract block");

    assert!(abstract_block.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("onecolabstract")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("One-column [?] abstract."));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("onecolabstract"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("One-column [?] abstract."));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("onecolabstract"));
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
fn bibliography_item_mkbib_wrappers_preserve_visible_punctuation() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MKBIB_WRAPPER_BIBLIOGRAPHY_SOURCE,
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
    let expected = r#""Alpha Title". (2024). [note]. {Supplement}. Emph. Bold. Italic."#;

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "mkbibquote",
        "mkbibparens",
        "mkbibbrackets",
        "mkbibbraces",
        "mkbibemph",
        "mkbibbold",
        "mkbibitalic",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_bibstring_and_acro_wrappers_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        BIBSTRING_ACRO_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Alpha et al. URL: https://example.test/paper.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["mkbibnamefamily", "bibstring", "mkbibacro", "andothers"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_punctuation_helpers_render_visible_punctuation() {
    let capture = capture_internal_render_ir(
        "main.tex",
        PUNCTUATION_HELPER_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Alpha, Beta Gamma: Delta; Epsilon.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "addcomma",
        "addspace",
        "newunit",
        "addcolon",
        "addsemicolon",
        "adddot",
        "finentry",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_mkbib_super_sub_wrappers_attach_to_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MKBIB_SUPER_SUB_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Edition2a {Supplement}.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["mkbibsuperscript", "mkbibsubscript", "mkbibbraces"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_low_level_punctuation_helpers_render_delimiters() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LOW_LEVEL_PUNCTUATION_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Alpha. Beta. Gamma. (Delta) [Epsilon] {Zeta}";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "adddotspace",
        "unspace",
        "isdot",
        "nopunct",
        "bibopenparen",
        "bibcloseparen",
        "bibopenbracket",
        "bibclosebracket",
        "bibopenbrace",
        "bibclosebrace",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_dash_and_slash_helpers_attach_correctly() {
    let capture = capture_internal_render_ir(
        "main.tex",
        DASH_SLASH_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Pages 10-20, Vol. 2/Issue 3-4-5--- appendix.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "bibrangedash",
        "addslash",
        "addhyphen",
        "textendash",
        "textemdash",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_parentext_and_spacing_helpers_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        PARENTEXT_SPACING_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Alpha Beta Gamma Delta Epsilon Zeta (Supplement).";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "addabbrvspace",
        "addnbspace",
        "addthinspace",
        "addlowpenspace",
        "addhighpenspace",
        "parentext",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_namedash_and_urlprefix_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NAMEDASH_URLPREFIX_BIBLIOGRAPHY_SOURCE,
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
    let expected = "---. https://example.test/paper.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["bibnamedash", "urlprefix"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_name_affix_wrappers_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NAME_AFFIX_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Doe, Jr..";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["mkbibnamefamily", "mkbibnameaffix"] {
        assert!(!extracted_text.contains(hidden));
    }
}

#[test]
fn bibliography_item_starred_wrappers_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        STARRED_WRAPPER_BIBLIOGRAPHY_SOURCE,
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
    let expected = r#"alpha title. beta title. "Alpha Title". (2024). [note]. {Supplement}. Emph. Bold. Italic."#;

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "MakeSentenceCase",
        "MakeTitleCase",
        "mkbibquote",
        "mkbibparens",
        "mkbibbrackets",
        "mkbibbraces",
        "mkbibemph",
        "mkbibbold",
        "mkbibitalic",
    ] {
        assert!(!extracted_text.contains(hidden));
    }
}

#[test]
fn bibliography_item_bibinfo_bibfield_wrappers_hide_field_names() {
    let capture = capture_internal_render_ir(
        "main.tex",
        BIBINFO_BIBFIELD_BIBLIOGRAPHY_SOURCE,
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
    let expected = "10.1000/example. Journal of Tests.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["bibinfo", "bibfield", "doi", "journal"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_doi_eprint_commands_render_visible_values() {
    let capture = capture_internal_render_ir(
        "main.tex",
        DOI_EPRINT_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Alpha entry. 10.1000/example. arXiv:2401.00001. Link.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["doi", "eprint", "href", "example.test"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
    for hidden in ["doi", "eprint", "href", "example.test"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn bibliography_item_natexlab_and_newblock_markup_is_hidden() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NATEXLAB_NEWBLOCK_BIBLIOGRAPHY_SOURCE,
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

    assert_eq!(bibliography.items[0].label.as_deref(), Some("Alpha 2024a"));
    assert_eq!(bibliography.items[0].content, "Alpha 2024a.");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha 2024a."), "{extracted_text}");
    for hidden in ["natexlab", "NAT@exlab", "newblock"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(
        display_list_text.contains("Alpha 2024a."),
        "{display_list_text}"
    );
    for hidden in ["natexlab", "NAT@exlab", "newblock"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn bibliography_item_phantom_wrappers_hide_invisible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        PHANTOM_BIBLIOGRAPHY_SOURCE,
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

    assert_eq!(bibliography.items[0].content, "Visible Text.");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Visible Text."), "{extracted_text}");
    for hidden in ["Ghost", "Wide", "Tall", "phantom", "hphantom", "vphantom"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(
        display_list_text.contains("Visible Text."),
        "{display_list_text}"
    );
    for hidden in ["Ghost", "Wide", "Tall", "phantom", "hphantom", "vphantom"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn bibliography_item_tex_spacing_commands_do_not_render_as_punctuation() {
    let capture = capture_internal_render_ir(
        "main.tex",
        TEX_SPACING_BIBLIOGRAPHY_SOURCE,
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
    let expected = "TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in ["Tight!Join", "Soft,Gap", "Wide;Gap", "Colon:Gap", "space"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_text_symbol_commands_render_visible_symbols() {
    let capture = capture_internal_render_ir(
        "main.tex",
        TEXT_SYMBOL_BIBLIOGRAPHY_SOURCE,
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
    let expected = "Quote's. Double\"q. Angles<x>. Pipe|join. Path/name.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "textquotesingle",
        "textquotedbl",
        "textless",
        "textgreater",
        "textbar",
        "slash",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_textstyle_and_box_wrappers_render_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        TEXTSTYLE_BOX_BIBLIOGRAPHY_SOURCE,
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
    let expected = "NASA. alpha title. beta title. Emph. Trimmed. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    for hidden in [
        "NoCaseChange",
        "MakeSentenceCase",
        "MakeTitleCase",
        "protect",
        "relax",
        "leavevmode",
        "ignorespaces",
        "unskip",
        "emph",
        "mbox",
        "hbox",
        "fbox",
        "framebox",
        "raisebox",
        "parbox",
        "makebox",
        "texttt",
        "textsf",
        "textsc",
        "textbf",
        "textit",
        "textrm",
        "textup",
        "textmd",
        "textnormal",
        "textsuperscript",
        "textsubscript",
        "2em",
        "0.5ex",
        "4em",
        "3em",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains(expected), "{display_list_text}");
}

#[test]
fn bibliography_item_urlstyle_declaration_does_not_leak() {
    let capture = capture_internal_render_ir(
        "main.tex",
        URLSTYLE_BIBLIOGRAPHY_SOURCE,
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
    let expected = "https://example.test/paper.";

    assert_eq!(bibliography.items[0].content, expected);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    assert!(!extracted_text.contains("urlstyle"), "{extracted_text}");
    assert!(!extracted_text.contains("same"), "{extracted_text}");

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
    assert!(!display_list_text.contains("same"), "{display_list_text}");
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
fn graphic_render_ir_capture_applies_includegraphics_width_hint() {
    let capture = capture_internal_render_ir("main.tex", GRAPHIC_SOURCE, &SemanticAux::default());
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert!((image.rect.width - (5.0 * 72.0 / 2.54)).abs() < 0.01);
    assert!(image.rect.height < 30.0);
    assert!(image.rect.height > 20.0);
}

#[test]
fn graphic_macro_dimension_options_survive_to_display_list_sizing() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\includegraphics[width=0.5\textwidth,height=0.25\textheight]{figures/plot.pdf}\end{document}",
        &SemanticAux::default(),
    );
    let graphic = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.pdf" => Some(graphic),
            _ => None,
        })
        .expect("graphic event");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        graphic.options.as_deref(),
        Some(r"width=0.5\textwidth,height=0.25\textheight")
    );
    assert!((image.rect.width - 234.0).abs() < 0.01);
    assert!((image.rect.height - 162.0).abs() < 0.01);
}

#[test]
fn graphic_page_and_content_dimension_aliases_affect_image_rect() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\includegraphics[width=0.5\paperwidth,height=0.25\vsize]{figures/plot.pdf}\end{document}",
        &SemanticAux::default(),
    );
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert!((image.rect.width - 306.0).abs() < 0.01);
    assert!((image.rect.height - 162.0).abs() < 0.01);
}

#[test]
fn graphic_provenance_preserves_invocation_and_path_argument_spans() {
    let capture = capture_internal_render_ir("main.tex", GRAPHIC_SOURCE, &SemanticAux::default());
    let graphic_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.pdf"
            )
        })
        .expect("graphic event");
    let graphic_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Graphic(graphic) if graphic.path == "figures/plot.pdf" => Some(graphic),
            _ => None,
        })
        .expect("graphic block");
    let image_op = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(graphic_event.meta.mode_hint, ModeHint::Vertical);
    for source in [
        &graphic_event.meta.source,
        &graphic_block.source,
        &image_op.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &GRAPHIC_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\includegraphics[width=5cm]{figures/plot.pdf}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::ArgumentContent
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &GRAPHIC_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == "figures/plot.pdf"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": GRAPHIC_SOURCE,
        "event": {
            "event": graphic_event.event,
            "meta": graphic_event.meta,
        },
        "ir": {
            "path": graphic_block.path,
            "options": graphic_block.options,
            "asset_format": graphic_block.asset_format,
            "asset_hash": graphic_block.asset_hash,
            "caption": graphic_block.caption,
            "source": graphic_block.source,
        },
        "display_list": {
            "asset_ref": image_op.asset_ref,
            "asset_format": image_op.asset_format,
            "asset_hash": image_op.asset_hash,
            "source": image_op.source,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/graphic.provenance.json",
        &provenance_json,
    );
}

#[test]
fn caption_provenance_preserves_text_and_invocation_spans() {
    let capture = capture_internal_render_ir("main.tex", GRAPHIC_SOURCE, &SemanticAux::default());
    let caption_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(&envelope.event, RenderEvent::Caption(caption) if caption.text == "Plot caption.")
        })
        .expect("caption event");
    assert_eq!(caption_event.meta.mode_hint, ModeHint::Vertical);
    let graphic_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Graphic(graphic) if graphic.path == "figures/plot.pdf" => Some(graphic),
            _ => None,
        })
        .expect("graphic block");
    let caption_source = graphic_block
        .caption_source
        .as_ref()
        .expect("caption source");
    let caption_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "Plot caption." => Some(run),
            _ => None,
        })
        .expect("caption text run");

    for source in [
        &caption_event.meta.source,
        caption_source,
        &caption_text_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &GRAPHIC_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "Plot caption."
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &GRAPHIC_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\caption{Plot caption.}"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": GRAPHIC_SOURCE,
        "event": {
            "event": caption_event.event,
            "meta": caption_event.meta,
        },
        "ir": {
            "caption": graphic_block.caption,
            "caption_source": graphic_block.caption_source,
        },
        "display_list": {
            "text": caption_text_run.text,
            "source": caption_text_run.source,
            "clusters": caption_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/caption.provenance.json",
        &provenance_json,
    );
}

#[test]
fn extensionless_graphic_assets_resolve_before_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        EXTENSIONLESS_GRAPHIC_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.pdf", "%PDF fake")],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.pdf"
                    && graphic.caption.as_deref() == Some("Plot caption.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf"
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.pdf]"));
    assert!(!pdf_text.contains("[image: figures/plot]"));
}

#[test]
fn extensionless_svg_graphic_asset_format_survives_render_boundaries() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        EXTENSIONLESS_SVG_GRAPHIC_SOURCE,
        &SemanticAux::default(),
        &[(
            "figures/vector.svg",
            r#"<svg width="2in" height="1in" viewBox="0 0 200 100"></svg>"#,
        )],
    );
    let graphic_event = capture
        .events
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/vector.svg" => {
                Some(graphic)
            }
            _ => None,
        })
        .expect("graphic event");
    let graphic_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Graphic(graphic) if graphic.path == "figures/vector.svg" => Some(graphic),
            _ => None,
        })
        .expect("graphic block");
    let image_op = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/vector.svg" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(graphic_event.asset_format, Some(GraphicAssetFormat::Svg));
    assert_eq!(
        graphic_event.asset_dimensions,
        Some(tex_render_model::GraphicAssetDimensions {
            width_px: 144,
            height_px: 72,
            density: None,
            natural_width_pt_milli: Some(144_000),
            natural_height_pt_milli: Some(72_000),
        })
    );
    assert_eq!(graphic_block.asset_format, Some(GraphicAssetFormat::Svg));
    assert_eq!(image_op.asset_format, Some(GraphicAssetFormat::Svg));
    assert!((image_op.rect.width - (5.0 * 72.0 / 2.54)).abs() < 0.01);
    assert!((image_op.rect.height - (2.5 * 72.0 / 2.54)).abs() < 0.01);
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/vector.svg]"));
}

#[test]
fn graphic_asset_hash_survives_render_boundaries_and_affects_page_hash() {
    let first = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        EXTENSIONLESS_GRAPHIC_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.pdf", "%PDF first")],
    );
    let second = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        EXTENSIONLESS_GRAPHIC_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.pdf", "%PDF second")],
    );
    let graphic_event = first
        .events
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.pdf" => Some(graphic),
            _ => None,
        })
        .expect("graphic event");
    let graphic_block = first
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Graphic(graphic) if graphic.path == "figures/plot.pdf" => Some(graphic),
            _ => None,
        })
        .expect("graphic block");
    let image_op = first.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    let asset_hash = graphic_event.asset_hash.as_deref().expect("asset hash");
    assert!(asset_hash.starts_with("blake3:"));
    assert_eq!(graphic_block.asset_hash.as_deref(), Some(asset_hash));
    assert_eq!(image_op.asset_hash.as_deref(), Some(asset_hash));
    assert_ne!(
        first.page_display_lists[0].content_hash,
        second.page_display_lists[0].content_hash
    );
}

#[test]
fn graphicspath_assets_resolve_before_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        GRAPHICSPATH_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.png", "fake png")],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.png"
                    && graphic.caption.as_deref() == Some("Plot caption.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png"
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.png]"));
    assert!(!pdf_text.contains("[image: plot]"));
}

#[test]
fn graphic_display_list_pdf_can_embed_resolved_png_assets() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        GRAPHICSPATH_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.png", "fake png")],
    );
    let pdf =
        tex_pdf::render_display_list_pdf_with_assets(&capture.page_display_lists, |asset_ref| {
            (asset_ref == "figures/plot.png").then(tiny_png_bytes)
        });
    let pdf_text = String::from_utf8_lossy(&pdf);

    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/XObject << /Im1 "));
    assert!(pdf_text.contains("/Im1 Do"));
    assert!(!pdf_text.contains("[image: figures/plot.png]"));
    assert!(pdf_text.contains("(Plot caption.) Tj"));
}

#[test]
fn project_root_render_ir_capture_embeds_png_assets_in_debug_pdf() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(root.join("main.tex").as_std_path(), GRAPHICSPATH_SOURCE).expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image)
                if image.asset_ref == "figures/plot.png"
                    && image.asset_format == Some(GraphicAssetFormat::Png)
                    && image.asset_hash.as_deref().is_some_and(|hash| hash.starts_with("blake3:"))
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/Im1 Do"));
    assert!(!pdf_text.contains("[image: figures/plot.png]"));
    assert!(pdf_text.contains("(Plot caption.) Tj"));

    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let artifact_pdf = fs::read(paths.display_list_pdf).expect("read display-list pdf");
    let artifact_pdf_text = String::from_utf8_lossy(&artifact_pdf);
    assert!(artifact_pdf_text.contains("/Subtype /Image"));
    assert!(!artifact_pdf_text.contains("[image: figures/plot.png]"));
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");
    assert!(display_list_svg.contains("data-image-asset-ref=\"figures/plot.png\""));
    assert!(display_list_svg.contains("data-image-asset-format=\"png\""));
    assert!(display_list_svg.contains("data-image-embedded=\"true\""));
    assert!(display_list_svg.contains("href=\"data:image/png,%89PNG"));
    assert!(!display_list_svg.contains("[image: figures/plot.png]"));
}

#[test]
fn project_root_render_ir_capture_embeds_svg_assets_in_debug_svg() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/vector.svg}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/vector.svg").as_std_path(),
        br#"<svg width="2in" height="1in" viewBox="0 0 200 100"><rect width="200" height="100"/></svg>"#,
    )
    .expect("write svg");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/vector.svg" => Some(image),
            _ => None,
        })
        .expect("svg image op");

    assert_eq!(image.asset_format, Some(GraphicAssetFormat::Svg));
    assert!(image.diagnostic.is_none(), "{:?}", image.diagnostic);
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[unsupported image: figures/vector.svg]"));

    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");

    assert!(display_list_svg.contains("data-image-asset-ref=\"figures/vector.svg\""));
    assert!(display_list_svg.contains("data-image-asset-format=\"svg\""));
    assert!(display_list_svg.contains("data-image-embedded=\"true\""));
    assert!(display_list_svg.contains("<image "));
    assert!(display_list_svg.contains("href=\"data:image/svg+xml;charset=utf-8,%3Csvg"));
    assert!(!display_list_svg.contains("[image: figures/vector.svg]"));
    assert!(!display_list_svg.contains("data-image-placeholder-kind="));
}

#[test]
fn project_root_render_ir_capture_uses_png_natural_dimensions() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let graphic = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.png" => Some(graphic),
            _ => None,
        })
        .expect("graphic event");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        graphic.asset_dimensions,
        Some(tex_render_model::GraphicAssetDimensions {
            width_px: 2,
            height_px: 2,
            density: None,
            natural_width_pt_milli: None,
            natural_height_pt_milli: None,
        })
    );
    assert!((image.rect.width - 2.0).abs() < 0.01);
    assert!((image.rect.height - 2.0).abs() < 0.01);
}

#[test]
fn project_root_render_ir_capture_uses_jpeg_density_for_natural_dimensions() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/photo.jpg}\end{document}",
    )
    .expect("write source");
    let mut asset_bytes = vec![
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x02, 0x01, 0x00,
        0x90, 0x00, 0x90, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x3c, 0x00, 0x78, 0x03,
    ];
    asset_bytes.resize(39, 0);
    fs::write(root.join("figures/photo.jpg").as_std_path(), asset_bytes).expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let graphic = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/photo.jpg" => {
                Some(graphic)
            }
            _ => None,
        })
        .expect("graphic event");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/photo.jpg" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        graphic.asset_dimensions,
        Some(tex_render_model::GraphicAssetDimensions {
            width_px: 120,
            height_px: 60,
            density: Some(GraphicAssetDensity {
                x_density: 144,
                y_density: 144,
                unit: GraphicAssetDensityUnit::PixelsPerInch,
            }),
            natural_width_pt_milli: None,
            natural_height_pt_milli: None,
        })
    );
    assert!((image.rect.width - 60.0).abs() < 0.01);
    assert!((image.rect.height - 30.0).abs() < 0.01);
}

#[test]
fn project_root_render_ir_capture_embeds_jpeg_assets_in_debug_artifacts() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/photo.jpg}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/photo.jpg").as_std_path(),
        tiny_jpeg_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/photo.jpg" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(image.asset_format, Some(GraphicAssetFormat::Jpeg));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(!pdf_text.contains("[image: figures/photo.jpg]"));

    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");
    assert!(display_list_svg.contains("data-image-asset-ref=\"figures/photo.jpg\""));
    assert!(display_list_svg.contains("data-image-asset-format=\"jpeg\""));
    assert!(display_list_svg.contains("data-image-embedded=\"true\""));
    assert!(display_list_svg.contains("href=\"data:image/jpeg,%FF%D8"));
    assert!(!display_list_svg.contains("[image: figures/photo.jpg]"));
}

#[test]
fn graphic_keepaspectratio_survives_project_root_render_ir_capture() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics[width=100pt,height=50pt,keepaspectratio]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert!((image.rect.width - 50.0).abs() < 0.01);
    assert!((image.rect.height - 50.0).abs() < 0.01);
}

#[test]
fn graphic_natwidth_natheight_options_drive_default_image_rect() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\includegraphics[natwidth=144pt,natheight=72pt]{figures/plot.pdf}\end{document}",
        &SemanticAux::default(),
    );
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(image.asset_format, Some(GraphicAssetFormat::Pdf));
    assert!((image.rect.width - 144.0).abs() < 0.01);
    assert!((image.rect.height - 72.0).abs() < 0.01);
}

#[test]
fn graphic_crop_options_survive_project_root_render_ir_capture() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics[trim=1pt 2pt 3pt 4pt,viewport=0pt 0pt 2pt 2pt,clip]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        image.crop,
        Some(ImageCrop {
            trim: Some(ImageTrim {
                left_pt: 1.0,
                bottom_pt: 2.0,
                right_pt: 3.0,
                top_pt: 4.0,
            }),
            viewport: Some(ImageViewport {
                llx_pt: 0.0,
                lly_pt: 0.0,
                urx_pt: 2.0,
                ury_pt: 2.0,
            }),
            clip: true,
        })
    );
}

#[test]
fn graphic_individual_bounding_box_options_drive_default_image_rect() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\includegraphics[bbllx=10pt,bblly=20pt,bburx=110pt,bbury=70pt]{figures/plot.pdf}\end{document}",
        &SemanticAux::default(),
    );
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        image.crop,
        Some(ImageCrop {
            trim: None,
            viewport: Some(ImageViewport {
                llx_pt: 10.0,
                lly_pt: 20.0,
                urx_pt: 110.0,
                ury_pt: 70.0,
            }),
            clip: false,
        })
    );
    assert!((image.rect.width - 100.0).abs() < 0.01);
    assert!((image.rect.height - 50.0).abs() < 0.01);
}

#[test]
fn graphic_trim_affects_project_root_default_image_rect() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics[trim=1pt 0pt 0pt 0pt,clip]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert!((image.rect.width - 1.0).abs() < 0.01);
    assert!((image.rect.height - 2.0).abs() < 0.01);
    assert!(pdf_text.contains("q 72 718 1 2 re W n q 2 0 0 2 71 718 cm /Im1 Do Q Q"));

    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");
    assert!(display_list_svg.contains("clip-path=\"url(#image-clip-0)\""));
    assert!(display_list_svg.contains("data-image-crop-rendered=\"true\""));
    assert!(display_list_svg.contains("<image x=\"71\" y=\"72\" width=\"2\" height=\"2\""));
}

#[test]
fn graphic_rotation_options_survive_render_ir_capture() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\includegraphics[width=5cm,angle=90,origin=c]{figures/plot.pdf}\end{document}",
        &SemanticAux::default(),
    );
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");

    assert_eq!(
        image.rotation,
        Some(ImageRotation {
            angle_degrees: 90.0,
            origin: Some("c".to_string()),
        })
    );
}

#[test]
fn project_root_missing_graphic_asset_emits_render_diagnostic() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/missing.png}Visible.\end{document}",
    )
    .expect("write source");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");

    assert!(capture.events.events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::Diagnostic(diagnostic)
            if diagnostic.message.contains("missing graphic asset")
                && diagnostic.message.contains("figures/missing.png")
    )));
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/missing.png" => Some(image),
            _ => None,
        })
        .expect("missing image op");
    assert!(image.asset_hash.is_none());
    assert!(image.diagnostic.as_deref().is_some_and(|diagnostic| {
        diagnostic.contains("missing graphic asset figures/missing.png")
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Visible."), "{extracted_text}");
    assert!(
        !extracted_text.contains("figures/missing"),
        "{extracted_text}"
    );
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[missing image: figures/missing.png]"));
    let svg = tex_pdf::render_display_list_svg(&capture.page_display_lists[0]);
    assert!(svg.contains("data-image-placeholder-kind=\"missing\""));
    assert!(svg.contains("data-image-diagnostic=\"missing graphic asset figures/missing.png\""));
    assert!(svg.contains("[missing image: figures/missing.png]"));
}

#[test]
fn graphic_draft_option_surfaces_placeholder_without_embedding_project_asset() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics[draft,width=10pt,height=5pt]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    let svg = tex_pdf::render_display_list_svg(&capture.page_display_lists[0]);

    assert!(
        image.diagnostic.as_deref().is_some_and(|diagnostic| {
            diagnostic.contains("draft graphic asset figures/plot.png")
        })
    );
    assert!(!pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("[draft image: figures/plot.png]"));
    assert!(svg.contains("data-image-placeholder-kind=\"draft\""));
    assert!(svg.contains("data-image-diagnostic=\"draft graphic asset figures/plot.png\""));
}

#[test]
fn graphicx_package_draft_option_surfaces_image_placeholder() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\documentclass{article}\usepackage[draft]{graphicx}\begin{document}\includegraphics[width=10pt,height=5pt]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let graphic = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.png" => Some(graphic),
            _ => None,
        })
        .expect("graphic event");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert_eq!(
        graphic.options.as_deref(),
        Some("draft,width=10pt,height=5pt")
    );
    assert_eq!(
        image.diagnostic.as_deref(),
        Some("draft graphic asset figures/plot.png")
    );
    assert!(!pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("[draft image: figures/plot.png]"));
}

#[test]
fn graphicx_package_draft_respects_local_final_image_embedding() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\documentclass{article}\usepackage[draft]{graphicx}\begin{document}\includegraphics[final,width=10pt,height=5pt]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert_eq!(image.diagnostic, None);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(!pdf_text.contains("[draft image: figures/plot.png]"));
}

#[test]
fn graphicx_class_draft_option_surfaces_image_placeholder() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\documentclass[draft]{article}\usepackage{graphicx}\begin{document}\includegraphics[width=10pt,height=5pt]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let graphic = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::GraphicRef(graphic) if graphic.path == "figures/plot.png" => Some(graphic),
            _ => None,
        })
        .expect("graphic event");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert_eq!(
        graphic.options.as_deref(),
        Some("draft,width=10pt,height=5pt")
    );
    assert_eq!(
        image.diagnostic.as_deref(),
        Some("draft graphic asset figures/plot.png")
    );
    assert!(!pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("[draft image: figures/plot.png]"));
}

#[test]
fn graphicx_pass_options_draft_surfaces_image_placeholder() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\documentclass{article}\PassOptionsToPackage{draft}{graphicx}\usepackage{graphicx}\begin{document}\includegraphics[width=10pt,height=5pt]{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert_eq!(
        image.diagnostic.as_deref(),
        Some("draft graphic asset figures/plot.png")
    );
    assert!(!pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("[draft image: figures/plot.png]"));
}

#[test]
fn graphicx_gin_setkeys_draft_surfaces_image_placeholder() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\documentclass{article}\usepackage{graphicx}\setkeys{Gin}{draft,width=10pt,height=5pt}\begin{document}\includegraphics{figures/plot.png}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.png").as_std_path(),
        tiny_png_bytes(),
    )
    .expect("write image");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert!((image.rect.width - 10.0).abs() < 0.01);
    assert!((image.rect.height - 5.0).abs() < 0.01);
    assert_eq!(
        image.diagnostic.as_deref(),
        Some("draft graphic asset figures/plot.png")
    );
    assert!(!pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("[draft image: figures/plot.png]"));
}

#[test]
fn graphic_draft_pdf_asset_is_not_reported_as_unsupported() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics[draft]{figures/plot.pdf}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.pdf").as_std_path(),
        b"%PDF-1.4\n1 0 obj\n<< /Type /Page /MediaBox [0 0 144 72] >>\nendobj\n",
    )
    .expect("write pdf");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert_eq!(
        image.diagnostic.as_deref(),
        Some("draft graphic asset figures/plot.pdf")
    );
    assert!(pdf_text.contains("[draft image: figures/plot.pdf]"));
    assert!(!pdf_text.contains("[unsupported image: figures/plot.pdf]"));
}

#[test]
fn project_root_render_ir_capture_converts_pdf_assets_in_debug_artifacts_when_gs_available() {
    let Ok(_) = which::which("gs") else {
        return;
    };
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/plot.pdf}\end{document}",
    )
    .expect("write source");
    let stream = "0.2 g 0 0 144 72 re f";
    let objects = [
        "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
        "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string(),
        "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 144 72] /Contents 4 0 R >> endobj\n"
            .to_string(),
        format!(
            "4 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ),
    ];
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0usize];
    for object in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object.as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    fs::write(root.join("figures/plot.pdf").as_std_path(), pdf).expect("write pdf");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("pdf image op");
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);

    assert!(image.asset_hash.is_some());
    assert_eq!(image.asset_format, Some(GraphicAssetFormat::Pdf));
    assert!(image.diagnostic.is_none());
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/Im1 Do"));
    assert!(!pdf_text.contains("[unsupported image: figures/plot.pdf]"));

    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");
    assert!(display_list_svg.contains("data-image-asset-ref=\"figures/plot.pdf\""));
    assert!(display_list_svg.contains("data-image-asset-format=\"pdf\""));
    assert!(display_list_svg.contains("data-image-converted-format=\"png\""));
    assert!(display_list_svg.contains("data-image-embedded=\"true\""));
    assert!(display_list_svg.contains("href=\"data:image/png,%89PNG"));
    assert!(!display_list_svg.contains("[unsupported image: figures/plot.pdf]"));
}

#[test]
fn project_root_unconvertible_pdf_graphic_surfaces_render_artifact_placeholder() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
    fs::create_dir_all(root.join("figures").as_std_path()).expect("create figures dir");
    fs::write(
        root.join("main.tex").as_std_path(),
        r"\begin{document}\includegraphics{figures/plot.pdf}\end{document}",
    )
    .expect("write source");
    fs::write(
        root.join("figures/plot.pdf").as_std_path(),
        b"%PDF-1.4\n1 0 obj\n<< /Type /Page /MediaBox [0 0 144 72] >>\nendobj\n",
    )
    .expect("write pdf");

    let capture =
        capture_internal_render_ir_from_project_root(&root, "main.tex", &SemanticAux::default())
            .expect("capture project render ir");
    let image = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
            _ => None,
        })
        .expect("pdf image op");

    assert!(image.asset_hash.is_some());
    assert_eq!(image.asset_format, Some(GraphicAssetFormat::Pdf));
    assert!(image.diagnostic.is_none());
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[unsupported image: figures/plot.pdf]"));
    let output_dir = root.join("render-artifacts");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let svg = fs::read_to_string(&paths.display_list_svgs[0]).expect("read display-list svg");
    assert!(svg.contains("data-image-placeholder-kind=\"unsupported\""));
    assert!(svg.contains("[unsupported image: figures/plot.pdf]"));
    assert!(!svg.contains("data-image-embedded=\"true\""));
}

#[test]
fn declared_graphic_extension_order_resolves_before_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        DECLARED_GRAPHIC_EXTENSIONS_SOURCE,
        &SemanticAux::default(),
        &[
            ("figures/plot.pdf", "%PDF fake"),
            ("figures/plot.png", "fake png"),
        ],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.png"
                    && graphic.caption.as_deref() == Some("Plot caption.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png"
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.png]"));
    assert!(!pdf_text.contains("[image: figures/plot.pdf]"));
}

#[test]
fn legacy_epsfig_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        LEGACY_EPSFIG_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.eps", "fake eps")],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.eps"
                    && graphic.options.as_deref() == Some("file=figures/plot,width=5cm")
                    && graphic.caption.as_deref() == Some("Plot caption.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.eps"
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.eps]"));
    assert!(!pdf_text.contains("file=figures/plot"));
}

#[test]
fn legacy_epsf_file_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        LEGACY_EPSF_FILE_SOURCE,
        &SemanticAux::default(),
        &[("figures/plot.eps", "fake eps")],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/plot.eps"
                    && graphic.options.is_none()
                    && graphic.caption.as_deref() == Some("Plot caption.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/plot.eps"
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/plot.eps]"));
    assert!(!pdf_text.contains("epsfbox"));
}

#[test]
fn graphic_layout_box_wrappers_preserve_images_without_argument_leakage() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        GRAPHIC_LAYOUT_BOX_WRAPPER_SOURCE,
        &SemanticAux::default(),
        &[
            ("figures/plot.pdf", "%PDF fake"),
            ("figures/other.eps", "fake eps"),
            ("figures/third.eps", "fake eps"),
        ],
    );

    for path in ["figures/plot.pdf", "figures/other.eps", "figures/third.eps"] {
        assert!(capture.document_ir.blocks.iter().any(|block| {
            matches!(
                block,
                IrBlock::Graphic(graphic) if graphic.path == path
            )
        }));
        assert!(capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::Image(image) if image.asset_ref == path
            )
        }));
    }
    let image_rects = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Image(image) => Some((
                image.asset_ref.as_str(),
                image.rect.width,
                image.rect.height,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::Image(image)
                    if image.asset_ref == "figures/plot.pdf"
                        && (image.rect.width - 374.4).abs() < 0.01
                        && (image.rect.height - 259.2).abs() < 0.01
            )
        }),
        "{image_rects:?}"
    );
    assert!(
        capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::Image(image)
                    if image.asset_ref == "figures/other.eps"
                        && (image.rect.width - 234.0).abs() < 0.01
                        && (image.rect.height - 168.0).abs() < 0.01
                        && image.scale == Some(ImageScale { x: 0.5, y: 2.0 })
            )
        }),
        "{image_rects:?}"
    );
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image)
                if image.asset_ref == "figures/third.eps"
                    && image.rotation
                        == Some(ImageRotation {
                            angle_degrees: 90.0,
                            origin: Some("c".to_string()),
                        })
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    for hidden in [
        "0.8",
        "0.4",
        "0.5",
        "origin",
        "90",
        "textwidth",
        "textheight",
    ] {
        assert!(!extracted_text.contains(hidden));
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn nested_graphic_layout_box_wrappers_thread_sizing_options() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        NESTED_GRAPHIC_LAYOUT_BOX_WRAPPER_SOURCE,
        &SemanticAux::default(),
        &[
            ("figures/nested.pdf", "%PDF fake"),
            ("figures/reflected.pdf", "%PDF fake"),
        ],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/nested.pdf"
                    && graphic.options.as_deref()
                        == Some("scale=0.5,yscale=2,width=0.5\\textwidth")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image)
                if image.asset_ref == "figures/nested.pdf"
                    && (image.rect.width - 234.0).abs() < 0.01
                    && (image.rect.height - 42.0).abs() < 0.01
                    && image.scale == Some(ImageScale { x: 0.5, y: 2.0 })
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image)
                if image.asset_ref == "figures/reflected.pdf"
                    && (image.rect.width - (2.0 * 72.0 / 2.54)).abs() < 0.01
                    && image.scale == Some(ImageScale { x: -1.0, y: 1.0 })
        )
    }));
}

#[test]
fn graphic_alignment_box_wrappers_preserve_images_without_argument_leakage() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        GRAPHIC_ALIGNMENT_BOX_WRAPPER_SOURCE,
        &SemanticAux::default(),
        &[
            ("figures/plot.pdf", "%PDF fake"),
            ("figures/other.pdf", "%PDF fake"),
            ("figures/third.eps", "fake eps"),
        ],
    );

    for path in ["figures/plot.pdf", "figures/other.pdf", "figures/third.eps"] {
        assert!(capture.document_ir.blocks.iter().any(|block| {
            matches!(
                block,
                IrBlock::Graphic(graphic) if graphic.path == path
            )
        }));
        assert!(capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::Image(image) if image.asset_ref == path
            )
        }));
    }

    let extracted_text = capture.document_ir.extracted_text();
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    for hidden in ["textwidth", "center", "adjustbox", "centerline", "makebox"] {
        assert!(!extracted_text.contains(hidden));
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn starred_graphic_capture_derives_display_list_image_without_visible_star() {
    let capture =
        capture_internal_render_ir("main.tex", STARRED_GRAPHIC_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/starred.pdf"
                    && graphic.options.as_deref() == Some("width=3cm")
                    && graphic.caption.as_deref() == Some("Starred plot.")
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Image(image) if image.asset_ref == "figures/starred.pdf"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Starred plot."));
    assert!(!extracted_text.contains('*'));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("[image: figures/starred.pdf]"));
}

#[test]
fn starred_float_graphic_capture_derives_display_list_image_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        STARRED_FLOAT_GRAPHIC_SOURCE,
        &SemanticAux::default(),
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/wide.pdf"
                    && graphic.caption.as_deref() == Some("Wide figure.")
        )
    }));
    assert!(
        capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(op, DrawOp::Image(image) if image.asset_ref == "figures/wide.pdf")
        })
    );
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("figure*")
        )
    }));
}

#[test]
fn starred_table_caption_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", STARRED_TABLE_SOURCE, &SemanticAux::default());

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Wide table."));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("table*")
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
    assert!(display_list_text.contains("Wide table."));
}

#[test]
fn sideways_float_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", SIDEWAYS_FLOAT_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/rotated.pdf"
                    && graphic.options.as_deref() == Some("width=4cm")
                    && graphic.caption.as_deref() == Some("Rotated figure.")
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Rotated figure."));
    assert!(extracted_text.contains("Rotated table."));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("sidewaysfigure" | "sidewaystable")
                )
        )
    }));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"fig:rot"));
    assert!(label_keys.contains(&"tab:rot"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Rotated figure."));
    assert!(display_list_text.contains("Rotated table."));
    assert!(
        capture.page_display_lists[0].ops.iter().any(
            |op| matches!(op, DrawOp::Image(image) if image.asset_ref == "figures/rotated.pdf")
        )
    );
}

#[test]
fn sidecap_float_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", SIDECAP_FLOAT_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/side.pdf"
                    && graphic.options.as_deref() == Some("width=4cm")
                    && graphic.caption.as_deref() == Some("Side [?].")
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Side [?]."));
    assert!(extracted_text.contains("Side table."));
    for hidden in ["[1]", "[ht]", "key", "fig:side", "tab:side"] {
        assert!(!extracted_text.contains(hidden));
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("SCfigure" | "SCtable"))
        )
    }));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"fig:side"));
    assert!(label_keys.contains(&"tab:side"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Side [?]."));
    assert!(display_list_text.contains("Side table."));
    for hidden in ["[1]", "[ht]", "key", "fig:side", "tab:side"] {
        assert!(!display_list_text.contains(hidden));
    }
    assert!(
        capture.page_display_lists[0]
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::Image(image) if image.asset_ref == "figures/side.pdf"))
    );
}

#[test]
fn wrap_float_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", WRAP_FLOAT_SOURCE, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/wrapped.pdf"
                    && graphic.options.as_deref() == Some("width=3cm")
                    && graphic.caption.as_deref() == Some("Wrapped figure.")
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Wrapped figure."));
    assert!(extracted_text.contains("Wrapped table."));
    assert!(!extracted_text.contains("0.35"));
    assert!(!extracted_text.contains("0.4"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("wrapfigure" | "wraptable")
                )
        )
    }));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"fig:wrap"));
    assert!(label_keys.contains(&"tab:wrap"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Wrapped figure."));
    assert!(display_list_text.contains("Wrapped table."));
    assert!(!display_list_text.contains("0.35"));
    assert!(!display_list_text.contains("0.4"));
    assert!(
        capture.page_display_lists[0].ops.iter().any(
            |op| matches!(op, DrawOp::Image(image) if image.asset_ref == "figures/wrapped.pdf")
        )
    );
}

#[test]
fn captionof_capture_uses_long_caption_without_type_or_short_title_leakage() {
    let capture = capture_internal_render_ir("main.tex", CAPTIONOF_SOURCE, &SemanticAux::default());

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Long Figure Title"));
    assert!(extracted_text.contains("Long Table Title"));
    assert!(extracted_text.contains("[?]"));
    for hidden in ["figure", "Short Figure", "fig:first", "table", "tab:first"] {
        assert!(!extracted_text.contains(hidden));
    }

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"fig:first"));
    assert!(label_keys.contains(&"tab:first"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Long Figure Title"));
    assert!(display_list_text.contains("Long Table Title"));
    assert!(display_list_text.contains("[?]"));
    for hidden in ["figure", "Short Figure", "fig:first", "table", "tab:first"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn captionof_provenance_preserves_long_caption_and_invocation_spans() {
    let capture = capture_internal_render_ir("main.tex", CAPTIONOF_SOURCE, &SemanticAux::default());

    let mut provenance_cases = Vec::new();
    for (caption_text, invocation_text) in [
        (
            "Long Figure Title",
            r"\captionof{figure}[Short Figure]{Long Figure Title}",
        ),
        ("Long Table Title", r"\captionof*{table}{Long Table Title}"),
    ] {
        let caption_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(&envelope.event, RenderEvent::Caption(caption) if caption.text == caption_text)
            })
            .expect("captionof event");
        let paragraph = capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Paragraph(paragraph)
                    if paragraph.content.iter().any(
                        |inline| matches!(inline, InlineNode::Text { text, .. } if text == caption_text),
                    ) =>
                {
                    Some(paragraph)
                }
                _ => None,
            })
            .expect("captionof paragraph");
        let inline_source = paragraph
            .content
            .iter()
            .find_map(|inline| match inline {
                InlineNode::Text { text, source } if text == caption_text => Some(source),
                _ => None,
            })
            .expect("captionof inline source");
        let caption_text_run = capture.page_display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::TextRun(run) if run.text == caption_text => Some(run),
                _ => None,
            })
            .expect("captionof text run");

        for source in [
            &caption_event.meta.source,
            &paragraph.source,
            inline_source,
            &caption_text_run.source,
        ] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &CAPTIONOF_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == caption_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &CAPTIONOF_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "caption": caption_text,
            "event": {
                "event": caption_event.event,
                "meta": caption_event.meta,
            },
            "ir": {
                "paragraph": paragraph,
                "inline_source": inline_source,
            },
            "display_list": {
                "text": caption_text_run.text,
                "source": caption_text_run.source,
                "clusters": caption_text_run.clusters,
            },
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": CAPTIONOF_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/captionof.provenance.json",
        &provenance_json,
    );
}

#[test]
fn starred_caption_capture_survives_ir_without_visible_star_or_label_key() {
    let capture =
        capture_internal_render_ir("main.tex", STARRED_CAPTION_SOURCE, &SemanticAux::default());

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Unnumbered Figure Caption"));
    assert!(extracted_text.contains("[?]"));
    for hidden in ["*", "fig:starred"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Unnumbered Figure Caption"));
    assert!(display_list_text.contains("[?]"));
    for hidden in ["*", "fig:starred"] {
        assert!(!display_list_text.contains(hidden));
    }
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
fn math_accents_and_delimiters_use_normalized_text_in_ir_and_display_list() {
    let source = r"\begin{document}Vector \(\hat{x} + \bar{y} + \vec{v} + \left\langle \alpha, \beta \right\rangle\).\end{document}";
    let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
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
            InlineNode::InlineMath {
                raw_source,
                normalized_text,
                ..
            } if raw_source
                == r"\hat{x} + \bar{y} + \vec{v} + \left\langle \alpha, \beta \right\rangle"
                && normalized_text.as_deref()
                    == Some("hat(x) + bar(y) + vec(v) + < alpha, beta >")
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
    assert!(display_list_text.contains("hat(x) + bar(y) + vec(v) + < alpha, beta >"));
    assert!(!display_list_text.contains(r"\hat{x}"));
    assert!(!display_list_text.contains(r"\left\langle"));
}

#[test]
fn math_operators_and_scripts_use_normalized_text_in_ir_and_display_list() {
    let source = r"\begin{document}Series \(\sum_{i=1}^{n} x_i + \int_{0}^{1} f(x)\,dx + \sin \theta\).\end{document}";
    let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
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
            InlineNode::InlineMath {
                raw_source,
                normalized_text,
                ..
            } if raw_source
                == r"\sum_{i=1}^{n} x_i + \int_{0}^{1} f(x)\,dx + \sin \theta"
                && normalized_text.as_deref()
                    == Some("sum_{i = 1}^{n} x_i + int_{0}^{1} f(x) dx + sin theta")
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
    assert!(display_list_text.contains("sum_{i = 1}^{n} x_i"));
    assert!(display_list_text.contains("int_{0}^{1} f(x) dx"));
    assert!(display_list_text.contains("sin theta"));
    assert!(!display_list_text.contains(r"\sum"));
    assert!(!display_list_text.contains(r"\int"));
}

#[test]
fn unknown_math_commands_use_raw_source_without_lossy_normalization() {
    let source = r"\begin{document}Set \(\mathbb{R} + \unknownmath{x}\).\end{document}";
    let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
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
            InlineNode::InlineMath {
                raw_source,
                normalized_text,
                ..
            } if raw_source == r"\mathbb{R} + \unknownmath{x}"
                && normalized_text.is_none()
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
    assert!(display_list_text.contains(r"\mathbb{R} + \unknownmath{x}"));
}

#[test]
fn matrix_math_environment_uses_normalized_text_in_ir_and_display_list() {
    let source = r"\begin{document}\[\begin{pmatrix} a & b \\ c & d \end{pmatrix}\]\end{document}";
    let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display)
                if display.raw_source == r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"
                    && display.normalized_text.as_deref() == Some("matrix(a, b; c, d)")
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
    assert!(display_list_text.contains("matrix(a, b; c, d)"));
    assert!(!display_list_text.contains(r"\begin{pmatrix}"));
}

#[test]
fn cases_math_environment_uses_normalized_text_in_ir_and_display_list() {
    let source = r"\begin{document}\[\begin{cases} x & x>0 \\ -x & x<0 \end{cases}\]\end{document}";
    let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display)
                if display.raw_source == r"\begin{cases} x & x>0 \\ -x & x<0 \end{cases}"
                    && display.normalized_text.as_deref()
                        == Some("cases(x, x > 0; - x, x < 0)")
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
    assert!(display_list_text.contains("cases(x, x > 0; - x, x < 0)"));
    assert!(!display_list_text.contains(r"\begin{cases}"));
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
                    && display.normalized_text.as_deref() == Some("a/b")
                    && matches!(
                        &display.source.primary,
                        ProvenanceSpan::File(span)
                            if &MATH_ENVIRONMENT_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\frac{a}{b}"
                    )
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == r"a&=b"
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == r"x&=y"
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == r"u&=&v"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("equation")
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("flalign*")
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("alignat*")
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("eqnarray*")
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
    assert!(display_list_text.contains("a/b"));
    assert!(!display_list_text.contains(r"\frac{a}{b}"));
    assert!(display_list_text.contains("a = b"));
    assert!(display_list_text.contains("x = y"));
    assert!(display_list_text.contains("u = v"));
}

#[test]
fn math_environment_label_definitions_do_not_leak_into_display_math() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MATH_ENVIRONMENT_LABEL_SOURCE,
        &SemanticAux::default(),
    );

    assert!(capture.document_ir.labels.iter().any(|label| {
        label.key == "eq:one"
            && matches!(
                &label.source.primary,
                ProvenanceSpan::File(span)
                    if &MATH_ENVIRONMENT_LABEL_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "eq:one"
            )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == "x"
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
    assert!(display_list_text.contains("x"));
    assert!(!display_list_text.contains(r"\label"));
    assert!(!display_list_text.contains("eq:one"));
}

#[test]
fn bracket_display_math_label_definitions_do_not_leak_into_display_math() {
    let capture = capture_internal_render_ir(
        "main.tex",
        BRACKET_DISPLAY_MATH_LABEL_SOURCE,
        &SemanticAux::default(),
    );

    assert!(
        capture
            .document_ir
            .labels
            .iter()
            .any(|label| label.key == "eq:bracket")
    );
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == "y"
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
    assert!(display_list_text.contains("y"));
    assert!(!display_list_text.contains(r"\label"));
    assert!(!display_list_text.contains("eq:bracket"));
}

#[test]
fn dollar_display_math_label_definitions_do_not_leak_into_display_math() {
    let capture = capture_internal_render_ir(
        "main.tex",
        DOLLAR_DISPLAY_MATH_LABEL_SOURCE,
        &SemanticAux::default(),
    );

    assert!(
        capture
            .document_ir
            .labels
            .iter()
            .any(|label| label.key == "eq:dollar")
    );
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display) if display.raw_source == "z"
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
    assert!(display_list_text.contains("z"));
    assert!(!display_list_text.contains(r"\label"));
    assert!(!display_list_text.contains("eq:dollar"));
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

    assert_eq!(citations.len(), 10);
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
                == r"\citep[see][p.~3]{alpha,beta}"
    ));
    assert!(citations[0].source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if &CITATION_VARIANTS_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "alpha,beta"
            )
    }));
    assert_eq!(citations[1].keys, vec!["gamma".to_string()]);
    assert_eq!(citations[1].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[2].keys, vec!["delta".to_string()]);
    assert_eq!(citations[2].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[3].keys, vec!["epsilon".to_string()]);
    assert_eq!(citations[3].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[4].keys, vec!["zeta".to_string()]);
    assert_eq!(citations[4].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[5].keys, vec!["eta".to_string()]);
    assert_eq!(citations[5].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[6].keys, vec!["theta".to_string()]);
    assert_eq!(citations[6].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[7].keys, vec!["iota".to_string()]);
    assert_eq!(citations[7].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[8].keys, vec!["lambda".to_string()]);
    assert_eq!(citations[8].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[9].keys, vec!["mu".to_string()]);
    assert_eq!(citations[9].style_hint, CitationStyleHint::Parenthetical);

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
    assert!(!display_list_text.contains("theta"));
    assert!(!display_list_text.contains("lambda"));
    assert!(!display_list_text.contains("mu"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains(r"\Cite"));
}

#[test]
fn citation_wrapper_macro_capture_survives_ir_without_dropping_keys() {
    let capture =
        capture_internal_render_ir("main.tex", CITATION_WRAPPER_SOURCE, &SemanticAux::default());
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

    let expected = [
        vec!["alpha".to_string(), "beta".to_string()],
        vec!["gamma".to_string(), "delta".to_string()],
        vec!["paper:one".to_string()],
        vec!["core".to_string()],
        vec!["core".to_string()],
        vec!["default".to_string()],
        vec!["default".to_string()],
    ];
    assert_eq!(citations.len(), expected.len());
    for (citation, keys) in citations.iter().zip(expected) {
        assert_eq!(citation.keys, keys);
        assert_eq!(citation.style_hint, CitationStyleHint::Numeric);
        assert_eq!(citation.display_text, "[?]");
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
    assert!(display_list_text.contains("See [?], [?], [?], [?], [?], [?], and [?]."));
    for hidden in [
        "alpha",
        "beta",
        "gamma",
        "delta",
        "paper:one",
        "core",
        "default",
    ] {
        assert!(!display_list_text.contains(hidden), "{display_list_text}");
    }
}

#[test]
fn citation_metadata_alias_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CITATION_METADATA_ALIAS_SOURCE,
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

    let expected = [
        ("alpha", CitationStyleHint::Textual),
        ("beta", CitationStyleHint::Textual),
        ("gamma", CitationStyleHint::Parenthetical),
        ("delta", CitationStyleHint::Textual),
        ("epsilon", CitationStyleHint::Textual),
        ("zeta", CitationStyleHint::Textual),
        ("eta", CitationStyleHint::Textual),
    ];
    assert_eq!(citations.len(), expected.len());
    for (citation, (key, style_hint)) in citations.iter().zip(expected) {
        assert_eq!(citation.keys, vec![key.to_string()]);
        assert_eq!(citation.style_hint, style_hint);
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert_eq!(extracted_text.matches("[?]").count(), expected.len());
    for key in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"] {
        assert!(!extracted_text.contains(key));
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
    for key in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"] {
        assert!(!display_list_text.contains(key));
    }
}

#[test]
fn citation_identifier_date_alias_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CITATION_IDENTIFIER_DATE_ALIAS_SOURCE,
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

    let expected = [
        ("doi", CitationStyleHint::Textual),
        ("eprint", CitationStyleHint::Textual),
        ("isbn", CitationStyleHint::Textual),
        ("issn", CitationStyleHint::Textual),
        ("url", CitationStyleHint::Textual),
        ("number", CitationStyleHint::Numeric),
        ("date", CitationStyleHint::Textual),
        ("capdate", CitationStyleHint::Textual),
        ("urldate", CitationStyleHint::Textual),
        ("capurldate", CitationStyleHint::Textual),
    ];
    assert_eq!(citations.len(), expected.len());
    for (citation, (key, style_hint)) in citations.iter().zip(expected) {
        assert_eq!(citation.keys, vec![key.to_string()]);
        assert_eq!(citation.style_hint, style_hint);
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert_eq!(extracted_text.matches("[?]").count(), expected.len());
    for key in [
        "doi",
        "eprint",
        "isbn",
        "issn",
        "url",
        "number",
        "date",
        "capdate",
        "urldate",
        "capurldate",
    ] {
        assert!(!extracted_text.contains(key));
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
    for key in [
        "doi",
        "eprint",
        "isbn",
        "issn",
        "url",
        "number",
        "date",
        "capdate",
        "urldate",
        "capurldate",
    ] {
        assert!(!display_list_text.contains(key));
    }
}

#[test]
fn citation_entry_alias_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CITATION_ENTRY_ALIAS_SOURCE,
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

    let expected = [
        ("online", CitationStyleHint::Numeric),
        ("smart", CitationStyleHint::Parenthetical),
        ("full", CitationStyleHint::Textual),
        ("footfull", CitationStyleHint::Unknown),
        ("entry", CitationStyleHint::Textual),
        ("textalias", CitationStyleHint::Textual),
        ("parenalias", CitationStyleHint::Parenthetical),
        ("capalias", CitationStyleHint::Textual),
    ];
    assert_eq!(citations.len(), expected.len());
    for (citation, (key, style_hint)) in citations.iter().zip(expected) {
        assert_eq!(citation.keys, vec![key.to_string()]);
        assert_eq!(citation.style_hint, style_hint);
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert_eq!(extracted_text.matches("[?]").count(), expected.len());
    for key in [
        "online",
        "smart",
        "full",
        "footfull",
        "entry",
        "textalias",
        "parenalias",
        "capalias",
    ] {
        assert!(!extracted_text.contains(key));
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
    for key in [
        "online",
        "smart",
        "full",
        "footfull",
        "entry",
        "textalias",
        "parenalias",
        "capalias",
    ] {
        assert!(!display_list_text.contains(key));
    }
}

#[test]
fn defcitealias_definition_does_not_leak_into_ir_or_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", DEFCITEALIAS_SOURCE, &SemanticAux::default());
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

    let expected = [
        CitationStyleHint::Textual,
        CitationStyleHint::Parenthetical,
        CitationStyleHint::Textual,
    ];
    assert_eq!(citations.len(), expected.len());
    for (citation, style_hint) in citations.iter().zip(expected) {
        assert_eq!(citation.keys, vec!["alpha".to_string()]);
        assert_eq!(citation.style_hint, style_hint);
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alias [?], [?], and [?]."));
    for hidden in ["alpha", "Paper I", "defcitealias"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Alias [?], [?], and [?]."));
    for hidden in ["alpha", "Paper I", "defcitealias"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn addbibresource_definition_does_not_leak_into_ir_or_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", ADDBIBRESOURCE_SOURCE, &SemanticAux::default());
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

    assert_eq!(citations.len(), 2);
    assert_eq!(citations[0].keys, vec!["alpha".to_string()]);
    assert_eq!(citations[0].style_hint, CitationStyleHint::Textual);
    assert_eq!(citations[1].keys, vec!["beta".to_string()]);
    assert_eq!(citations[1].style_hint, CitationStyleHint::Parenthetical);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Bib [?] and [?]."));
    for hidden in [
        "refs.bib",
        "location",
        "local",
        "addbibresource",
        "alpha",
        "beta",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Bib [?] and [?]."));
    for hidden in [
        "refs.bib",
        "location",
        "local",
        "addbibresource",
        "alpha",
        "beta",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn lineno_commands_do_not_leak_into_ir_or_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", LINENO_COMMAND_SOURCE, &SemanticAux::default());
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Visible [?] text."));
    assert!(extracted_text.contains("After."));
    for hidden in ["linenumbers", "modulo", "[2]", "[7]", "{key}"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Visible [?] text."));
    assert!(display_list_text.contains("After."));
    for hidden in ["linenumbers", "modulo", "[2]", "[7]", "{key}"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn layout_spacing_commands_do_not_leak_into_ir_or_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LAYOUT_SPACING_COMMAND_SOURCE,
        &SemanticAux::default(),
    );
    let extracted_text = capture.document_ir.extracted_text();
    for visible in ["Before", "After", "Gap.", "Text", "Next."] {
        assert!(extracted_text.contains(visible));
    }
    for hidden in ["-1em", "2mm", "[4]", "vspace", "hspace", "pagebreak"] {
        assert!(!extracted_text.contains(hidden));
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
    for visible in ["Before", "After", "Gap.", "Text", "Next."] {
        assert!(display_list_text.contains(visible));
    }
    for hidden in ["-1em", "2mm", "[4]", "vspace", "hspace", "pagebreak"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn layout_helper_commands_do_not_leak_into_ir_or_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LAYOUT_HELPER_COMMAND_SOURCE,
        &SemanticAux::default(),
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha Beta."));
    assert!(extracted_text.contains("Visible."));
    for hidden in [
        "FloatBarrier",
        "balance",
        "phantomsection",
        "addcontentsline",
        "Hidden Entry",
        "toc",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Alpha Beta."));
    assert!(display_list_text.contains("Visible."));
    for hidden in [
        "FloatBarrier",
        "balance",
        "phantomsection",
        "addcontentsline",
        "Hidden Entry",
        "toc",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn siunitx_commands_render_readable_text_without_raw_syntax() {
    let capture =
        capture_internal_render_ir("main.tex", SIUNITX_COMMAND_SOURCE, &SemanticAux::default());
    let extracted_text = capture.document_ir.extracted_text();
    for visible in [
        "Speed 3.5 m/s",
        "count 1200",
        "unit kg",
        "range 1--2 m",
        "macro 9 m/s",
        "freq 5 kHz",
    ] {
        assert!(
            extracted_text.contains(visible),
            "extracted text missing {visible:?}: {extracted_text:?}"
        );
    }
    for hidden in [
        r"\SI",
        r"\num",
        r"\si",
        r"\SIrange",
        r"\meter",
        r"\hertz",
        "sisetup",
        "{3.5}",
        "{m/s}",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    for visible in [
        "Speed 3.5 m/s",
        "count 1200",
        "unit kg",
        "range 1--2 m",
        "macro 9 m/s",
        "freq 5 kHz",
    ] {
        assert!(
            display_list_text.contains(visible),
            "display-list text missing {visible:?}: {display_list_text:?}"
        );
    }
    for hidden in [
        r"\SI",
        r"\num",
        r"\si",
        r"\SIrange",
        r"\meter",
        r"\hertz",
        "sisetup",
        "{3.5}",
        "{m/s}",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn printbibliography_capture_creates_empty_bibliography_without_option_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        PRINTBIBLIOGRAPHY_SOURCE,
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

    assert!(bibliography.items.is_empty());
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Before [?]."));
    for hidden in ["printbibliography", "heading", "none", "alpha"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Before [?]."));
    for hidden in ["printbibliography", "heading", "none", "alpha"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn printbibliography_reads_jobname_bbl_into_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        PRINTBIBLIOGRAPHY_SOURCE,
        &SemanticAux::default(),
        &[("main.bbl", LEGACY_BIBLIOGRAPHY_BBL_SOURCE)],
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

    assert_eq!(bibliography.items.len(), 1);
    assert_eq!(bibliography.items[0].key, "alpha");
    assert_eq!(bibliography.items[0].content, "Author. Title [?].");

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Before [?].", "Author. Title [?]."] {
        assert!(extracted_text.contains(expected), "{extracted_text}");
    }
    for hidden in ["printbibliography", "heading", "none", "alpha", "beta"] {
        assert!(!extracted_text.contains(hidden));
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
    for expected in ["Before [?].", "Author. Title [?]."] {
        assert!(display_list_text.contains(expected), "{display_list_text}");
    }
    for hidden in ["printbibliography", "heading", "none", "alpha", "beta"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn legacy_bibliography_capture_creates_empty_bibliography_without_database_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LEGACY_BIBLIOGRAPHY_SOURCE,
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

    assert!(bibliography.items.is_empty());
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Before [?]."));
    for hidden in [
        "bibliographystyle",
        "bibliography",
        "plain",
        "refs",
        "alpha",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Before [?]."));
    for hidden in [
        "bibliographystyle",
        "bibliography",
        "plain",
        "refs",
        "alpha",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn legacy_bibliography_reads_jobname_bbl_into_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        LEGACY_BIBLIOGRAPHY_WITH_BBL_SOURCE,
        &SemanticAux::default(),
        &[("main.bbl", LEGACY_BIBLIOGRAPHY_BBL_SOURCE)],
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

    assert_eq!(bibliography.items.len(), 1);
    assert_eq!(bibliography.items[0].key, "alpha");
    assert_eq!(bibliography.items[0].content, "Author. Title [?].");

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Before [?].", "After.", "Author. Title [?]."] {
        assert!(extracted_text.contains(expected), "{extracted_text}");
    }
    for hidden in ["bibliography", "refs", "alpha", "beta"] {
        assert!(!extracted_text.contains(hidden));
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
    for expected in ["Before [?].", "After.", "Author. Title [?]."] {
        assert!(display_list_text.contains(expected), "{display_list_text}");
    }
    for hidden in ["bibliography", "refs", "alpha", "beta"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn nocite_capture_does_not_leak_hidden_keys_into_ir_or_display_list() {
    let capture = capture_internal_render_ir("main.tex", NOCITE_SOURCE, &SemanticAux::default());
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

    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].keys, vec!["visible".to_string()]);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Before [?]."));
    for hidden in ["hidden", "other", "visible", "nocite", "*"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Before [?]."));
    for hidden in ["hidden", "other", "visible", "nocite", "*"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn citefield_capture_survives_ir_without_field_or_key_leakage() {
    let capture = capture_internal_render_ir("main.tex", CITEFIELD_SOURCE, &SemanticAux::default());
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

    assert_eq!(citations.len(), 3);
    assert_eq!(citations[0].keys, vec!["alpha".to_string()]);
    assert_eq!(citations[1].keys, vec!["beta".to_string()]);
    assert_eq!(citations[2].keys, vec!["gamma".to_string()]);
    for citation in citations {
        assert_eq!(citation.style_hint, CitationStyleHint::Textual);
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Fields [?], [?], and nested [?]."));
    for hidden in ["alpha", "beta", "gamma", "doi", "year", "journal"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Fields [?], [?], and nested [?]."));
    for hidden in ["alpha", "beta", "gamma", "doi", "year", "journal"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn multicite_capture_survives_ir_without_option_or_key_leakage() {
    let capture = capture_internal_render_ir("main.tex", MULTICITE_SOURCE, &SemanticAux::default());
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

    assert_eq!(citations.len(), 3);
    assert_eq!(
        citations[0].keys,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(citations[0].style_hint, CitationStyleHint::Textual);
    assert_eq!(
        citations[1].keys,
        vec!["gamma".to_string(), "delta".to_string()]
    );
    assert_eq!(citations[1].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(
        citations[2].keys,
        vec!["epsilon".to_string(), "zeta".to_string()]
    );
    assert_eq!(citations[2].style_hint, CitationStyleHint::Parenthetical);
    for citation in citations {
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Multi [?] and [?], nested [?]."),
        "{extracted_text}"
    );
    for hidden in [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "see", "chap", "cf.", "pp.", "note",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(
        display_list_text.contains("Multi [?] and [?], nested [?]."),
        "{display_list_text}"
    );
    for hidden in [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "see", "chap", "cf.", "pp.", "note",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn citetext_capture_survives_ir_without_nested_key_leakage() {
    let capture = capture_internal_render_ir("main.tex", CITETEXT_SOURCE, &SemanticAux::default());
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

    assert_eq!(citations.len(), 3);
    assert_eq!(citations[0].keys, vec!["beta".to_string()]);
    assert_eq!(citations[0].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[1].keys, vec!["alpha".to_string()]);
    assert_eq!(citations[1].style_hint, CitationStyleHint::Parenthetical);
    assert_eq!(citations[2].keys, vec!["gamma".to_string()]);
    assert_eq!(citations[2].style_hint, CitationStyleHint::Parenthetical);
    for citation in citations {
        assert_eq!(citation.display_text, "[?]");
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See compare [?] with [?], nested see [?]."),
        "{extracted_text}"
    );
    for hidden in [
        "alpha",
        "beta",
        "gamma",
        "citetext",
        "citealp",
        "citeyearpar",
        "citep",
    ] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(
        display_list_text.contains("See compare [?] with [?], nested see [?]."),
        "{display_list_text}"
    );
    for hidden in [
        "alpha",
        "beta",
        "gamma",
        "citetext",
        "citealp",
        "citeyearpar",
        "citep",
    ] {
        assert!(!display_list_text.contains(hidden));
    }
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
                == r"\ref{sec:intro}"
    ));
    assert!(references[0].source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if &REFERENCE_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "sec:intro"
            )
    }));
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
fn reference_provenance_preserves_invocation_and_key_spans() {
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

    let mut provenance_cases = Vec::new();
    for (reference, invocation_text, key_text) in [
        (references[0], r"\ref{sec:intro}", "sec:intro"),
        (references[1], r"\eqref{eq:main}", "eq:main"),
        (references[2], r"\cref{fig:a,tab:b}", "fig:a,tab:b"),
    ] {
        assert!(matches!(
            &reference.source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &REFERENCE_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == invocation_text
        ));
        assert!(reference.source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &REFERENCE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == key_text
                )
        }));

        provenance_cases.push(serde_json::json!({
            "command": reference.command,
            "keys": reference.keys,
            "display_text": reference.display_text,
            "source": reference.source,
        }));
    }

    let reference_events = capture
        .events
        .events
        .iter()
        .filter(|envelope| matches!(&envelope.event, RenderEvent::InlineReference(_)))
        .collect::<Vec<_>>();
    assert_eq!(reference_events.len(), 3);
    for envelope in &reference_events {
        assert_eq!(envelope.meta.mode_hint, ModeHint::Horizontal);
    }
    let event_cases = reference_events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineReference(reference) => Some(serde_json::json!({
                "event": reference,
                "meta": envelope.meta,
            })),
            _ => None,
        })
        .collect::<Vec<_>>();
    let provenance_snapshot = serde_json::json!({
        "source": REFERENCE_SOURCE,
        "events": event_cases,
        "ir": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/reference.provenance.json",
        &provenance_json,
    );
}

#[test]
fn direct_inline_command_aliases_survive_ir_without_hidden_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        DIRECT_INLINE_ALIAS_SOURCE,
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

    assert_eq!(capture.document_ir.labels.len(), 1);
    assert_eq!(capture.document_ir.labels[0].key, "sec:intro");
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()]
                    && citation.display_text == "[?]"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
                    && reference.display_text == "[?]"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["fig:a".to_string(), "fig:b".to_string()]
                    && reference.display_text == "[?]"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Link(link)
                if link.target == "https://hidden.test" && link.display_text == "paper link"
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
    assert!(
        display_list_text.contains("See [?], [?], [?], and paper link."),
        "{display_list_text}"
    );
    for hidden in ["key", "sec:intro", "fig:a", "fig:b", "https://hidden.test"] {
        assert!(!display_list_text.contains(hidden), "{display_list_text}");
    }
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link) if link.target == "https://hidden.test"
        )
    }));
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

    assert_eq!(references.len(), 5);
    assert_eq!(references[0].command, "ref");
    assert_eq!(references[0].keys, vec!["sec:intro".to_string()]);
    assert_eq!(references[1].command, "autoref");
    assert_eq!(references[1].keys, vec!["fig:plot".to_string()]);
    assert_eq!(references[2].command, "Cref");
    assert_eq!(references[2].keys, vec!["tab:data".to_string()]);
    assert_eq!(references[3].command, "eqref");
    assert_eq!(references[3].keys, vec!["eq:main".to_string()]);
    assert_eq!(references[3].display_text, "(?)");
    assert_eq!(references[4].command, "nameref");
    assert_eq!(references[4].keys, vec!["sec:name".to_string()]);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [?], [?], [?], (?), and [?]."));
    for hidden in ["sec:intro", "fig:plot", "tab:data", "eq:main", "sec:name"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("See [?], [?], [?], (?), and [?]."));
    for hidden in ["sec:intro", "fig:plot", "tab:data", "eq:main", "sec:name"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn starred_reference_provenance_preserves_starred_invocation_and_key_spans() {
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

    let mut provenance_cases = Vec::new();
    for (reference, invocation_text, key_text) in [
        (references[0], r"\ref*{sec:intro}", "sec:intro"),
        (references[1], r"\autoref*{fig:plot}", "fig:plot"),
        (references[2], r"\Cref*{tab:data}", "tab:data"),
        (references[3], r"\eqref*{eq:main}", "eq:main"),
        (references[4], r"\nameref*{sec:name}", "sec:name"),
    ] {
        assert!(matches!(
            &reference.source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &STARRED_REFERENCE_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == invocation_text
        ));
        assert!(reference.source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &STARRED_REFERENCE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == key_text
                )
        }));

        provenance_cases.push(serde_json::json!({
            "command": reference.command,
            "keys": reference.keys,
            "display_text": reference.display_text,
            "source": reference.source,
        }));
    }

    let event_cases = capture
        .events
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RenderEvent::InlineReference(reference) => Some(serde_json::json!({
                "event": reference,
                "meta": envelope.meta,
            })),
            _ => None,
        })
        .collect::<Vec<_>>();
    let provenance_snapshot = serde_json::json!({
        "source": STARRED_REFERENCE_SOURCE,
        "events": event_cases,
        "ir": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/starred-reference.provenance.json",
        &provenance_json,
    );
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
fn theorem_reference_capture_survives_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        THEOREM_REFERENCE_SOURCE,
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
        ("thmref", "thm:one"),
        ("Thmref", "thm:two"),
        ("subeqref", "eq:part"),
    ];
    assert_eq!(references.len(), expected.len());
    for (reference, (command, key)) in references.iter().zip(expected) {
        assert_eq!(reference.command, command);
        assert_eq!(reference.keys, vec![key.to_string()]);
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [?], [?], and [?]."));
    assert!(!extracted_text.contains("thm:one"));
    assert!(!extracted_text.contains("thm:two"));
    assert!(!extracted_text.contains("eq:part"));

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
    assert!(!display_list_text.contains("thm:one"));
    assert!(!display_list_text.contains("thm:two"));
    assert!(!display_list_text.contains("eq:part"));
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
fn reference_range_alias_capture_survives_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        REFERENCE_RANGE_ALIAS_SOURCE,
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
        ("pagerefrange", vec!["page:a", "page:b"]),
        ("vpagerefrange", vec!["vp:a", "vp:b"]),
        ("vrefrange", vec!["sec:a", "sec:b"]),
        ("Vrefrange", vec!["chap:a", "chap:b"]),
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
    assert!(extracted_text.contains("See [?], [?], [?], and [?]."));
    for label in [
        "page:a", "page:b", "vp:a", "vp:b", "sec:a", "sec:b", "chap:a", "chap:b",
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
    assert!(display_list_text.contains("See [?], [?], [?], and [?]."));
    for label in [
        "page:a", "page:b", "vp:a", "vp:b", "sec:a", "sec:b", "chap:a", "chap:b",
    ] {
        assert!(!display_list_text.contains(label));
    }
}

#[test]
fn reference_range_wrapper_macro_capture_survives_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        REFERENCE_RANGE_WRAPPER_SOURCE,
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
        ("crefrange", vec!["fig:a", "fig:b"]),
        ("crefrange", vec!["sec:a", "sec:b"]),
    ];
    assert_eq!(references.len(), expected.len());
    for (reference, (command, keys)) in references.iter().zip(expected.iter()) {
        assert_eq!(reference.command, *command);
        assert_eq!(
            reference.keys,
            keys.iter().map(|key| key.to_string()).collect::<Vec<_>>()
        );
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
    assert!(display_list_text.contains("See [?] and [?]."));
    for hidden in ["fig:a", "fig:b", "sec:a", "sec:b"] {
        assert!(!display_list_text.contains(hidden), "{display_list_text}");
    }
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
fn link_wrapper_macro_capture_survives_ir_and_display_list_annotations() {
    let capture =
        capture_internal_render_ir("main.tex", LINK_WRAPPER_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");

    for (target, display_text) in [
        ("https://hidden.test", "paper link"),
        ("https://alias.test", "alias link"),
    ] {
        let event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.target == target
                            && link.text == display_text
                            && link.command == "href"
                )
            })
            .expect("link event");
        assert!(matches!(
            &event.meta.source.primary,
            ProvenanceSpan::File(span)
                if &LINK_WRAPPER_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                    == display_text
        ));

        assert!(paragraph.content.iter().any(|node| {
            matches!(
                node,
                InlineNode::Link(link)
                    if link.target == target
                        && link.display_text == display_text
                        && matches!(
                            &link.source.primary,
                            ProvenanceSpan::File(span)
                                if &LINK_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                        )
            )
        }));
        assert!(capture.page_display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::LinkAnnotation(link) if link.target == target && link.rect.width > 0.0
            )
        }));
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Read paper link and alias link."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains("https://alias.test"));
    assert!(!extracted_text.contains(r"\mylink"));
    assert!(!extracted_text.contains(r"\paperlink"));

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
        display_list_text.contains("Read paper link and alias link."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains("https://alias.test"));
}

#[test]
fn constant_target_link_wrapper_macro_capture_survives_ir_and_display_list_annotations() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CONSTANT_TARGET_LINK_WRAPPER_SOURCE,
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

    for (target, display_text) in [
        ("https://constant.test", "docs"),
        ("https://constant.test", "guide"),
        ("https://doi.org/10.1000/foo", "10.1000/foo"),
        ("https://constant.test", "manual"),
        ("https://doi.org/10.1000/default", "10.1000/default"),
    ] {
        let event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.target == target
                            && link.text == display_text
                            && link.command == "href"
                )
            })
            .expect("link event");
        assert!(matches!(
            &event.meta.source.primary,
            ProvenanceSpan::File(span)
                if &CONSTANT_TARGET_LINK_WRAPPER_SOURCE
                    [span.start_utf8 as usize..span.end_utf8 as usize]
                    == display_text
        ));

        assert!(paragraph.content.iter().any(|node| {
            matches!(
                node,
                InlineNode::Link(link)
                    if link.target == target
                        && link.display_text == display_text
                        && matches!(
                            &link.source.primary,
                            ProvenanceSpan::File(span)
                                if &CONSTANT_TARGET_LINK_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                        )
            )
        }));
    }

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://constant.test" && link.rect.width > 0.0
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://doi.org/10.1000/foo" && link.rect.width > 0.0
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://doi.org/10.1000/default" && link.rect.width > 0.0
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
    assert!(
        display_list_text.contains("Read docs, guide, 10.1000/foo, manual, and 10.1000/default."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("https://constant.test"));
    assert!(!display_list_text.contains("https://doi.org/"));
    assert!(!display_list_text.contains(r"\doclink"));
    assert!(!display_list_text.contains(r"\aliasdoclink"));
    assert!(!display_list_text.contains(r"\doilink"));
}

#[test]
fn link_provenance_preserves_text_target_and_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (command, display_text, target_text, invocation_text, target_argument) in [
        (
            "href",
            "paper link",
            "https://example.test/paper",
            r"\href{https://example.test/paper}{paper link}",
            Some("https://example.test/paper"),
        ),
        (
            "url",
            "https://example.test/raw",
            "https://example.test/raw",
            r"\url{https://example.test/raw}",
            None,
        ),
        (
            "url",
            "https://example.test/delimited",
            "https://example.test/delimited",
            r"\url|https://example.test/delimited|",
            None,
        ),
    ] {
        let link_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.command == command
                            && link.text == display_text
                            && link.target == target_text
                )
            })
            .expect("link event");
        assert_eq!(link_event.meta.mode_hint, ModeHint::Horizontal);
        let link = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Link(link)
                    if link.display_text == display_text && link.target == target_text =>
                {
                    Some(link)
                }
                _ => None,
            })
            .expect("link ir");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "link text run for {display_text}");

        for source in [&link_event.meta.source, &link.source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &LINK_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }
        for text_run in &text_runs {
            let source = &text_run.source;
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &LINK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &LINK_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }

        provenance_cases.push(serde_json::json!({
            "command": command,
            "event": {
                "event": link_event.event,
                "meta": link_event.meta,
            },
            "ir": link,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": LINK_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/link.provenance.json",
        &provenance_json,
    );
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
                    && link.display_text == "see [?], [?], [?], and [?]"
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Read see [?], [?], [?], and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("cited"));
    assert!(!extracted_text.contains("starred"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("sec:starred"));
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
        display_list_text.contains("Read see [?], [?], [?], and [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("cited"));
    assert!(!display_list_text.contains("starred"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("sec:starred"));
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
fn hyperref_visible_text_provenance_preserves_invocation_and_target_spans() {
    let capture = capture_internal_render_ir(
        "main.tex",
        HYPERREF_VISIBLE_TEXT_SOURCE,
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

    let mut provenance_cases = Vec::new();
    for (display_text, invocation_text, target_argument) in [
        ("intro", r"\hyperref[sec:intro]{intro}", "sec:intro"),
        (
            "anchor text",
            r"\hyperlink{hidden-anchor}{anchor text}",
            "hidden-anchor",
        ),
        (
            "target text",
            r"\hypertarget{target-id}{target text}",
            "target-id",
        ),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("hyperref text event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("hyperref text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &HYPERREF_VISIBLE_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !text_runs.is_empty(),
            "hyperref text run for {display_text}"
        );

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &HYPERREF_VISIBLE_TEXT_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &HYPERREF_VISIBLE_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Argument
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &HYPERREF_VISIBLE_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == target_argument
                    )
            }));
        }
        for text_run in &text_runs {
            let source = &text_run.source;
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &HYPERREF_VISIBLE_TEXT_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &HYPERREF_VISIBLE_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Argument
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &HYPERREF_VISIBLE_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == target_argument
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": HYPERREF_VISIBLE_TEXT_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/hyperref-visible.provenance.json",
        &provenance_json,
    );
}

#[test]
fn nohyper_suppresses_links_while_preserving_visible_text() {
    let capture = capture_internal_render_ir("main.tex", NOHYPER_SOURCE, &SemanticAux::default());
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "NoHyper" => Some(environment),
            _ => None,
        })
        .expect("NoHyper environment");

    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "paper"
        )
    }));
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "https://visible.test/raw"
        )
    }));
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(
        !environment
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::Link(_)))
    );

    let expected = "Read paper and https://visible.test/raw with [?].";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected), "{extracted_text}");
    assert!(!extracted_text.contains("https://hidden.test"));
    assert!(!extracted_text.contains(r"\href"));
    assert!(!extracted_text.contains("{key}"));

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
    assert!(!display_list_text.contains("https://hidden.test"));
    assert!(!display_list_text.contains(r"\href"));
    assert!(!display_list_text.contains("{key}"));
    assert!(
        !capture.page_display_lists[0]
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::LinkAnnotation(_)))
    );
}

#[test]
fn nohyper_link_provenance_preserves_visible_text_and_invocation_spans() {
    let capture = capture_internal_render_ir("main.tex", NOHYPER_SOURCE, &SemanticAux::default());
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "NoHyper" => Some(environment),
            _ => None,
        })
        .expect("NoHyper environment");

    let mut provenance_cases = Vec::new();
    for (display_text, invocation_text, target_argument) in [
        (
            "paper",
            r"\href{https://hidden.test}{paper}",
            Some("https://hidden.test"),
        ),
        (
            "https://visible.test/raw",
            r"\url{https://visible.test/raw}",
            None,
        ),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("NoHyper text event");
        let ir_source = environment
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("NoHyper text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NOHYPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "NoHyper text run for {display_text}");

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NOHYPER_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NOHYPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &NOHYPER_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }
        for text_run in &text_runs {
            let source = &text_run.source;
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NOHYPER_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NOHYPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &NOHYPER_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NOHYPER_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nohyper-link.provenance.json",
        &provenance_json,
    );
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
fn url_text_wrapper_provenance_preserves_content_and_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (display_text, invocation_text) in [
        (
            "https://example.test/paper",
            r"\nolinkurl{https://example.test/paper}",
        ),
        (
            "https://example.test/delimited",
            r"\nolinkurl|https://example.test/delimited|",
        ),
        ("/tmp/archive", r"\path{/tmp/archive}"),
        ("/var/tmp", r"\path|/var/tmp|"),
        (r"\foo+*", r"\detokenize{\foo+*}"),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("url-like text event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("url-like text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &URL_TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !text_runs.is_empty(),
            "url-like text run for {display_text}"
        );

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &URL_TEXT_WRAPPER_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &URL_TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &URL_TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": URL_TEXT_WRAPPER_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/url-text-wrapper.provenance.json",
        &provenance_json,
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
fn text_wrapper_provenance_preserves_content_and_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (display_text, invocation_text) in [
        ("important", r"\emph{important}"),
        ("bold text", r"\textbf{bold text}"),
        ("code_path", r"\texttt{code_path}"),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("text wrapper event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("text wrapper IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !text_runs.is_empty(),
            "text wrapper text run for {display_text}"
        );

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &TEXT_WRAPPER_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &TEXT_WRAPPER_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": TEXT_WRAPPER_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/text-wrapper.provenance.json",
        &provenance_json,
    );
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
            InlineNode::Citation(citation) if citation.keys == vec!["starred".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["fig:a".to_string(), "fig:b".to_string()]
                    && reference.command == "crefrange"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:starred".to_string()]
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested important [?], [?], [?], [?], and [?] text."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("{important"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("starred"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("fig:a"));
    assert!(!extracted_text.contains("fig:b"));
    assert!(!extracted_text.contains("sec:starred"));

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
        display_list_text.contains("Nested important [?], [?], [?], [?], and [?] text."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("{important"));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("starred"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("fig:a"));
    assert!(!display_list_text.contains("fig:b"));
    assert!(!display_list_text.contains("sec:starred"));
}

#[test]
fn nested_text_wrapper_inline_provenance_preserves_invocation_and_key_spans() {
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
    let mut provenance_cases = Vec::new();

    let citation_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineCitation(citation)
                    if citation.keys == vec!["key".to_string()]
            )
        })
        .expect("nested citation event");
    let citation_inline = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Citation(citation) if citation.keys == vec!["key".to_string()] => {
                Some(citation)
            }
            _ => None,
        })
        .expect("nested citation IR node");
    let citation_text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run)
                if matches!(
                    &run.source.primary,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\cite{key}"
                ) =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!citation_text_runs.is_empty(), "citation text run");

    for source in [&citation_event.meta.source, &citation_inline.source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\cite{key}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::CitationKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                )
        }));
    }
    for text_run in &citation_text_runs {
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::CitationKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                )
        }));
    }
    provenance_cases.push(serde_json::json!({
        "case": "citation",
        "event": {
            "event": citation_event.event,
            "meta": citation_event.meta,
        },
        "ir_source": citation_inline.source,
        "display_list": citation_text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    }));

    let reference_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineReference(reference)
                    if reference.keys == vec!["sec:intro".to_string()]
                        && reference.command == "ref"
            )
        })
        .expect("nested reference event");
    let reference_inline = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
                    && reference.command == "ref" =>
            {
                Some(reference)
            }
            _ => None,
        })
        .expect("nested reference IR node");
    let reference_text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run)
                if matches!(
                    &run.source.primary,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\ref{sec:intro}"
                ) =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!reference_text_runs.is_empty(), "reference text run");

    for source in [&reference_event.meta.source, &reference_inline.source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\ref{sec:intro}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "sec:intro"
                )
        }));
    }
    for text_run in &reference_text_runs {
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "sec:intro"
                )
        }));
    }
    provenance_cases.push(serde_json::json!({
        "case": "reference",
        "event": {
            "event": reference_event.event,
            "meta": reference_event.meta,
        },
        "ir_source": reference_inline.source,
        "display_list": reference_text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    }));

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-inline.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_link_provenance_preserves_text_target_and_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (display_text, target, invocation_text, target_argument) in [
        (
            "paper",
            "https://hidden.test",
            r"\href{https://hidden.test}{paper}",
            Some("https://hidden.test"),
        ),
        (
            "https://shown.test",
            "https://shown.test",
            r"\url{https://shown.test}",
            None,
        ),
    ] {
        let link_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.text == display_text && link.target == target
                )
            })
            .expect("nested link event");
        let ir_link = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Link(link)
                    if link.display_text == display_text && link.target == target =>
                {
                    Some(link)
                }
                _ => None,
            })
            .expect("nested link IR node");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let annotations = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::LinkAnnotation(link) if link.target == target => Some(link),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "link text run for {display_text}");
        assert!(
            !annotations.is_empty(),
            "link annotation for {display_text}"
        );

        for source in [&link_event.meta.source, &ir_link.source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for annotation in &annotations {
            assert!(annotation.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": link_event.event,
                "meta": link_event.meta,
            },
            "ir_source": ir_link.source,
            "display_list": {
                "text_runs": text_runs
                    .iter()
                    .map(|run| serde_json::json!({
                        "text": run.text,
                        "source": run.source,
                        "clusters": run.clusters,
                    }))
                    .collect::<Vec<_>>(),
                "annotations": annotations
                    .iter()
                    .map(|annotation| serde_json::json!({
                        "target": annotation.target,
                        "source": annotation.source,
                    }))
                    .collect::<Vec<_>>(),
            },
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_LINK_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-link.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_label_provenance_preserves_key_and_invocation_spans() {
    let mut provenance_cases = Vec::new();

    for (case, source) in [
        ("direct", NESTED_TEXT_WRAPPER_LABEL_SOURCE),
        (
            "unknown_command",
            NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LABEL_SOURCE,
        ),
    ] {
        let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
        let label_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(&envelope.event, RenderEvent::LabelDefinition(label) if label.key == "sec:intro")
            })
            .expect("nested label event");
        let label = capture
            .document_ir
            .labels
            .iter()
            .find(|label| label.key == "sec:intro")
            .expect("nested label IR");

        for source_provenance in [&label_event.meta.source, &label.source] {
            assert!(matches!(
                &source_provenance.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == "sec:intro"
            ));
            assert!(source_provenance.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == r"\label{sec:intro}"
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "case": case,
            "source": source,
            "event": {
                "event": label_event.event,
                "meta": label_event.meta,
            },
            "ir": label,
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-label.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_math_provenance_preserves_body_and_delimiter_spans() {
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

    let mut provenance_cases = Vec::new();
    for (raw_source, invocation_text) in [("x^2", "$x^2$"), ("y^2", r"\(y^2\)")] {
        let math_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineMath(math) if math.raw_source == raw_source
                )
            })
            .expect("inline math event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::InlineMath {
                    raw_source: node_raw_source,
                    source,
                    ..
                } if node_raw_source == raw_source => Some(source),
                _ => None,
            })
            .expect("inline math IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if run.text == raw_source => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "text run for {raw_source}");

        for source in [&math_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_MATH_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == raw_source
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for text_run in &text_runs {
            assert!(matches!(
                &text_run.source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_MATH_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == raw_source
            ));
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "raw_source": raw_source,
            "event": {
                "event": math_event.event,
                "meta": math_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_MATH_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-math.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_provenance_preserves_inner_wrapper_content_and_invocation_spans() {
    let mut provenance_cases = Vec::new();

    for (case, source, display_text, invocation_text) in [
        (
            "direct",
            NESTED_TEXT_WRAPPER_WRAPPER_SOURCE,
            "inner text",
            r"\textbf{inner text}",
        ),
        (
            "unknown_command",
            NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_TEXT_WRAPPER_SOURCE,
            "inner text",
            r"\textbf{inner text}",
        ),
    ] {
        let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());
        let paragraph = capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .expect("paragraph");
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("inner wrapper text event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("inner wrapper text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "text run for {case}");

        for source_provenance in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source_provenance.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source_provenance.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "case": case,
            "source": source,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-text-wrapper.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_unknown_command_provenance_preserves_content_and_invocation_spans() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE,
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

    let text_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Text(text) if text.text == "visible text"
            )
        })
        .expect("unknown command text event");
    let ir_source = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Text { text, source } if text == "visible text" => Some(source),
            _ => None,
        })
        .expect("unknown command text IR source");
    let text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "visible text" => Some(run),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!text_runs.is_empty(), "visible text run");

    for source in [&text_event.meta.source, ir_source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "visible text"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\unknowntext{visible text}"
                )
        }));
    }
    for text_run in &text_runs {
        assert!(matches!(
            &text_run.source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "visible text"
        ));
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\unknowntext{visible text}"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE,
        "event": {
            "event": text_event.event,
            "meta": text_event.meta,
        },
        "ir_source": ir_source,
        "display_list": text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-unknown-command.provenance.json",
        &provenance_json,
    );
}

#[test]
fn declared_top_level_wrapper_survives_ir_without_raw_command_noise() {
    for source in [
        DECLARED_TOP_LEVEL_WRAPPER_SOURCE,
        DEF_DECLARED_TOP_LEVEL_WRAPPER_SOURCE,
    ] {
        let capture = capture_internal_render_ir("main.tex", source, &SemanticAux::default());

        let expected_text = "A TODO: check [?], [?], and paper B.";
        let extracted_text = capture.document_ir.extracted_text();
        assert!(extracted_text.contains(expected_text), "{extracted_text}");
        assert!(!extracted_text.contains("reviewnote"));
        assert!(!extracted_text.contains("color"));
        assert!(!extracted_text.contains("red"));
        assert!(!extracted_text.contains("key"));
        assert!(!extracted_text.contains("sec:intro"));
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
            display_list_text.contains(expected_text),
            "{display_list_text}"
        );
        assert!(!display_list_text.contains("reviewnote"));
        assert!(!display_list_text.contains("color"));
        assert!(!display_list_text.contains("red"));
        assert!(!display_list_text.contains("key"));
        assert!(!display_list_text.contains("sec:intro"));
        assert!(!display_list_text.contains("https://hidden.test"));
    }
}

#[test]
fn color_decoration_commands_preserve_visible_text_without_color_noise() {
    let capture =
        capture_internal_render_ir("main.tex", COLOR_DECORATION_SOURCE, &SemanticAux::default());

    let expected_text = "A colored word and visible [?] plus boxed [?] and framed paper.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    for hidden in [
        "colorbox",
        "fcolorbox",
        "magenta",
        "cyan",
        "yellow",
        "black",
        "white",
        "key",
        "sec:intro",
        "https://hidden.test",
    ] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    assert!(
        display_list_text.contains(expected_text),
        "{display_list_text}"
    );
    for hidden in [
        "colorbox",
        "fcolorbox",
        "magenta",
        "cyan",
        "yellow",
        "black",
        "white",
        "key",
        "sec:intro",
        "https://hidden.test",
    ] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn color_decoration_provenance_preserves_visible_content_and_invocation_spans() {
    let capture =
        capture_internal_render_ir("main.tex", COLOR_DECORATION_SOURCE, &SemanticAux::default());
    let paragraph = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("paragraph");

    let mut provenance_cases = Vec::new();
    for (display_text, content_source, invocation_text, argument_sources) in [
        (
            "visible [?]",
            r"visible \cite{key}",
            r"\textcolor{cyan}{visible \cite{key}}",
            &["cyan"][..],
        ),
        (
            "boxed [?]",
            r"boxed \ref{sec:intro}",
            r"\colorbox{yellow}{boxed \ref{sec:intro}}",
            &["yellow"][..],
        ),
        (
            "framed paper",
            r"framed \href{https://hidden.test}{paper}",
            r"\fcolorbox{black}{white}{framed \href{https://hidden.test}{paper}}",
            &["black", "white"][..],
        ),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("color wrapper text event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("color wrapper text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &COLOR_DECORATION_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == content_source
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !text_runs.is_empty(),
            "color wrapper text run for {display_text}"
        );

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &COLOR_DECORATION_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == content_source
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &COLOR_DECORATION_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            for argument_source in argument_sources {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &COLOR_DECORATION_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == *argument_source
                        )
                }));
            }
        }
        for text_run in &text_runs {
            let source = &text_run.source;
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &COLOR_DECORATION_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            for argument_source in argument_sources {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &COLOR_DECORATION_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == *argument_source
                        )
                }));
            }
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": COLOR_DECORATION_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/color-wrapper.provenance.json",
        &provenance_json,
    );
}

#[test]
fn input_file_content_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", INPUT_CHILD_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "Included"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Before.", "Included", "See [?] and [?].", "After."] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["input", "child", "key", "sec:intro"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["Before.", "Included", "See [?] and [?].", "After."] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["input", "child", "key", "sec:intro"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn input_file_heading_provenance_points_to_included_source() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", INPUT_CHILD_SOURCE)],
    );
    let heading_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Heading(heading) if heading.text == "Included"
            )
        })
        .expect("heading event");
    let heading_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "Included"
                ) =>
            {
                Some(heading)
            }
            _ => None,
        })
        .expect("heading block");
    let heading_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "Included" => Some(run),
            _ => None,
        })
        .expect("heading display-list run");

    for source in [
        &heading_event.meta.source,
        &heading_block.source,
        &heading_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "child.tex"
                    && &INPUT_CHILD_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "Included"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "child.tex"
                            && &INPUT_CHILD_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\section{Included}"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "main_source": INPUT_MAIN_SOURCE,
        "child_source": INPUT_CHILD_SOURCE,
        "event": {
            "event": heading_event.event,
            "meta": heading_event.meta,
        },
        "ir": {
            "level": heading_block.level,
            "content": heading_block.content,
            "source": heading_block.source,
        },
        "display_list": {
            "text": heading_run.text,
            "source": heading_run.source,
            "clusters": heading_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/input.provenance.json",
        &provenance_json,
    );
}

#[test]
fn unbraced_input_and_include_files_survive_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        UNBRACED_INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[
            ("child.tex", UNBRACED_INPUT_CHILD_SOURCE),
            ("second.tex", UNBRACED_INCLUDE_CHILD_SOURCE),
        ],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "Unbraced Input"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in [
        "Before.",
        "Unbraced Input",
        "See [?].",
        "Second body.",
        "After.",
    ] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["input", "include", "child", "second", "key"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in [
        "Before.",
        "Unbraced Input",
        "See [?].",
        "Second body.",
        "After.",
    ] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["input", "include", "child", "second", "key"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn at_input_files_honor_endinput_in_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        AT_INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", AT_INPUT_CHILD_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "At Input"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Before.", "At Input", "Included [?].", "After."] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["@input", "child", "endinput", "Hidden", "key", "hidden"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["Before.", "At Input", "Included [?].", "After."] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["@input", "child", "endinput", "Hidden", "key", "hidden"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn input_files_reuse_declared_section_and_wrapper_state() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        INPUT_SHARED_STATE_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", INPUT_SHARED_STATE_CHILD_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "Included"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Included", "TODO: check [?]"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["Included", "TODO: check [?]"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn input_macro_heading_provenance_preserves_call_and_definition_files() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        INPUT_SHARED_STATE_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", INPUT_SHARED_STATE_CHILD_SOURCE)],
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "Included",
        "child.tex",
        INPUT_SHARED_STATE_CHILD_SOURCE,
        r"\mysection{Included}",
        "main.tex",
        INPUT_SHARED_STATE_MAIN_SOURCE,
        r"\newcommand{\mysection}[1]{\section{#1}}",
        &[
            ("main_source", INPUT_SHARED_STATE_MAIN_SOURCE),
            ("child_source", INPUT_SHARED_STATE_CHILD_SOURCE),
        ],
        "tests/goldens/render_ir/input-macro-heading.provenance.json",
    );
}

#[test]
fn preamble_input_macros_are_reused_in_document_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        PREAMBLE_INPUT_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("macros.tex", PREAMBLE_INPUT_MACRO_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "From Preamble"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["From Preamble", "TODO: check [?]"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in [
        "input",
        "macros",
        "mysection",
        "reviewnote",
        "color",
        "red",
        "key",
    ] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["From Preamble", "TODO: check [?]"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in [
        "input",
        "macros",
        "mysection",
        "reviewnote",
        "color",
        "red",
        "key",
    ] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn preamble_input_macro_heading_provenance_preserves_definition_file() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        PREAMBLE_INPUT_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("macros.tex", PREAMBLE_INPUT_MACRO_SOURCE)],
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "From Preamble",
        "main.tex",
        PREAMBLE_INPUT_MACRO_MAIN_SOURCE,
        r"\mysection{From Preamble}",
        "macros.tex",
        PREAMBLE_INPUT_MACRO_SOURCE,
        r"\newcommand{\mysection}[1]{\section{#1}}",
        &[
            ("main_source", PREAMBLE_INPUT_MACRO_MAIN_SOURCE),
            ("macros_source", PREAMBLE_INPUT_MACRO_SOURCE),
        ],
        "tests/goldens/render_ir/preamble-input-macro-heading.provenance.json",
    );
}

#[test]
fn cross_file_optional_default_wrapper_provenance_survives_ir_and_display_list() {
    let macros = r"\newcommand{\defaultcite}[1][core]{\cite{#1}}
\newcommand{\defaultref}[1][sec:intro]{\ref{#1}}
\newcommand{\defaultlabel}[1][sec:intro]{\label{#1}}
\newcommand{\defaultdoclink}[1][guide]{\href{https://constant.test}{#1}}";
    let source = r"\input{macros}
\begin{document}
\section{Intro}\defaultlabel
See \defaultcite, \defaultref, and \defaultdoclink.
\end{document}";
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        source,
        &SemanticAux::default(),
        &[("macros.tex", macros)],
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
    let citation = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Citation(citation) => Some(citation),
            _ => None,
        })
        .expect("citation");
    assert_eq!(citation.keys, vec!["core".to_string()]);
    assert!(matches!(
        &citation.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize] == r"\defaultcite"
    ));
    assert!(citation.source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "core"
            )
    }));

    let reference = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .expect("reference");
    assert_eq!(reference.keys, vec!["sec:intro".to_string()]);
    assert!(matches!(
        &reference.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize] == r"\defaultref"
    ));
    assert!(reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
            )
    }));

    let link = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Link(link) => Some(link),
            _ => None,
        })
        .expect("link");
    assert_eq!(link.target, "https://constant.test");
    assert_eq!(link.display_text, "guide");
    assert!(matches!(
        &link.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "macros.tex"
                && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "guide"
    ));
    assert!(link.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == r"\defaultdoclink"
            )
    }));

    let label = capture
        .document_ir
        .labels
        .iter()
        .find(|label| label.key == "sec:intro")
        .expect("label");
    assert!(matches!(
        &label.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "macros.tex"
                && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
    ));
    assert!(label.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == r"\defaultlabel"
            )
    }));

    let destination = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::NamedDestination(destination) if destination.name == "sec:intro" => {
                Some(destination)
            }
            _ => None,
        })
        .expect("default label should emit a named destination");
    assert!(matches!(
        &destination.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "macros.tex"
                && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
    ));
    assert!(destination.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == r"\defaultlabel"
            )
    }));

    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "guide"
                    && matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "macros.tex"
                                && &macros[span.start_utf8 as usize..span.end_utf8 as usize]
                                    == "guide"
                    )
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "[?]"
                    && run.source.related.iter().any(|related| {
                        related.role == SourceSpanRole::CitationKey
                            && matches!(
                                &related.span,
                                ProvenanceSpan::File(span)
                                    if span.path.as_str() == "macros.tex"
                                        && &macros
                                            [span.start_utf8 as usize..span.end_utf8 as usize]
                                            == "core"
                            )
                    })
        )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::TextRun(run)
                if run.text == "[?]"
                    && run.source.related.iter().any(|related| {
                        related.role == SourceSpanRole::ReferenceKey
                            && matches!(
                                &related.span,
                                ProvenanceSpan::File(span)
                                    if span.path.as_str() == "macros.tex"
                                        && &macros
                                            [span.start_utf8 as usize..span.end_utf8 as usize]
                                            == "sec:intro"
                            )
                    })
        )
    }));
}

#[test]
fn cross_file_optional_default_link_target_provenance_survives_ir_and_display_list() {
    let macros = r"\newcommand{\defaulttargetlink}[2][https://default.test]{\href{#1}{#2}}";
    let source = r"\input{macros}
\begin{document}
Read \defaulttargetlink{visible link}.
\end{document}";
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        source,
        &SemanticAux::default(),
        &[("macros.tex", macros)],
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
    let link = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Link(link) => Some(link),
            _ => None,
        })
        .expect("link");
    assert_eq!(link.target, "https://default.test");
    assert_eq!(link.display_text, "visible link");
    assert!(matches!(
        &link.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "visible link"
    ));
    assert!(link.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Invocation
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == r"\defaulttargetlink{visible link}"
            )
    }));
    assert!(link.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Argument
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize]
                            == "https://default.test"
            )
    }));

    let display_link = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "visible link" => Some(run),
            _ => None,
        })
        .expect("link display-list run");
    assert!(matches!(
        &display_link.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "visible link"
    ));
    assert!(display_link.source.related.iter().any(|related| {
        related.role == SourceSpanRole::Argument
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize]
                            == "https://default.test"
            )
    }));
    assert!(capture.page_display_lists[0].ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://default.test" && link.rect.width > 0.0
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
    assert!(
        display_list_text.contains("Read visible link."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("https://default.test"));
    assert!(!display_list_text.contains(r"\defaulttargetlink"));
}

#[test]
fn cross_file_optional_default_range_provenance_survives_ir_and_display_list() {
    let macros = r"\newcommand{\defaultrange}[2][fig:a]{\crefrange{#1}{#2}}";
    let source = r"\input{macros}
\begin{document}
See \defaultrange{fig:b}.
\end{document}";
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        source,
        &SemanticAux::default(),
        &[("macros.tex", macros)],
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
    let reference = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .expect("reference");
    assert_eq!(reference.command, "crefrange");
    assert_eq!(
        reference.keys,
        vec!["fig:a".to_string(), "fig:b".to_string()]
    );
    assert!(matches!(
        &reference.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                    == r"\defaultrange{fig:b}"
    ));
    assert!(reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "fig:a"
            )
    }));
    assert!(reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "fig:b"
            )
    }));

    let display_reference = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "[?]" => Some(run),
            _ => None,
        })
        .expect("reference display-list run");
    assert!(display_reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "fig:a"
            )
    }));
    assert!(display_reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "fig:b"
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
    assert!(
        display_list_text.contains("See [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("fig:a"));
    assert!(!display_list_text.contains("fig:b"));
    assert!(!display_list_text.contains(r"\defaultrange"));
}

#[test]
fn cross_file_optional_default_multikey_citation_provenance_survives_ir_and_display_list() {
    let macros = r"\newcommand{\defaultcitepair}[2][core]{\cite{#1,#2}}";
    let source = r"\input{macros}
\begin{document}
See \defaultcitepair{extra}.
\end{document}";
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        source,
        &SemanticAux::default(),
        &[("macros.tex", macros)],
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
    let citation = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Citation(citation) => Some(citation),
            _ => None,
        })
        .expect("citation");
    assert_eq!(citation.keys, vec!["core".to_string(), "extra".to_string()]);
    assert!(matches!(
        &citation.source.primary,
        ProvenanceSpan::File(span)
            if span.path.as_str() == "main.tex"
                && &source[span.start_utf8 as usize..span.end_utf8 as usize]
                    == r"\defaultcitepair{extra}"
    ));
    assert!(citation.source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "core"
            )
    }));
    assert!(citation.source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "extra"
            )
    }));

    let display_citation = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "[?]" => Some(run),
            _ => None,
        })
        .expect("citation display-list run");
    assert!(display_citation.source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "macros.tex"
                        && &macros[span.start_utf8 as usize..span.end_utf8 as usize] == "core"
            )
    }));
    assert!(display_citation.source.related.iter().any(|related| {
        related.role == SourceSpanRole::CitationKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &source[span.start_utf8 as usize..span.end_utf8 as usize] == "extra"
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
    assert!(
        display_list_text.contains("See [?]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("core"));
    assert!(!display_list_text.contains("extra"));
    assert!(!display_list_text.contains(r"\defaultcitepair"));
}

#[test]
fn package_file_macros_are_reused_in_document_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        PACKAGE_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("macros.sty", PACKAGE_MACRO_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "From Package"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["From Package", "TODO: package [?]"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["macros", "mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["From Package", "TODO: package [?]"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["macros", "mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn package_macro_heading_provenance_preserves_definition_file() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        PACKAGE_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("macros.sty", PACKAGE_MACRO_SOURCE)],
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "From Package",
        "main.tex",
        PACKAGE_MACRO_MAIN_SOURCE,
        r"\mysection{From Package}",
        "macros.sty",
        PACKAGE_MACRO_SOURCE,
        r"\newcommand{\mysection}[1]{\section{#1}}",
        &[
            ("main_source", PACKAGE_MACRO_MAIN_SOURCE),
            ("package_source", PACKAGE_MACRO_SOURCE),
        ],
        "tests/goldens/render_ir/package-macro-heading.provenance.json",
    );
}

#[test]
fn class_file_macros_are_reused_in_document_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        CLASS_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("wrapper.cls", CLASS_MACRO_SOURCE)],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "From Class"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["From Class", "TODO: class [?]"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["wrapper", "mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["From Class", "TODO: class [?]"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["wrapper", "mysection", "reviewnote", "color", "red", "key"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn class_macro_heading_provenance_preserves_definition_file() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        CLASS_MACRO_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("wrapper.cls", CLASS_MACRO_SOURCE)],
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "From Class",
        "main.tex",
        CLASS_MACRO_MAIN_SOURCE,
        r"\mysection{From Class}",
        "wrapper.cls",
        CLASS_MACRO_SOURCE,
        r"\newcommand{\mysection}[1]{\section{#1}}",
        &[
            ("main_source", CLASS_MACRO_MAIN_SOURCE),
            ("class_source", CLASS_MACRO_SOURCE),
        ],
        "tests/goldens/render_ir/class-macro-heading.provenance.json",
    );
}

#[test]
fn missing_package_and_class_files_emit_render_diagnostics_without_visible_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MISSING_PACKAGE_CLASS_MAIN_SOURCE,
        &SemanticAux::default(),
    );

    for missing in ["missing class ghost.cls", "missing package missing.sty"] {
        let event = capture
            .events
            .events
            .iter()
            .find(|event| {
                matches!(
                    &event.event,
                    RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains(missing)
                )
            })
            .expect("missing diagnostic event");
        assert_eq!(event.meta.producer, EventProducer::Unknown);
        assert_eq!(event.meta.confidence, SemanticConfidence::Low);
        assert_eq!(event.meta.source.generated_by, GeneratedBy::Source);
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Visible."), "{extracted_text}");
    for hidden in ["documentclass", "usepackage", "ghost", "missing"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    assert!(
        display_list_text.contains("Visible."),
        "{display_list_text}"
    );
    for hidden in ["documentclass", "usepackage", "ghost", "missing"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn conditional_file_inputs_survive_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        CONDITIONAL_FILE_INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[
            ("sections/setup.tex", CONDITIONAL_FILE_INPUT_SETUP_SOURCE),
            ("sections/config.cfg", CONDITIONAL_FILE_INPUT_CONFIG_SOURCE),
            ("body.tex", CONDITIONAL_FILE_INPUT_BODY_SOURCE),
        ],
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == "From Config"
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    for expected in [
        "From Config",
        "TODO: found [?]",
        "Body [?].",
        "after",
        "fallback",
    ] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in [
        "InputIfFileExists",
        "IfFileExists",
        "input",
        "sections",
        "setup",
        "config",
        "body",
        "ghost",
        "missing",
        "mysection",
        "reviewnote",
        "color",
        "red",
        "key",
    ] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in [
        "From Config",
        "TODO: found [?]",
        "Body [?].",
        "after",
        "fallback",
    ] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in [
        "InputIfFileExists",
        "IfFileExists",
        "input",
        "sections",
        "setup",
        "config",
        "body",
        "ghost",
        "missing",
        "mysection",
        "reviewnote",
        "color",
        "red",
        "key",
    ] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn includeonly_limits_render_ir_include_files() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        INCLUDEONLY_MAIN_SOURCE,
        &SemanticAux::default(),
        &[
            ("first.tex", INCLUDEONLY_FIRST_SOURCE),
            ("second.tex", INCLUDEONLY_SECOND_SOURCE),
        ],
    );

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["A", "First body.", "B", "C"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["Skipped body.", "includeonly", "include", "first", "second"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["A", "First body.", "B", "C"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["Skipped body.", "includeonly", "include", "first", "second"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn missing_input_files_emit_render_diagnostics_without_visible_filename_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MISSING_INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
    );

    for missing in ["missing.tex", "missing-two.tex"] {
        assert!(capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic)
                if diagnostic.message.contains("missing input")
                    && diagnostic.message.contains(missing)
        )));
    }

    let extracted_text = capture.document_ir.extracted_text();
    for expected in ["Before", "Middle", "After"] {
        assert!(
            extracted_text.contains(expected),
            "{expected} missing in {extracted_text}"
        );
    }
    for hidden in ["input", "include", "missing"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    for expected in ["Before", "Middle", "After"] {
        assert!(
            display_list_text.contains(expected),
            "{expected} missing in {display_list_text}"
        );
    }
    for hidden in ["input", "include", "missing"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
}

#[test]
fn cyclic_input_files_are_skipped_once_in_ir_and_display_list() {
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        CYCLIC_INPUT_MAIN_SOURCE,
        &SemanticAux::default(),
        &[("child.tex", CYCLIC_INPUT_CHILD_SOURCE)],
    );

    assert!(capture.events.events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("cyclic")
    )));

    let extracted_text = capture.document_ir.extracted_text();
    assert_eq!(extracted_text.matches("Child start.").count(), 1);
    assert_eq!(extracted_text.matches("Child end.").count(), 1);
    assert!(extracted_text.contains("Root start."));
    assert!(extracted_text.contains("Root end."));
    for hidden in ["input", "child"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    assert_eq!(display_list_text.matches("Child start.").count(), 1);
    assert_eq!(display_list_text.matches("Child end.").count(), 1);
    assert!(display_list_text.contains("Root start."));
    assert!(display_list_text.contains("Root end."));
    for hidden in ["input", "child"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }
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
            InlineNode::Citation(citation) if citation.keys == vec!["starred".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["fig:a".to_string(), "fig:b".to_string()]
                    && reference.command == "crefrange"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:starred".to_string()]
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("Nested before see [?], [?], [?], [?], and [?] after."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("{see"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("starred"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("fig:a"));
    assert!(!extracted_text.contains("fig:b"));
    assert!(!extracted_text.contains("sec:starred"));

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
        display_list_text.contains("Nested before see [?], [?], [?], [?], and [?] after."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("unknowntext"));
    assert!(!display_list_text.contains("{see"));
    assert!(!display_list_text.contains("key"));
    assert!(!display_list_text.contains("starred"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("fig:a"));
    assert!(!display_list_text.contains("fig:b"));
    assert!(!display_list_text.contains("sec:starred"));
}

#[test]
fn nested_text_wrapper_unknown_command_inline_provenance_preserves_key_spans() {
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
    let mut provenance_cases = Vec::new();

    let citation_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineCitation(citation)
                    if citation.keys == vec!["key".to_string()]
            )
        })
        .expect("nested unknown citation event");
    let citation_inline = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Citation(citation) if citation.keys == vec!["key".to_string()] => {
                Some(citation)
            }
            _ => None,
        })
        .expect("nested unknown citation IR node");
    let citation_text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run)
                if matches!(
                    &run.source.primary,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\cite{key}"
                ) =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!citation_text_runs.is_empty(), "citation text run");

    for source in [&citation_event.meta.source, &citation_inline.source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\cite{key}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::CitationKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                )
        }));
    }
    for text_run in &citation_text_runs {
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::CitationKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "key"
                )
        }));
    }
    provenance_cases.push(serde_json::json!({
        "case": "citation",
        "event": {
            "event": citation_event.event,
            "meta": citation_event.meta,
        },
        "ir_source": citation_inline.source,
        "display_list": citation_text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    }));

    let reference_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineReference(reference)
                    if reference.keys == vec!["sec:intro".to_string()]
                        && reference.command == "ref"
            )
        })
        .expect("nested unknown reference event");
    let reference_inline = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
                    && reference.command == "ref" =>
            {
                Some(reference)
            }
            _ => None,
        })
        .expect("nested unknown reference IR node");
    let reference_text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run)
                if matches!(
                    &run.source.primary,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\ref{sec:intro}"
                ) =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!reference_text_runs.is_empty(), "reference text run");

    for source in [&reference_event.meta.source, &reference_inline.source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\ref{sec:intro}"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "sec:intro"
                )
        }));
    }
    for text_run in &reference_text_runs {
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == "sec:intro"
                )
        }));
    }
    provenance_cases.push(serde_json::json!({
        "case": "reference",
        "event": {
            "event": reference_event.event,
            "meta": reference_event.meta,
        },
        "ir_source": reference_inline.source,
        "display_list": reference_text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    }));

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-unknown-inline.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_unknown_command_link_math_provenance_preserves_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (display_text, target, invocation_text, target_argument) in [
        (
            "paper",
            "https://hidden.test",
            r"\href{https://hidden.test}{paper}",
            Some("https://hidden.test"),
        ),
        (
            "https://shown.test",
            "https://shown.test",
            r"\url{https://shown.test}",
            None,
        ),
    ] {
        let link_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.text == display_text && link.target == target
                )
            })
            .expect("nested unknown link event");
        let ir_link = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Link(link)
                    if link.display_text == display_text && link.target == target =>
                {
                    Some(link)
                }
                _ => None,
            })
            .expect("nested unknown link IR node");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let annotations = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::LinkAnnotation(link) if link.target == target => Some(link),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "link text run for {display_text}");
        assert!(
            !annotations.is_empty(),
            "link annotation for {display_text}"
        );

        for source in [&link_event.meta.source, &ir_link.source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for annotation in &annotations {
            assert!(annotation.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "case": "link",
            "text": display_text,
            "target": target,
            "event": {
                "event": link_event.event,
                "meta": link_event.meta,
            },
            "ir_source": ir_link.source,
            "display_list": {
                "text_runs": text_runs
                    .iter()
                    .map(|run| serde_json::json!({
                        "text": run.text,
                        "source": run.source,
                        "clusters": run.clusters,
                    }))
                    .collect::<Vec<_>>(),
                "annotations": annotations
                    .iter()
                    .map(|annotation| serde_json::json!({
                        "target": annotation.target,
                        "source": annotation.source,
                    }))
                    .collect::<Vec<_>>(),
            },
        }));
    }

    let raw_source = "x^2";
    let invocation_text = "$x^2$";
    let math_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::InlineMath(math) if math.raw_source == raw_source
            )
        })
        .expect("nested unknown inline math event");
    let ir_source = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::InlineMath {
                raw_source: node_raw_source,
                source,
                ..
            } if node_raw_source == raw_source => Some(source),
            _ => None,
        })
        .expect("nested unknown inline math IR source");
    let text_runs = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) if run.text == raw_source => Some(run),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!text_runs.is_empty(), "math text run for {raw_source}");

    for source in [&math_event.meta.source, ir_source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == raw_source
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == invocation_text
                )
        }));
    }
    for text_run in &text_runs {
        assert!(text_run.source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == invocation_text
                )
        }));
    }

    provenance_cases.push(serde_json::json!({
        "case": "inline_math",
        "raw_source": raw_source,
        "event": {
            "event": math_event.event,
            "meta": math_event.meta,
        },
        "ir_source": ir_source,
        "display_list": text_runs
            .iter()
            .map(|run| serde_json::json!({
                "text": run.text,
                "source": run.source,
                "clusters": run.clusters,
            }))
            .collect::<Vec<_>>(),
    }));

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-unknown-link-math.provenance.json",
        &provenance_json,
    );
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
            InlineNode::Citation(citation) if citation.keys == vec!["starred".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:intro".to_string()]
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["fig:a".to_string(), "fig:b".to_string()]
                    && reference.command == "crefrange"
        )
    }));
    assert!(paragraph.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Reference(reference)
                if reference.keys == vec!["sec:starred".to_string()]
        )
    }));

    let expected_text = "Nested before outer see [?], [?], [?], [?], and [?] done after.";
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(expected_text), "{extracted_text}");
    assert!(!extracted_text.contains("unknowntext"));
    assert!(!extracted_text.contains("innerunknown"));
    assert!(!extracted_text.contains("key"));
    assert!(!extracted_text.contains("starred"));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("fig:a"));
    assert!(!extracted_text.contains("fig:b"));
    assert!(!extracted_text.contains("sec:starred"));

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
    assert!(!display_list_text.contains("starred"));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("fig:a"));
    assert!(!display_list_text.contains("fig:b"));
    assert!(!display_list_text.contains("sec:starred"));
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
fn nested_text_wrapper_unknown_command_nested_unknown_link_provenance_preserves_invocation_spans() {
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

    let mut provenance_cases = Vec::new();
    for (display_text, target, invocation_text, target_argument) in [
        (
            "paper",
            "https://hidden.test",
            r"\href{https://hidden.test}{paper}",
            Some("https://hidden.test"),
        ),
        (
            "https://shown.test",
            "https://shown.test",
            r"\url{https://shown.test}",
            None,
        ),
    ] {
        let link_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::InlineLink(link)
                        if link.text == display_text && link.target == target
                )
            })
            .expect("nested inner unknown link event");
        let ir_link = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Link(link)
                    if link.display_text == display_text && link.target == target =>
                {
                    Some(link)
                }
                _ => None,
            })
            .expect("nested inner unknown link IR node");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let annotations = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::LinkAnnotation(link) if link.target == target => Some(link),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "link text run for {display_text}");
        assert!(
            !annotations.is_empty(),
            "link annotation for {display_text}"
        );

        for source in [&link_event.meta.source, &ir_link.source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
            if let Some(target_argument) = target_argument {
                assert!(source.related.iter().any(|related| {
                    related.role == SourceSpanRole::Argument
                        && matches!(
                            &related.span,
                            ProvenanceSpan::File(span)
                                if span.path.as_str() == "main.tex"
                                    && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == target_argument
                        )
                }));
            }
        }
        for text_run in &text_runs {
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for annotation in &annotations {
            assert!(annotation.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "target": target,
            "event": {
                "event": link_event.event,
                "meta": link_event.meta,
            },
            "ir_source": ir_link.source,
            "display_list": {
                "text_runs": text_runs
                    .iter()
                    .map(|run| serde_json::json!({
                        "text": run.text,
                        "source": run.source,
                        "clusters": run.clusters,
                    }))
                    .collect::<Vec<_>>(),
                "annotations": annotations
                    .iter()
                    .map(|annotation| serde_json::json!({
                        "target": annotation.target,
                        "source": annotation.source,
                    }))
                    .collect::<Vec<_>>(),
            },
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-inner-unknown-link.provenance.json",
        &provenance_json,
    );
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
fn nested_text_wrapper_unknown_command_nested_unknown_url_text_provenance_preserves_invocation_spans()
 {
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

    let mut provenance_cases = Vec::new();
    for (display_text, invocation_text) in [
        (
            "https://visible.test/path",
            r"\nolinkurl{https://visible.test/path}",
        ),
        ("/tmp/archive", r"\path|/tmp/archive|"),
        (r"\foo+*", r"\detokenize{\foo+*}"),
    ] {
        let text_event = capture
            .events
            .events
            .iter()
            .find(|envelope| {
                matches!(
                    &envelope.event,
                    RenderEvent::Text(text) if text.text == display_text
                )
            })
            .expect("URL-like text event");
        let ir_source = paragraph
            .content
            .iter()
            .find_map(|node| match node {
                InlineNode::Text { text, source } if text == display_text => Some(source),
                _ => None,
            })
            .expect("URL-like text IR source");
        let text_runs = capture.page_display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run)
                    if matches!(
                        &run.source.primary,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == display_text
                    ) =>
                {
                    Some(run)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text_runs.is_empty(), "text run for {display_text}");

        for source in [&text_event.meta.source, ir_source] {
            assert!(matches!(
                &source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }
        for text_run in &text_runs {
            assert!(matches!(
                &text_run.source.primary,
                ProvenanceSpan::File(span)
                    if span.path.as_str() == "main.tex"
                        && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == display_text
            ));
            assert!(text_run.source.related.iter().any(|related| {
                related.role == SourceSpanRole::Invocation
                    && matches!(
                        &related.span,
                        ProvenanceSpan::File(span)
                            if span.path.as_str() == "main.tex"
                                && &NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE
                                    [span.start_utf8 as usize..span.end_utf8 as usize]
                                    == invocation_text
                    )
            }));
        }

        provenance_cases.push(serde_json::json!({
            "text": display_text,
            "event": {
                "event": text_event.event,
                "meta": text_event.meta,
            },
            "ir_source": ir_source,
            "display_list": text_runs
                .iter()
                .map(|run| serde_json::json!({
                    "text": run.text,
                    "source": run.source,
                    "clusters": run.clusters,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE,
        "cases": provenance_cases,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/nested-unknown-url-text.provenance.json",
        &provenance_json,
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
fn linebreak_provenance_preserves_delimiter_and_optional_spacing_span() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LINEBREAK_OPTIONAL_SOURCE,
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
    let linebreak_event = capture
        .events
        .events
        .iter()
        .find(|envelope| matches!(&envelope.event, RenderEvent::LineBreak(_)))
        .expect("linebreak event");
    let ir_source = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::LineBreak { source } => Some(source),
            _ => None,
        })
        .expect("linebreak IR source");

    assert_eq!(linebreak_event.meta.mode_hint, ModeHint::Horizontal);
    for source in [&linebreak_event.meta.source, ir_source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &LINEBREAK_OPTIONAL_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\\[0.5em]"
        ));
    }

    let provenance_snapshot = serde_json::json!({
        "source": LINEBREAK_OPTIONAL_SOURCE,
        "event": {
            "event": linebreak_event.event,
            "meta": linebreak_event.meta,
        },
        "ir_source": ir_source,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/linebreak.provenance.json",
        &provenance_json,
    );
}

#[test]
fn tabular_capture_builds_table_ir() {
    let capture =
        capture_internal_render_ir("main.tex", TABULAR_FALLBACK_SOURCE, &SemanticAux::default());
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Alpha");
    assert_eq!(table.rows[0].cells[1].text, "Beta");
    assert_eq!(table.rows[1].cells[0].text, "Gamma");
    assert_eq!(table.rows[1].cells[1].text, "Delta");
    assert!(table.rows[1].rule_below);
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tabular")
        )
    }));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha | Beta"));
    assert!(extracted_text.contains("Gamma | Delta"));
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
        .join("\n");
    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(display_list_text.contains("Gamma | Delta"));
    assert!(!display_list_text.contains("&"));
    assert!(!display_list_text.contains("hline"));
    let display_list_pdf = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(!display_list_pdf.contains("-------------"));
    let rule_ops = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(rule_ops.len(), 1, "{rule_ops:?}");
}

#[test]
fn tabular_display_list_aligns_columns_by_cell_width() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{ll}A & Longer \\ Alpha & B\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A     | Longer"), "{table_lines:?}");
    assert!(table_lines.contains(&"Alpha | B"), "{table_lines:?}");
}

#[test]
fn tabular_column_specs_survive_ir_and_align_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lcr}A & B & Long \\ Left & Wide & 9\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Left);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Center);
    assert_eq!(table.columns[2].alignment, TableColumnAlignment::Right);
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        table_lines.contains(&"A    |  B   | Long"),
        "{table_lines:?}"
    );
    assert!(
        table_lines.contains(&"Left | Wide |    9"),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_repeated_column_specs_expand_before_display_list_alignment() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{*{3}{r}}1 & 22 & 333 \\ 4444 & 5 & 6\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 3);
    assert!(
        table
            .columns
            .iter()
            .all(|column| column.alignment == TableColumnAlignment::Right)
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"   1 | 22 | 333"), "{table_lines:?}");
    assert!(table_lines.contains(&"4444 |  5 |   6"), "{table_lines:?}");
}

#[test]
fn tabular_fixed_width_column_specs_survive_ir_and_align_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{w{r}{2cm}W{c}{1cm}}A & B \\ Long & Z\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Right);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Center);
    assert_eq!(table.columns[0].width_pt_milli, Some(56_693));
    assert_eq!(table.columns[1].width_pt_milli, Some(28_346));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"        A |   B"), "{table_lines:?}");
    assert!(table_lines.contains(&"     Long |   Z"), "{table_lines:?}");
}

#[test]
fn tabular_array_column_hooks_do_not_hide_real_columns() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{>{\raggedright\arraybackslash}p{2cm}@{\quad}!{\vrule}<{\hfill}r}Alpha & 1 \\ Beta & 22\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Left);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Right);
    assert_eq!(table.columns[0].width_pt_milli, Some(56_693));
    assert!(table.columns[0].rule_after);
    assert_eq!(table.columns[0].rule_after_count, 1);
    assert!(table.columns[1].rule_before);
    assert_eq!(table.columns[1].rule_before_count, 1);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha | 1"));
    assert!(extracted_text.contains("Beta | 22"));
    assert!(!extracted_text.contains("raggedright"));
    assert!(!extracted_text.contains("arraybackslash"));

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let vertical_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Rule(rule)
                    if rule.height > rule.width && (rule.width - 0.8).abs() < 0.001
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(vertical_rules.len(), 2, "{vertical_rules:?}");
    assert!(table_lines.contains(&"Alpha        1"), "{table_lines:?}");
    assert!(table_lines.contains(&"Beta        22"), "{table_lines:?}");
    assert!(
        !table_lines.iter().any(|line| line.contains('|')),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_intercolumn_visible_separators_drive_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{l@{--}r}A & 1 \\ B & 2\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].separator_after.as_deref(), Some("--"));
    assert!(!table.columns[0].rule_after);
    assert!(!table.columns[1].rule_before);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | 1"));
    assert!(extracted_text.contains("B | 2"));

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A--1"), "{table_lines:?}");
    assert!(table_lines.contains(&"B--2"), "{table_lines:?}");
    assert!(
        !table_lines.iter().any(|line| line.contains(" | ")),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_column_visible_cell_hooks_drive_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{>{+}l<{!}r}A & 1 \\ B & 2\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].cell_prefix.as_deref(), Some("+"));
    assert_eq!(table.columns[0].cell_suffix.as_deref(), Some("!"));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | 1"));
    assert!(extracted_text.contains("B | 2"));

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"+A! | 1"), "{table_lines:?}");
    assert!(table_lines.contains(&"+B! | 2"), "{table_lines:?}");
}

#[test]
fn tabular_column_alignment_hooks_drive_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{>{\raggedleft\arraybackslash}l>{\centering\arraybackslash}l>{\raggedright\arraybackslash}l}A & B & C \\ Longer & Wide & Tail\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Right);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Center);
    assert_eq!(table.columns[2].alignment, TableColumnAlignment::Left);
    let extracted_text = capture.document_ir.extracted_text();
    for hidden in ["raggedleft", "centering", "raggedright", "arraybackslash"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text}");
    }

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        table_lines.contains(&"     A |  B   | C"),
        "{table_lines:?}"
    );
    assert!(
        table_lines.contains(&"Longer | Wide | Tail"),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_multicolumn_spec_alignment_drives_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{r}{Hdr} & Z \\ Alpha & Beta & Y\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(TableColumnAlignment::Right)
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"         Hdr | Z"), "{table_lines:?}");
    assert!(table_lines.contains(&"Alpha | Beta | Y"), "{table_lines:?}");
}

#[test]
fn tabular_multicolumn_vertical_rules_emit_row_scoped_rule_ops() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{|c|}{Hdr} & Z \\ Alpha & Beta & Y\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(TableColumnAlignment::Center)
    );
    assert_eq!(table.rows[0].cells[0].rule_before_count, 1);
    assert_eq!(table.rows[0].cells[0].rule_after_count, 1);
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let header_line = table_lines
        .iter()
        .find(|line| line.contains("Hdr"))
        .expect("header row");

    assert!(!header_line.contains('|'), "{table_lines:?}");
    assert!(table_lines.contains(&"Alpha | Beta | Y"), "{table_lines:?}");
    let vertical_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Rule(rule)
                    if rule.height > rule.width && (rule.width - 0.8).abs() < 0.001
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(vertical_rules.len(), 2, "{vertical_rules:?}");
}

#[test]
fn tabular_multicolumn_vrule_hooks_emit_row_scoped_rule_ops() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{@{\vrule}c!{\vline}}{Hdr} & Z \\ Alpha & Beta & Y\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(TableColumnAlignment::Center)
    );
    assert_eq!(table.rows[0].cells[0].rule_before_count, 1);
    assert_eq!(table.rows[0].cells[0].rule_after_count, 1);
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let header_line = table_lines
        .iter()
        .find(|line| line.contains("Hdr"))
        .expect("header row");

    assert!(!header_line.contains('|'), "{table_lines:?}");
    let vertical_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Rule(rule)
                    if rule.height > rule.width && (rule.width - 0.8).abs() < 0.001
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(vertical_rules.len(), 2, "{vertical_rules:?}");
}

#[test]
fn tabular_multicolumn_visible_cell_hooks_drive_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{>{+}c<{!}}{Hdr} & Z \\ Alpha & Beta & Y\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(TableColumnAlignment::Center)
    );
    assert_eq!(table.rows[0].cells[0].cell_prefix.as_deref(), Some("+"));
    assert_eq!(table.rows[0].cells[0].cell_suffix.as_deref(), Some("!"));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.iter().any(|line| line.contains("+Hdr!")));
    assert!(
        !capture
            .document_ir
            .extracted_text()
            .contains(r"\multicolumn"),
        "{}",
        capture.document_ir.extracted_text()
    );
}

#[test]
fn tabular_multicolumn_visible_separator_hooks_drive_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{@{--}c!{++}}{Hdr} & Z \\ Alpha & Beta & Y\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(TableColumnAlignment::Center)
    );
    assert_eq!(table.rows[0].cells[0].cell_prefix.as_deref(), Some("--"));
    assert_eq!(table.rows[0].cells[0].cell_suffix.as_deref(), Some("++"));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.iter().any(|line| line.contains("--Hdr++")));
    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        !extracted_text.contains(r"\multicolumn"),
        "{extracted_text}"
    );
}

#[test]
fn tabular_numeric_column_specs_survive_ir_and_align_display_list_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{dcolumn}\begin{document}\begin{tabular}{S[table-format=2.1]D{.}{.}{-1}}1.2 & 3.4 \\ 22.0 & 5\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert!(
        table
            .columns
            .iter()
            .all(|column| column.alignment == TableColumnAlignment::Decimal)
    );
    assert!(
        !capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("dcolumn.sty")
        )),
        "dcolumn shim should be recognized without a missing-package diagnostic"
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&" 1.2 | 3.4"), "{table_lines:?}");
    assert!(table_lines.contains(&"22.0 | 5  "), "{table_lines:?}");
}

#[test]
fn tabular_unknown_custom_column_specs_preserve_column_count() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{L{2cm}Y[foo]{bar}r}A & B & 9 \\ Longer & Wide & 10\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Unknown);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Unknown);
    assert_eq!(table.columns[2].alignment, TableColumnAlignment::Right);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(!extracted_text.contains("2cm"), "{extracted_text}");
    assert!(!extracted_text.contains("foo"), "{extracted_text}");
    assert!(!extracted_text.contains("bar"), "{extracted_text}");

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        table_lines.contains(&"A      | B    |  9"),
        "{table_lines:?}"
    );
    assert!(
        table_lines.contains(&"Longer | Wide | 10"),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_newcolumntype_specs_drive_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\newcolumntype{L}[1]{>{\raggedright\arraybackslash}p{#1}}\newcolumntype{R}{>{\raggedleft\arraybackslash}l}\begin{document}\begin{tabular}{L{10pt}R}A & 9 \\ Longer & 10\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Left);
    assert_eq!(table.columns[0].width_pt_milli, Some(10_000));
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Right);
    let extracted_text = capture.document_ir.extracted_text();
    for hidden in [
        "newcolumntype",
        "raggedright",
        "raggedleft",
        "arraybackslash",
    ] {
        assert!(!extracted_text.contains(hidden), "{extracted_text}");
    }

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A      |  9"), "{table_lines:?}");
    assert!(table_lines.contains(&"Longer | 10"), "{table_lines:?}");
}

#[test]
fn tabular_vertical_column_rules_emit_display_list_rules() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{|l|r|}A & 1 \\ B & 22\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert!(table.columns[0].rule_before);
    assert!(table.columns[0].rule_after);
    assert!(table.columns[1].rule_after);
    let vertical_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(vertical_rules.len(), 6, "{vertical_rules:?}");
    assert!(table_lines.contains(&"A    1"), "{table_lines:?}");
    assert!(table_lines.contains(&"B   22"), "{table_lines:?}");
    assert!(
        !table_lines.iter().any(|line| line.contains('|')),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_repeated_vertical_column_rules_emit_multiple_display_list_rules() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{||l||r||}A & 1\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].rule_before_count, 2);
    assert_eq!(table.columns[0].rule_after_count, 2);
    assert_eq!(table.columns[1].rule_before_count, 2);
    assert_eq!(table.columns[1].rule_after_count, 2);
    let vertical_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(vertical_rules.len(), 6, "{vertical_rules:?}");
    assert!(table_lines.contains(&"A   1"), "{table_lines:?}");
    assert!(
        !table_lines.iter().any(|line| line.contains('|')),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_multirow_visible_text_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{multirow}\begin{document}\begin{tabular}{ll}\multirow{2}{*}{Span} & A \\ B & C\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Span");
    assert_eq!(table.rows[0].cells[0].row_span, Some(2));
    assert!(!capture.document_ir.extracted_text().contains("multirow"));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Span | A"), "{table_lines:?}");
    assert!(table_lines.contains(&"B    | C"), "{table_lines:?}");
}

#[test]
fn tabular_multirow_display_list_offsets_omitted_spanned_cells() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{multirow}\begin{document}\begin{tabular}{ll}\multirow{2}{*}{Span} & A \\ B\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].row_span, Some(2));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Span | A"), "{table_lines:?}");
    assert!(table_lines.contains(&"     | B"), "{table_lines:?}");
}

#[test]
fn tabular_negative_multirow_does_not_offset_following_rows() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{multirow}\begin{document}\begin{tabular}{ll}A & B \\ \multirow{-2}{*}{Span} & C \\ D & E\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[1].cells[0].text, "Span");
    assert_eq!(table.rows[1].cells[0].row_span, None);
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A    | B"), "{table_lines:?}");
    assert!(table_lines.contains(&"Span | C"), "{table_lines:?}");
    assert!(table_lines.contains(&"D    | E"), "{table_lines:?}");
}

#[test]
fn tabular_multirow_multicolumn_offsets_omitted_spanned_columns() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{multirow}\begin{document}\begin{tabular}{lll}\multirow{2}{*}{\multicolumn{2}{|c|}{Span}} & Tail \\ A\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Span");
    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(table.rows[0].cells[0].row_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].alignment,
        Some(tex_render_model::TableColumnAlignment::Center)
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Span   Tail"), "{table_lines:?}");
    assert!(table_lines.contains(&" |   | A"), "{table_lines:?}");
}

#[test]
fn tabular_multirowcell_multicolumn_offsets_omitted_spanned_columns() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{makecell}\begin{document}\begin{tabular}{lll}\multirowcell{2}{\multicolumn{2}{c}{Span}} & Tail \\ A\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Span");
    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(table.rows[0].cells[0].row_span, Some(2));
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Span | Tail"), "{table_lines:?}");
    assert!(table_lines.contains(&" |   | A"), "{table_lines:?}");
}

#[test]
fn tabular_starred_makecell_helpers_do_not_leak_commands() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{makecell}\begin{document}\begin{tabular}{ll}\makecell*{Cell \cite{key}} & \thead*{Head}\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Cell [?]");
    assert_eq!(table.rows[0].cells[1].text, "Head");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(!extracted_text.contains("makecell"));
    assert!(!extracted_text.contains("thead"));
    assert!(!extracted_text.contains("key"));
}

#[test]
fn tabular_slashbox_helpers_render_readable_cell_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{slashbox}\begin{document}\begin{tabular}{ll}\backslashbox{Rows}{Cols} & Value \\ \slashbox{Left}{Right} & Tail\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Rows/Cols");
    assert_eq!(table.rows[1].cells[0].text, "Left/Right");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Rows/Cols | Value"));
    assert!(extracted_text.contains("Left/Right | Tail"));
    assert!(!extracted_text.contains("slashbox"));
    assert!(!extracted_text.contains("backslashbox"));
    assert!(
        !capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("slashbox.sty")
        )),
        "slashbox shim should be recognized without a missing-package diagnostic"
    );
}

#[test]
fn tabular_cell_linebreak_helpers_stay_inside_cells() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{makecell}\begin{document}\begin{tabular}{ll}\makecell{Top\\Bottom} & \shortstack{Left\\Right} \\ Tail & End\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Top Bottom");
    assert_eq!(table.rows[0].cells[1].text, "Left Right");
    assert_eq!(table.rows[1].cells[0].text, "Tail");
    assert_eq!(table.rows[1].cells[1].text, "End");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Top Bottom | Left Right"));
    assert!(extracted_text.contains("Tail | End"));
    for hidden in ["makecell", "shortstack", r"\\"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text:?}");
    }
}

#[test]
fn tabular_box_wrappers_hide_layout_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{llll}\rotatebox[origin=c]{90}{Rotated} & \scalebox{.8}[1.2]{Scaled} & \resizebox{2cm}{!}{Sized} & \reflectbox{Reflected}\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Rotated");
    assert_eq!(table.rows[0].cells[1].text, "Scaled");
    assert_eq!(table.rows[0].cells[2].text, "Sized");
    assert_eq!(table.rows[0].cells[3].text, "Reflected");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Rotated | Scaled | Sized | Reflected"));
    for hidden in [
        "rotatebox",
        "scalebox",
        "resizebox",
        "reflectbox",
        "origin",
        "90",
        ".8",
        "1.2",
        "2cm",
    ] {
        assert!(!extracted_text.contains(hidden), "{extracted_text:?}");
    }
}

#[test]
fn tabular_overlap_wrappers_preserve_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{llll}\rlap{Right} & \llap{Left} & \clap{Center} & \smash{Flat}\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Right");
    assert_eq!(table.rows[0].cells[1].text, "Left");
    assert_eq!(table.rows[0].cells[2].text, "Center");
    assert_eq!(table.rows[0].cells[3].text, "Flat");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Right | Left | Center | Flat"));
    for hidden in ["rlap", "llap", "clap", "smash"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text:?}");
    }
}

#[test]
fn resizebox_wrapped_tabular_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\resizebox{\textwidth}{!}{\begin{tabular}{ll}A & B \\ C & D\end{tabular}}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].text, "D");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | B"));
    assert!(extracted_text.contains("C | D"));
    assert!(!extracted_text.contains("resizebox"));
    assert!(!extracted_text.contains("textwidth"));
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        display_list_text.contains(&"A | B"),
        "{display_list_text:?}"
    );
    assert!(
        display_list_text.contains(&"C | D"),
        "{display_list_text:?}"
    );
}

#[test]
fn adjustbox_environment_tabular_survives_ir_without_option_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{adjustbox}\begin{document}\begin{adjustbox}{width=\textwidth,center}\begin{tabular}{ll}A & B \\ C & D\end{tabular}\end{adjustbox}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].text, "D");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | B"));
    assert!(extracted_text.contains("C | D"));
    for hidden in ["adjustbox", "textwidth", "center"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text:?}");
    }
    assert!(
        !capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("adjustbox.sty")
        )),
        "adjustbox shim should be recognized without a missing-package diagnostic"
    );
}

#[test]
fn tabular_partial_rules_survive_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}Head & Value & Tail \\\cline{2-3} A & B & C\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(
        table.rows[0].partial_rules_below,
        vec![TableRuleSpan {
            start_column: 1,
            end_column: 2,
            trim_start: false,
            trim_end: false,
        }]
    );
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        table_lines.contains(&"Head | Value | Tail"),
        "{table_lines:?}"
    );
    assert!(
        !table_lines
            .iter()
            .any(|line| line.contains(".......") || line.contains("------------")),
        "{table_lines:?}"
    );
    assert!(table_lines.contains(&"A    | B     | C"), "{table_lines:?}");
    let table_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(table_rules.len(), 1, "{table_rules:?}");
    assert!(
        table_rules[0].x > 72.0 && table_rules[0].width > 70.0,
        "{table_rules:?}"
    );
}

#[test]
fn tabular_partial_rule_rects_use_visible_separator_widths() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{l@{--}r@{+++}l}A & B & C \\\cline{2-3} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let row_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "A--B+++C" => Some(run),
            _ => None,
        })
        .expect("first table row text");
    let table_rules = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(table_rules.len(), 1, "{table_rules:?}");
    let glyph_width = row_run.approximate_advance_pt / row_run.text.chars().count() as f32;
    let expected_start_x = row_run.origin.x + glyph_width * "A--".chars().count() as f32;
    let expected_width = glyph_width * "B+++C".chars().count() as f32;

    assert!(
        (table_rules[0].x - expected_start_x).abs() <= glyph_width * 0.1,
        "rule {:?}, row_run {:?}, expected_start_x {expected_start_x}",
        table_rules[0],
        row_run
    );
    assert!(
        (table_rules[0].width - expected_width).abs() <= glyph_width * 0.1,
        "rule {:?}, row_run {:?}, expected_width {expected_width}",
        table_rules[0],
        row_run
    );
}

#[test]
fn booktabs_cmidrule_trim_options_shorten_partial_rule_rects() {
    let untrimmed = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}A & B & C \\\cline{1-2} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let trimmed = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{booktabs}\begin{document}\begin{tabular}{lll}A & B & C \\\cmidrule(lr){1-2} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = trimmed
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("trimmed table");
    let span = table.rows[0]
        .partial_rules_below
        .first()
        .expect("cmidrule span");

    assert!(span.trim_start);
    assert!(span.trim_end);

    let untrimmed_rule = untrimmed.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
            _ => None,
        })
        .expect("untrimmed rule");
    let trimmed_rule = trimmed.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
            _ => None,
        })
        .expect("trimmed rule");

    assert!(
        trimmed_rule.x > untrimmed_rule.x,
        "{trimmed_rule:?} {untrimmed_rule:?}"
    );
    assert!(
        trimmed_rule.width < untrimmed_rule.width,
        "{trimmed_rule:?} {untrimmed_rule:?}"
    );
}

#[test]
fn booktabs_cmidrule_single_sided_trim_options_are_directional() {
    let untrimmed = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}A & B & C \\\cline{1-2} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let left_trimmed = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{booktabs}\begin{document}\begin{tabular}{lll}A & B & C \\\cmidrule(l){1-2} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let right_trimmed = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{booktabs}\begin{document}\begin{tabular}{lll}A & B & C \\\cmidrule(r){1-2} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table_span = |capture: &InternalRenderIrCapture| {
        capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Table(table) if table.environment == "tabular" => {
                    table.rows[0].partial_rules_below.first()
                }
                _ => None,
            })
            .expect("cmidrule span")
            .to_owned()
    };
    let horizontal_rule = |capture: &InternalRenderIrCapture| {
        capture.page_display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
                _ => None,
            })
            .expect("horizontal rule")
            .to_owned()
    };

    assert!(table_span(&left_trimmed).trim_start);
    assert!(!table_span(&left_trimmed).trim_end);
    assert!(!table_span(&right_trimmed).trim_start);
    assert!(table_span(&right_trimmed).trim_end);

    let untrimmed_rule = horizontal_rule(&untrimmed);
    let left_rule = horizontal_rule(&left_trimmed);
    let right_rule = horizontal_rule(&right_trimmed);
    let untrimmed_end = untrimmed_rule.x + untrimmed_rule.width;
    let left_end = left_rule.x + left_rule.width;
    let right_end = right_rule.x + right_rule.width;

    assert!(
        left_rule.x > untrimmed_rule.x,
        "{left_rule:?} {untrimmed_rule:?}"
    );
    assert!(
        (left_end - untrimmed_end).abs() <= 0.01,
        "{left_rule:?} {untrimmed_rule:?}"
    );
    assert!(
        (right_rule.x - untrimmed_rule.x).abs() <= 0.01,
        "{right_rule:?} {untrimmed_rule:?}"
    );
    assert!(
        right_end < untrimmed_end,
        "{right_rule:?} {untrimmed_rule:?}"
    );
}

#[test]
fn booktabs_spacing_commands_do_not_leak_into_table_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{booktabs}\begin{document}\begin{tabular}{lr}\toprule[.08em] A & B \\ \addlinespace[2pt]\midrule[.03em] C & D \\\morecmidrules\cmidrule(lr){1-2}\specialrule{.05em}{.2em}{.2em} E & F \\ \bottomrule[.08em]\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[0].text, "C");
    assert_eq!(table.rows[2].cells[0].text, "E");
    assert!(table.rows[0].rule_above);
    assert!(table.rows[0].rule_below);
    assert!(table.rows[1].rule_below);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | B"));
    assert!(extracted_text.contains("C | D"));
    assert!(extracted_text.contains("E | F"));
    assert!(!extracted_text.contains("toprule"));
    assert!(!extracted_text.contains("addlinespace"));
    assert!(!extracted_text.contains("morecmidrules"));
    assert!(!extracted_text.contains("specialrule"));
    assert!(!extracted_text.contains(".08em"));

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let rule_ops = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A | B"), "{table_lines:?}");
    assert!(table_lines.contains(&"C | D"), "{table_lines:?}");
    assert!(table_lines.contains(&"E | F"), "{table_lines:?}");
    assert!(rule_ops.len() >= 5, "{rule_ops:?}");
}

#[test]
fn makecell_xrule_commands_do_not_leak_into_table_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{makecell}\begin{document}\begin{tabular}{lll}\Xhline{1pt} A & B & C \\\Xcline{2-3}{0.5pt} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert!(table.rows[0].rule_above);
    assert_eq!(
        table.rows[0].partial_rules_below,
        vec![TableRuleSpan {
            start_column: 1,
            end_column: 2,
            trim_start: false,
            trim_end: false,
        }]
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | B | C"));
    assert!(extracted_text.contains("D | E | F"));
    for hidden in ["Xhline", "Xcline", "1pt", "0.5pt"] {
        assert!(!extracted_text.contains(hidden), "{extracted_text:?}");
    }
    let rule_ops = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(rule_ops.len() >= 2, "{rule_ops:?}");
}

#[test]
fn hhline_rule_does_not_leak_into_table_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{hhline}\begin{document}\begin{tabular}{|l|r|}A & 1 \\\hhline{|=|=|} B & 22\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "A");
    assert!(table.rows[0].rule_below);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | 1"));
    assert!(extracted_text.contains("B | 22"));
    assert!(!extracted_text.contains("hhline"));
    assert!(!extracted_text.contains("|=|=|"));
    assert!(
        !capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("hhline.sty")
        )),
        "hhline shim should be recognized without a missing-package diagnostic"
    );

    let rule_ops = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(rule_ops.len() >= 4, "{rule_ops:?}");
}

#[test]
fn hhline_partial_pattern_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{hhline}\begin{document}\begin{tabular}{lll}A & B & C \\\hhline{=~=} D & E & F\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(
        table.rows[0].partial_rules_below,
        vec![
            TableRuleSpan {
                start_column: 0,
                end_column: 0,
                trim_start: false,
                trim_end: false,
            },
            TableRuleSpan {
                start_column: 2,
                end_column: 2,
                trim_start: false,
                trim_end: false,
            },
        ]
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | B | C"));
    assert!(extracted_text.contains("D | E | F"));
    assert!(!extracted_text.contains("hhline"));
    assert!(!extracted_text.contains("=~="));

    let rule_ops = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rule(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(rule_ops.len(), 2, "{rule_ops:?}");
}

#[test]
fn table_color_commands_do_not_leak_into_table_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{colortbl}\begin{document}\begin{tabular}{|l|r|}\rowcolor{gray!20} A & \cellcolor[gray]{0.9}1 \\ \arrayrulecolor{red}\hline B & 22\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "A");
    assert_eq!(table.rows[0].cells[1].text, "1");
    assert_eq!(table.rows[1].cells[0].text, "B");
    assert!(table.rows[0].rule_below);
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("A | 1"));
    assert!(extracted_text.contains("B | 22"));
    assert!(!extracted_text.contains("rowcolor"));
    assert!(!extracted_text.contains("cellcolor"));
    assert!(!extracted_text.contains("arrayrulecolor"));
    assert!(!extracted_text.contains("gray"));
    assert!(!extracted_text.contains("red"));
    assert!(
        !capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains("colortbl.sty")
        )),
        "colortbl shim should be recognized without a missing-package diagnostic"
    );

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"A    1"), "{table_lines:?}");
    assert!(table_lines.contains(&"B   22"), "{table_lines:?}");
    assert!(
        !table_lines.iter().any(|line| line.contains('|')),
        "{table_lines:?}"
    );
}

#[test]
fn tabular_multicolumn_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabular}{lll}\multicolumn{2}{c}{Wide} & Tail \\ A & B & C\end{tabular}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("tabular table");

    assert_eq!(table.rows[0].cells[0].text, "Wide");
    assert_eq!(table.rows[0].cells[0].column_span, Some(2));
    assert_eq!(table.rows[0].cells[1].text, "Tail");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Wide | Tail"));
    assert!(!extracted_text.contains("multicolumn"));
    assert!(!extracted_text.contains("{2}"));
    assert!(!extracted_text.contains("{c}"));

    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Wide  | Tail"), "{table_lines:?}");
    assert!(table_lines.contains(&"A | B | C"), "{table_lines:?}");
}

#[test]
fn longtable_capture_builds_table_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LONGTABLE_FALLBACK_SOURCE,
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "longtable" => Some(table),
            _ => None,
        })
        .expect("longtable table");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Alpha");
    assert_eq!(table.rows[0].cells[1].text, "Beta");
    assert_eq!(table.rows[1].cells[0].text, "Gamma");
    assert_eq!(table.rows[1].cells[1].text, "Delta");
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Alpha | Beta"));
    assert!(extracted_text.contains("Gamma | Delta"));
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
        .join("\n");
    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(display_list_text.contains("Gamma | Delta"));
    assert!(!display_list_text.contains("&"));
    assert!(!display_list_text.contains("hline"));
}

#[test]
fn tabularx_capture_builds_table_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\begin{document}\begin{tabularx}{\textwidth}{lX}Alpha & Beta \\ Gamma & Delta\end{tabularx}\end{document}",
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabularx" => Some(table),
            _ => None,
        })
        .expect("tabularx table");

    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].alignment, TableColumnAlignment::Left);
    assert_eq!(table.columns[1].alignment, TableColumnAlignment::Paragraph);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Alpha");
    assert_eq!(table.rows[0].cells[1].text, "Beta");
    assert_eq!(table.rows[1].cells[0].text, "Gamma");
    assert_eq!(table.rows[1].cells[1].text, "Delta");
    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(display_list_text.contains("Gamma | Delta"));
}

#[test]
fn tabu_capture_builds_table_ir() {
    let capture = capture_internal_render_ir(
        "main.tex",
        r"\documentclass{article}\usepackage{tabu}\begin{document}\begin{tabu}{X[l]r}Alpha & 1 \\ Beta & 22\end{tabu}\begin{longtabu} to \linewidth {Xr}Long & 3 \\ Tail & 44\end{longtabu}\end{document}",
        &SemanticAux::default(),
    );
    let tabu = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabu" => Some(table),
            _ => None,
        })
        .expect("tabu table");
    let longtabu = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "longtabu" => Some(table),
            _ => None,
        })
        .expect("longtabu table");

    assert_eq!(tabu.columns.len(), 2);
    assert_eq!(tabu.columns[0].alignment, TableColumnAlignment::Paragraph);
    assert_eq!(tabu.columns[1].alignment, TableColumnAlignment::Right);
    assert_eq!(tabu.rows[0].cells[0].text, "Alpha");
    assert_eq!(longtabu.columns.len(), 2);
    assert_eq!(
        longtabu.columns[0].alignment,
        TableColumnAlignment::Paragraph
    );
    assert_eq!(longtabu.columns[1].alignment, TableColumnAlignment::Right);
    assert_eq!(longtabu.rows[0].cells[0].text, "Long");
    let table_lines = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(table_lines.contains(&"Alpha |  1"), "{table_lines:?}");
    assert!(table_lines.contains(&"Beta  | 22"), "{table_lines:?}");
    assert!(table_lines.contains(&"Long |  3"), "{table_lines:?}");
    assert!(table_lines.contains(&"Tail | 44"), "{table_lines:?}");
}

#[test]
fn longtable_table_ir_labels_survive_without_visible_key() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LONGTABLE_FALLBACK_LABEL_SOURCE,
        &SemanticAux::default(),
    );
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "longtable" => Some(table),
            _ => None,
        })
        .expect("longtable table");

    let visible = table.visible_text();
    assert!(visible.contains("Long table."));
    assert!(visible.contains("Alpha | Beta"));
    assert!(!visible.contains("tab:long"));
    assert!(!visible.contains("label"));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"tab:long"));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Long table."));
    assert!(extracted_text.contains("Alpha | Beta"));
    assert!(!extracted_text.contains("tab:long"));
    assert!(!extracted_text.contains("label"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("Long table."));
    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(!display_list_text.contains("tab:long"));
    assert!(!display_list_text.contains("label"));
}

#[test]
fn table_float_caption_and_tabular_body_build_table_ir() {
    let capture =
        capture_internal_render_ir("main.tex", TABLE_FLOAT_BODY_SOURCE, &SemanticAux::default());
    let table = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Table(table) if table.environment == "tabular" => Some(table),
            _ => None,
        })
        .expect("table float table");

    assert_eq!(table.caption.as_deref(), Some("Data table."));
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Alpha");
    assert_eq!(table.rows[0].cells[1].text, "Beta");
    assert_eq!(table.rows[1].cells[0].text, "Gamma");
    assert_eq!(table.rows[1].cells[1].text, "Delta");
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("table")
                    || fallback.environment.as_deref() == Some("tabular")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Data table."));
    assert!(extracted_text.contains("Alpha | Beta"));
    assert!(extracted_text.contains("Gamma | Delta"));
    assert!(!extracted_text.contains("&"));
    assert!(!extracted_text.contains("ll"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display_list_text.contains("Data table."));
    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(display_list_text.contains("Gamma | Delta"));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("(Data table.) Tj"));
    assert!(pdf_text.contains("(Alpha | Beta) Tj"));
    assert!(pdf_text.contains("(Gamma | Delta) Tj"));
}

#[test]
fn verbatim_fallback_preserves_raw_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        VERBATIM_FALLBACK_SOURCE,
        &SemanticAux::default(),
    );
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("verbatim") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("verbatim fallback");

    assert_eq!(
        fallback.normalized_visible_text.as_deref(),
        Some(r"\alpha_{i} {raw}")
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(r"\alpha_{i} {raw}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(r"\alpha_{i} {raw}"));
}

#[test]
fn code_listing_fallback_preserves_raw_visible_text_without_begin_options() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CODE_LISTING_FALLBACK_SOURCE,
        &SemanticAux::default(),
    );
    let lstlisting = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("lstlisting") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("lstlisting fallback");
    let minted = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("minted") => {
                Some(fallback)
            }
            _ => None,
        })
        .expect("minted fallback");

    assert_eq!(
        lstlisting.normalized_visible_text.as_deref(),
        Some(r#"fn main() { println!("hi"); }"#)
    );
    assert_eq!(
        minted.normalized_visible_text.as_deref(),
        Some(r"let value = \alpha_{i} + {raw};")
    );

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(r#"fn main() { println!("hi"); }"#));
    assert!(extracted_text.contains(r"let value = \alpha_{i} + {raw};"));
    assert!(!extracted_text.contains("language=Rust"));
    assert!(!extracted_text.contains("{rust}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(r#"fn main() { println!("hi"); }"#));
    assert!(display_list_text.contains(r"let value = \alpha_{i} + {raw};"));
    assert!(!display_list_text.contains("language=Rust"));
    assert!(!display_list_text.contains("{rust}"));
}

#[test]
fn fancyvrb_fallback_preserves_raw_visible_text_without_begin_options() {
    let capture = capture_internal_render_ir(
        "main.tex",
        FANCYVRB_FALLBACK_SOURCE,
        &SemanticAux::default(),
    );
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("Verbatim") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("Verbatim fallback");

    assert_eq!(
        fallback.normalized_visible_text.as_deref(),
        Some(r"\foo_{bar} {baz}")
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains(r"\foo_{bar} {baz}"));
    assert!(!extracted_text.contains("fontsize"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains(r"\foo_{bar} {baz}"));
    assert!(!display_list_text.contains("fontsize"));
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
    let fallback_event = capture
        .events
        .events
        .iter()
        .find_map(|event| match &event.event {
            RenderEvent::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("unknownenv") =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("unknownenv fallback event");

    assert_eq!(fallback_event.meta.mode_hint, ModeHint::Vertical);
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
fn tikz_fallback_uses_placeholder_in_ir_and_display_list() {
    let capture =
        capture_internal_render_ir("main.tex", TIKZ_FALLBACK_SOURCE, &SemanticAux::default());
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tikzpicture") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("tikz fallback");

    assert_eq!(
        fallback.normalized_visible_text.as_deref(),
        Some("[unsupported tikzpicture]")
    );
    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("[unsupported tikzpicture]"));
    for hidden in ["draw", "node", "Should not render", "(0,0)", "(1,1)"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("[unsupported tikzpicture]"));
    for hidden in ["draw", "node", "Should not render", "(0,0)", "(1,1)"] {
        assert!(!display_list_text.contains(hidden));
    }
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

    assert_eq!(environments.len(), 4);
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
    assert_eq!(environments[2].name, "theorem");
    assert!(environments[2].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Theorem"
        )
    }));
    assert_eq!(environments[3].name, "proof");
    assert!(environments[3].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Proof"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("quote" | "center" | "theorem" | "proof")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Quoted [?]."));
    assert!(extracted_text.contains("Centered text."));
    assert!(extracted_text.contains("Theorem text."));
    assert!(extracted_text.contains("Proof text."));
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
    assert!(display_list_text.contains("Theorem text."));
    assert!(display_list_text.contains("Proof text."));
    assert!(!display_list_text.contains("key"));
}

#[test]
fn verse_environment_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        VERSE_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "verse" => Some(environment),
            _ => None,
        })
        .expect("verse environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("verse")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Line [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("verse"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Line [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("verse"));
}

#[test]
fn acknowledgements_environment_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        ACKNOWLEDGEMENTS_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    for environment in [
        "acknowledgements",
        "acknowledgments",
        "acknowledgement",
        "acknowledgment",
    ] {
        assert!(environment_names.contains(&environment));
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some(
                        "acknowledgements"
                            | "acknowledgments"
                            | "acknowledgement"
                            | "acknowledgment"
                    )
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Thanks [?]."));
    assert!(extracted_text.contains("US spelling."));
    assert!(extracted_text.contains("Singular."));
    assert!(extracted_text.contains("Singular US."));
    assert!(!extracted_text.contains("grant"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Thanks [?]."));
    assert!(display_list_text.contains("US spelling."));
    assert!(display_list_text.contains("Singular."));
    assert!(display_list_text.contains("Singular US."));
    assert!(!display_list_text.contains("grant"));
}

#[test]
fn keywords_environment_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        KEYWORDS_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    for environment in ["keywords", "keyword", "IEEEkeywords"] {
        assert!(environment_names.contains(&environment));
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("keywords" | "keyword" | "IEEEkeywords")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("vision; [?]"));
    assert!(extracted_text.contains("single keyword"));
    assert!(extracted_text.contains("systems, latex"));
    assert!(!extracted_text.contains(r"\cite"));
    assert!(!extracted_text.contains("{key}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("vision; [?]"));
    assert!(display_list_text.contains("single keyword"));
    assert!(display_list_text.contains("systems, latex"));
    assert!(!display_list_text.contains(r"\cite"));
    assert!(!display_list_text.contains("{key}"));
}

#[test]
fn frontmatter_environment_capture_preserves_metadata_and_abstract() {
    let capture = capture_internal_render_ir(
        "main.tex",
        FRONTMATTER_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );

    assert!(capture.events.events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::SetDocumentMetadata(metadata)
            if metadata.field == MetadataField::Title
                && metadata.value == "Wrapped Paper"
    )));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Environment(environment) if environment.name == "frontmatter"
        )
    }));
    let abstract_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Abstract(abstract_block) => Some(abstract_block),
            _ => None,
        })
        .expect("abstract block");
    assert!(abstract_block.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("frontmatter")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Wrapped abstract [?]."));
    assert!(!extracted_text.contains(r"\title"));
    assert!(!extracted_text.contains("frontmatter"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Wrapped abstract [?]."));
    assert!(!display_list_text.contains(r"\title"));
    assert!(!display_list_text.contains("frontmatter"));
}

#[test]
fn wide_text_wrappers_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        WIDE_TEXT_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"widetext"));
    assert!(environment_names.contains(&"strip"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("widetext" | "strip"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Wide [?] text."));
    assert!(extracted_text.contains("Strip text."));
    assert!(!extracted_text.contains("{key}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Wide [?] text."));
    assert!(display_list_text.contains("Strip text."));
    assert!(!display_list_text.contains("{key}"));
}

#[test]
fn fullwidth_wrapper_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        FULLWIDTH_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "fullwidth" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("fullwidth environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("fullwidth")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Full [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("fullwidth"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Full [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("fullwidth"));
}

#[test]
fn landscape_wrapper_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LANDSCAPE_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "landscape" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("landscape environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("landscape")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Rotated [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("landscape"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Rotated [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("landscape"));
}

#[test]
fn cjk_wrapper_capture_hides_encoding_and_font_arguments() {
    let capture =
        capture_internal_render_ir("main.tex", CJK_WRAPPER_SOURCE, &SemanticAux::default());
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"CJK"));
    assert!(environment_names.contains(&"CJK*"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("CJK" | "CJK*"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("CJK [?] text."));
    assert!(extracted_text.contains("Star text."));
    for hidden in ["UTF8", "gbsn", "bsmi", "{key}"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("CJK [?] text."));
    assert!(display_list_text.contains("Star text."));
    for hidden in ["UTF8", "gbsn", "bsmi", "{key}"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn sloppypar_wrapper_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        SLOPPYPAR_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "sloppypar" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("sloppypar environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("sloppypar")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Loose [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("sloppypar"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Loose [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("sloppypar"));
}

#[test]
fn size_environment_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", SIZE_ENVIRONMENT_SOURCE, &SemanticAux::default());
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for environment in ["small", "footnotesize", "Large"] {
        assert!(environment_names.contains(&environment));
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("small" | "footnotesize" | "Large")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Small [?] text."));
    assert!(extracted_text.contains("Foot text."));
    assert!(extracted_text.contains("Large text."));
    assert!(!extracted_text.contains("{key}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Small [?] text."));
    assert!(display_list_text.contains("Foot text."));
    assert!(display_list_text.contains("Large text."));
    assert!(!display_list_text.contains("{key}"));
}

#[test]
fn flush_alignment_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", FLUSH_ALIGNMENT_SOURCE, &SemanticAux::default());
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"flushleft"));
    assert!(environment_names.contains(&"flushright"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("flushleft" | "flushright"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Left [?] text."));
    assert!(extracted_text.contains("Right text."));
    assert!(!extracted_text.contains("{key}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Left [?] text."));
    assert!(display_list_text.contains("Right text."));
    assert!(!display_list_text.contains("{key}"));
}

#[test]
fn samepage_wrapper_capture_survives_ir_without_fallback() {
    let capture =
        capture_internal_render_ir("main.tex", SAMEPAGE_WRAPPER_SOURCE, &SemanticAux::default());
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "samepage" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("samepage environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("samepage")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Together [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("samepage"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Together [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("samepage"));
}

#[test]
fn titlepage_wrapper_capture_survives_ir_without_fallback() {
    let capture = capture_internal_render_ir(
        "main.tex",
        TITLEPAGE_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "titlepage" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("titlepage environment");
    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["key".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("titlepage")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Title [?] text."));
    assert!(!extracted_text.contains("{key}"));
    assert!(!extracted_text.contains("titlepage"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Title [?] text."));
    assert!(!display_list_text.contains("{key}"));
    assert!(!display_list_text.contains("titlepage"));
}

#[test]
fn boxed_wrappers_capture_hides_style_options() {
    let capture =
        capture_internal_render_ir("main.tex", BOXED_WRAPPER_SOURCE, &SemanticAux::default());
    for environment_name in ["framed", "shaded", "tcolorbox", "mdframed"] {
        let environment = capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Environment(environment) if environment.name == environment_name => {
                    Some(environment)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{environment_name} environment"));
        assert!(!environment.content.is_empty());
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("framed" | "shaded" | "tcolorbox" | "mdframed")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Frame [?] text."));
    assert!(extracted_text.contains("Shade text."));
    assert!(extracted_text.contains("Color text."));
    assert!(extracted_text.contains("Border text."));
    for argument in ["colback", "yellow", "linecolor", "red"] {
        assert!(!extracted_text.contains(argument));
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
    assert!(display_list_text.contains("Frame [?] text."));
    assert!(display_list_text.contains("Shade text."));
    assert!(display_list_text.contains("Color text."));
    assert!(display_list_text.contains("Border text."));
    for argument in ["colback", "yellow", "linecolor", "red"] {
        assert!(!display_list_text.contains(argument));
    }
}

#[test]
fn csquotes_display_environments_capture_hides_optional_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CSQUOTES_DISPLAY_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    for environment_name in ["displayquote", "displayquotation"] {
        let environment = capture
            .document_ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Environment(environment) if environment.name == environment_name => {
                    Some(environment)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{environment_name} environment"));
        assert!(!environment.content.is_empty());
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("displayquote" | "displayquotation")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Quoted [?] text."));
    assert!(extracted_text.contains("Long quote text."));
    for hidden in ["Hidden", "Source", "Punct", "key"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Quoted [?] text."));
    assert!(display_list_text.contains("Long quote text."));
    for hidden in ["Hidden", "Source", "Punct", "key"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn comment_environment_body_is_hidden_from_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        COMMENT_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("comment")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Before."));
    assert!(extracted_text.contains("After."));
    for hidden in ["Hidden", "key", "comment"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Before."));
    assert!(display_list_text.contains("After."));
    for hidden in ["Hidden", "key", "comment"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn custom_comment_environments_follow_include_exclude_policy() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CUSTOM_COMMENT_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let kept = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "keptnote" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("keptnote environment");
    assert!(kept.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Citation(citation)
                if citation.keys == vec!["shown".to_string()] && citation.display_text == "[?]"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("draftnote" | "keptnote"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Before."));
    assert!(extracted_text.contains("Kept [?] text."));
    assert!(extracted_text.contains("After."));
    for hidden in ["Hidden", "key", "shown", "draftnote", "keptnote"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Before."));
    assert!(display_list_text.contains("Kept [?] text."));
    assert!(display_list_text.contains("After."));
    for hidden in ["Hidden", "key", "shown", "draftnote", "keptnote"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn spacing_wrappers_capture_hides_layout_arguments() {
    let capture =
        capture_internal_render_ir("main.tex", SPACING_WRAPPER_SOURCE, &SemanticAux::default());
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for environment in ["spacing", "onehalfspace", "doublespace", "singlespace"] {
        assert!(environment_names.contains(&environment));
    }
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("spacing" | "onehalfspace" | "doublespace" | "singlespace")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Spaced [?] text."));
    assert!(extracted_text.contains("Half text."));
    assert!(extracted_text.contains("Double text."));
    assert!(extracted_text.contains("Single text."));
    assert!(!extracted_text.contains("1.5"));
    assert!(!extracted_text.contains("{key}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Spaced [?] text."));
    assert!(display_list_text.contains("Half text."));
    assert!(display_list_text.contains("Double text."));
    assert!(display_list_text.contains("Single text."));
    assert!(!display_list_text.contains("1.5"));
    assert!(!display_list_text.contains("{key}"));
}

#[test]
fn adjustwidth_wrappers_capture_hides_margin_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        ADJUSTWIDTH_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"adjustwidth"));
    assert!(environment_names.contains(&"adjustwidth*"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("adjustwidth" | "adjustwidth*"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Margin [?] text."));
    assert!(extracted_text.contains("Star text."));
    for argument in ["1cm", "2cm", "-1em", "0pt", "{key}"] {
        assert!(!extracted_text.contains(argument));
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
    assert!(display_list_text.contains("Margin [?] text."));
    assert!(display_list_text.contains("Star text."));
    for argument in ["1cm", "2cm", "-1em", "0pt", "{key}"] {
        assert!(!display_list_text.contains(argument));
    }
}

#[test]
fn addmargin_wrappers_capture_hides_margin_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        ADDMARGIN_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"addmargin"));
    assert!(environment_names.contains(&"addmargin*"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("addmargin" | "addmargin*"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Inset [?] text."));
    assert!(extracted_text.contains("Star text."));
    for argument in ["1em", "2em", "3em", "{key}"] {
        assert!(!extracted_text.contains(argument));
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
    assert!(display_list_text.contains("Inset [?] text."));
    assert!(display_list_text.contains("Star text."));
    for argument in ["1em", "2em", "3em", "{key}"] {
        assert!(!display_list_text.contains(argument));
    }
}

#[test]
fn appendices_environment_capture_preserves_nested_headings() {
    let capture = capture_internal_render_ir(
        "main.tex",
        APPENDICES_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"appendices"));
    assert!(environment_names.contains(&"subappendices"));

    let heading_text = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Heading(heading) => heading.content.iter().find_map(|node| match node {
                InlineNode::Text { text, .. } => Some(text.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(heading_text.contains(&"Extra"));
    assert!(heading_text.contains(&"More"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("appendices" | "subappendices")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Extra"));
    assert!(extracted_text.contains("Appendix [?] text."));
    assert!(extracted_text.contains("More"));
    assert!(extracted_text.contains("More text."));
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("appendices"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Extra"));
    assert!(display_list_text.contains("Appendix [?] text."));
    assert!(display_list_text.contains("More"));
    assert!(display_list_text.contains("More text."));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("appendices"));
}

#[test]
fn minipage_environment_capture_hides_layout_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MINIPAGE_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Environment(environment) if environment.name == "minipage" => {
                Some(environment)
            }
            _ => None,
        })
        .expect("minipage environment");

    assert!(environment.content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Box"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback) if fallback.environment.as_deref() == Some("minipage")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Box text."));
    assert!(!extracted_text.contains("0.5"));
    assert!(!extracted_text.contains("textwidth"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Box text."));
    assert!(!display_list_text.contains("0.5"));
    assert!(!display_list_text.contains("textwidth"));
}

#[test]
fn multicols_environment_capture_hides_layout_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MULTICOLS_ENVIRONMENT_SOURCE,
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

    assert!(environments.iter().any(|environment| {
        environment.name == "multicols"
            && environment.content.iter().any(|node| {
                matches!(
                    node,
                    InlineNode::Citation(citation)
                        if citation.keys == vec!["key".to_string()]
                )
            })
    }));
    assert!(
        environments
            .iter()
            .any(|environment| environment.name == "multicols*")
    );
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("multicols" | "multicols*")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Column [?] text."));
    assert!(extracted_text.contains("Wide text."));
    assert!(!extracted_text.contains("{2}"));
    assert!(!extracted_text.contains("{3}"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Column [?] text."));
    assert!(display_list_text.contains("Wide text."));
    assert!(!display_list_text.contains("{2}"));
    assert!(!display_list_text.contains("{3}"));
}

#[test]
fn paracol_environment_capture_hides_column_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        PARACOL_ENVIRONMENT_SOURCE,
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

    assert!(environments.iter().any(|environment| {
        environment.name == "paracol"
            && environment.content.iter().any(|node| {
                matches!(
                    node,
                    InlineNode::Citation(citation)
                        if citation.keys == vec!["key".to_string()]
                )
            })
    }));
    assert!(
        environments
            .iter()
            .any(|environment| environment.name == "paracol*")
    );
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("paracol" | "paracol*"))
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Column [?] text."));
    assert!(extracted_text.contains("Wide text."));
    for hidden in ["{2}", "{3}", "key"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Column [?] text."));
    assert!(display_list_text.contains("Wide text."));
    for hidden in ["{2}", "{3}", "key"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn threeparttable_capture_preserves_caption_and_notes_without_option_leakage() {
    let capture =
        capture_internal_render_ir("main.tex", THREEPARTTABLE_SOURCE, &SemanticAux::default());
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"threeparttable"));
    assert!(environment_names.contains(&"tablenotes"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("threeparttable" | "tablenotes")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Measured table."));
    assert!(extracted_text.contains("A | B"));
    assert!(extracted_text.contains("Note [?]."));
    for hidden in ["flushleft", "{key}"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Measured table."));
    assert!(display_list_text.contains("A | B"));
    assert!(display_list_text.contains("Note [?]."));
    for hidden in ["flushleft", "{key}"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn subcaption_wrappers_capture_hides_layout_arguments() {
    let capture = capture_internal_render_ir(
        "main.tex",
        SUBCAPTION_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(environment_names.contains(&"subfigure"));
    assert!(environment_names.contains(&"subtable"));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(fallback.environment.as_deref(), Some("subfigure" | "subtable"))
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Graphic(graphic)
                if graphic.path == "figures/panel-a.pdf"
                    && graphic.options.as_deref() == Some("width=4cm")
                    && graphic.caption.as_deref() == Some("Panel [?].")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Panel [?]."));
    assert!(extracted_text.contains("Panel table."));
    for hidden in ["0.45", "0.4", "textwidth", "{key}"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Panel [?]."));
    assert!(display_list_text.contains("Panel table."));
    for hidden in ["0.45", "0.4", "textwidth", "{key}"] {
        assert!(!display_list_text.contains(hidden));
    }
}

#[test]
fn subfloat_commands_capture_as_graphics_with_captions() {
    let capture =
        capture_internal_render_ir("main.tex", SUBFLOAT_COMMAND_SOURCE, &SemanticAux::default());

    for (path, options, caption) in [
        ("figures/a.pdf", Some("width=3cm"), "Panel [?]."),
        ("figures/b.pdf", Some("width=2cm"), "Box [?]."),
    ] {
        assert!(capture.document_ir.blocks.iter().any(|block| {
            matches!(
                block,
                IrBlock::Graphic(graphic)
                    if graphic.path == path
                        && graphic.options.as_deref() == options
                        && graphic.caption.as_deref() == Some(caption)
            )
        }));
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Panel [?]."));
    assert!(extracted_text.contains("Box [?]."));
    for hidden in ["subfloat", "subcaptionbox", "0.4", "textwidth", "key"] {
        assert!(!extracted_text.contains(hidden));
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
    assert!(display_list_text.contains("Panel [?]."));
    assert!(display_list_text.contains("Box [?]."));
    for hidden in ["subfloat", "subcaptionbox", "0.4", "textwidth", "key"] {
        assert!(!display_list_text.contains(hidden));
    }
    for path in ["figures/a.pdf", "figures/b.pdf"] {
        assert!(
            capture.page_display_lists[0]
                .ops
                .iter()
                .any(|op| matches!(op, DrawOp::Image(image) if image.asset_ref == path))
        );
    }
}

#[test]
fn algorithm_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        ALGORITHM_ENVIRONMENT_SOURCE,
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
    assert_eq!(environments[0].name, "algorithm");
    assert!(environments[0].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Step"
        )
    }));
    assert_eq!(environments[1].name, "algorithm*");
    assert!(environments[1].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Wide"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("algorithm" | "algorithm*")
                )
        )
    }));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"alg:first"));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Procedure."));
    assert!(extracted_text.contains("Step text."));
    assert!(extracted_text.contains("Wide step."));
    assert!(!extracted_text.contains("alg:first"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Procedure."));
    assert!(display_list_text.contains("Step text."));
    assert!(display_list_text.contains("Wide step."));
    assert!(!display_list_text.contains("alg:first"));
}

#[test]
fn algorithmic_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        ALGORITHMIC_ENVIRONMENT_SOURCE,
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
    assert_eq!(environments[0].name, "algorithmic");
    assert!(environments[0].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Step"
        )
    }));
    assert_eq!(environments[1].name, "algorithmic*");
    assert!(environments[1].content.iter().any(|node| {
        matches!(
            node,
            InlineNode::Text { text, .. } if text == "Wide"
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("algorithmic" | "algorithmic*")
                )
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Step one."));
    assert!(extracted_text.contains("Wide step."));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Step one."));
    assert!(display_list_text.contains("Wide step."));
}

#[test]
fn subequations_wrapper_preserves_inner_display_math() {
    let capture = capture_internal_render_ir(
        "main.tex",
        SUBEQUATIONS_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );

    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::Environment(environment) if environment.name == "subequations"
        )
    }));
    assert!(capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::DisplayMath(display)
                if display.raw_source == "x&=y"
                    && display.normalized_text.as_deref() == Some("x = y")
        )
    }));
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if matches!(
                    fallback.environment.as_deref(),
                    Some("subequations" | "align")
                )
        )
    }));

    let label_keys = capture
        .document_ir
        .labels
        .iter()
        .map(|label| label.key.as_str())
        .collect::<Vec<_>>();
    assert!(label_keys.contains(&"eq:group"));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("x = y"));
    assert!(!extracted_text.contains("x&=y"));
    assert!(!extracted_text.contains("eq:group"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("x = y"));
    assert!(!display_list_text.contains("x&=y"));
    assert!(!display_list_text.contains("eq:group"));
}

#[test]
fn theorem_like_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        THEOREM_LIKE_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        environment_names,
        vec![
            "lemma",
            "proposition",
            "corollary",
            "definition",
            "remark",
            "example"
        ]
    );
    for environment in [
        "lemma",
        "proposition",
        "corollary",
        "definition",
        "remark",
        "example",
    ] {
        assert!(!capture.document_ir.blocks.iter().any(|block| {
            matches!(
                block,
                IrBlock::RawFallback(fallback)
                    if fallback.environment.as_deref() == Some(environment)
            )
        }));
    }

    let extracted_text = capture.document_ir.extracted_text();
    for expected in [
        "Lemma text.",
        "Proposition text.",
        "Corollary text.",
        "Definition text.",
        "Remark text.",
        "Example text.",
    ] {
        assert!(extracted_text.contains(expected));
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
    for expected in [
        "Lemma text.",
        "Proposition text.",
        "Corollary text.",
        "Definition text.",
        "Remark text.",
        "Example text.",
    ] {
        assert!(display_list_text.contains(expected));
    }
}

#[test]
fn theorem_environment_optional_titles_do_not_leak_raw_brackets() {
    let capture = capture_internal_render_ir(
        "main.tex",
        THEOREM_ENVIRONMENT_TITLE_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(environment_names, vec!["theorem", "proof"]);

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Sharp bound Body."));
    assert!(extracted_text.contains("Sketch Done."));
    assert!(!extracted_text.contains("[Sharp bound]"));
    assert!(!extracted_text.contains("[Sketch]"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Sharp bound Body."));
    assert!(display_list_text.contains("Sketch Done."));
    assert!(!display_list_text.contains("[Sharp bound]"));
    assert!(!display_list_text.contains("[Sketch]"));
}

#[test]
fn newtheorem_defined_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(environment_names, vec!["claim"]);
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("claim")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Named claim Claim body."));
    assert!(!extracted_text.contains("[Named claim]"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Named claim Claim body."));
    assert!(!display_list_text.contains("[Named claim]"));
}

#[test]
fn starred_newtheorem_defined_environment_capture_survives_ir_and_display_list() {
    let capture = capture_internal_render_ir(
        "main.tex",
        STARRED_NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE,
        &SemanticAux::default(),
    );
    let environment_names = capture
        .document_ir
        .blocks
        .iter()
        .filter_map(|block| match block {
            IrBlock::Environment(environment) => Some(environment.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(environment_names, vec!["namedclaim"]);
    assert!(!capture.document_ir.blocks.iter().any(|block| {
        matches!(
            block,
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("namedclaim")
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Starred claim body."));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Starred claim body."));
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
                                == r"\ref{sec:intro}"
                    )
                    && run.source.related.iter().any(|related| {
                        related.role == SourceSpanRole::ReferenceKey
                            && matches!(
                                &related.span,
                                ProvenanceSpan::File(span)
                                    if &AUX_RESOLUTION_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == "sec:intro"
                            )
                    })
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
                                == r"\cite{key}"
                    )
                    && run.source.related.iter().any(|related| {
                        related.role == SourceSpanRole::CitationKey
                            && matches!(
                                &related.span,
                                ProvenanceSpan::File(span)
                                    if &AUX_RESOLUTION_SOURCE
                                        [span.start_utf8 as usize..span.end_utf8 as usize]
                                        == "key"
                            )
                    })
        )
    }));
    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("([7]) Tj"));
    assert!(!pdf_text.contains("key"));
    assert!(!pdf_text.contains("sec:intro"));
}

#[test]
fn aux_resolved_numeric_citation_ranges_survive_ir_and_display_list() {
    let mut aux = SemanticAux::default();
    for (key, label) in [
        ("alpha", "1"),
        ("beta", "2"),
        ("gamma", "3"),
        ("delta", "5"),
    ] {
        aux.bibliography.push(BibliographyEntry {
            key: key.to_string(),
            text: format!("{key} entry."),
            label: Some(label.to_string()),
            file: Utf8PathBuf::from("refs.bbl"),
        });
    }

    let capture = capture_internal_render_ir("main.tex", AUX_CITATION_RANGE_SOURCE, &aux);
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
                if citation.keys
                    == vec![
                        "alpha".to_string(),
                        "beta".to_string(),
                        "gamma".to_string(),
                        "delta".to_string()
                    ]
                    && citation.resolved_label.as_deref() == Some("[1-3,5]")
                    && citation.display_text == "[1-3,5]"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("See [1-3,5]."), "{extracted_text}");
    for hidden in ["alpha", "beta", "gamma", "delta"] {
        assert!(
            !extracted_text.contains(hidden),
            "{hidden} leaked in {extracted_text}"
        );
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
    assert!(
        display_list_text.contains("See [1-3,5]."),
        "{display_list_text}"
    );
    for hidden in ["alpha", "beta", "gamma", "delta"] {
        assert!(
            !display_list_text.contains(hidden),
            "{hidden} leaked in {display_list_text}"
        );
    }

    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("([1-3,5]) Tj"));
    for hidden in ["alpha", "beta", "gamma", "delta"] {
        assert!(!pdf_text.contains(hidden), "{hidden} leaked in {pdf_text}");
    }
}

#[test]
fn aux_natexlab_labels_are_normalized_in_ir_and_display_list() {
    let mut aux = SemanticAux::default();
    aux.bibliography.push(BibliographyEntry {
        key: "alpha".to_string(),
        text: "Alpha entry.".to_string(),
        label: Some(r"Alpha 2024\natexlab{a}".to_string()),
        file: Utf8PathBuf::from("refs.bbl"),
    });
    aux.bibliography.push(BibliographyEntry {
        key: "beta".to_string(),
        text: "Beta entry.".to_string(),
        label: Some(r"Beta 2023\NAT@exlab{b}".to_string()),
        file: Utf8PathBuf::from("refs.bbl"),
    });

    let capture = capture_internal_render_ir("main.tex", AUX_NATEXLAB_SOURCE, &aux);
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
                if citation.keys == vec!["alpha".to_string(), "beta".to_string()]
                    && citation.resolved_label.as_deref() == Some("[Alpha 2024a,Beta 2023b]")
                    && citation.display_text == "[Alpha 2024a,Beta 2023b]"
        )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(
        extracted_text.contains("See [Alpha 2024a,Beta 2023b]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("natexlab"));
    assert!(!extracted_text.contains("NAT@exlab"));

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
        display_list_text.contains("See [Alpha 2024a,Beta 2023b]."),
        "{display_list_text}"
    );
    assert!(!display_list_text.contains("natexlab"));
    assert!(!display_list_text.contains("NAT@exlab"));
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

    let destination = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::NamedDestination(destination) => Some(destination),
            _ => None,
        })
        .expect("label should emit a named destination");
    assert_eq!(destination.name, "sec:intro");
    assert!(matches!(
        &destination.source.primary,
        ProvenanceSpan::File(span)
            if &LABEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize] == "sec:intro"
    ));

    let pdf_text = String::from_utf8_lossy(&capture.display_list_pdf);
    assert!(pdf_text.contains("/Names << /Dests << /Names ["));
    assert!(pdf_text.contains("(sec:intro) [16 0 R /XYZ"));
    assert!(!pdf_text.contains("(sec:intro) Tj"));
}

#[test]
fn label_wrapper_macro_capture_survives_ir_without_visible_key() {
    let capture =
        capture_internal_render_ir("main.tex", LABEL_WRAPPER_SOURCE, &SemanticAux::default());

    assert_eq!(capture.document_ir.labels.len(), 2);
    for (key, source_text) in [("sec:intro", "sec:intro"), ("sec:alias", "sec:alias")] {
        let label = capture
            .document_ir
            .labels
            .iter()
            .find(|label| label.key == key)
            .expect("label ir");
        assert!(matches!(
            &label.source.primary,
            ProvenanceSpan::File(span)
                if &LABEL_WRAPPER_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                    == source_text
        ));
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Intro"), "{extracted_text}");
    assert!(
        extracted_text.contains("See [?] and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains("sec:alias"));
    assert!(!extracted_text.contains(r"\seclabel"));
    assert!(!extracted_text.contains(r"\aliaslabel"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("Intro"));
    assert!(display_list_text.contains("See [?] and [?]."));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains("sec:alias"));
}

#[test]
fn templated_reference_and_label_wrapper_keys_survive_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        TEMPLATED_REFERENCE_LABEL_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );

    let label = capture
        .document_ir
        .labels
        .iter()
        .find(|label| label.key == "sec:intro")
        .expect("templated label");
    assert!(matches!(
        &label.source.primary,
        ProvenanceSpan::File(span)
            if &TEMPLATED_REFERENCE_LABEL_WRAPPER_SOURCE
                [span.start_utf8 as usize..span.end_utf8 as usize]
                == "intro"
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
    let reference = paragraph
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .expect("templated reference");
    assert_eq!(reference.command, "ref");
    assert_eq!(reference.keys, vec!["sec:intro".to_string()]);
    assert!(matches!(
        &reference.source.primary,
        ProvenanceSpan::File(span)
            if &TEMPLATED_REFERENCE_LABEL_WRAPPER_SOURCE
                [span.start_utf8 as usize..span.end_utf8 as usize]
                == r"\secref{intro}"
    ));
    assert!(reference.source.related.iter().any(|related| {
        related.role == SourceSpanRole::ReferenceKey
            && matches!(
                &related.span,
                ProvenanceSpan::File(span)
                    if &TEMPLATED_REFERENCE_LABEL_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "intro"
            )
    }));

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Intro"), "{extracted_text}");
    assert!(extracted_text.contains("See [?]."), "{extracted_text}");
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\seclabel"));
    assert!(!extracted_text.contains(r"\secref"));

    let display_list_text = capture.page_display_lists[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(display_list_text.contains("See [?]."));
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\seclabel"));
    assert!(!display_list_text.contains(r"\secref"));
}

#[test]
fn constant_reference_and_label_wrapper_keys_survive_ir_without_visible_keys() {
    let capture = capture_internal_render_ir(
        "main.tex",
        CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );

    let label = capture
        .document_ir
        .labels
        .iter()
        .find(|label| label.key == "sec:intro")
        .expect("constant label");
    assert!(matches!(
        &label.source.primary,
        ProvenanceSpan::File(span)
            if &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                [span.start_utf8 as usize..span.end_utf8 as usize]
                == "sec:intro"
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
    let references = paragraph
        .content
        .iter()
        .filter_map(|node| match node {
            InlineNode::Reference(reference) => Some(reference),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(references.len(), 4);
    for reference in references {
        assert_eq!(reference.command, "ref");
        assert_eq!(reference.keys, vec!["sec:intro".to_string()]);
        assert!(matches!(
            &reference.source.primary,
            ProvenanceSpan::File(span)
                if &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                    [span.start_utf8 as usize..span.end_utf8 as usize]
                    == r"\introref"
                    || &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\aliasintroref"
                    || &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\defaultref"
                    || &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\aliasdefaultref"
        ));
        assert!(reference.source.related.iter().any(|related| {
            related.role == SourceSpanRole::ReferenceKey
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if &CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == "sec:intro"
                )
        }));
    }

    let extracted_text = capture.document_ir.extracted_text();
    assert!(extracted_text.contains("Intro"), "{extracted_text}");
    assert!(
        extracted_text.contains("See [?], [?], [?], and [?]."),
        "{extracted_text}"
    );
    assert!(!extracted_text.contains("sec:intro"));
    assert!(!extracted_text.contains(r"\introlabel"));
    assert!(!extracted_text.contains(r"\introref"));

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
    assert!(!display_list_text.contains("sec:intro"));
    assert!(!display_list_text.contains(r"\introlabel"));
    assert!(!display_list_text.contains(r"\introref"));
}

#[test]
fn label_definition_provenance_preserves_key_and_invocation_spans() {
    let capture = capture_internal_render_ir("main.tex", LABEL_SOURCE, &SemanticAux::default());
    let label_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(&envelope.event, RenderEvent::LabelDefinition(label) if label.key == "sec:intro")
        })
        .expect("label event");
    let label = capture
        .document_ir
        .labels
        .iter()
        .find(|label| label.key == "sec:intro")
        .expect("label ir");

    for source in [&label_event.meta.source, &label.source] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "main.tex"
                    && &LABEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                        == "sec:intro"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "main.tex"
                            && &LABEL_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\label{sec:intro}"
                )
        }));
    }

    let provenance_snapshot = serde_json::json!({
        "source": LABEL_SOURCE,
        "event": {
            "event": label_event.event,
            "meta": label_event.meta,
        },
        "ir": label,
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/label.provenance.json",
        &provenance_json,
    );
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
    assert!(display_list_svg.contains("data-text-clusters=\""));
    assert!(display_list_svg.contains(&format!(
        "data-page-id=\"{}\"",
        capture.page_display_lists[0].page_id
    )));
    assert!(display_list_svg.contains(&format!(
        "data-content-hash=\"{}\"",
        capture.page_display_lists[0].content_hash
    )));
    assert!(
        fs::read(paths.display_list_pdf)
            .expect("display list pdf")
            .starts_with(b"%PDF-1.4")
    );
}

#[test]
fn raw_fallback_debug_artifacts_write_full_source_files() {
    let capture =
        capture_internal_render_ir("main.tex", TIKZ_FALLBACK_SOURCE, &SemanticAux::default());
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("tikzpicture") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("tikz fallback");
    let artifact = fallback
        .full_source_artifact
        .as_deref()
        .expect("full source artifact");

    assert!(artifact.starts_with("fallbacks/"));
    assert!(artifact.ends_with(".tex"));
    assert_eq!(fallback.source.generated_by, GeneratedBy::Fallback);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let output_dir = Utf8PathBuf::from_path_buf(tempdir.path().join("render-artifacts"))
        .expect("utf8 temp path");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let artifact_path = output_dir.join(artifact);
    assert_eq!(paths.fallback_sources, vec![artifact_path.clone()]);

    let written_source = fs::read_to_string(&artifact_path).expect("fallback source artifact");
    assert!(matches!(
        &fallback.source.primary,
        ProvenanceSpan::File(span)
            if written_source
                == TIKZ_FALLBACK_SOURCE[span.start_utf8 as usize..span.end_utf8 as usize]
    ));
    assert!(written_source.contains(r"\draw (0,0) -- (1,1);"));
    assert!(written_source.contains(r"\node {Should not render};"));

    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("display list svg");
    assert!(display_list_svg.contains("data-source-generated-by=\"fallback\""));
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

#[test]
fn link_target_default_display_list_svg_preserves_cross_file_provenance() {
    let macros = r"\newcommand{\defaulttargetlink}[2][https://default.test]{\href{#1}{#2}}";
    let source = r"\input{macros}
\begin{document}
Read \defaulttargetlink{visible link}.
\end{document}";
    let capture = capture_internal_render_ir_with_mounted_files(
        "main.tex",
        source,
        &SemanticAux::default(),
        &[("macros.tex", macros)],
    );
    let display_link = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "visible link" => Some(run),
            _ => None,
        })
        .expect("link display-list run");
    let primary_span = match &display_link.source.primary {
        ProvenanceSpan::File(span) => span,
        ProvenanceSpan::Generated(_) => panic!("link text should use a file source"),
    };
    let target_span = display_link
        .source
        .related
        .iter()
        .find_map(|related| match &related.span {
            ProvenanceSpan::File(span) if related.role == SourceSpanRole::Argument => Some(span),
            _ => None,
        })
        .expect("target argument span");

    let tempdir = tempfile::tempdir().expect("tempdir");
    let output_dir = Utf8PathBuf::from_path_buf(tempdir.path().join("render-artifacts"))
        .expect("utf8 temp path");
    let paths = capture
        .write_debug_artifacts(&output_dir)
        .expect("write debug artifacts");
    let display_list_svg =
        fs::read_to_string(&paths.display_list_svgs[0]).expect("display list svg");

    assert!(display_list_svg.contains(">visible link</text>"));
    assert!(display_list_svg.contains(&format!(
        "data-source-path=\"{}\" data-source-start-utf8=\"{}\" data-source-end-utf8=\"{}\"",
        primary_span.path, primary_span.start_utf8, primary_span.end_utf8
    )));
    assert!(display_list_svg.contains("data-source-related-roles=\"invocation,argument\""));
    assert!(display_list_svg.contains(&format!(
        "argument:file:{}:{}:{}",
        target_span.path, target_span.start_utf8, target_span.end_utf8
    )));
}

#[test]
fn macro_heading_provenance_matches_golden() {
    let capture =
        capture_internal_render_ir("main.tex", MACRO_SECTION_SOURCE, &SemanticAux::default());
    let heading_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(&envelope.event, RenderEvent::Heading(heading) if heading.text == "Intro")
        })
        .expect("heading event");
    let heading_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Heading(heading) => Some(heading),
            _ => None,
        })
        .expect("heading block");
    let heading_text_source = heading_block
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Text { text, source } if text == "Intro" => Some(source),
            _ => None,
        })
        .expect("heading text source");
    let heading_text_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "Intro" => Some(run),
            _ => None,
        })
        .expect("heading text run");
    let provenance_snapshot = serde_json::json!({
        "source": MACRO_SECTION_SOURCE,
        "event": {
            "event": heading_event.event,
            "meta": heading_event.meta,
        },
        "ir": {
            "level": heading_block.level,
            "number": heading_block.number,
            "source": heading_block.source,
            "text_source": heading_text_source,
        },
        "display_list": {
            "text": heading_text_run.text,
            "source": heading_text_run.source,
            "clusters": heading_text_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/macro-heading.provenance.json",
        &provenance_json,
    );
}

#[test]
fn starred_providecommand_macro_heading_provenance_matches_golden() {
    let capture = capture_internal_render_ir(
        "main.tex",
        STARRED_PROVIDED_MACRO_SECTION_SOURCE,
        &SemanticAux::default(),
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "Provided",
        "main.tex",
        STARRED_PROVIDED_MACRO_SECTION_SOURCE,
        r"\mysection{Provided}",
        "main.tex",
        STARRED_PROVIDED_MACRO_SECTION_SOURCE,
        r"\providecommand*{\mysection}[1]{\section{#1}}",
        &[("source", STARRED_PROVIDED_MACRO_SECTION_SOURCE)],
        "tests/goldens/render_ir/starred-providecommand-macro-heading.provenance.json",
    );
}

#[test]
fn optional_section_macro_heading_provenance_matches_golden() {
    let capture = capture_internal_render_ir(
        "main.tex",
        OPTIONAL_MACRO_SECTION_SOURCE,
        &SemanticAux::default(),
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "Long Title",
        "main.tex",
        OPTIONAL_MACRO_SECTION_SOURCE,
        r"\mysection[Short]{Long Title}",
        "main.tex",
        OPTIONAL_MACRO_SECTION_SOURCE,
        r"\newcommand{\mysection}[2][]{\section[#1]{#2}}",
        &[("source", OPTIONAL_MACRO_SECTION_SOURCE)],
        "tests/goldens/render_ir/optional-section-macro-heading.provenance.json",
    );
}

#[test]
fn def_section_macro_heading_provenance_matches_golden() {
    let capture = capture_internal_render_ir(
        "main.tex",
        DEF_MACRO_SECTION_SOURCE,
        &SemanticAux::default(),
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "Plain Def",
        "main.tex",
        DEF_MACRO_SECTION_SOURCE,
        r"\mysection{Plain Def}",
        "main.tex",
        DEF_MACRO_SECTION_SOURCE,
        r"\def\mysection#1{\section{#1}}",
        &[("source", DEF_MACRO_SECTION_SOURCE)],
        "tests/goldens/render_ir/def-section-macro-heading.provenance.json",
    );
}

#[test]
fn let_section_alias_heading_provenance_matches_golden() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LET_SECTION_ALIAS_SOURCE,
        &SemanticAux::default(),
    );

    assert_macro_heading_provenance_golden(
        &capture,
        "Alias Title",
        "main.tex",
        LET_SECTION_ALIAS_SOURCE,
        r"\mysection{Alias Title}",
        "main.tex",
        LET_SECTION_ALIAS_SOURCE,
        r"\let\mysection\section",
        &[("source", LET_SECTION_ALIAS_SOURCE)],
        "tests/goldens/render_ir/let-section-alias-heading.provenance.json",
    );
}

const COMPACT_SOURCE: &str = r"\title{A Paper}\author{Ada Lovelace}\date{May 1843}\begin{document}\maketitle\begin{abstract}Short abstract.\end{abstract}\section{Intro}Hello \cite{key}.\[x^2\]\begin{thebibliography}{1}\bibitem{key} Author. Title.\end{thebibliography}\begin{unknownenv}Fallback text.\end{unknownenv}\end{document}";

const TITLE_INLINE_KEY_SOURCE: &str =
    r"\title{See \cite{key} and \ref{sec:intro}.}\begin{document}\maketitle\end{document}";

const AUTHBLK_FRONTMATTER_SOURCE: &str = r"\usepackage{authblk}\title{Quantum Paper}\author[1]{Nai-Hui Chia\thanks{nc67@rice.edu}}\author[2]{Atsuya Hasegawa}\affil[1]{\textit{Department of Computer Science}}\affil[2]{Graduate School of Mathematics}\begin{document}\maketitle\end{document}";

const LLNCS_FRONTMATTER_SOURCE: &str = r"\documentclass{llncs}\title{LNCS Paper}\author{Alice \inst{1}\orcidID{0000} \and Bob \inst{2}}\institute{Lab One \email{alice@example.test} \and Lab Two}\begin{document}\maketitle\end{document}";

const REVTEX_FRONTMATTER_SOURCE: &str = r"\documentclass{revtex4-2}\title{REVTeX Paper}\author{Alice}\email{alice@example.test}\affiliation{Quantum Lab}\begin{document}\maketitle\end{document}";

const WACV_FRONTMATTER_SOURCE: &str = r"\usepackage{wacv}\title{WACV Paper}\author{Alice}\affiliation{Vision Lab}\begin{document}\maketitle\end{document}";

const IEEE_FRONTMATTER_SOURCE: &str = r"\documentclass{IEEEtran}\title{IEEE Paper}\author{\IEEEauthorblockN{Alice Smith\IEEEauthorrefmark{1} \and Bob Jones\IEEEauthorrefmark{2}}\IEEEauthorblockA{Vision Lab}}\begin{document}\maketitle\end{document}";

const FOOTNOTE_BODY_SOURCE: &str = r"\begin{document}Text\footnote{Note \cite{key} and \ref{sec:intro}.} after.\footnotetext[1]{Loose note.}\end{document}";

const STARRED_ABSTRACT_SOURCE: &str =
    r"\begin{document}\begin{abstract*}Starred \cite{key} abstract.\end{abstract*}\end{document}";

const ONECOL_ABSTRACT_SOURCE: &str = r"\begin{document}\begin{onecolabstract}One-column \cite{key} abstract.\end{onecolabstract}\end{document}";

const BIBLIOGRAPHY_ITEM_INLINE_KEY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{entry} See \cite{cited} and \ref{sec:intro}.\end{thebibliography}\end{document}";

const MKBIB_WRAPPER_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\mkbibquote{Alpha Title}. \mkbibparens{2024}. \mkbibbrackets{note}. \mkbibbraces{Supplement}. \mkbibemph{Emph}. \mkbibbold{Bold}. \mkbibitalic{Italic}.\end{thebibliography}\end{document}";

const BIBSTRING_ACRO_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\mkbibnamefamily{Alpha} \bibstring{andothers}. \mkbibacro{URL}: \url{https://example.test/paper}.\end{thebibliography}\end{document}";

const PUNCTUATION_HELPER_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Alpha\addcomma\addspace Beta\newunit Gamma\addcolon\addspace Delta\addsemicolon\addspace Epsilon\adddot\finentry\end{thebibliography}\end{document}";

const MKBIB_SUPER_SUB_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Edition\mkbibsuperscript{2}\mkbibsubscript{a} \mkbibbraces{Supplement}.\end{thebibliography}\end{document}";

const LOW_LEVEL_PUNCTUATION_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Alpha\adddotspace Beta\unspace\isdot\nopunct Gamma\isdot \bibopenparen Delta\bibcloseparen \bibopenbracket Epsilon\bibclosebracket \bibopenbrace Zeta\bibclosebrace\end{thebibliography}\end{document}";

const DASH_SLASH_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Pages 10\bibrangedash20\addcomma\addspace Vol\adddot 2\addslash Issue 3\addhyphen4\textendash5\textemdash appendix.\end{thebibliography}\end{document}";

const PARENTEXT_SPACING_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Alpha\addabbrvspace Beta\addnbspace Gamma\addthinspace Delta\addlowpenspace Epsilon\addhighpenspace Zeta\parentext{Supplement}.\end{thebibliography}\end{document}";

const NAMEDASH_URLPREFIX_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\bibnamedash. \urlprefix\url{https://example.test/paper}.\end{thebibliography}\end{document}";

const NAME_AFFIX_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\mkbibnamefamily{Doe}, \mkbibnameaffix{Jr.}.\end{thebibliography}\end{document}";

const STARRED_WRAPPER_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\MakeSentenceCase*{alpha title}. \MakeTitleCase*{beta title}. \mkbibquote*{Alpha Title}. \mkbibparens*{2024}. \mkbibbrackets*{note}. \mkbibbraces*{Supplement}. \mkbibemph*{Emph}. \mkbibbold*{Bold}. \mkbibitalic*{Italic}.\end{thebibliography}\end{document}";

const BIBINFO_BIBFIELD_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\bibinfo{doi}{10.1000/example}. \bibfield{journal}{Journal of Tests}.\end{thebibliography}\end{document}";

const DOI_EPRINT_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Alpha entry. \doi{10.1000/example}. \eprint{arXiv:2401.00001}. \href{https://example.test}{Link}.\end{thebibliography}\end{document}";

const NATEXLAB_NEWBLOCK_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem[Alpha 2024\natexlab{a}]{alpha}Alpha \newblock 2024\NAT@exlab{a}.\end{thebibliography}\end{document}";

const PHANTOM_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Visible \phantom{Ghost}\hphantom{Wide}\vphantom{Tall}Text.\end{thebibliography}\end{document}";

const TEX_SPACING_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Tight\!Join. Soft\,Gap. Wide\;Gap. Colon\:Gap. Named\space Gap. Backslash\ Gap.\end{thebibliography}\end{document}";

const TEXT_SYMBOL_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}Quote\textquotesingle s. Double\textquotedbl q. Angles\textless x\textgreater. Pipe\textbar join. Path\slash name.\end{thebibliography}\end{document}";

const TEXTSTYLE_BOX_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\NoCaseChange{NASA}. \MakeSentenceCase{alpha title}. \MakeTitleCase*{beta title}. \protect\relax\leavevmode\ignorespaces   \emph{Emph}. Trimmed \unskip. \mbox{Stable}. \hbox{Fixed}. \fbox{Framed}. \framebox[2em][c]{Wide}. \raisebox{0.5ex}[1ex][0ex]{Raised}. \parbox[t]{4em}{Paragraph}. \makebox[3em][l]{Inline}. \texttt{Code}. \textsf{Sans}. \textsc{Caps}. \textbf{Bold}. \textit{Italic}. \textrm{Roman}. \textup{Upright}. \textmd{Medium}. \textnormal{Normal}. Edition\textsuperscript{2}\textsubscript{a}.\end{thebibliography}\end{document}";

const URLSTYLE_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}\begin{thebibliography}{1}\bibitem{alpha}\urlstyle{same}\url{https://example.test/paper}.\end{thebibliography}\end{document}";

const RAW_FALLBACK_INLINE_KEY_SOURCE: &str = r"\begin{document}\begin{unknownenv}See \cite{cited} and \ref{sec:intro}.\end{unknownenv}\end{document}";

const TIKZ_FALLBACK_SOURCE: &str = r"\begin{document}\begin{tikzpicture}\draw (0,0) -- (1,1); \node {Should not render};\end{tikzpicture}\end{document}";

const VERBATIM_FALLBACK_SOURCE: &str =
    r"\begin{document}\begin{verbatim}\alpha_{i} {raw}\end{verbatim}\end{document}";

const LONGTABLE_FALLBACK_SOURCE: &str = r"\begin{document}\begin{longtable}{ll}Alpha & Beta \\ Gamma & \textbf{Delta} \\\hline\end{longtable}\end{document}";

const LONGTABLE_FALLBACK_LABEL_SOURCE: &str = r"\begin{document}\begin{longtable}{ll}\caption{Long table.}\label{tab:long}\\ Alpha & Beta\end{longtable}\end{document}";

const CODE_LISTING_FALLBACK_SOURCE: &str = r#"\begin{document}\begin{lstlisting}[language=Rust]fn main() { println!("hi"); }\end{lstlisting}\begin{minted}{rust}let value = \alpha_{i} + {raw};\end{minted}\end{document}"#;

const FANCYVRB_FALLBACK_SOURCE: &str = r"\begin{document}\begin{Verbatim}[fontsize=\small]\foo_{bar} {baz}\end{Verbatim}\end{document}";

const GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\end{figure}\end{document}";

const EXTENSIONLESS_GRAPHIC_SOURCE: &str = r"\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot}\caption{Plot caption.}\end{figure}\end{document}";

const EXTENSIONLESS_SVG_GRAPHIC_SOURCE: &str = r"\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/vector}\caption{Vector caption.}\end{figure}\end{document}";

const GRAPHICSPATH_SOURCE: &str = r"\graphicspath{{figures/}{unused/}}\begin{document}\begin{figure}\includegraphics[width=5cm]{plot}\caption{Plot caption.}\end{figure}\end{document}";

const DECLARED_GRAPHIC_EXTENSIONS_SOURCE: &str = r"\DeclareGraphicsExtensions{.png,.pdf}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot}\caption{Plot caption.}\end{figure}\end{document}";

const LEGACY_EPSFIG_SOURCE: &str = r"\begin{document}\begin{figure}\epsfig{file=figures/plot,width=5cm}\caption{Plot caption.}\end{figure}\end{document}";

const LEGACY_EPSF_FILE_SOURCE: &str = r"\begin{document}\begin{figure}\epsfbox{figures/plot}\caption{Plot caption.}\end{figure}\end{document}";

const GRAPHIC_LAYOUT_BOX_WRAPPER_SOURCE: &str = r"\begin{document}\resizebox{0.8\textwidth}{0.4\textheight}{\includegraphics[width=5cm]{figures/plot}}\scalebox{0.5}[2]{\epsfbox{figures/other}}\rotatebox[origin=c]{90}{\psfig{figure=figures/third.eps,width=2cm}}\end{document}";

const NESTED_GRAPHIC_LAYOUT_BOX_WRAPPER_SOURCE: &str = r"\begin{document}\resizebox{0.5\textwidth}{!}{\scalebox{0.5}[2]{\includegraphics{figures/nested}}}\reflectbox{\resizebox{2cm}{!}{\includegraphics{figures/reflected}}}\end{document}";

const GRAPHIC_ALIGNMENT_BOX_WRAPPER_SOURCE: &str = r"\begin{document}\adjustbox{width=\textwidth,center}{\includegraphics{figures/plot}}\centerline{\includegraphics{figures/other}}\makebox[\textwidth][c]{\epsfbox{figures/third}}\end{document}";

const STARRED_GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics*[width=3cm]{figures/starred.pdf}\caption{Starred plot.}\end{figure}\end{document}";

const STARRED_FLOAT_GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure*}\includegraphics[width=6cm]{figures/wide.pdf}\caption{Wide figure.}\end{figure*}\end{document}";

const STARRED_TABLE_SOURCE: &str = r"\def\caption#1{#1}\begin{document}\begin{table*}\caption{Wide table.}\end{table*}\end{document}";

const SIDEWAYS_FLOAT_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{sidewaysfigure}\includegraphics[width=4cm]{figures/rotated.pdf}\caption{Rotated figure.}\label{fig:rot}\end{sidewaysfigure}\begin{sidewaystable}\caption{Rotated table.}\label{tab:rot}\end{sidewaystable}\end{document}";

const SIDECAP_FLOAT_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{SCfigure}[1][ht]\includegraphics[width=4cm]{figures/side.pdf}\caption{Side \cite{key}.}\label{fig:side}\end{SCfigure}\begin{SCtable}\caption{Side table.}\label{tab:side}\end{SCtable}\end{document}";

const WRAP_FLOAT_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{wrapfigure}{r}{0.35\textwidth}\includegraphics[width=3cm]{figures/wrapped.pdf}\caption{Wrapped figure.}\label{fig:wrap}\end{wrapfigure}\begin{wraptable}{l}{0.4\textwidth}\caption{Wrapped table.}\label{tab:wrap}\end{wraptable}\end{document}";

const FIGURE_TABLE_LABEL_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\label{fig:plot}\end{figure}\begin{table}\caption{Table caption.}\label{tab:data}\end{table}\end{document}";

const CAPTIONOF_SOURCE: &str = r"\begin{document}\captionof{figure}[Short Figure]{Long Figure Title}\label{fig:first}See \autoref{fig:first}.\captionof*{table}{Long Table Title}\label{tab:first}See \autoref{tab:first}.\end{document}";

const STARRED_CAPTION_SOURCE: &str = r"\begin{document}\begin{figure}\caption*{Unnumbered Figure Caption}\label{fig:starred}\end{figure}See \autoref{fig:starred}.\end{document}";

const CAPTION_INLINE_KEY_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{See \cite{key} and \ref{sec:intro}.}\end{figure}\end{document}";

const CAPTION_HREF_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Read \href{https://hidden.test}{paper} and \cite{key}.}\end{figure}\end{document}";

const CAPTION_URL_LIKE_WRAPPER_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Visit \url|https://shown.test/path| at \path|/tmp/archive| via \nolinkurl|https://visible.test/raw| and \detokenize{\foo+*}.}\end{figure}\end{document}";

const INLINE_MATH_SOURCE: &str = r"\begin{document}Area \(x^2 + y^2\).\end{document}";

const DOLLAR_MATH_SOURCE: &str = r"\begin{document}Area $x^2 + y^2$.$$z^2$$\end{document}";

const MATH_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{equation}\frac{a}{b}\end{equation}\begin{flalign*}a&=b\end{flalign*}\begin{alignat*}{2}x&=y\end{alignat*}\begin{eqnarray*}u&=&v\end{eqnarray*}\end{document}";

const MATH_ENVIRONMENT_LABEL_SOURCE: &str =
    r"\begin{document}\begin{equation}\label{eq:one}x\end{equation}\end{document}";

const BRACKET_DISPLAY_MATH_LABEL_SOURCE: &str =
    r"\begin{document}\[\label{eq:bracket}y\]\end{document}";

const DOLLAR_DISPLAY_MATH_LABEL_SOURCE: &str =
    r"\begin{document}$$\label{eq:dollar}z$$\end{document}";

const HEADING_LEVEL_SOURCE: &str = r"\begin{document}\section[Short]{Long Section}\subsection*{Methods}\subsubsection{Details}\paragraph{Sketch}\end{document}";

const HEADING_INLINE_KEY_SOURCE: &str =
    r"\begin{document}\section{See \cite{key} and \ref{sec:intro}.}\end{document}";

const CITATION_VARIANTS_SOURCE: &str = r"\begin{document}\citep[see][p.~3]{alpha,beta}\citet*{gamma}\parencite{delta}\textcite{epsilon}\citep*{zeta}\citealt*{eta}\citealp*{theta}\Textcite*{iota}\Citealt{lambda}\Citealp{mu}\end{document}";

const CITATION_WRAPPER_SOURCE: &str = r"\newcommand{\mycitepair}[2]{\cite{#1,#2}}\let\aliascitepair\mycitepair\newcommand{\papercite}[1]{\cite{paper:#1}}\newcommand{\corecite}{\cite{core}}\let\aliascorecite\corecite\newcommand{\defaultcite}[1][default]{\cite{#1}}\let\aliasdefaultcite\defaultcite\begin{document}See \mycitepair{alpha}{beta}, \aliascitepair{gamma}{delta}, \papercite{one}, \corecite, \aliascorecite, \defaultcite, and \aliasdefaultcite.\end{document}";

const CITATION_METADATA_ALIAS_SOURCE: &str = r"\begin{document}\Citeauthor{alpha} \Citeyear{beta} \Citeyearpar{gamma} \citetitle{delta} \Citetitle{epsilon} \citefullauthor{zeta} \Citefullauthor*{eta}\end{document}";

const CITATION_IDENTIFIER_DATE_ALIAS_SOURCE: &str = r"\begin{document}\citedoi{doi} \citeeprint{eprint} \citeisbn{isbn} \citeissn{issn} \citeurl{url} \citenum{number} \citedate{date} \Citedate{capdate} \citeurldate{urldate} \Citeurldate{capurldate}\end{document}";

const CITATION_ENTRY_ALIAS_SOURCE: &str = r"\begin{document}\onlinecite{online} \smartcite{smart} \fullcite{full} \footfullcite{footfull} \bibentry{entry} \citetalias{textalias} \citepalias{parenalias} \Citetalias{capalias}\end{document}";

const DEFCITEALIAS_SOURCE: &str = r"\begin{document}\defcitealias{alpha}{Paper I}Alias \citetalias{alpha}, \citepalias{alpha}, and \Citetalias{alpha}.\end{document}";

const ADDBIBRESOURCE_SOURCE: &str = r"\begin{document}\addbibresource[location=local]{refs.bib}Bib \textcite{alpha} and \parencite{beta}.\end{document}";

const LINENO_COMMAND_SOURCE: &str = r"\begin{document}\linenumbers\modulolinenumbers[2]Visible \cite{key} text.\resetlinenumber[7]\nolinenumbers After.\end{document}";

const LAYOUT_SPACING_COMMAND_SOURCE: &str = r"\begin{document}Before \vspace*{-1em} After \hspace{2mm} Gap.\smallskip \noindent Text\pagebreak[4] Next.\end{document}";

const LAYOUT_HELPER_COMMAND_SOURCE: &str = r"\begin{document}Alpha\xspace Beta.\FloatBarrier\balance\phantomsection\addcontentsline{toc}{section}{Hidden Entry}Visible.\end{document}";

const SIUNITX_COMMAND_SOURCE: &str = r"\begin{document}Speed \SI{3.5}{m/s}; count \num{1200}; unit \si{kg}; range \SIrange{1}{2}{m}; macro \SI{9}{\meter\per\second}; freq \SI{5}{\kilo\hertz}.\end{document}";

const PRINTBIBLIOGRAPHY_SOURCE: &str =
    r"\begin{document}Before \textcite{alpha}.\printbibliography[heading=none]\end{document}";

const LEGACY_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}Before \cite{alpha}.\bibliographystyle{plain}\bibliography{refs}\end{document}";

const LEGACY_BIBLIOGRAPHY_WITH_BBL_SOURCE: &str =
    r"\begin{document}Before \cite{alpha}. \bibliography{refs} After.\end{document}";

const LEGACY_BIBLIOGRAPHY_BBL_SOURCE: &str = r"\begin{thebibliography}{1}\bibitem{alpha}Author. \newblock Title \cite{beta}.\end{thebibliography}";

const NOCITE_SOURCE: &str =
    r"\begin{document}Before \nocite{hidden,other}\nocite{*}\cite{visible}.\end{document}";

const CITEFIELD_SOURCE: &str = r"\begin{document}Fields \citefield{alpha}{doi}, \citefield{beta}{year}, and nested \emph{\citefield{gamma}{journal}}.\end{document}";

const MULTICITE_SOURCE: &str = r"\begin{document}Multi \textcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta} and \parencites{gamma}[note]{delta}, nested \emph{\smartcites{epsilon}[cf.]{zeta}}.\end{document}";

const CITETEXT_SOURCE: &str = r"\begin{document}See \citetext{compare \citealp{beta} with \citeyearpar{alpha}}, nested \emph{\citetext{see \citep{gamma}}}.\end{document}";

const REFERENCE_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \eqref{eq:main}; \cref{fig:a,tab:b}.\end{document}";

const DIRECT_INLINE_ALIAS_SOURCE: &str = r"\let\mycite\cite\let\myref\ref\let\myrange\crefrange\let\myhref\href\let\mylabel\label\begin{document}\section{Intro}\mylabel{sec:intro}See \mycite{key}, \myref{sec:intro}, \myrange{fig:a}{fig:b}, and \myhref{https://hidden.test}{paper link}.\end{document}";

const STARRED_REFERENCE_SOURCE: &str = r"\begin{document}See \ref*{sec:intro}, \autoref*{fig:plot}, \Cref*{tab:data}, \eqref*{eq:main}, and \nameref*{sec:name}.\end{document}";

const REFERENCE_ALIAS_SOURCE: &str = r"\begin{document}See \subref{sub:a}, \vref{sec:intro}, \Vref{chap:main}, \vpageref{page:two}, \fullref{sec:full}, \namecref{thm:one}, and \labelcref{item:x}.\end{document}";

const REFERENCE_PAGE_NAME_ALIAS_SOURCE: &str = r"\begin{document}See \cpageref{page:intro}, \Cpageref{sub:scope}, \autopageref{sec:auto}, \labelcpageref{eq:main}, \Fullref{sec:full}, \titleref{sec:title}, \Titleref{chap:title}, \nameCref{thm:upper}, \lcnamecref{sub:lower}, \namecrefs{thm:a,thm:b}, \nameCrefs{lem:a,lem:b}, and \lcnamecrefs{def:a,def:b}.\end{document}";

const THEOREM_REFERENCE_SOURCE: &str = r"\begin{document}See \thmref{thm:one}, \Thmref{thm:two}, and \subeqref{eq:part}.\end{document}";

const REFERENCE_RANGE_SOURCE: &str = r"\begin{document}See \crefrange{fig:a}{fig:b}, \Crefrange{sec:a}{sec:b}, \cpagerefrange{p:a}{p:b}, and \Cpagerefrange{app:a}{app:b}.\end{document}";

const REFERENCE_RANGE_ALIAS_SOURCE: &str = r"\begin{document}See \pagerefrange{page:a}{page:b}, \vpagerefrange{vp:a}{vp:b}, \vrefrange{sec:a}{sec:b}, and \Vrefrange{chap:a}{chap:b}.\end{document}";

const REFERENCE_RANGE_WRAPPER_SOURCE: &str = r"\newcommand{\myrange}[2]{\crefrange{#1}{#2}}\let\aliasrange\myrange\begin{document}See \myrange{fig:a}{fig:b} and \aliasrange{sec:a}{sec:b}.\end{document}";

const LINK_SOURCE: &str = r"\begin{document}Read \href{https://example.test/paper}{paper link}, \url{https://example.test/raw}, and \url|https://example.test/delimited|.\end{document}";

const LINK_WRAPPER_SOURCE: &str = r"\newcommand{\mylink}[2]{\href{#1}{#2}}\let\paperlink\mylink\begin{document}Read \mylink{https://hidden.test}{paper link} and \paperlink{https://alias.test}{alias link}.\end{document}";

const CONSTANT_TARGET_LINK_WRAPPER_SOURCE: &str = r"\newcommand{\doclink}[1]{\href{https://constant.test}{#1}}\let\aliasdoclink\doclink\newcommand{\doilink}[1]{\href{https://doi.org/#1}{#1}}\newcommand{\defaultdoclink}[1][manual]{\href{https://constant.test}{#1}}\newcommand{\defaultdoilink}[1][10.1000/default]{\href{https://doi.org/#1}{#1}}\begin{document}Read \doclink{docs}, \aliasdoclink{guide}, \doilink{10.1000/foo}, \defaultdoclink, and \defaultdoilink.\end{document}";

const LINK_TEXT_INLINE_KEY_SOURCE: &str = r"\begin{document}Read \href{https://hidden.test}{see \cite{cited}, \citep*{starred}, \ref{sec:intro}, and \ref*{sec:starred}}.\end{document}";

const HYPERREF_VISIBLE_TEXT_SOURCE: &str = r"\begin{document}Read \hyperref[sec:intro]{intro}, \hyperlink{hidden-anchor}{anchor text}, and \hypertarget{target-id}{target text}.\end{document}";

const NOHYPER_SOURCE: &str = r"\begin{document}\begin{NoHyper}Read \href{https://hidden.test}{paper} and \url{https://visible.test/raw} with \cite{key}.\end{NoHyper}\end{document}";

const URL_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Use \nolinkurl{https://example.test/paper}, \nolinkurl|https://example.test/delimited|, at \path{/tmp/archive} and \path|/var/tmp| via \detokenize{\foo+*}.\end{document}";

const TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Styled \emph{important} and \textbf{bold text} with \texttt{code_path}.\end{document}";

const NESTED_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Nested \emph{important \cite{key}, \citep*{starred}, \ref{sec:intro}, \crefrange*{fig:a}{fig:b}, and \ref*{sec:starred}} text.\end{document}";

const NESTED_TEXT_WRAPPER_LINK_SOURCE: &str = r"\begin{document}Nested \emph{read \href{https://hidden.test}{paper} and \url{https://shown.test}}.\end{document}";

const NESTED_TEXT_WRAPPER_LABEL_SOURCE: &str =
    r"\begin{document}Nested \emph{Intro\label{sec:intro} text}.\end{document}";

const NESTED_TEXT_WRAPPER_MATH_SOURCE: &str =
    r"\begin{document}Nested \emph{area $x^2$ and \(y^2\)} text.\end{document}";

const NESTED_TEXT_WRAPPER_WRAPPER_SOURCE: &str =
    r"\begin{document}Nested \emph{outer \textbf{inner text} done}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_SOURCE: &str =
    r"\begin{document}Nested \emph{before \unknowntext{visible text} after}.\end{document}";

const DECLARED_TOP_LEVEL_WRAPPER_SOURCE: &str = r"\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}\begin{document}A \reviewnote{check \cite{key}, \ref{sec:intro}, and \href{https://hidden.test}{paper}} B.\end{document}";

const DEF_DECLARED_TOP_LEVEL_WRAPPER_SOURCE: &str = r"\def\reviewnote#1{{\color{red}[TODO: #1]}}\begin{document}A \reviewnote{check \cite{key}, \ref{sec:intro}, and \href{https://hidden.test}{paper}} B.\end{document}";

const COLOR_DECORATION_SOURCE: &str = r"\begin{document}A \color{magenta}colored word and \textcolor{cyan}{visible \cite{key}} plus \colorbox{yellow}{boxed \ref{sec:intro}} and \fcolorbox{black}{white}{framed \href{https://hidden.test}{paper}}.\end{document}";

const INPUT_MAIN_SOURCE: &str = r"\begin{document}Before. \input{child} After.\end{document}";

const INPUT_CHILD_SOURCE: &str = r"\section{Included}See \cite{key} and \ref{sec:intro}.";

const UNBRACED_INPUT_MAIN_SOURCE: &str =
    r"\begin{document}Before. \input child Middle. \include second After.\end{document}";

const UNBRACED_INPUT_CHILD_SOURCE: &str = r"\section{Unbraced Input}See \cite{key}.";

const UNBRACED_INCLUDE_CHILD_SOURCE: &str = "Second body.";

const AT_INPUT_MAIN_SOURCE: &str =
    r"\makeatletter\begin{document}Before. \@input{child} After.\end{document}\makeatother";

const AT_INPUT_CHILD_SOURCE: &str =
    r"\section{At Input}Included \cite{key}.\endinput Hidden \cite{hidden}.";

const INPUT_SHARED_STATE_MAIN_SOURCE: &str = r"\newcommand{\mysection}[1]{\section{#1}}\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}\begin{document}\input{child}\end{document}";

const INPUT_SHARED_STATE_CHILD_SOURCE: &str = r"\mysection{Included}\reviewnote{check \cite{key}}";

const PREAMBLE_INPUT_MACRO_MAIN_SOURCE: &str = r"\input{macros}\begin{document}\mysection{From Preamble}\reviewnote{check \cite{key}}\end{document}";

const PREAMBLE_INPUT_MACRO_SOURCE: &str =
    r"\newcommand{\mysection}[1]{\section{#1}}\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}";

const PACKAGE_MACRO_MAIN_SOURCE: &str = r"\usepackage{macros}\begin{document}\mysection{From Package}\reviewnote{package \cite{key}}\end{document}";

const PACKAGE_MACRO_SOURCE: &str = r"\ProvidesPackage{macros}\newcommand{\mysection}[1]{\section{#1}}\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}";

const CLASS_MACRO_MAIN_SOURCE: &str = r"\documentclass{wrapper}\begin{document}\mysection{From Class}\reviewnote{class \cite{key}}\end{document}";

const CLASS_MACRO_SOURCE: &str = r"\ProvidesClass{wrapper}\newcommand{\mysection}[1]{\section{#1}}\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}";

const MISSING_PACKAGE_CLASS_MAIN_SOURCE: &str =
    r"\documentclass{ghost}\usepackage{missing}\begin{document}Visible.\end{document}";

const CONDITIONAL_FILE_INPUT_MAIN_SOURCE: &str = r"\input{sections/setup}\begin{document}\mysection{From Config}\IfFileExists{sections/config.cfg}{\reviewnote{found \cite{key}}}{missing}\InputIfFileExists{body.tex}{ after}{missing}\IfFileExists{ghost.cfg}{ghost}{fallback}\end{document}";

const CONDITIONAL_FILE_INPUT_SETUP_SOURCE: &str = r"\InputIfFileExists{config.cfg}{}{}";

const CONDITIONAL_FILE_INPUT_CONFIG_SOURCE: &str =
    r"\newcommand{\mysection}[1]{\section{#1}}\newcommand{\reviewnote}[1]{{\color{red}[TODO: #1]}}";

const CONDITIONAL_FILE_INPUT_BODY_SOURCE: &str = r"Body \cite{body}.";

const INCLUDEONLY_MAIN_SOURCE: &str =
    r"\includeonly{first}\begin{document}A \include{first} B \include{second} C\end{document}";

const INCLUDEONLY_FIRST_SOURCE: &str = "First body.";

const INCLUDEONLY_SECOND_SOURCE: &str = "Skipped body.";

const MISSING_INPUT_MAIN_SOURCE: &str =
    r"\begin{document}Before \input{missing} Middle \include missing-two After\end{document}";

const CYCLIC_INPUT_MAIN_SOURCE: &str =
    r"\begin{document}Root start. \input{child} Root end.\end{document}";

const CYCLIC_INPUT_CHILD_SOURCE: &str = r"Child start. \input{child} Child end.";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_INLINE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{see \cite{key}, \citep*{starred}, \ref{sec:intro}, \crefrange*{fig:a}{fig:b}, and \ref*{sec:starred}} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_LINK_MATH_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{see \href{https://hidden.test}{paper}, \url{https://shown.test}, and $x^2$} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_ESCAPED_VISIBLE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{50\% A\&B costs \$5\_0 \#1 \{x\}} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \textbf{inner text} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{inner text} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_INLINE_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{see \cite{key}, \citep*{starred}, \ref{sec:intro}, \crefrange*{fig:a}{fig:b}, and \ref*{sec:starred}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LINK_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{see \href{https://hidden.test}{paper} and \url{https://shown.test}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_URL_TEXT_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{use \nolinkurl{https://visible.test/path}, \path|/tmp/archive|, and \detokenize{\foo+*}} done} after}.\end{document}";

const NESTED_TEXT_WRAPPER_UNKNOWN_COMMAND_NESTED_UNKNOWN_LABEL_SOURCE: &str = r"\begin{document}Nested \emph{before \unknowntext{outer \innerunknown{Intro\label{sec:intro} text} done} after}.\end{document}";

const ESCAPED_VISIBLE_SOURCE: &str =
    r"\begin{document}50\% A\&B costs \$5\_0 \#1 \{x\} A\ B.\end{document}";

const NONBREAKING_TILDE_SOURCE: &str =
    r"\begin{document}Figure~1 references Related~Work.\section{Related~Work}\end{document}";

const LINEBREAK_SOURCE: &str = r"\begin{document}First line\\Second line.\end{document}";
const LINEBREAK_OPTIONAL_SOURCE: &str =
    r"\begin{document}First line\\[0.5em]Second line.\end{document}";

const TABULAR_FALLBACK_SOURCE: &str = r"\begin{document}\begin{tabular}{ll}Alpha & Beta \\ Gamma & \textbf{Delta} \\\hline\end{tabular}\end{document}";

const TABLE_FLOAT_BODY_SOURCE: &str = r"\def\caption#1{#1}\begin{document}\begin{table}\caption{Data table.}\begin{tabular}{ll}Alpha & Beta \\ Gamma & Delta\end{tabular}\end{table}\end{document}";

const LIST_SOURCE: &str = r"\begin{document}\begin{itemize}\item First \cite{key}\item[Custom] Second\end{itemize}\begin{enumerate}\item One\item Two\end{enumerate}\begin{description}\item[Term] Meaning \cite{key}\item[Other] More\end{description}\end{document}";

const SIMPLE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{quote}Quoted \cite{key}.\end{quote}\begin{center}Centered text.\end{center}\begin{theorem}Theorem text.\end{theorem}\begin{proof}Proof text.\end{proof}\end{document}";

const VERSE_ENVIRONMENT_SOURCE: &str =
    r"\begin{document}\begin{verse}Line \cite{key} text.\end{verse}\end{document}";

const ACKNOWLEDGEMENTS_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{acknowledgements}Thanks \cite{grant}.\end{acknowledgements}\begin{acknowledgments}US spelling.\end{acknowledgments}\begin{acknowledgement}Singular.\end{acknowledgement}\begin{acknowledgment}Singular US.\end{acknowledgment}\end{document}";

const KEYWORDS_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{keywords}vision; \cite{key}\end{keywords}\begin{keyword}single keyword\end{keyword}\begin{IEEEkeywords}systems, latex\end{IEEEkeywords}\end{document}";

const FRONTMATTER_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{frontmatter}\title{Wrapped Paper}\author{Ada}\begin{abstract}Wrapped abstract \cite{key}.\end{abstract}\end{frontmatter}\end{document}";

const WIDE_TEXT_WRAPPER_SOURCE: &str = r"\begin{document}\begin{widetext}Wide \cite{key} text.\end{widetext}\begin{strip}Strip text.\end{strip}\end{document}";

const FULLWIDTH_WRAPPER_SOURCE: &str =
    r"\begin{document}\begin{fullwidth}Full \cite{key} text.\end{fullwidth}\end{document}";

const LANDSCAPE_WRAPPER_SOURCE: &str =
    r"\begin{document}\begin{landscape}Rotated \cite{key} text.\end{landscape}\end{document}";

const CJK_WRAPPER_SOURCE: &str = r"\begin{document}\begin{CJK}{UTF8}{gbsn}CJK \cite{key} text.\end{CJK}\begin{CJK*}{UTF8}{bsmi}Star text.\end{CJK*}\end{document}";

const SLOPPYPAR_WRAPPER_SOURCE: &str =
    r"\begin{document}\begin{sloppypar}Loose \cite{key} text.\end{sloppypar}\end{document}";

const SIZE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{small}Small \cite{key} text.\end{small}\begin{footnotesize}Foot text.\end{footnotesize}\begin{Large}Large text.\end{Large}\end{document}";

const FLUSH_ALIGNMENT_SOURCE: &str = r"\begin{document}\begin{flushleft}Left \cite{key} text.\end{flushleft}\begin{flushright}Right text.\end{flushright}\end{document}";

const SAMEPAGE_WRAPPER_SOURCE: &str =
    r"\begin{document}\begin{samepage}Together \cite{key} text.\end{samepage}\end{document}";

const TITLEPAGE_WRAPPER_SOURCE: &str =
    r"\begin{document}\begin{titlepage}Title \cite{key} text.\end{titlepage}\end{document}";

const BOXED_WRAPPER_SOURCE: &str = r"\begin{document}\begin{framed}Frame \cite{key} text.\end{framed}\begin{shaded}Shade text.\end{shaded}\begin{tcolorbox}[colback=yellow]Color text.\end{tcolorbox}\begin{mdframed}[linecolor=red]Border text.\end{mdframed}\end{document}";

const CSQUOTES_DISPLAY_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{displayquote}[Hidden Source]Quoted \cite{key} text.\end{displayquote}\begin{displayquotation}[Hidden Source][Hidden Punct]Long quote text.\end{displayquotation}\end{document}";

const COMMENT_ENVIRONMENT_SOURCE: &str = r"\begin{document}Before.\begin{comment}Hidden \cite{key} text.\end{comment} After.\end{document}";

const CUSTOM_COMMENT_ENVIRONMENT_SOURCE: &str = r"\excludecomment{draftnote}\includecomment{keptnote}\begin{document}Before.\begin{draftnote}Hidden \cite{key} text.\end{draftnote}\begin{keptnote}Kept \cite{shown} text.\end{keptnote} After.\end{document}";

const SPACING_WRAPPER_SOURCE: &str = r"\begin{document}\begin{spacing}{1.5}Spaced \cite{key} text.\end{spacing}\begin{onehalfspace}Half text.\end{onehalfspace}\begin{doublespace}Double text.\end{doublespace}\begin{singlespace}Single text.\end{singlespace}\end{document}";

const ADJUSTWIDTH_WRAPPER_SOURCE: &str = r"\begin{document}\begin{adjustwidth}{1cm}{2cm}Margin \cite{key} text.\end{adjustwidth}\begin{adjustwidth*}{-1em}{0pt}Star text.\end{adjustwidth*}\end{document}";

const ADDMARGIN_WRAPPER_SOURCE: &str = r"\begin{document}\begin{addmargin}[1em]{2em}Inset \cite{key} text.\end{addmargin}\begin{addmargin*}{3em}Star text.\end{addmargin*}\end{document}";

const APPENDICES_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{appendices}\section{Extra}Appendix \ref{sec:intro} text.\end{appendices}\begin{subappendices}\subsection{More}More text.\end{subappendices}\end{document}";

const MINIPAGE_ENVIRONMENT_SOURCE: &str =
    r"\begin{document}\begin{minipage}[t]{0.5\textwidth}Box text.\end{minipage}\end{document}";

const MULTICOLS_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{multicols}{2}Column \cite{key} text.\end{multicols}\begin{multicols*}{3}Wide text.\end{multicols*}\end{document}";

const PARACOL_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{paracol}{2}Column \cite{key} text.\end{paracol}\begin{paracol*}{3}Wide text.\end{paracol*}\end{document}";

const THREEPARTTABLE_SOURCE: &str = r"\begin{document}\begin{threeparttable}\caption{Measured table.}\begin{tabular}{ll}A & B \\\end{tabular}\begin{tablenotes}[flushleft]\item Note \cite{key}.\end{tablenotes}\end{threeparttable}\end{document}";

const SUBCAPTION_WRAPPER_SOURCE: &str = r"\begin{document}\begin{subfigure}[b]{0.45\textwidth}\includegraphics[width=4cm]{figures/panel-a.pdf}\caption{Panel \cite{key}.}\end{subfigure}\begin{subtable}{0.4\textwidth}\caption{Panel table.}\end{subtable}\end{document}";

const SUBFLOAT_COMMAND_SOURCE: &str = r"\begin{document}\begin{figure}\subfloat[Panel \cite{key}.]{\includegraphics[width=3cm]{figures/a.pdf}}\subcaptionbox{Box \cite{key}.}[0.4\textwidth]{\includegraphics[width=2cm]{figures/b.pdf}}\end{figure}\end{document}";

const ALGORITHM_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{algorithm}\caption{Procedure.}\label{alg:first}Step text.\end{algorithm}\begin{algorithm*}Wide step.\end{algorithm*}\end{document}";

const ALGORITHMIC_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{algorithmic}\State Step one.\end{algorithmic}\begin{algorithmic*}Wide step.\end{algorithmic*}\end{document}";

const SUBEQUATIONS_WRAPPER_SOURCE: &str = r"\begin{document}\begin{subequations}\label{eq:group}\begin{align}x&=y\end{align}\end{subequations}\end{document}";

const THEOREM_LIKE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{lemma}Lemma text.\end{lemma}\begin{proposition}Proposition text.\end{proposition}\begin{corollary}Corollary text.\end{corollary}\begin{definition}Definition text.\end{definition}\begin{remark}Remark text.\end{remark}\begin{example}Example text.\end{example}\end{document}";

const THEOREM_ENVIRONMENT_TITLE_SOURCE: &str = r"\begin{document}\begin{theorem}[Sharp bound]Body.\end{theorem}\begin{proof}[Sketch]Done.\end{proof}\end{document}";

const NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE: &str = r"\newtheorem{claim}{Claim}\begin{document}\begin{claim}[Named claim]Claim body.\end{claim}\end{document}";

const STARRED_NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE: &str = r"\newtheorem*{namedclaim}{Named Claim}\begin{document}\begin{namedclaim}Starred claim body.\end{namedclaim}\end{document}";

const AUX_RESOLUTION_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \cite{key}.\end{document}";

const AUX_CITATION_RANGE_SOURCE: &str =
    r"\begin{document}See \cite{alpha,beta,gamma,delta}.\end{document}";

const AUX_NATEXLAB_SOURCE: &str = r"\begin{document}See \cite{alpha,beta}.\end{document}";

const LABEL_SOURCE: &str =
    r"\begin{document}\section{Intro}\label{sec:intro}See \ref{sec:intro}.\end{document}";

const LABEL_WRAPPER_SOURCE: &str = r"\newcommand{\seclabel}[1]{\label{#1}}\let\aliaslabel\seclabel\begin{document}\section{Intro}\seclabel{sec:intro}See \ref{sec:intro} and \ref{sec:alias}.\aliaslabel{sec:alias}\end{document}";

const TEMPLATED_REFERENCE_LABEL_WRAPPER_SOURCE: &str = r"\newcommand{\seclabel}[1]{\label{sec:#1}}\newcommand{\secref}[1]{\ref{sec:#1}}\begin{document}\section{Intro}\seclabel{intro}See \secref{intro}.\end{document}";

const CONSTANT_REFERENCE_LABEL_WRAPPER_SOURCE: &str = r"\newcommand{\introlabel}{\label{sec:intro}}\newcommand{\introref}{\ref{sec:intro}}\let\aliasintroref\introref\newcommand{\defaultlabel}[1][sec:intro]{\label{#1}}\newcommand{\defaultref}[1][sec:intro]{\ref{#1}}\let\aliasdefaultref\defaultref\begin{document}\section{Intro}\introlabel\defaultlabel See \introref, \aliasintroref, \defaultref, and \aliasdefaultref.\end{document}";

const MACRO_SECTION_SOURCE: &str =
    r"\newcommand{\mysection}[1]{\section{#1}}\begin{document}\mysection{Intro}\end{document}";

const STARRED_PROVIDED_MACRO_SECTION_SOURCE: &str = r"\providecommand*{\mysection}[1]{\section{#1}}\begin{document}\mysection{Provided}\end{document}";

const OPTIONAL_MACRO_SECTION_SOURCE: &str = r"\newcommand{\mysection}[2][]{\section[#1]{#2}}\begin{document}\mysection[Short]{Long Title}\end{document}";

const DEF_MACRO_SECTION_SOURCE: &str =
    r"\def\mysection#1{\section{#1}}\begin{document}\mysection{Plain Def}\end{document}";

const LET_SECTION_ALIAS_SOURCE: &str =
    r"\let\mysection\section\begin{document}\mysection{Alias Title}\end{document}";

fn assert_macro_heading_provenance_golden(
    capture: &InternalRenderIrCapture,
    heading_text: &str,
    primary_path: &str,
    primary_source: &str,
    invocation_text: &str,
    definition_path: &str,
    definition_source: &str,
    definition_text: &str,
    snapshot_sources: &[(&str, &str)],
    golden_path: &str,
) {
    let heading_event = capture
        .events
        .events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RenderEvent::Heading(heading) if heading.text == heading_text
            )
        })
        .expect("heading event");
    let heading_block = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::Heading(heading)
                if matches!(
                    heading.content.first(),
                    Some(InlineNode::Text { text, .. }) if text == heading_text
                ) =>
            {
                Some(heading)
            }
            _ => None,
        })
        .expect("heading block");
    let heading_text_source = heading_block
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Text { text, source } if text == heading_text => Some(source),
            _ => None,
        })
        .expect("heading text source");
    let heading_run = capture.page_display_lists[0]
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == heading_text => Some(run),
            _ => None,
        })
        .expect("heading display-list run");

    for source in [
        &heading_event.meta.source,
        &heading_block.source,
        heading_text_source,
        &heading_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == primary_path
                    && &primary_source[span.start_utf8 as usize..span.end_utf8 as usize]
                        == heading_text
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == primary_path
                            && &primary_source[span.start_utf8 as usize..span.end_utf8 as usize]
                                == invocation_text
                )
        }));

        assert_eq!(source.expansion_stack.len(), 1);
        let expansion = &source.expansion_stack[0];
        assert_eq!(expansion.command_name.as_deref(), Some("mysection"));
        assert!(matches!(
            &expansion.call_span,
            ProvenanceSpan::File(span)
                if span.path.as_str() == primary_path
                    && &primary_source[span.start_utf8 as usize..span.end_utf8 as usize]
                        == invocation_text
        ));
        assert!(
            matches!(
                &expansion.definition_span,
                Some(ProvenanceSpan::File(span))
                    if span.path.as_str() == definition_path
                        && &definition_source[span.start_utf8 as usize..span.end_utf8 as usize]
                            == definition_text
            ),
            "unexpected definition span: {:?}",
            expansion.definition_span
        );
    }

    let mut provenance_snapshot = serde_json::Map::new();
    for (key, source) in snapshot_sources {
        provenance_snapshot.insert(
            (*key).to_string(),
            serde_json::Value::String((*source).into()),
        );
    }
    provenance_snapshot.insert(
        "event".to_string(),
        serde_json::json!({
            "event": heading_event.event,
            "meta": heading_event.meta,
        }),
    );
    provenance_snapshot.insert(
        "ir".to_string(),
        serde_json::json!({
            "level": heading_block.level,
            "content": heading_block.content,
            "source": heading_block.source,
            "text_source": heading_text_source,
        }),
    );
    provenance_snapshot.insert(
        "display_list".to_string(),
        serde_json::json!({
            "text": heading_run.text,
            "source": heading_run.source,
            "clusters": heading_run.clusters,
        }),
    );
    let provenance_json =
        to_pretty_json(&serde_json::Value::Object(provenance_snapshot)).expect("provenance json");

    assert_or_update_golden(golden_path, &provenance_json);
}

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
