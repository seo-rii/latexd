use std::{env, fs, path::Path, process::Command};

use camino::Utf8PathBuf;
use latexd::compiler::{capture_internal_render_ir, capture_internal_render_ir_with_mounted_files};
use tex_aux::{BibliographyEntry, SemanticAux, SemanticLabel};
use tex_render_model::{
    CitationStyleHint, DrawOp, GraphicAssetFormat, ListKind, MetadataField, RenderEvent,
    to_pretty_json, to_semantic_pretty_json,
};
use tex_render_model::{InlineNode, IrBlock, ProvenanceSpan, SourceSpanRole};

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
        &[("figures/vector.svg", "<svg/>")],
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
    assert_eq!(graphic_block.asset_format, Some(GraphicAssetFormat::Svg));
    assert_eq!(image_op.asset_format, Some(GraphicAssetFormat::Svg));
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
    assert!(display_list_text.contains(r"\frac{a}{b}"));
    assert!(display_list_text.contains("a&=b"));
    assert!(display_list_text.contains("x&=y"));
    assert!(display_list_text.contains("u&=&v"));
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
    let capture = capture_internal_render_ir(
        "main.tex",
        DECLARED_TOP_LEVEL_WRAPPER_SOURCE,
        &SemanticAux::default(),
    );

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
    let heading_text_source = heading_block
        .content
        .iter()
        .find_map(|node| match node {
            InlineNode::Text { text, source } if text == "Included" => Some(source),
            _ => None,
        })
        .expect("heading text source");
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
        heading_text_source,
        &heading_run.source,
    ] {
        assert!(matches!(
            &source.primary,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "child.tex"
                    && &INPUT_SHARED_STATE_CHILD_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == "Included"
        ));
        assert!(source.related.iter().any(|related| {
            related.role == SourceSpanRole::Invocation
                && matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path.as_str() == "child.tex"
                            && &INPUT_SHARED_STATE_CHILD_SOURCE
                                [span.start_utf8 as usize..span.end_utf8 as usize]
                                == r"\mysection{Included}"
                )
        }));

        assert_eq!(source.expansion_stack.len(), 1);
        let expansion = &source.expansion_stack[0];
        assert_eq!(expansion.command_name.as_deref(), Some("mysection"));
        assert!(matches!(
            &expansion.call_span,
            ProvenanceSpan::File(span)
                if span.path.as_str() == "child.tex"
                    && &INPUT_SHARED_STATE_CHILD_SOURCE
                        [span.start_utf8 as usize..span.end_utf8 as usize]
                        == r"\mysection{Included}"
        ));
        assert!(
            matches!(
                &expansion.definition_span,
                Some(ProvenanceSpan::File(span))
                    if span.path.as_str() == "main.tex"
                        && &INPUT_SHARED_STATE_MAIN_SOURCE
                            [span.start_utf8 as usize..span.end_utf8 as usize]
                            == r"\newcommand{\mysection}[1]{\section{#1}}"
            ),
            "unexpected definition span: {:?}",
            expansion.definition_span
        );
    }

    let provenance_snapshot = serde_json::json!({
        "main_source": INPUT_SHARED_STATE_MAIN_SOURCE,
        "child_source": INPUT_SHARED_STATE_CHILD_SOURCE,
        "event": {
            "event": heading_event.event,
            "meta": heading_event.meta,
        },
        "ir": {
            "level": heading_block.level,
            "content": heading_block.content,
            "source": heading_block.source,
            "text_source": heading_text_source,
        },
        "display_list": {
            "text": heading_run.text,
            "source": heading_run.source,
            "clusters": heading_run.clusters,
        },
    });
    let provenance_json = to_pretty_json(&provenance_snapshot).expect("provenance json");

    assert_or_update_golden(
        "tests/goldens/render_ir/input-macro-heading.provenance.json",
        &provenance_json,
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
fn missing_package_and_class_files_emit_render_diagnostics_without_visible_leakage() {
    let capture = capture_internal_render_ir(
        "main.tex",
        MISSING_PACKAGE_CLASS_MAIN_SOURCE,
        &SemanticAux::default(),
    );

    for missing in ["missing class ghost.cls", "missing package missing.sty"] {
        assert!(capture.events.events.iter().any(|event| matches!(
            &event.event,
            RenderEvent::Diagnostic(diagnostic) if diagnostic.message.contains(missing)
        )));
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
fn longtable_fallback_capture_uses_normalized_visible_text() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LONGTABLE_FALLBACK_SOURCE,
        &SemanticAux::default(),
    );
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("longtable") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("longtable fallback");

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
fn longtable_fallback_labels_survive_without_visible_key() {
    let capture = capture_internal_render_ir(
        "main.tex",
        LONGTABLE_FALLBACK_LABEL_SOURCE,
        &SemanticAux::default(),
    );
    let fallback = capture
        .document_ir
        .blocks
        .iter()
        .find_map(|block| match block {
            IrBlock::RawFallback(fallback)
                if fallback.environment.as_deref() == Some("longtable") =>
            {
                Some(fallback)
            }
            _ => None,
        })
        .expect("longtable fallback");

    let visible = fallback
        .normalized_visible_text
        .as_deref()
        .expect("visible fallback text");
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
        .join("");
    assert!(display_list_text.contains("Long table."));
    assert!(display_list_text.contains("Alpha | Beta"));
    assert!(!display_list_text.contains("tab:long"));
    assert!(!display_list_text.contains("label"));
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
            IrBlock::DisplayMath(display) if display.raw_source == "x&=y"
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
    assert!(extracted_text.contains("x&=y"));
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
    assert!(display_list_text.contains("x&=y"));
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

const GRAPHIC_LAYOUT_BOX_WRAPPER_SOURCE: &str = r"\begin{document}\resizebox{0.8\textwidth}{0.4\textheight}{\includegraphics[width=5cm]{figures/plot}}\scalebox{0.5}{\epsfbox{figures/other}}\rotatebox[origin=c]{90}{\psfig{figure=figures/third.eps,width=2cm}}\end{document}";

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

const STARRED_REFERENCE_SOURCE: &str = r"\begin{document}See \ref*{sec:intro}, \autoref*{fig:plot}, \Cref*{tab:data}, \eqref*{eq:main}, and \nameref*{sec:name}.\end{document}";

const REFERENCE_ALIAS_SOURCE: &str = r"\begin{document}See \subref{sub:a}, \vref{sec:intro}, \Vref{chap:main}, \vpageref{page:two}, \fullref{sec:full}, \namecref{thm:one}, and \labelcref{item:x}.\end{document}";

const REFERENCE_PAGE_NAME_ALIAS_SOURCE: &str = r"\begin{document}See \cpageref{page:intro}, \Cpageref{sub:scope}, \autopageref{sec:auto}, \labelcpageref{eq:main}, \Fullref{sec:full}, \titleref{sec:title}, \Titleref{chap:title}, \nameCref{thm:upper}, \lcnamecref{sub:lower}, \namecrefs{thm:a,thm:b}, \nameCrefs{lem:a,lem:b}, and \lcnamecrefs{def:a,def:b}.\end{document}";

const THEOREM_REFERENCE_SOURCE: &str = r"\begin{document}See \thmref{thm:one}, \Thmref{thm:two}, and \subeqref{eq:part}.\end{document}";

const REFERENCE_RANGE_SOURCE: &str = r"\begin{document}See \crefrange{fig:a}{fig:b}, \Crefrange{sec:a}{sec:b}, \cpagerefrange{p:a}{p:b}, and \Cpagerefrange{app:a}{app:b}.\end{document}";

const REFERENCE_RANGE_ALIAS_SOURCE: &str = r"\begin{document}See \pagerefrange{page:a}{page:b}, \vpagerefrange{vp:a}{vp:b}, \vrefrange{sec:a}{sec:b}, and \Vrefrange{chap:a}{chap:b}.\end{document}";

const LINK_SOURCE: &str = r"\begin{document}Read \href{https://example.test/paper}{paper link}, \url{https://example.test/raw}, and \url|https://example.test/delimited|.\end{document}";

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
