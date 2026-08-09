use tex_render_model::{
    BrowserBuildMetadata, BrowserCompileMode, BrowserFontAsset, BrowserGlyphOutline,
    BrowserPagesArtifact, DrawOp, FontFaceId, GlyphIdKind, GlyphOutlineCommand, GraphicAssetFormat,
    PageDisplayList, PositionedImage, Rect, SourceProvenance,
};

fn page(page_id: &str, content_hash: &str, ops: Vec<DrawOp>) -> PageDisplayList {
    PageDisplayList {
        page_id: page_id.to_string(),
        width_pt: 612.0,
        height_pt: 792.0,
        ops,
        source_spans: Vec::new(),
        content_hash: content_hash.to_string(),
    }
}

#[test]
fn one_shot_browser_artifacts_preserve_compiler_page_identity_and_assets() {
    let pages = vec![
        page("page-a", "hash-a", Vec::new()),
        page(
            "page-b",
            "hash-b",
            vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 36.0,
                    y: 72.0,
                    width: 120.0,
                    height: 90.0,
                },
                asset_ref: "figures/result.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("asset-hash".to_string()),
                natural_width_pt: Some(240.0),
                natural_height_pt: Some(180.0),
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::generated("figure", "test figure"),
            })],
        ),
    ];

    let font = BrowserFontAsset {
        face_id: FontFaceId::new("cmr10"),
        postscript_name: "CMR10".to_string(),
        glyph_id_kind: GlyphIdKind::Type1CharCode,
        content_hash: "blake3:font".to_string(),
        glyphs: vec![BrowserGlyphOutline {
            glyph_id: u32::from(b'A'),
            commands: vec![
                GlyphOutlineCommand::MoveTo { x: 0.0, y: 0.0 },
                GlyphOutlineCommand::LineTo { x: 0.5, y: 1.0 },
                GlyphOutlineCommand::Close,
            ],
        }],
    };
    let artifact = BrowserPagesArtifact::one_shot(17, pages, vec![font]);

    assert_eq!(artifact.schema_version, 2);
    assert_eq!(artifact.revision, 17);
    assert_eq!(artifact.changed_page_ids, ["page-a", "page-b"]);
    assert!(artifact.removed_page_ids.is_empty());
    assert_eq!(artifact.pages[1].page_id, "page-b");
    assert_eq!(artifact.pages[1].content_hash, "hash-b");
    assert_eq!(artifact.assets.len(), 1);
    assert_eq!(artifact.assets[0].asset_ref, "figures/result.png");
    assert_eq!(artifact.assets[0].format, Some(GraphicAssetFormat::Png));
    assert_eq!(
        artifact.assets[0].content_hash.as_deref(),
        Some("asset-hash")
    );
    assert_eq!(artifact.fonts.len(), 1);
    assert_eq!(artifact.fonts[0].face_id.as_str(), "cmr10");
    assert_eq!(artifact.fonts[0].glyphs[0].glyph_id, u32::from(b'A'));

    let metadata = BrowserBuildMetadata::one_shot(17, 42, 3, &artifact);
    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.revision, 17);
    assert_eq!(metadata.compile_mode, BrowserCompileMode::OneShot);
    assert_eq!(metadata.event_count, 42);
    assert_eq!(metadata.diagnostic_count, 3);
    assert_eq!(metadata.pages.total, 2);
    assert_eq!(metadata.pages.changed, 2);
    assert_eq!(metadata.pages.reused, 0);
    assert_eq!(metadata.pages.removed, 0);

    let encoded = serde_json::to_string(&artifact).expect("serialize browser page artifact");
    let decoded: BrowserPagesArtifact =
        serde_json::from_str(&encoded).expect("deserialize browser page artifact");
    assert_eq!(decoded, artifact);
}

#[test]
fn browser_asset_manifest_deduplicates_repeated_image_references() {
    let image = DrawOp::Image(PositionedImage {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        asset_ref: "figure.pdf".to_string(),
        asset_format: Some(GraphicAssetFormat::Pdf),
        page_selection: None,
        asset_hash: Some("pdf-hash".to_string()),
        natural_width_pt: None,
        natural_height_pt: None,
        crop: None,
        scale: None,
        rotation: None,
        diagnostic: None,
        source: SourceProvenance::generated("figure", "test figure"),
    });
    let artifact = BrowserPagesArtifact::one_shot(
        1,
        vec![
            page("page-a", "hash-a", vec![image.clone()]),
            page("page-b", "hash-b", vec![image]),
        ],
        Vec::new(),
    );

    assert_eq!(artifact.assets.len(), 1);
    assert_eq!(artifact.assets[0].asset_ref, "figure.pdf");
    assert_eq!(artifact.assets[0].format, Some(GraphicAssetFormat::Pdf));
    assert_eq!(artifact.assets[0].content_hash.as_deref(), Some("pdf-hash"));
}
