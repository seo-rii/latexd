use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::{
    GraphicAssetFormat, GraphicPageSelection, PreparedPdfForm, PreparedRasterFallback,
    SourceProvenance, SourceSpan, VectorScene,
};

pub type PageId = String;
pub const MATERIALIZED_GRAPHIC_ASSET_HASH_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageDisplayList {
    pub page_id: PageId,
    pub width_pt: f32,
    pub height_pt: f32,
    pub ops: Vec<DrawOp>,
    #[serde(default)]
    pub source_spans: Vec<SourceSpan>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawOp {
    Save,
    Restore,
    ClipRect(Rect),
    TextRun(PositionedTextRun),
    Rule(Rect),
    Image(PositionedImage),
    LinkAnnotation(LinkAnnotation),
    NamedDestination(Destination),
}

impl DrawOp {
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Save | Self::Restore => {}
            Self::ClipRect(rect) | Self::Rule(rect) => {
                rect.x += dx;
                rect.y += dy;
            }
            Self::TextRun(run) => {
                run.origin.x += dx;
                run.origin.y += dy;
            }
            Self::Image(image) => {
                image.rect.x += dx;
                image.rect.y += dy;
            }
            Self::LinkAnnotation(link) => {
                link.rect.x += dx;
                link.rect.y += dy;
            }
            Self::NamedDestination(destination) => {
                destination.point.x += dx;
                destination.point.y += dy;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionedTextRun {
    pub origin: Point,
    pub text: String,
    pub font: FontRequest,
    pub size_pt: f32,
    pub approximate_advance_pt: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyphs: Option<Vec<PositionedGlyph>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clusters: Option<Vec<TextCluster>>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionedGlyph {
    pub glyph_id: u32,
    pub advance_pt: f32,
    pub offset: Point,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextCluster {
    pub text_start_utf8: u32,
    pub text_end_utf8: u32,
    pub glyph_start: u32,
    pub glyph_end: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionedImage {
    pub rect: Rect,
    pub asset_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_format: Option<GraphicAssetFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_selection: Option<GraphicPageSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_width_pt: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_height_pt: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<ImageCrop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<ImageScale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<ImageRotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphicAssetRequest {
    pub asset_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_format: Option<GraphicAssetFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_selection: Option<GraphicPageSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_hash: Option<String>,
}

impl GraphicAssetRequest {
    pub fn for_embedded_asset(asset_ref: impl Into<String>) -> Self {
        let asset_ref = asset_ref.into();
        Self {
            source_format: GraphicAssetFormat::from_path(&asset_ref),
            asset_ref,
            page_selection: None,
            asset_hash: None,
        }
    }
}

impl From<&PositionedImage> for GraphicAssetRequest {
    fn from(image: &PositionedImage) -> Self {
        Self {
            asset_ref: image.asset_ref.clone(),
            source_format: image
                .asset_format
                .or_else(|| GraphicAssetFormat::from_path(&image.asset_ref)),
            page_selection: image.page_selection.clone(),
            asset_hash: image.asset_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterializedGraphicAsset {
    pub request: GraphicAssetRequest,
    pub bytes: Vec<u8>,
    pub source_format: Option<GraphicAssetFormat>,
    pub format: GraphicAssetFormat,
    pub asset_hash: Option<String>,
    pub content_hash: String,
    pub vector_scene: Option<VectorScene>,
    pub embeddable_svg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_form: Option<PreparedPdfForm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raster_fallback: Option<PreparedRasterFallback>,
}

impl MaterializedGraphicAsset {
    fn base_content_hash(
        request: &GraphicAssetRequest,
        bytes: &[u8],
        format: GraphicAssetFormat,
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"materialized-graphic-asset");
        hasher.update(&MATERIALIZED_GRAPHIC_ASSET_HASH_VERSION.to_le_bytes());
        let mut update_field = |value: &[u8]| {
            hasher.update(&(value.len() as u64).to_le_bytes());
            hasher.update(value);
        };
        update_field(request.asset_ref.as_bytes());
        update_field(&[u8::from(request.source_format.is_some())]);
        if let Some(source_format) = request.source_format {
            update_field(source_format.as_str().as_bytes());
        }
        update_field(format.as_str().as_bytes());
        let page = request
            .page_selection
            .as_ref()
            .and_then(|selection| selection.page);
        update_field(&[u8::from(page.is_some())]);
        if let Some(page) = page {
            update_field(&page.to_le_bytes());
        }
        let pagebox = request
            .page_selection
            .as_ref()
            .and_then(|selection| selection.pagebox.as_deref());
        update_field(&[u8::from(pagebox.is_some())]);
        if let Some(pagebox) = pagebox {
            update_field(pagebox.as_bytes());
        }
        update_field(&[u8::from(request.asset_hash.is_some())]);
        if let Some(asset_hash) = request.asset_hash.as_deref() {
            update_field(asset_hash.as_bytes());
        }
        update_field(bytes);
        format!("blake3:{}", hasher.finalize().to_hex())
    }

    fn derived_content_hash(
        base_content_hash: &str,
        vector_scene: Option<&VectorScene>,
        embeddable_svg: Option<&str>,
        pdf_form: Option<&PreparedPdfForm>,
        raster_fallback: Option<&PreparedRasterFallback>,
    ) -> Option<String> {
        struct HashWriter<'a>(&'a mut blake3::Hasher);

        impl Write for HashWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"materialized-graphic-derived-payloads");
        hasher.update(&MATERIALIZED_GRAPHIC_ASSET_HASH_VERSION.to_le_bytes());
        hasher.update(base_content_hash.as_bytes());
        serde_json::to_writer(
            HashWriter(&mut hasher),
            &(vector_scene, embeddable_svg, pdf_form, raster_fallback),
        )
        .ok()?;
        Some(format!("blake3:{}", hasher.finalize().to_hex()))
    }

    fn refresh_derived_content_hash(&mut self) {
        let base_content_hash = Self::base_content_hash(&self.request, &self.bytes, self.format);
        self.content_hash = if self.vector_scene.is_none()
            && self.embeddable_svg.is_none()
            && self.pdf_form.is_none()
            && self.raster_fallback.is_none()
        {
            base_content_hash
        } else {
            Self::derived_content_hash(
                &base_content_hash,
                self.vector_scene.as_ref(),
                self.embeddable_svg.as_deref(),
                self.pdf_form.as_ref(),
                self.raster_fallback.as_ref(),
            )
            .expect("derived graphic payloads must serialize")
        };
    }

    pub fn from_source(request: &GraphicAssetRequest, bytes: Vec<u8>) -> Option<Self> {
        let format = request
            .source_format
            .or_else(|| GraphicAssetFormat::from_bytes(&bytes))?;
        let content_hash = Self::base_content_hash(request, &bytes, format);
        Some(Self {
            request: request.clone(),
            bytes,
            source_format: Some(format),
            format,
            asset_hash: request.asset_hash.clone(),
            content_hash,
            vector_scene: None,
            embeddable_svg: None,
            pdf_form: None,
            raster_fallback: None,
        })
    }

    pub fn converted(
        request: &GraphicAssetRequest,
        bytes: Vec<u8>,
        format: GraphicAssetFormat,
    ) -> Self {
        let content_hash = Self::base_content_hash(request, &bytes, format);
        Self {
            request: request.clone(),
            bytes,
            source_format: request.source_format,
            format,
            asset_hash: request.asset_hash.clone(),
            content_hash,
            vector_scene: None,
            embeddable_svg: None,
            pdf_form: None,
            raster_fallback: None,
        }
    }

    pub fn with_vector_scene(mut self, scene: VectorScene, embeddable_svg: String) -> Self {
        self.vector_scene = Some(scene);
        self.embeddable_svg = Some(embeddable_svg);
        self.refresh_derived_content_hash();
        self
    }

    pub fn with_pdf_form(mut self, pdf_form: PreparedPdfForm) -> Self {
        self.pdf_form = Some(pdf_form);
        self.refresh_derived_content_hash();
        self
    }

    pub fn with_raster_fallback(mut self, bytes: Vec<u8>, format: GraphicAssetFormat) -> Self {
        self.raster_fallback = Some(PreparedRasterFallback { format, bytes });
        self.refresh_derived_content_hash();
        self
    }

    pub fn has_valid_content_hash(&self, request: &GraphicAssetRequest) -> bool {
        if &self.request != request
            || self.asset_hash != request.asset_hash
            || match request.source_format {
                Some(source_format) => self.source_format != Some(source_format),
                None => self
                    .source_format
                    .is_some_and(|source| source != self.format),
            }
        {
            return false;
        }
        if !matches!(
            (&self.vector_scene, &self.embeddable_svg),
            (None, None) | (Some(_), Some(_))
        ) || self
            .pdf_form
            .as_ref()
            .is_some_and(|pdf_form| !pdf_form.is_complete())
            || self.raster_fallback.as_ref().is_some_and(|fallback| {
                fallback.bytes.is_empty()
                    || !matches!(
                        fallback.format,
                        GraphicAssetFormat::Png | GraphicAssetFormat::Jpeg
                    )
            })
        {
            return false;
        }
        let base_content_hash = Self::base_content_hash(request, &self.bytes, self.format);
        if self.vector_scene.is_none()
            && self.embeddable_svg.is_none()
            && self.pdf_form.is_none()
            && self.raster_fallback.is_none()
        {
            self.content_hash == base_content_hash
        } else {
            Self::derived_content_hash(
                &base_content_hash,
                self.vector_scene.as_ref(),
                self.embeddable_svg.as_deref(),
                self.pdf_form.as_ref(),
                self.raster_fallback.as_ref(),
            )
            .is_some_and(|content_hash| self.content_hash == content_hash)
        }
    }

    pub fn is_converted(&self) -> bool {
        self.source_format
            .is_some_and(|source| source != self.format)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageCrop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<ImageTrim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<ImageViewport>,
    #[serde(default)]
    pub clip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageTrim {
    pub left_pt: f32,
    pub bottom_pt: f32,
    pub right_pt: f32,
    pub top_pt: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageViewport {
    pub llx_pt: f32,
    pub lly_pt: f32,
    pub urx_pt: f32,
    pub ury_pt: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageScale {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRotation {
    pub angle_degrees: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkAnnotation {
    pub rect: Rect,
    pub target: String,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Destination {
    pub name: String,
    pub point: Point,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontRequest {
    pub family: FontFamilyRequest,
    pub series: FontSeries,
    pub shape: FontShape,
    pub size_pt: f32,
    pub role: FontRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontFamilyRequest {
    Serif,
    Sans,
    Mono,
    Math,
    Symbol,
    MathExtension,
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSeries {
    Regular,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontShape {
    Upright,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontRole {
    Body,
    Heading,
    Math,
    Mono,
}

#[cfg(test)]
mod tests {
    use crate::{
        Destination, DrawOp, GraphicAssetFormat, GraphicAssetRequest, GraphicPageSelection,
        MaterializedGraphicAsset, Point, PositionedImage, PreparedPdfDictionaryEntry,
        PreparedPdfForm, PreparedPdfObject, Rect, SourceProvenance, VectorAspectAlign,
        VectorAspectScale, VectorPreserveAspectRatio, VectorScene,
    };

    #[test]
    fn draw_ops_translate_renderer_geometry() {
        let mut rule = DrawOp::Rule(Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        });
        let mut destination = DrawOp::NamedDestination(Destination {
            name: "target".to_string(),
            point: Point { x: 5.0, y: 6.0 },
            source: SourceProvenance::generated("target", "test target"),
        });

        rule.translate(10.0, 20.0);
        destination.translate(10.0, 20.0);

        assert!(matches!(
            rule,
            DrawOp::Rule(Rect {
                x: 11.0,
                y: 22.0,
                ..
            })
        ));
        assert!(matches!(
            destination,
            DrawOp::NamedDestination(Destination {
                point: Point { x: 15.0, y: 26.0 },
                ..
            })
        ));
    }

    #[test]
    fn graphic_asset_requests_preserve_renderer_neutral_selection_and_identity() {
        let image = PositionedImage {
            rect: Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            asset_ref: "figures/paper.pdf".to_string(),
            asset_format: Some(GraphicAssetFormat::Pdf),
            page_selection: Some(GraphicPageSelection {
                page: Some(2),
                pagebox: Some("cropbox".to_string()),
            }),
            asset_hash: Some("blake3:asset".to_string()),
            natural_width_pt: None,
            natural_height_pt: None,
            crop: None,
            scale: None,
            rotation: None,
            diagnostic: None,
            source: SourceProvenance::generated("graphic", "test graphic"),
        };

        let request = GraphicAssetRequest::from(&image);

        assert_eq!(request.asset_ref, "figures/paper.pdf");
        assert_eq!(request.source_format, Some(GraphicAssetFormat::Pdf));
        assert_eq!(request.page_selection, image.page_selection);
        assert_eq!(request.asset_hash.as_deref(), Some("blake3:asset"));

        let mut other_page = request.clone();
        other_page
            .page_selection
            .as_mut()
            .expect("page selection")
            .page = Some(3);
        let keys = std::collections::BTreeSet::from([request, other_page]);
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn materialized_graphic_assets_distinguish_source_and_render_formats() {
        let request = GraphicAssetRequest {
            asset_ref: "figures/paper.pdf".to_string(),
            source_format: Some(GraphicAssetFormat::Pdf),
            page_selection: None,
            asset_hash: Some("blake3:asset".to_string()),
        };

        let materialized =
            MaterializedGraphicAsset::converted(&request, vec![1, 2, 3], GraphicAssetFormat::Png);

        assert_eq!(materialized.source_format, Some(GraphicAssetFormat::Pdf));
        assert_eq!(materialized.format, GraphicAssetFormat::Png);
        assert_eq!(materialized.asset_hash, request.asset_hash);
        assert!(materialized.is_converted());
        assert!(materialized.content_hash.starts_with("blake3:"));

        let same =
            MaterializedGraphicAsset::converted(&request, vec![1, 2, 3], GraphicAssetFormat::Png);
        let changed_bytes =
            MaterializedGraphicAsset::converted(&request, vec![1, 2, 4], GraphicAssetFormat::Png);
        let mut changed_page_request = request.clone();
        changed_page_request.page_selection = Some(GraphicPageSelection {
            page: Some(2),
            pagebox: Some("cropbox".to_string()),
        });
        let changed_page = MaterializedGraphicAsset::converted(
            &changed_page_request,
            vec![1, 2, 3],
            GraphicAssetFormat::Png,
        );

        assert_eq!(same.content_hash, materialized.content_hash);
        assert_ne!(changed_bytes.content_hash, materialized.content_hash);
        assert_ne!(changed_page.content_hash, materialized.content_hash);
    }

    #[test]
    fn materialized_graphic_asset_roundtrips_with_a_valid_content_hash() {
        let request = GraphicAssetRequest {
            asset_ref: "figures/vector.svg".to_string(),
            source_format: Some(GraphicAssetFormat::Svg),
            page_selection: None,
            asset_hash: Some("blake3:asset".to_string()),
        };
        let scene = VectorScene {
            natural_width_pt: 100.0,
            natural_height_pt: 50.0,
            view_box_aspect_ratio: 2.0,
            preserve_aspect_ratio: VectorPreserveAspectRatio {
                x_align: VectorAspectAlign::Mid,
                y_align: VectorAspectAlign::Mid,
                scale: VectorAspectScale::Meet,
            },
            rects: Vec::new(),
            lines: Vec::new(),
            ellipses: Vec::new(),
            polys: Vec::new(),
            paths: Vec::new(),
            texts: Vec::new(),
            embedded_images: Vec::new(),
        };
        let materialized = MaterializedGraphicAsset::from_source(
            &request,
            br#"<svg viewBox="0 0 100 50"/>"#.to_vec(),
        )
        .expect("materialize SVG")
        .with_vector_scene(scene, "<svg viewBox=\"0 0 100 50\"/>".to_string());

        let json = serde_json::to_vec(&materialized).expect("serialize materialized asset");
        let decoded: MaterializedGraphicAsset =
            serde_json::from_slice(&json).expect("deserialize materialized asset");

        assert_eq!(decoded, materialized);
        assert!(decoded.has_valid_content_hash(&request));
    }

    #[test]
    fn materialized_graphic_asset_rejects_tampered_bytes_or_content_hash() {
        let request = GraphicAssetRequest::for_embedded_asset("figures/image.png");
        let materialized =
            MaterializedGraphicAsset::converted(&request, vec![1, 2, 3], GraphicAssetFormat::Png);
        assert!(materialized.has_valid_content_hash(&request));

        let mut changed_bytes = materialized.clone();
        changed_bytes.bytes.push(4);
        assert!(!changed_bytes.has_valid_content_hash(&request));

        let mut changed_hash = materialized;
        changed_hash.content_hash.push('0');
        assert!(!changed_hash.has_valid_content_hash(&request));

        let mut changed_metadata = changed_hash;
        changed_metadata.content_hash.pop();
        changed_metadata.asset_hash = Some("blake3:wrong".to_string());
        assert!(!changed_metadata.has_valid_content_hash(&request));
    }

    #[test]
    fn materialized_graphic_asset_rejects_incomplete_vector_payloads() {
        let request = GraphicAssetRequest::for_embedded_asset("figures/vector.svg");
        let mut materialized = MaterializedGraphicAsset::from_source(
            &request,
            br#"<svg viewBox="0 0 100 50"/>"#.to_vec(),
        )
        .expect("materialize SVG");
        let scene: VectorScene = serde_json::from_value(serde_json::json!({
            "natural_width_pt": 100.0,
            "natural_height_pt": 50.0,
            "view_box_aspect_ratio": 2.0,
            "preserve_aspect_ratio": { "x_align": "mid", "y_align": "mid", "scale": "meet" },
            "rects": [],
            "lines": [],
            "ellipses": [],
            "polys": [],
            "paths": [],
            "texts": [],
            "embedded_images": []
        }))
        .expect("deserialize vector scene fixture");

        materialized.vector_scene = Some(scene);
        assert!(!materialized.has_valid_content_hash(&request));

        materialized.vector_scene = None;
        materialized.embeddable_svg = Some("<svg/>".to_string());
        assert!(!materialized.has_valid_content_hash(&request));
    }

    #[test]
    fn materialized_pdf_form_and_raster_fallback_are_hashed_and_validated() {
        let request = GraphicAssetRequest {
            asset_ref: "figures/page.pdf".to_string(),
            source_format: Some(GraphicAssetFormat::Pdf),
            page_selection: None,
            asset_hash: Some("blake3:pdf".to_string()),
        };
        let form = PreparedPdfForm {
            root_object_id: 1,
            natural_width_pt: 100.0,
            natural_height_pt: 50.0,
            objects: std::collections::BTreeMap::from([(
                1,
                PreparedPdfObject::Stream {
                    entries: vec![
                        PreparedPdfDictionaryEntry {
                            key: b"Type".to_vec(),
                            value: PreparedPdfObject::Name {
                                value: b"XObject".to_vec(),
                            },
                        },
                        PreparedPdfDictionaryEntry {
                            key: b"Subtype".to_vec(),
                            value: PreparedPdfObject::Name {
                                value: b"Form".to_vec(),
                            },
                        },
                        PreparedPdfDictionaryEntry {
                            key: b"BBox".to_vec(),
                            value: PreparedPdfObject::Array {
                                values: vec![
                                    PreparedPdfObject::Integer { value: 0 },
                                    PreparedPdfObject::Integer { value: 0 },
                                    PreparedPdfObject::Integer { value: 1 },
                                    PreparedPdfObject::Integer { value: 1 },
                                ],
                            },
                        },
                        PreparedPdfDictionaryEntry {
                            key: b"Resources".to_vec(),
                            value: PreparedPdfObject::Dictionary {
                                entries: Vec::new(),
                            },
                        },
                    ],
                    data: b"0 0 1 1 re f".to_vec(),
                },
            )]),
        };
        let materialized = MaterializedGraphicAsset::from_source(&request, b"%PDF-1.7".to_vec())
            .expect("materialize PDF")
            .with_pdf_form(form)
            .with_raster_fallback(vec![1, 2, 3], GraphicAssetFormat::Png);

        assert!(materialized.has_valid_content_hash(&request));
        let json = serde_json::to_vec(&materialized).expect("serialize materialized PDF");
        let decoded: MaterializedGraphicAsset =
            serde_json::from_slice(&json).expect("deserialize materialized PDF");
        assert_eq!(decoded, materialized);

        let mut changed_form = materialized.clone();
        let Some(PreparedPdfObject::Stream { data, .. }) = changed_form
            .pdf_form
            .as_mut()
            .and_then(|form| form.objects.get_mut(&form.root_object_id))
        else {
            panic!("prepared form stream");
        };
        data.push(b'Q');
        assert!(!changed_form.has_valid_content_hash(&request));

        let mut changed_fallback = materialized;
        changed_fallback
            .raster_fallback
            .as_mut()
            .expect("raster fallback")
            .bytes
            .push(4);
        assert!(!changed_fallback.has_valid_content_hash(&request));
    }
}
