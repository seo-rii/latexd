use crate::{FontSeries, FontShape};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedRasterImage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorScene {
    pub natural_width_pt: f32,
    pub natural_height_pt: f32,
    pub view_box_aspect_ratio: f32,
    pub preserve_aspect_ratio: VectorPreserveAspectRatio,
    pub rects: Vec<VectorRect>,
    pub lines: Vec<VectorLine>,
    pub ellipses: Vec<VectorEllipse>,
    pub polys: Vec<VectorPoly>,
    pub paths: Vec<VectorPath>,
    pub texts: Vec<VectorText>,
    pub embedded_images: Vec<VectorEmbeddedImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorPreserveAspectRatio {
    pub x_align: VectorAspectAlign,
    pub y_align: VectorAspectAlign,
    pub scale: VectorAspectScale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorAspectAlign {
    Min,
    Mid,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorAspectScale {
    None,
    Meet,
    Slice,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorPaint {
    pub rgb: (f32, f32, f32),
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorDashArray {
    pub values: [f32; 8],
    pub len: usize,
    pub offset_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorClipRect {
    pub x_ratio: f32,
    pub y_ratio: f32,
    pub width_ratio: f32,
    pub height_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorStrokeLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorStrokeLineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorPaintOrder {
    Normal,
    StrokeFill,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorStrokeStyle {
    pub linecap: VectorStrokeLineCap,
    pub linejoin: VectorStrokeLineJoin,
    pub miterlimit: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorRect {
    pub x_ratio: f32,
    pub y_ratio: f32,
    pub width_ratio: f32,
    pub height_ratio: f32,
    pub fill: Option<VectorPaint>,
    pub fill_rule: VectorFillRule,
    pub stroke: Option<VectorPaint>,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub paint_order: VectorPaintOrder,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorLine {
    pub x1_ratio: f32,
    pub y1_ratio: f32,
    pub x2_ratio: f32,
    pub y2_ratio: f32,
    pub stroke: VectorPaint,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VectorEllipse {
    pub cx_ratio: f32,
    pub cy_ratio: f32,
    pub rx_ratio: f32,
    pub ry_ratio: f32,
    pub fill: Option<VectorPaint>,
    pub fill_rule: VectorFillRule,
    pub stroke: Option<VectorPaint>,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub paint_order: VectorPaintOrder,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPoly {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub fill: Option<VectorPaint>,
    pub fill_rule: VectorFillRule,
    pub stroke: Option<VectorPaint>,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub paint_order: VectorPaintOrder,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub ops: Vec<VectorPathOp>,
    pub fill: Option<VectorPaint>,
    pub fill_rule: VectorFillRule,
    pub stroke: Option<VectorPaint>,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub paint_order: VectorPaintOrder,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorText {
    pub x_ratio: f32,
    pub y_ratio: f32,
    pub matrix_a: f32,
    pub matrix_b: f32,
    pub matrix_c: f32,
    pub matrix_d: f32,
    pub font_size_ratio: f32,
    pub letter_spacing_ratio: f32,
    pub word_spacing_ratio: f32,
    pub anchor: VectorTextAnchor,
    pub font_family: VectorFontFamily,
    pub font_series: FontSeries,
    pub font_shape: FontShape,
    pub fill: Option<VectorPaint>,
    pub stroke: Option<VectorPaint>,
    pub stroke_width_ratio: f32,
    pub stroke_dasharray: Option<VectorDashArray>,
    pub stroke_style: VectorStrokeStyle,
    pub paint_order: VectorPaintOrder,
    pub decoration: VectorTextDecoration,
    pub decoration_paint: Option<Option<VectorPaint>>,
    pub decoration_thickness_ratio: Option<f32>,
    pub decoration_style: VectorTextDecorationStyle,
    pub clip_rect: Option<VectorClipRect>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorEmbeddedImage {
    pub x_ratio: f32,
    pub y_ratio: f32,
    pub width_ratio: f32,
    pub height_ratio: f32,
    pub image: EmbeddedRasterImage,
    pub preserve_aspect_ratio: VectorPreserveAspectRatio,
    pub opacity: f32,
    pub clip_rect: Option<VectorClipRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorTextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorTextDecoration {
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorTextDecorationStyle {
    Solid,
    Double,
    Wavy,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorFontFamily {
    Serif,
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorTextBaseline {
    Alphabetic,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VectorPathOp {
    MoveTo((f32, f32)),
    LineTo((f32, f32)),
    CubicTo {
        ctrl1: (f32, f32),
        ctrl2: (f32, f32),
        to: (f32, f32),
    },
    Close,
}

#[cfg(test)]
mod tests {
    use crate::{from_pretty_json, to_pretty_json};

    use super::{VectorAspectAlign, VectorAspectScale, VectorPreserveAspectRatio, VectorScene};

    #[test]
    fn vector_scene_roundtrips_through_json_goldens() {
        let scene = VectorScene {
            natural_width_pt: 144.0,
            natural_height_pt: 72.0,
            view_box_aspect_ratio: 2.0,
            preserve_aspect_ratio: VectorPreserveAspectRatio {
                x_align: VectorAspectAlign::Mid,
                y_align: VectorAspectAlign::Min,
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

        let json = to_pretty_json(&scene).expect("serialize vector scene");
        let decoded = from_pretty_json::<VectorScene>(&json).expect("deserialize vector scene");

        assert_eq!(decoded, scene);
        assert!(json.contains("\"natural_width_pt\": 144.0"));
        assert!(json.contains("\"scale\": \"meet\""));
    }
}
