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
    use crate::{Destination, DrawOp, Point, Rect, SourceProvenance};

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
}
