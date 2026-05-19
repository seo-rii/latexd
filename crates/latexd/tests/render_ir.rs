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
                == "alpha,beta"
    ));
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

const VERBATIM_FALLBACK_SOURCE: &str =
    r"\begin{document}\begin{verbatim}\alpha_{i} {raw}\end{verbatim}\end{document}";

const CODE_LISTING_FALLBACK_SOURCE: &str = r#"\begin{document}\begin{lstlisting}[language=Rust]fn main() { println!("hi"); }\end{lstlisting}\begin{minted}{rust}let value = \alpha_{i} + {raw};\end{minted}\end{document}"#;

const FANCYVRB_FALLBACK_SOURCE: &str = r"\begin{document}\begin{Verbatim}[fontsize=\small]\foo_{bar} {baz}\end{Verbatim}\end{document}";

const GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics[width=5cm]{figures/plot.pdf}\caption{Plot caption.}\end{figure}\end{document}";

const STARRED_GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure}\includegraphics*[width=3cm]{figures/starred.pdf}\caption{Starred plot.}\end{figure}\end{document}";

const STARRED_FLOAT_GRAPHIC_SOURCE: &str = r"\def\includegraphics[#1]#2{[image]}\def\caption#1{#1}\begin{document}\begin{figure*}\includegraphics[width=6cm]{figures/wide.pdf}\caption{Wide figure.}\end{figure*}\end{document}";

const STARRED_TABLE_SOURCE: &str = r"\def\caption#1{#1}\begin{document}\begin{table*}\caption{Wide table.}\end{table*}\end{document}";

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

const PRINTBIBLIOGRAPHY_SOURCE: &str =
    r"\begin{document}Before \textcite{alpha}.\printbibliography[heading=none]\end{document}";

const LEGACY_BIBLIOGRAPHY_SOURCE: &str = r"\begin{document}Before \cite{alpha}.\bibliographystyle{plain}\bibliography{refs}\end{document}";

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

const TABULAR_FALLBACK_SOURCE: &str = r"\begin{document}\begin{tabular}{ll}Alpha & Beta \\ Gamma & \textbf{Delta} \\\hline\end{tabular}\end{document}";

const LIST_SOURCE: &str = r"\begin{document}\begin{itemize}\item First \cite{key}\item[Custom] Second\end{itemize}\begin{enumerate}\item One\item Two\end{enumerate}\begin{description}\item[Term] Meaning \cite{key}\item[Other] More\end{description}\end{document}";

const SIMPLE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{quote}Quoted \cite{key}.\end{quote}\begin{center}Centered text.\end{center}\begin{theorem}Theorem text.\end{theorem}\begin{proof}Proof text.\end{proof}\end{document}";

const ALGORITHM_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{algorithm}\caption{Procedure.}\label{alg:first}Step text.\end{algorithm}\begin{algorithm*}Wide step.\end{algorithm*}\end{document}";

const ALGORITHMIC_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{algorithmic}\State Step one.\end{algorithmic}\begin{algorithmic*}Wide step.\end{algorithmic*}\end{document}";

const THEOREM_LIKE_ENVIRONMENT_SOURCE: &str = r"\begin{document}\begin{lemma}Lemma text.\end{lemma}\begin{proposition}Proposition text.\end{proposition}\begin{corollary}Corollary text.\end{corollary}\begin{definition}Definition text.\end{definition}\begin{remark}Remark text.\end{remark}\begin{example}Example text.\end{example}\end{document}";

const THEOREM_ENVIRONMENT_TITLE_SOURCE: &str = r"\begin{document}\begin{theorem}[Sharp bound]Body.\end{theorem}\begin{proof}[Sketch]Done.\end{proof}\end{document}";

const NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE: &str = r"\newtheorem{claim}{Claim}\begin{document}\begin{claim}[Named claim]Claim body.\end{claim}\end{document}";

const STARRED_NEWTHEOREM_DEFINED_ENVIRONMENT_SOURCE: &str = r"\newtheorem*{namedclaim}{Named Claim}\begin{document}\begin{namedclaim}Starred claim body.\end{namedclaim}\end{document}";

const AUX_RESOLUTION_SOURCE: &str =
    r"\begin{document}See \ref{sec:intro} and \cite{key}.\end{document}";

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
