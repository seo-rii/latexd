use serde::{Deserialize, Serialize};

use crate::{GraphicAssetFormat, GraphicPageSelection, SourceProvenance, SourceSpan};

pub type PageId = String;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedGraphicAsset {
    pub bytes: Vec<u8>,
    pub source_format: Option<GraphicAssetFormat>,
    pub format: GraphicAssetFormat,
    pub asset_hash: Option<String>,
}

impl MaterializedGraphicAsset {
    pub fn from_source(request: &GraphicAssetRequest, bytes: Vec<u8>) -> Option<Self> {
        let format = request
            .source_format
            .or_else(|| GraphicAssetFormat::from_bytes(&bytes))?;
        Some(Self {
            bytes,
            source_format: Some(format),
            format,
            asset_hash: request.asset_hash.clone(),
        })
    }

    pub fn converted(
        request: &GraphicAssetRequest,
        bytes: Vec<u8>,
        format: GraphicAssetFormat,
    ) -> Self {
        Self {
            bytes,
            source_format: request.source_format,
            format,
            asset_hash: request.asset_hash.clone(),
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
        MaterializedGraphicAsset, Point, PositionedImage, Rect, SourceProvenance,
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
    }
}
