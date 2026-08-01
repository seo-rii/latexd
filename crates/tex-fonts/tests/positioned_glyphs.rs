use tex_fonts::{TexFontFace, shape_text};
use tex_render_model::GlyphIdKind;

#[test]
fn computer_modern_shape_preserves_face_slots_positions_and_clusters() {
    let Some(shaped) = shape_text(TexFontFace::Roman10, "AV", 10.0) else {
        eprintln!("skipping positioned glyph test because cmr10 is not installed");
        return;
    };

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
