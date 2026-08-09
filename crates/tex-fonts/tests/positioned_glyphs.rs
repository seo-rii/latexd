use tex_fonts::{
    BUNDLED_TEX_FONT_FACES, MAX_BROWSER_OUTLINE_COMMANDS_PER_GLYPH, TexFontFace,
    browser_font_assets_for_pages, outline_glyph, shape_text,
};
use tex_render_model::{
    DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, GlyphIdKind,
    GlyphOutlineCommand, PageDisplayList, Point, PositionedTextRun, SourceProvenance,
};

#[test]
fn computer_modern_shape_preserves_face_slots_positions_and_clusters() {
    let shaped = shape_text(TexFontFace::Roman10, "AV", 10.0)
        .expect("shape text with hermetic bundled cmr10");

    assert_eq!(shaped.glyphs.len(), 2);
    assert_eq!(shaped.resolved_font.face_id.as_str(), "cmr10");
    assert_eq!(shaped.resolved_font.postscript_name, "CMR10");
    assert_eq!(
        shaped.resolved_font.glyph_id_kind,
        GlyphIdKind::Type1CharCode
    );
    assert_eq!(shaped.glyphs[0].glyph_id, u32::from(b'A'));
    assert_eq!(shaped.glyphs[1].glyph_id, u32::from(b'V'));
    assert_eq!(shaped.glyphs[0].offset.x, 0.0);
    assert!(shaped.glyphs[1].offset.x > 0.0);
    assert_eq!(shaped.glyphs[0].offset.y, 0.0);
    assert_eq!(shaped.clusters.len(), 2);
    assert_eq!(shaped.clusters[0].text_start_utf8, 0);
    assert_eq!(shaped.clusters[0].text_end_utf8, 1);
    assert_eq!(shaped.clusters[0].glyph_start, 0);
    assert_eq!(shaped.clusters[0].glyph_end, 1);
    assert_eq!(shaped.clusters[1].text_start_utf8, 1);
    assert_eq!(shaped.clusters[1].text_end_utf8, 2);
    assert_eq!(shaped.clusters[1].glyph_start, 1);
    assert_eq!(shaped.clusters[1].glyph_end, 2);

    let total_advance: f32 = shaped.glyphs.iter().map(|glyph| glyph.advance_pt).sum();
    assert!((total_advance - shaped.advance_pt).abs() < 0.000_1);
}

#[test]
fn bundled_type1_glyphs_expose_renderer_neutral_outline_commands() {
    let outline =
        outline_glyph(TexFontFace::Roman10, u32::from(b'A')).expect("bundled cmr10 A outline");

    assert_eq!(outline.glyph_id, u32::from(b'A'));
    assert!(matches!(
        outline.commands.first(),
        Some(GlyphOutlineCommand::MoveTo { .. })
    ));
    assert!(
        outline
            .commands
            .iter()
            .any(|command| matches!(command, GlyphOutlineCommand::CurveTo { .. }))
    );
    assert!(
        outline
            .commands
            .iter()
            .any(|command| matches!(command, GlyphOutlineCommand::Close))
    );
    assert!(outline.commands.iter().all(GlyphOutlineCommand::is_finite));

    let other =
        outline_glyph(TexFontFace::Roman10, u32::from(b'V')).expect("bundled cmr10 V outline");
    assert_ne!(outline.commands, other.commands);
    assert!(
        !outline_glyph(TexFontFace::Roman10, u32::from(b' '))
            .expect("raw bundled cmr10 slot 32 outline")
            .commands
            .is_empty()
    );
    assert!(outline_glyph(TexFontFace::Roman10, 256).is_none());
    assert!(outline_glyph(TexFontFace::TimesRoman, u32::from(b'A')).is_none());
}

#[test]
fn browser_font_artifacts_include_each_used_glyph_once() {
    let shaped =
        shape_text(TexFontFace::Roman10, "A VA", 10.0).expect("shape text with bundled cmr10");
    let page = PageDisplayList {
        page_id: "page-a".to_string(),
        width_pt: 612.0,
        height_pt: 792.0,
        ops: vec![DrawOp::TextRun(PositionedTextRun {
            origin: Point { x: 10.0, y: 20.0 },
            text: "A VA".to_string(),
            font: FontRequest {
                family: FontFamilyRequest::Serif,
                series: FontSeries::Regular,
                shape: FontShape::Upright,
                size_pt: 10.0,
                role: FontRole::Body,
            },
            size_pt: 10.0,
            approximate_advance_pt: shaped.advance_pt,
            resolved_font: Some(shaped.resolved_font.clone()),
            glyphs: Some(shaped.glyphs),
            clusters: Some(shaped.clusters),
            source: SourceProvenance::generated("run-a", "test run"),
        })],
        source_spans: Vec::new(),
        content_hash: "page-hash".to_string(),
    };

    let fonts = browser_font_assets_for_pages(&[page]);
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].face_id.as_str(), "cmr10");
    assert_eq!(fonts[0].content_hash, shaped.resolved_font.content_hash);
    assert_eq!(
        fonts[0]
            .glyphs
            .iter()
            .map(|glyph| glyph.glyph_id)
            .collect::<Vec<_>>(),
        [u32::from(b' '), u32::from(b'A'), u32::from(b'V')]
    );
    assert!(fonts[0].glyphs[0].commands.is_empty());
}

#[test]
fn every_bundled_type1_face_has_deterministic_bounded_outlines() {
    for face in BUNDLED_TEX_FONT_FACES {
        let mut available = 0usize;
        let mut max_commands = 0usize;
        for glyph_id in 0..=u8::MAX {
            let first = outline_glyph(face, u32::from(glyph_id));
            let second = outline_glyph(face, u32::from(glyph_id));
            assert_eq!(first, second, "{} slot {glyph_id}", face.stem());
            if let Some(outline) = first {
                available += 1;
                max_commands = max_commands.max(outline.commands.len());
                assert!(outline.commands.iter().all(GlyphOutlineCommand::is_finite));
                assert!(outline.commands.len() <= MAX_BROWSER_OUTLINE_COMMANDS_PER_GLYPH);
            }
        }
        assert!(available > 0, "{} has no outlineable glyphs", face.stem());
        eprintln!(
            "{}: {available} outlineable slots, max {max_commands} commands",
            face.stem()
        );
    }
}
