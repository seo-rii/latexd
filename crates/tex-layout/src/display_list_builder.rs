use tex_render_model::{
    BibliographyBlock, Destination, DocumentIr, DrawOp, FontFamilyRequest, FontRequest, FontRole,
    FontSeries, FontShape, GraphicAssetDensityUnit, ImageCrop, ImageRotation, ImageScale,
    ImageTrim, ImageViewport, InlineNode, IrBlock, LayoutAlignment, LinkAnnotation,
    PageDisplayList, Point, PositionedImage, PositionedTextRun, ProvenanceSpan, Rect,
    SourceProvenance, SourceSpan, TableColumnAlignment, TableRow, TableRuleSpan, TextCluster,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PageDisplayListOptions {
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub margin_left_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub front_matter_top_pt: Option<f32>,
    pub column_count: usize,
    pub column_gap_pt: f32,
    pub abstract_indent_pt: f32,
    pub list_continuation_indent_pt: f32,
    pub bibliography_continuation_indent_pt: f32,
    pub max_chars_per_line: usize,
    pub line_height_pt: f32,
    pub block_gap_pt: f32,
    pub paragraph_first_line_indent_pt: f32,
    pub paragraph_gap_pt: Option<f32>,
    pub body_font_size_pt: f32,
    pub heading_font_size_pt: f32,
    pub title_font_size_pt: f32,
    pub title_font_bold: bool,
    pub title_gap_pt: Option<f32>,
    pub author_date_font_size_pt: Option<f32>,
    pub authors_on_single_line: bool,
    pub author_date_gap_pt: f32,
    pub front_matter_gap_pt: f32,
    pub abstract_font_size_pt: Option<f32>,
    pub abstract_line_height_pt: Option<f32>,
    pub abstract_first_line_indent_pt: f32,
    pub abstract_heading_bold: bool,
    pub abstract_heading_centered: bool,
    pub abstract_heading_gap_pt: f32,
    pub show_page_numbers: bool,
    pub page_number_font_size_pt: f32,
    pub page_number_offset_pt: f32,
}

impl Default for PageDisplayListOptions {
    fn default() -> Self {
        Self {
            page_width_pt: 612.0,
            page_height_pt: 792.0,
            margin_left_pt: 72.0,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            front_matter_top_pt: None,
            column_count: 1,
            column_gap_pt: 18.0,
            abstract_indent_pt: 18.0,
            list_continuation_indent_pt: 18.0,
            bibliography_continuation_indent_pt: 24.0,
            max_chars_per_line: 72,
            line_height_pt: 14.0,
            block_gap_pt: 7.0,
            paragraph_first_line_indent_pt: 0.0,
            paragraph_gap_pt: None,
            body_font_size_pt: 11.0,
            heading_font_size_pt: 15.0,
            title_font_size_pt: 18.0,
            title_font_bold: true,
            title_gap_pt: None,
            author_date_font_size_pt: None,
            authors_on_single_line: false,
            author_date_gap_pt: 0.0,
            front_matter_gap_pt: 14.0,
            abstract_font_size_pt: None,
            abstract_line_height_pt: None,
            abstract_first_line_indent_pt: 0.0,
            abstract_heading_bold: false,
            abstract_heading_centered: false,
            abstract_heading_gap_pt: 0.0,
            show_page_numbers: false,
            page_number_font_size_pt: 10.0,
            page_number_offset_pt: 18.0,
        }
    }
}

impl PageDisplayListOptions {
    pub fn for_document_ir(document_ir: &DocumentIr) -> Self {
        let mut options = Self::default();
        let has_package_layout_profile = document_ir
            .layout
            .as_ref()
            .and_then(|layout| layout.profile.as_ref())
            .is_some();
        if let Some(document_class) = &document_ir.document_class {
            let class_name = document_class.name.trim().to_ascii_lowercase();
            options.max_chars_per_line = usize::MAX;
            if document_class
                .options
                .iter()
                .any(|option| option.trim().eq_ignore_ascii_case("a4paper"))
            {
                options.page_width_pt = 595.276;
                options.page_height_pt = 841.89;
            }
            let requested_font_size_pt = document_class.options.iter().find_map(|option| {
                match option.trim().to_ascii_lowercase().as_str() {
                    "10pt" => Some(10.0),
                    "11pt" => Some(11.0),
                    "12pt" => Some(12.0),
                    _ => None,
                }
            });
            match requested_font_size_pt {
                Some(10.0) => {
                    options.body_font_size_pt = 10.0;
                    options.line_height_pt = 12.0;
                    options.heading_font_size_pt = 14.4;
                    options.title_font_size_pt = 17.0;
                    options.block_gap_pt = 6.0;
                }
                None if class_name == "article" => {
                    options.body_font_size_pt = 10.0;
                    options.line_height_pt = 12.0;
                    options.heading_font_size_pt = 14.4;
                    options.title_font_size_pt = 17.0;
                    options.block_gap_pt = 6.0;
                }
                Some(11.0) => {
                    options.body_font_size_pt = 11.0;
                    options.line_height_pt = 13.6;
                    options.heading_font_size_pt = 14.4;
                    options.title_font_size_pt = 17.0;
                    options.block_gap_pt = 6.5;
                }
                Some(12.0) => {
                    options.body_font_size_pt = 12.0;
                    options.line_height_pt = 14.5;
                    options.heading_font_size_pt = 17.28;
                    options.title_font_size_pt = 20.0;
                }
                _ => {}
            }
            let explicitly_one_column = document_class
                .options
                .iter()
                .any(|option| option.trim().eq_ignore_ascii_case("onecolumn"));
            let explicitly_two_column = document_class
                .options
                .iter()
                .any(|option| option.trim().eq_ignore_ascii_case("twocolumn"));
            let class_defaults_to_two_columns = matches!(class_name.as_str(), "ieeetran");
            let uses_two_columns =
                !explicitly_one_column && (explicitly_two_column || class_defaults_to_two_columns);
            if class_name == "article" && !has_package_layout_profile {
                const TEX_POINT_TO_PDF_POINT: f32 = 72.0 / 72.27;
                const TEX_INCH_PT: f32 = 72.27;
                let (
                    title_font_size_tex_pt,
                    author_date_font_size_tex_pt,
                    abstract_font_size_tex_pt,
                    abstract_line_height_tex_pt,
                    paragraph_indent_tex_pt,
                ) = match requested_font_size_pt {
                    Some(12.0) => (20.74, 14.4, 10.95, 13.6, 18.0),
                    Some(11.0) => (17.28, 12.0, 10.0, 12.0, 17.0),
                    _ => (17.28, 12.0, 9.0, 11.0, 15.0),
                };
                let front_matter_scale = options.body_font_size_pt / 10.0;
                let paper_width_tex_pt = options.page_width_pt / TEX_POINT_TO_PDF_POINT;
                let paper_height_tex_pt = options.page_height_pt / TEX_POINT_TO_PDF_POINT;
                let available_width_tex_pt = (paper_width_tex_pt - 2.0 * TEX_INCH_PT).max(1.0);
                let text_width_tex_pt = if uses_two_columns {
                    available_width_tex_pt.min(690.0)
                } else {
                    available_width_tex_pt.min(345.0)
                };
                let baseline_skip_tex_pt = options.line_height_pt;
                let top_skip_tex_pt = options.body_font_size_pt;
                let vertical_room_tex_pt =
                    (paper_height_tex_pt - 3.5 * TEX_INCH_PT).max(baseline_skip_tex_pt);
                let text_line_count = (vertical_room_tex_pt / baseline_skip_tex_pt)
                    .floor()
                    .max(1.0);
                let text_height_tex_pt = text_line_count * baseline_skip_tex_pt + top_skip_tex_pt;
                let head_height_tex_pt = 12.0;
                let head_sep_tex_pt = 25.0;
                let foot_skip_tex_pt = 30.0;
                let top_margin_tex_pt = (paper_height_tex_pt
                    - 2.0 * TEX_INCH_PT
                    - head_height_tex_pt
                    - head_sep_tex_pt
                    - text_height_tex_pt
                    - foot_skip_tex_pt)
                    / 2.0;
                let text_top_pt =
                    (TEX_INCH_PT + top_margin_tex_pt + head_height_tex_pt + head_sep_tex_pt)
                        * TEX_POINT_TO_PDF_POINT;
                let text_height_pt = text_height_tex_pt * TEX_POINT_TO_PDF_POINT;

                options.margin_left_pt =
                    (options.page_width_pt - text_width_tex_pt * TEX_POINT_TO_PDF_POINT) / 2.0;
                options.margin_top_pt = text_top_pt + top_skip_tex_pt * TEX_POINT_TO_PDF_POINT;
                options.margin_bottom_pt =
                    (options.page_height_pt - text_top_pt - text_height_pt).max(0.0);
                options.front_matter_top_pt =
                    Some(options.margin_top_pt + 41.0 * front_matter_scale);
                options.paragraph_first_line_indent_pt = if uses_two_columns {
                    options.body_font_size_pt * TEX_POINT_TO_PDF_POINT
                } else {
                    paragraph_indent_tex_pt * TEX_POINT_TO_PDF_POINT
                };
                options.paragraph_gap_pt = Some(0.0);
                options.title_font_size_pt = title_font_size_tex_pt * TEX_POINT_TO_PDF_POINT;
                options.title_font_bold = false;
                options.title_gap_pt = Some(16.0 * front_matter_scale);
                options.author_date_font_size_pt =
                    Some(author_date_font_size_tex_pt * TEX_POINT_TO_PDF_POINT);
                options.authors_on_single_line = true;
                options.author_date_gap_pt = 11.5 * front_matter_scale;
                options.front_matter_gap_pt = 26.0 * front_matter_scale;
                options.abstract_indent_pt = 25.0 * front_matter_scale * TEX_POINT_TO_PDF_POINT;
                options.abstract_font_size_pt =
                    Some(abstract_font_size_tex_pt * TEX_POINT_TO_PDF_POINT);
                options.abstract_line_height_pt =
                    Some(abstract_line_height_tex_pt * TEX_POINT_TO_PDF_POINT);
                options.abstract_first_line_indent_pt =
                    1.5 * abstract_font_size_tex_pt * TEX_POINT_TO_PDF_POINT;
                options.abstract_heading_bold = true;
                options.abstract_heading_centered = true;
                options.abstract_heading_gap_pt = 4.5 * front_matter_scale;
                options.column_count = if uses_two_columns { 2 } else { 1 };
                options.column_gap_pt = 10.0 * TEX_POINT_TO_PDF_POINT;
                options.show_page_numbers = true;
                options.page_number_font_size_pt = options.body_font_size_pt;
                options.page_number_offset_pt = foot_skip_tex_pt * TEX_POINT_TO_PDF_POINT;
            } else if class_name == "llncs" {
                const TEX_POINT_TO_PDF_POINT: f32 = 72.0 / 72.27;
                const TEX_INCH_PT: f32 = 72.27;
                let text_width_pt = 12.2 * 72.0 / 2.54;
                let text_height_pt = 19.3 * 72.0 / 2.54;
                let text_top_pt = (TEX_INCH_PT + 16.0 + 12.0 + 16.0) * TEX_POINT_TO_PDF_POINT;

                options.margin_left_pt = (options.page_width_pt - text_width_pt) / 2.0;
                options.margin_top_pt = text_top_pt + 10.0 * TEX_POINT_TO_PDF_POINT;
                options.margin_bottom_pt = options.page_height_pt - text_top_pt - text_height_pt;
                options.front_matter_top_pt = Some(options.margin_top_pt);
                options.abstract_indent_pt = 0.0;
                options.line_height_pt = 12.0;
                options.block_gap_pt = 6.0;
                options.paragraph_first_line_indent_pt = 15.0 * TEX_POINT_TO_PDF_POINT;
                options.paragraph_gap_pt = Some(0.0);
                options.body_font_size_pt = 10.0;
                options.heading_font_size_pt = 12.0;
                options.title_font_size_pt = 14.0;
                options.front_matter_gap_pt = 18.0;
            } else if uses_two_columns {
                options.column_count = 2;
                options.column_gap_pt = 18.0;
                options.margin_top_pt = 54.0;
                options.margin_bottom_pt = 54.0;
                options.front_matter_top_pt = Some(96.0);
                options.block_gap_pt = 5.0;
                options.abstract_indent_pt = 9.0;
                options.list_continuation_indent_pt = 12.0;
                options.bibliography_continuation_indent_pt = 18.0;
                options.heading_font_size_pt = 12.5;
                options.title_font_size_pt = 16.0;
                options.front_matter_gap_pt = 36.0;
                if class_name == "ieeetran" {
                    options.margin_left_pt = 49.5;
                    options.body_font_size_pt = 9.0;
                    options.line_height_pt = 10.0;
                } else {
                    options.margin_left_pt = 54.0;
                    options.body_font_size_pt = 9.5;
                    options.line_height_pt = 10.5;
                }
            }
        }

        if let Some(layout) = &document_ir.layout {
            if let Some(value) = layout.page_width_pt_milli {
                options.page_width_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.page_height_pt_milli {
                options.page_height_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.margin_top_pt_milli {
                options.margin_top_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.text_width_pt_milli {
                let text_width_pt = value as f32 / 1000.0;
                options.margin_left_pt = ((options.page_width_pt - text_width_pt) / 2.0).max(0.0);
            }
            if let Some(value) = layout.margin_left_pt_milli {
                options.margin_left_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.text_height_pt_milli {
                let text_height_pt = value as f32 / 1000.0;
                options.margin_bottom_pt =
                    (options.page_height_pt - options.margin_top_pt - text_height_pt).max(0.0);
            }
            if let Some(value) = layout.front_matter_top_pt_milli {
                options.front_matter_top_pt = Some(value as f32 / 1000.0);
            }
            if let Some(value) = layout.column_count {
                options.column_count = value.max(1) as usize;
            }
            if let Some(value) = layout.column_gap_pt_milli {
                options.column_gap_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.body_font_size_pt_milli {
                options.body_font_size_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.line_height_pt_milli {
                options.line_height_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.heading_font_size_pt_milli {
                options.heading_font_size_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.title_font_size_pt_milli {
                options.title_font_size_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.block_gap_pt_milli {
                options.block_gap_pt = value as f32 / 1000.0;
            }
            if let Some(value) = layout.abstract_indent_pt_milli {
                options.abstract_indent_pt = value as f32 / 1000.0;
            }
        }
        options
    }
}

fn parse_table_width_spec_pt(
    width_spec: &str,
    content_width_pt: f32,
    options: &PageDisplayListOptions,
) -> Option<f32> {
    let normalized_width = width_spec
        .trim()
        .trim_matches(|ch| ch == '{' || ch == '}')
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let reference_widths = [
        ("\\hsize", content_width_pt),
        ("\\linewidth", content_width_pt),
        ("\\textwidth", content_width_pt),
        ("\\columnwidth", content_width_pt),
        ("\\paperwidth", options.page_width_pt),
        ("\\pagewidth", options.page_width_pt),
        ("\\tabcolsep", 6.0),
        ("\\arraycolsep", 5.0),
    ];
    let unit_widths = [
        ("truept", 1.0),
        ("bp", 1.0),
        ("pt", 1.0),
        ("in", 72.0),
        ("cm", 72.0 / 2.54),
        ("mm", 72.0 / 25.4),
        ("pc", 12.0),
        ("em", options.body_font_size_pt),
        ("ex", options.body_font_size_pt * 0.5),
    ];
    let parse_width_atom = |atom: &str| -> Option<f32> {
        let atom = atom.trim();
        if atom.is_empty() {
            return None;
        }
        reference_widths
            .iter()
            .find_map(|(name, reference_pt)| {
                if atom == *name {
                    return Some(*reference_pt);
                }
                atom.strip_suffix(name).and_then(|prefix| {
                    let factor = prefix.strip_suffix('*').unwrap_or(prefix);
                    let factor = if factor.is_empty() {
                        Some(1.0)
                    } else {
                        factor.parse::<f32>().ok()
                    }?;
                    let dimension = reference_pt * factor;
                    (dimension.is_finite() && dimension > 0.0).then_some(dimension)
                })
            })
            .or_else(|| {
                for (unit, multiplier) in unit_widths {
                    if let Some(number) = atom.strip_suffix(unit) {
                        let dimension = number.parse::<f32>().ok()? * multiplier;
                        if dimension.is_finite() && dimension > 0.0 {
                            return Some(dimension);
                        }
                    }
                }
                let dimension = atom.parse::<f32>().ok()?;
                (dimension.is_finite() && dimension > 0.0).then_some(dimension)
            })
    };
    let parse_width_expression = |expression: &str| -> Option<f32> {
        let mut expression = expression;
        if let Some(inner) = expression.strip_prefix("\\dimexpr") {
            expression = inner.strip_suffix("\\relax").unwrap_or(inner);
        }
        let mut total = 0.0;
        let mut sign = 1.0;
        let mut term_start = 0usize;
        let mut saw_operator = false;
        for (index, ch) in expression.char_indices() {
            if ch != '+' && ch != '-' {
                continue;
            }
            if index == term_start {
                sign = if ch == '-' { -1.0 } else { 1.0 };
                term_start = index + ch.len_utf8();
                continue;
            }
            total += sign * parse_width_atom(&expression[term_start..index])?;
            saw_operator = true;
            sign = if ch == '-' { -1.0 } else { 1.0 };
            term_start = index + ch.len_utf8();
        }
        if term_start >= expression.len() {
            return None;
        }
        total += sign * parse_width_atom(&expression[term_start..])?;
        (saw_operator && total.is_finite() && total > 0.0).then_some(total)
    };

    parse_width_atom(&normalized_width).or_else(|| parse_width_expression(&normalized_width))
}

struct PendingPage {
    ops: Vec<DrawOp>,
    source_spans: Vec<SourceSpan>,
    text: String,
    hash_input: String,
}

struct PendingImageRow {
    y: f32,
    used_width_pt: f32,
    height_pt: f32,
    gap_after_pt: f32,
    packable: bool,
}

#[derive(Clone)]
struct LogicalTextSegment {
    text: String,
    source: SourceProvenance,
    link_target: Option<String>,
    table_rule: bool,
    table_rule_trim_start_pt: Option<f32>,
    table_rule_trim_end_pt: Option<f32>,
    table_vertical_rule_offsets: Vec<(usize, u8)>,
}

struct LogicalTextRun {
    segments: Vec<LogicalTextSegment>,
    source: SourceProvenance,
    font: FontRequest,
    size_pt: f32,
    line_height_pt: f32,
    gap_after_pt: f32,
    first_line_indent_pt: f32,
    continuation_indent_pt: f32,
    right_indent_pt: f32,
    preserve_leading_whitespace: bool,
    full_width: bool,
}

struct LogicalImage {
    path: String,
    options: Option<String>,
    page_selection: Option<tex_render_model::GraphicPageSelection>,
    asset_format: Option<tex_render_model::GraphicAssetFormat>,
    asset_hash: Option<String>,
    asset_dimensions: Option<tex_render_model::GraphicAssetDimensions>,
    caption: Option<String>,
    caption_source: Option<SourceProvenance>,
    source: SourceProvenance,
    gap_after_pt: f32,
    full_width: bool,
}

struct LogicalContainer {
    name: String,
    width_pt: f32,
    height_pt: f32,
    alignment: Option<LayoutAlignment>,
    ops: Vec<DrawOp>,
    source_spans: Vec<SourceSpan>,
    source: SourceProvenance,
    content_hash: String,
}

enum LogicalItem {
    Text(LogicalTextRun),
    Image(LogicalImage),
    FullPageImage(LogicalImage),
    PageBreak,
    Container(LogicalContainer),
    ContainerRow(Vec<LogicalContainer>),
}

pub fn build_page_display_lists(
    document_ir: &DocumentIr,
    options: PageDisplayListOptions,
) -> Vec<PageDisplayList> {
    let column_count = options.column_count.max(1);
    let column_gap_pt = if options.column_gap_pt.is_finite() {
        options.column_gap_pt.max(0.0)
    } else {
        0.0
    };
    let page_content_width_pt = (options.page_width_pt - options.margin_left_pt * 2.0).max(1.0);
    let total_column_gap_pt = column_gap_pt * column_count.saturating_sub(1) as f32;
    let column_width_pt =
        ((page_content_width_pt - total_column_gap_pt).max(1.0) / column_count as f32).max(1.0);
    let body_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: FontSeries::Regular,
        shape: FontShape::Upright,
        size_pt: options.body_font_size_pt,
        role: FontRole::Body,
    };
    let heading_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: FontSeries::Bold,
        shape: FontShape::Upright,
        size_pt: options.heading_font_size_pt,
        role: FontRole::Heading,
    };
    let title_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: if options.title_font_bold {
            FontSeries::Bold
        } else {
            FontSeries::Regular
        },
        shape: FontShape::Upright,
        size_pt: options.title_font_size_pt,
        role: FontRole::Heading,
    };
    let author_date_font_size_pt = options
        .author_date_font_size_pt
        .unwrap_or(options.body_font_size_pt);
    let author_date_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: FontSeries::Regular,
        shape: FontShape::Upright,
        size_pt: author_date_font_size_pt,
        role: FontRole::Body,
    };
    let abstract_font_size_pt = options
        .abstract_font_size_pt
        .unwrap_or(options.body_font_size_pt);
    let abstract_line_height_pt = options
        .abstract_line_height_pt
        .unwrap_or(options.line_height_pt);
    let abstract_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: FontSeries::Regular,
        shape: FontShape::Upright,
        size_pt: abstract_font_size_pt,
        role: FontRole::Body,
    };
    let abstract_heading_font = FontRequest {
        family: FontFamilyRequest::Serif,
        series: if options.abstract_heading_bold {
            FontSeries::Bold
        } else {
            FontSeries::Regular
        },
        shape: FontShape::Upright,
        size_pt: abstract_font_size_pt,
        role: FontRole::Heading,
    };
    let math_font = FontRequest {
        family: FontFamilyRequest::Math,
        series: FontSeries::Regular,
        shape: FontShape::Upright,
        size_pt: options.body_font_size_pt,
        role: FontRole::Math,
    };
    let inline_segments = |content: &[InlineNode]| {
        let mut segments = Vec::new();
        for node in content {
            match node {
                InlineNode::Text { text, source } => {
                    segments.push(LogicalTextSegment {
                        text: text.clone(),
                        source: source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::Space { source } => {
                    segments.push(LogicalTextSegment {
                        text: " ".to_string(),
                        source: source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::LineBreak { source } => {
                    segments.push(LogicalTextSegment {
                        text: "\n".to_string(),
                        source: source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::Citation(citation) => {
                    segments.push(LogicalTextSegment {
                        text: citation.display_text.clone(),
                        source: citation.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::Reference(reference) => {
                    segments.push(LogicalTextSegment {
                        text: reference.display_text.clone(),
                        source: reference.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::Link(link) => {
                    segments.push(LogicalTextSegment {
                        text: link.display_text.clone(),
                        source: link.source.clone(),
                        link_target: Some(link.target.clone()),
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::InlineMath {
                    raw_source,
                    normalized_text,
                    source,
                } => {
                    segments.push(LogicalTextSegment {
                        text: normalized_text
                            .clone()
                            .unwrap_or_else(|| raw_source.clone()),
                        source: source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                InlineNode::RawFallback(fallback) => {
                    segments.push(LogicalTextSegment {
                        text: fallback
                            .normalized_visible_text
                            .clone()
                            .unwrap_or_else(|| fallback.source_excerpt.clone()),
                        source: fallback.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
            }
        }
        segments
    };

    let mut logical_items = Vec::new();
    for (block_index, block) in document_ir.blocks.iter().enumerate() {
        match block {
            IrBlock::TitleBlock(block) => {
                if let Some(title) = &block.title {
                    let source = block
                        .title_source
                        .clone()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: title.clone(),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: title_font.clone(),
                        size_pt: options.title_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: options.title_gap_pt.unwrap_or(options.block_gap_pt * 2.0),
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
                let author_gap_after =
                    if block.affiliations.is_empty() && block.correspondence.is_empty() {
                        if block.date.is_some() {
                            options.author_date_gap_pt
                        } else if block.keywords.is_empty() && block.pacs.is_empty() {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                if options.authors_on_single_line && !block.authors.is_empty() {
                    let mut segments = Vec::new();
                    for (index, author) in block.authors.iter().enumerate() {
                        let source = block
                            .author_sources
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| block.source.clone());
                        if index > 0 {
                            segments.push(LogicalTextSegment {
                                text: "            ".to_string(),
                                source: block.source.clone(),
                                link_target: None,
                                table_rule: false,
                                table_rule_trim_start_pt: None,
                                table_rule_trim_end_pt: None,
                                table_vertical_rule_offsets: Vec::new(),
                            });
                        }
                        segments.push(LogicalTextSegment {
                            text: author.clone(),
                            source,
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        });
                    }
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments,
                        source: block.source.clone(),
                        font: author_date_font.clone(),
                        size_pt: author_date_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: author_gap_after,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                } else {
                    for (index, author) in block.authors.iter().enumerate() {
                        let source = block
                            .author_sources
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| block.source.clone());
                        logical_items.push(LogicalItem::Text(LogicalTextRun {
                            segments: vec![LogicalTextSegment {
                                text: author.clone(),
                                source: source.clone(),
                                link_target: None,
                                table_rule: false,
                                table_rule_trim_start_pt: None,
                                table_rule_trim_end_pt: None,
                                table_vertical_rule_offsets: Vec::new(),
                            }],
                            source,
                            font: author_date_font.clone(),
                            size_pt: author_date_font_size_pt,
                            line_height_pt: options.line_height_pt,
                            gap_after_pt: if index + 1 == block.authors.len() {
                                author_gap_after
                            } else {
                                0.0
                            },
                            first_line_indent_pt: 0.0,
                            continuation_indent_pt: 0.0,
                            right_indent_pt: 0.0,
                            preserve_leading_whitespace: false,
                            full_width: true,
                        }));
                    }
                }
                for (index, affiliation) in block.affiliations.iter().enumerate() {
                    let source = block
                        .affiliation_sources
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: affiliation.clone(),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if index + 1 == block.affiliations.len()
                            && block.correspondence.is_empty()
                            && block.date.is_none()
                            && block.keywords.is_empty()
                            && block.pacs.is_empty()
                        {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
                for (index, correspondence) in block.correspondence.iter().enumerate() {
                    let source = block
                        .correspondence_sources
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: correspondence.clone(),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if index + 1 == block.correspondence.len()
                            && block.date.is_none()
                            && block.keywords.is_empty()
                            && block.pacs.is_empty()
                        {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
                if let Some(date) = &block.date {
                    let source = block
                        .date_source
                        .clone()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: date.clone(),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: author_date_font.clone(),
                        size_pt: author_date_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if block.keywords.is_empty() && block.pacs.is_empty() {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
                for (index, keyword) in block.keywords.iter().enumerate() {
                    let source = block
                        .keyword_sources
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: format!("Keywords: {keyword}"),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if block.pacs.is_empty() && index + 1 == block.keywords.len()
                        {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
                for (index, pacs) in block.pacs.iter().enumerate() {
                    let source = block
                        .pacs_sources
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: format!("PACS: {pacs}"),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if index + 1 == block.pacs.len() {
                            options.front_matter_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: true,
                    }));
                }
            }
            IrBlock::Abstract(block) => {
                let abstract_heading_indent_pt = if options.abstract_heading_centered {
                    let heading_width_pt =
                        text_advance_pt("Abstract", &abstract_heading_font, abstract_font_size_pt);
                    ((column_width_pt - heading_width_pt) / 2.0).max(0.0)
                } else {
                    0.0
                };
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: vec![LogicalTextSegment {
                        text: "Abstract".to_string(),
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    }],
                    source: block.source.clone(),
                    font: abstract_heading_font.clone(),
                    size_pt: abstract_font_size_pt,
                    line_height_pt: abstract_line_height_pt,
                    gap_after_pt: options.abstract_heading_gap_pt,
                    first_line_indent_pt: abstract_heading_indent_pt,
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: abstract_font.clone(),
                    size_pt: abstract_font_size_pt,
                    line_height_pt: abstract_line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: options.abstract_indent_pt
                        + options.abstract_first_line_indent_pt,
                    continuation_indent_pt: options.abstract_indent_pt,
                    right_indent_pt: options.abstract_indent_pt,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
            IrBlock::Heading(block) => {
                let mut segments = Vec::new();
                if let Some(number) = &block.number {
                    segments.push(LogicalTextSegment {
                        text: number.clone(),
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                    segments.push(LogicalTextSegment {
                        text: " ".to_string(),
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                segments.extend(inline_segments(&block.content));
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments,
                    source: block.source.clone(),
                    font: heading_font.clone(),
                    size_pt: options.heading_font_size_pt,
                    line_height_pt: options.line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
            IrBlock::Paragraph(block) => {
                let follows_heading = block_index > 0
                    && matches!(document_ir.blocks[block_index - 1], IrBlock::Heading(_));
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    line_height_pt: options.line_height_pt,
                    gap_after_pt: options.paragraph_gap_pt.unwrap_or(options.block_gap_pt),
                    first_line_indent_pt: if follows_heading {
                        0.0
                    } else {
                        options.paragraph_first_line_indent_pt
                    },
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
            IrBlock::Environment(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    line_height_pt: options.line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
            IrBlock::LayoutContainer(block) => {
                let width_pt =
                    parse_table_width_spec_pt(&block.width_spec, column_width_pt, &options)
                        .unwrap_or(column_width_pt)
                        .clamp(1.0, column_width_pt);
                let declared_height_pt = block.height_spec.as_deref().and_then(|height_spec| {
                    parse_table_width_spec_pt(
                        height_spec,
                        (options.page_height_pt - options.margin_top_pt - options.margin_bottom_pt)
                            .max(1.0),
                        &options,
                    )
                });
                let nested_options = PageDisplayListOptions {
                    page_width_pt: width_pt,
                    page_height_pt: 100_000.0,
                    margin_left_pt: 0.0,
                    margin_top_pt: 0.0,
                    margin_bottom_pt: 0.0,
                    column_count: 1,
                    column_gap_pt: 0.0,
                    ..options.clone()
                };
                let nested_pages = build_page_display_lists(
                    &DocumentIr::new(block.children.clone()),
                    nested_options,
                );
                let mut ops = Vec::new();
                let mut source_spans = Vec::new();
                let mut content_height = 0.0_f32;
                let mut hash_input = format!(
                    "layout-container:{}:{}:{:?}:{:?}:{:?}",
                    block.name,
                    block.width_spec,
                    block.alignment,
                    block.height_spec,
                    block.inner_alignment
                );
                for page in nested_pages {
                    let page_height = page.ops.iter().fold(0.0_f32, |height, op| {
                        let bottom = match op {
                            DrawOp::Save | DrawOp::Restore => 0.0,
                            DrawOp::ClipRect(rect) | DrawOp::Rule(rect) => rect.y + rect.height,
                            DrawOp::TextRun(run) => run.origin.y + options.line_height_pt,
                            DrawOp::Image(image) => image.rect.y + image.rect.height,
                            DrawOp::LinkAnnotation(link) => link.rect.y + link.rect.height,
                            DrawOp::NamedDestination(destination) => destination.point.y,
                        };
                        height.max(bottom)
                    });
                    for mut op in page.ops {
                        op.translate(0.0, content_height);
                        ops.push(op);
                    }
                    for span in page.source_spans {
                        if !source_spans.contains(&span) {
                            source_spans.push(span);
                        }
                    }
                    hash_input.push('\u{1f}');
                    hash_input.push_str(&page.content_hash);
                    if page_height > 0.0 {
                        content_height += page_height + options.block_gap_pt;
                    }
                }
                if content_height > 0.0 {
                    content_height -= options.block_gap_pt;
                }
                let height_pt = declared_height_pt
                    .unwrap_or(content_height)
                    .max(content_height)
                    .max(options.line_height_pt);
                let inner_offset_y = match block.inner_alignment {
                    Some(LayoutAlignment::Bottom) => height_pt - content_height,
                    Some(LayoutAlignment::Center) => (height_pt - content_height) / 2.0,
                    Some(LayoutAlignment::Top | LayoutAlignment::Stretch) | None => 0.0,
                };
                if inner_offset_y > 0.0 {
                    for op in &mut ops {
                        op.translate(0.0, inner_offset_y);
                    }
                }
                logical_items.push(LogicalItem::Container(LogicalContainer {
                    name: block.name.clone(),
                    width_pt,
                    height_pt,
                    alignment: block.alignment,
                    ops,
                    source_spans,
                    source: block.source.clone(),
                    content_hash: blake3::hash(hash_input.as_bytes()).to_hex().to_string(),
                }));
            }
            IrBlock::List(block) => {
                for (index, item) in block.items.iter().enumerate() {
                    let mut segments = vec![LogicalTextSegment {
                        text: format!("{} ", item.marker),
                        source: item.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    }];
                    segments.extend(inline_segments(&item.content));
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments,
                        source: item.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if index + 1 == block.items.len() {
                            options.block_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.list_continuation_indent_pt,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: false,
                    }));
                }
                if block.items.is_empty() {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: Vec::new(),
                        source: block.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.list_continuation_indent_pt,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: false,
                    }));
                }
            }
            IrBlock::DisplayMath(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: vec![LogicalTextSegment {
                        text: block
                            .normalized_text
                            .clone()
                            .unwrap_or_else(|| block.raw_source.clone()),
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    }],
                    source: block.source.clone(),
                    font: math_font.clone(),
                    size_pt: options.body_font_size_pt,
                    line_height_pt: options.line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
            IrBlock::Bibliography(block) => {
                let BibliographyBlock { items, source } = block;
                for (index, item) in items.iter().enumerate() {
                    let text = if let Some(label) = &item.label {
                        format!("[{label}] {}", item.content)
                    } else {
                        item.content.clone()
                    };
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text,
                            source: item.source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source: item.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: if index + 1 == items.len() {
                            options.block_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.bibliography_continuation_indent_pt,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: false,
                    }));
                }
                if items.is_empty() {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: Vec::new(),
                        source: source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.bibliography_continuation_indent_pt,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width: false,
                    }));
                }
            }
            graphic @ (IrBlock::Graphic(_) | IrBlock::FullWidthGraphic(_)) => {
                let (block, full_width) = match graphic {
                    IrBlock::Graphic(block) => (block, false),
                    IrBlock::FullWidthGraphic(block) => (block, true),
                    _ => unreachable!(),
                };
                logical_items.push(LogicalItem::Image(LogicalImage {
                    path: block.path.clone(),
                    options: block.options.clone(),
                    page_selection: block.page_selection.clone(),
                    asset_format: block.asset_format,
                    asset_hash: block.asset_hash.clone(),
                    asset_dimensions: block.asset_dimensions,
                    caption: None,
                    caption_source: None,
                    source: block.source.clone(),
                    gap_after_pt: if block.caption.is_some() {
                        0.0
                    } else {
                        options.block_gap_pt
                    },
                    full_width,
                }));
                if let Some(caption) = &block.caption {
                    let source = block
                        .caption_source
                        .clone()
                        .unwrap_or_else(|| block.source.clone());
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: vec![LogicalTextSegment {
                            text: caption.clone(),
                            source: source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        line_height_pt: options.line_height_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                        right_indent_pt: 0.0,
                        preserve_leading_whitespace: false,
                        full_width,
                    }));
                }
            }
            IrBlock::IncludedPdfPage(block) => {
                logical_items.push(LogicalItem::FullPageImage(LogicalImage {
                    path: block.path.clone(),
                    options: block.options.clone(),
                    page_selection: block.page_selection.clone(),
                    asset_format: block.asset_format,
                    asset_hash: block.asset_hash.clone(),
                    asset_dimensions: block.asset_dimensions,
                    caption: None,
                    caption_source: None,
                    source: block.source.clone(),
                    gap_after_pt: 0.0,
                    full_width: true,
                }));
            }
            IrBlock::PageBreak(_) => logical_items.push(LogicalItem::PageBreak),
            table @ (IrBlock::Table(_) | IrBlock::FullWidthTable(_)) => {
                let (block, full_width) = match table {
                    IrBlock::Table(block) => (block, false),
                    IrBlock::FullWidthTable(block) => (block, true),
                    _ => unreachable!(),
                };
                struct RenderedTableCell {
                    text: String,
                    column_span: usize,
                    column_index: usize,
                    alignment: Option<TableColumnAlignment>,
                    rule_before_count: u8,
                    rule_after_count: u8,
                }

                let row_column_count = |row: &TableRow| {
                    row.cells
                        .iter()
                        .map(|cell| cell.column_span.unwrap_or(1).max(1))
                        .sum::<usize>()
                };
                let expected_column_count = block
                    .columns
                    .len()
                    .max(block.rows.iter().map(row_column_count).max().unwrap_or(0));
                let mut active_row_spans = Vec::<usize>::new();
                let mut rendered_rows = Vec::new();
                for row in &block.rows {
                    let active_column_count = active_row_spans
                        .iter()
                        .take(expected_column_count)
                        .filter(|remaining| **remaining > 0)
                        .count();
                    let skip_active_columns = active_column_count > 0
                        && row_column_count(row) + active_column_count <= expected_column_count;
                    let mut column_index = 0usize;
                    let mut rendered_cells = Vec::new();
                    for cell in &row.cells {
                        if skip_active_columns {
                            while active_row_spans.get(column_index).copied().unwrap_or(0) > 0 {
                                rendered_cells.push(RenderedTableCell {
                                    text: String::new(),
                                    column_span: 1,
                                    column_index,
                                    alignment: None,
                                    rule_before_count: 0,
                                    rule_after_count: 0,
                                });
                                column_index += 1;
                            }
                        }
                        let column_span = cell.column_span.unwrap_or(1).max(1);
                        let row_span = cell.row_span.filter(|row_span| *row_span > 1);
                        while column_index + column_span > active_row_spans.len() {
                            active_row_spans.push(0);
                        }
                        if let Some(row_span) = row_span {
                            for column in column_index..column_index + column_span {
                                active_row_spans[column] = active_row_spans[column].max(row_span);
                            }
                        }
                        let mut text = String::new();
                        if let Some(prefix) = block
                            .columns
                            .get(column_index)
                            .and_then(|column| column.cell_prefix.as_deref())
                        {
                            text.push_str(prefix);
                        }
                        if let Some(prefix) = &cell.cell_prefix {
                            text.push_str(prefix);
                        }
                        text.push_str(&cell.text);
                        if let Some(suffix) = &cell.cell_suffix {
                            text.push_str(suffix);
                        }
                        if let Some(suffix) = block
                            .columns
                            .get(column_index)
                            .and_then(|column| column.cell_suffix.as_deref())
                        {
                            text.push_str(suffix);
                        }
                        rendered_cells.push(RenderedTableCell {
                            text,
                            column_span,
                            column_index,
                            alignment: cell.alignment,
                            rule_before_count: cell.rule_before_count,
                            rule_after_count: cell.rule_after_count,
                        });
                        column_index += column_span;
                    }
                    for remaining in &mut active_row_spans {
                        *remaining = remaining.saturating_sub(1);
                    }
                    rendered_rows.push(rendered_cells);
                }
                let mut segments = Vec::new();
                let mut column_widths = Vec::new();
                for row in &rendered_rows {
                    for cell in row {
                        let column_index = cell.column_index;
                        let column_span = cell.column_span;
                        while column_index + column_span > column_widths.len() {
                            column_widths.push(0usize);
                        }
                        if column_span == 1 {
                            column_widths[column_index] =
                                column_widths[column_index].max(cell.text.chars().count());
                        }
                    }
                }
                let mut decimal_left_widths = vec![0usize; column_widths.len()];
                let mut decimal_right_widths = vec![0usize; column_widths.len()];
                for row in &rendered_rows {
                    for cell in row {
                        let column_index = cell.column_index;
                        if cell.column_span != 1 {
                            continue;
                        }
                        let Some(column) = block.columns.get(column_index) else {
                            continue;
                        };
                        if !matches!(column.alignment, TableColumnAlignment::Decimal) {
                            continue;
                        }
                        while column_index >= decimal_left_widths.len() {
                            decimal_left_widths.push(0usize);
                            decimal_right_widths.push(0usize);
                        }
                        let (left_width, right_width) = if let Some(dot_index) = cell.text.find('.')
                        {
                            (
                                cell.text[..dot_index].chars().count(),
                                cell.text[dot_index + 1..].chars().count(),
                            )
                        } else {
                            (cell.text.chars().count(), 0)
                        };
                        decimal_left_widths[column_index] =
                            decimal_left_widths[column_index].max(left_width);
                        decimal_right_widths[column_index] =
                            decimal_right_widths[column_index].max(right_width);
                    }
                }
                for column_index in 0..column_widths.len() {
                    if !block.columns.get(column_index).is_some_and(|column| {
                        matches!(column.alignment, TableColumnAlignment::Decimal)
                    }) {
                        continue;
                    }
                    let right_width = decimal_right_widths.get(column_index).copied().unwrap_or(0);
                    let decimal_width = decimal_left_widths.get(column_index).copied().unwrap_or(0)
                        + if right_width > 0 { 1 + right_width } else { 0 };
                    column_widths[column_index] = column_widths[column_index].max(decimal_width);
                }
                let column_rule_before_count = |column_index: usize| -> u8 {
                    block
                        .columns
                        .get(column_index)
                        .map(|column| {
                            if column.rule_before_count > 0 {
                                column.rule_before_count
                            } else if column.rule_before {
                                1
                            } else {
                                0
                            }
                        })
                        .unwrap_or(0)
                };
                let column_rule_after_count = |column_index: usize| -> u8 {
                    block
                        .columns
                        .get(column_index)
                        .map(|column| {
                            if column.rule_after_count > 0 {
                                column.rule_after_count
                            } else if column.rule_after {
                                1
                            } else {
                                0
                            }
                        })
                        .unwrap_or(0)
                };
                let base_separator_width =
                    |previous_column_index: usize, column_index: usize| -> usize {
                        let rule_count = column_rule_after_count(previous_column_index)
                            .max(column_rule_before_count(column_index));
                        if rule_count > 0 {
                            3
                        } else if let Some(separator) = block
                            .columns
                            .get(previous_column_index)
                            .and_then(|column| column.separator_after.as_deref())
                        {
                            separator.chars().count()
                        } else {
                            3
                        }
                    };
                let base_spanned_separator_width =
                    |start_column: usize, end_column: usize| -> usize {
                        (start_column + 1..end_column)
                            .map(|column| base_separator_width(column - 1, column))
                            .sum()
                    };
                for row in &rendered_rows {
                    for cell in row {
                        let column_index = cell.column_index;
                        let column_span = cell.column_span;
                        let end_column = (column_index + column_span).min(column_widths.len());
                        let mut spanned_width = column_widths[column_index..end_column]
                            .iter()
                            .sum::<usize>();
                        spanned_width += base_spanned_separator_width(column_index, end_column);
                        let text_width = cell.text.chars().count();
                        if column_span > 1 && text_width > spanned_width && end_column > 0 {
                            column_widths[end_column - 1] += text_width - spanned_width;
                        }
                    }
                }
                let table_glyph_width_pt = (options.body_font_size_pt * 0.6).max(1.0);
                let table_area_width_pt = if full_width {
                    page_content_width_pt
                } else {
                    column_width_pt
                };
                for (column_index, column) in block.columns.iter().enumerate() {
                    if let Some(width_pt_milli) = column.width_pt_milli {
                        while column_index >= column_widths.len() {
                            column_widths.push(0usize);
                        }
                        let min_chars = (((width_pt_milli as f32 / 1000.0) / table_glyph_width_pt)
                            - 0.001)
                            .ceil()
                            .max(1.0) as usize;
                        column_widths[column_index] = column_widths[column_index].max(min_chars);
                    }
                }
                let mut separator_extra_widths =
                    vec![0usize; column_widths.len().saturating_sub(1)];
                let mut requested_table_width_pt = None;
                if let Some(width_spec) = block.width_spec.as_deref() {
                    if let Some(table_width_pt) =
                        parse_table_width_spec_pt(width_spec, table_area_width_pt, &options)
                        && !column_widths.is_empty()
                    {
                        let table_width_pt = table_width_pt.clamp(1.0, table_area_width_pt);
                        requested_table_width_pt = Some(table_width_pt);
                        let line_char_budget = options.max_chars_per_line.max(1).min(
                            ((table_area_width_pt / table_glyph_width_pt).floor() as usize).max(1),
                        );
                        let current_chars = column_widths.iter().sum::<usize>()
                            + base_spanned_separator_width(0, column_widths.len());
                        let target_chars = ((table_width_pt / table_glyph_width_pt).floor()
                            as usize)
                            .max(1)
                            .min(line_char_budget);
                        if target_chars > current_chars {
                            let stretch_columns = block
                                .columns
                                .iter()
                                .enumerate()
                                .filter_map(|(index, column)| {
                                    (index < column_widths.len()
                                        && matches!(
                                            column.alignment,
                                            TableColumnAlignment::Paragraph
                                        )
                                        && column.width_pt_milli.is_none())
                                    .then_some(index)
                                })
                                .collect::<Vec<_>>();
                            let extra_chars = target_chars - current_chars;
                            if !stretch_columns.is_empty() {
                                let base_extra = extra_chars / stretch_columns.len();
                                let mut remainder = extra_chars % stretch_columns.len();
                                for column_index in stretch_columns {
                                    column_widths[column_index] += base_extra;
                                    if remainder > 0 {
                                        column_widths[column_index] += 1;
                                        remainder -= 1;
                                    }
                                }
                            } else if !separator_extra_widths.is_empty() {
                                let base_extra = extra_chars / separator_extra_widths.len();
                                let mut remainder = extra_chars % separator_extra_widths.len();
                                for extra_width in &mut separator_extra_widths {
                                    *extra_width += base_extra;
                                    if remainder > 0 {
                                        *extra_width += 1;
                                        remainder -= 1;
                                    }
                                }
                            } else if let Some(last_width) = column_widths.last_mut() {
                                *last_width += extra_chars;
                            }
                        }
                    }
                }
                let separator_width =
                    |previous_column_index: usize, column_index: usize| -> usize {
                        let extra_width = if column_index == previous_column_index + 1 {
                            separator_extra_widths
                                .get(previous_column_index)
                                .copied()
                                .unwrap_or(0)
                        } else {
                            0
                        };
                        base_separator_width(previous_column_index, column_index) + extra_width
                    };
                let spanned_separator_width = |start_column: usize, end_column: usize| -> usize {
                    (start_column + 1..end_column)
                        .map(|column| separator_width(column - 1, column))
                        .sum()
                };
                let rule_width = column_widths.iter().sum::<usize>()
                    + spanned_separator_width(0, column_widths.len());
                let natural_table_width_pt = rule_width.max(1) as f32 * table_glyph_width_pt;
                let fitted_table_width_pt = requested_table_width_pt
                    .unwrap_or(table_area_width_pt)
                    .min(table_area_width_pt);
                let table_font_scale = if natural_table_width_pt > fitted_table_width_pt {
                    (fitted_table_width_pt / natural_table_width_pt).clamp(0.1, 1.0)
                } else {
                    1.0
                };
                let table_font_size_pt = options.body_font_size_pt * table_font_scale;
                let table_line_height_pt = options.line_height_pt * table_font_scale;
                let table_side_indent_pt = if full_width {
                    0.0
                } else if let Some(width) = requested_table_width_pt {
                    ((table_area_width_pt - width) / 2.0).max(0.0)
                } else if block.caption.is_none() {
                    options.paragraph_first_line_indent_pt
                } else {
                    0.0
                };
                let rule_text = "-".repeat(rule_width.max(3));
                let partial_rule_text = |span: &TableRuleSpan| {
                    if column_widths.is_empty() {
                        return rule_text.clone();
                    }
                    let start_column = span.start_column.min(column_widths.len().saturating_sub(1));
                    let end_column = span.end_column.min(column_widths.len().saturating_sub(1));
                    if end_column < start_column {
                        return rule_text.clone();
                    }
                    let mut start_offset = 0usize;
                    for column in 0..start_column {
                        start_offset += column_widths[column] + separator_width(column, column + 1);
                    }
                    let mut end_offset = start_offset;
                    for column in start_column..=end_column {
                        end_offset += column_widths[column];
                        if column < end_column {
                            end_offset += separator_width(column, column + 1);
                        }
                    }
                    let mut chars = vec![' '; rule_width.max(3)];
                    for index in start_offset..end_offset.min(chars.len()) {
                        chars[index] = '-';
                    }
                    chars.into_iter().collect::<String>()
                };
                let partial_rule_trim_pt = |enabled: bool, trim_pt_milli: Option<u32>| {
                    enabled.then(|| {
                        trim_pt_milli
                            .map(|trim| trim as f32 / 1000.0)
                            .unwrap_or(table_glyph_width_pt)
                    })
                };
                let table_vertical_rule_offsets = || {
                    let mut offsets = Vec::new();
                    if column_widths.is_empty() {
                        return offsets;
                    }

                    let left_rule_count = column_rule_before_count(0);
                    if left_rule_count > 0 {
                        offsets.push((0, left_rule_count));
                    }

                    let mut char_offset = 0usize;
                    for previous_column_index in 0..column_widths.len().saturating_sub(1) {
                        char_offset += column_widths[previous_column_index];
                        let column_index = previous_column_index + 1;
                        let rule_count = column_rule_after_count(previous_column_index)
                            .max(column_rule_before_count(column_index));
                        let separator_width = separator_width(previous_column_index, column_index);
                        if rule_count > 0 {
                            offsets.push((char_offset + separator_width / 2, rule_count));
                        }
                        char_offset += separator_width;
                    }

                    let last_column_index = column_widths.len() - 1;
                    char_offset += column_widths[last_column_index];
                    let right_rule_count = column_rule_after_count(last_column_index);
                    if right_rule_count > 0 {
                        offsets.push((char_offset, right_rule_count));
                    }
                    offsets
                };
                let table_vertical_rule_offsets_for_partial_rule =
                    |rule_text: &str, rule: &TableRuleSpan| {
                        let rule_chars = rule_text.chars().collect::<Vec<_>>();
                        let first_dash_offset = rule_chars.iter().position(|ch| *ch == '-');
                        let after_last_dash_offset = rule_chars
                            .iter()
                            .rposition(|ch| *ch == '-')
                            .map(|offset| offset + 1);
                        table_vertical_rule_offsets()
                            .into_iter()
                            .filter(|(offset, _)| {
                                let touches_rule =
                                    rule_chars.get(*offset).is_some_and(|ch| *ch == '-')
                                        || offset
                                            .checked_sub(1)
                                            .and_then(|previous| rule_chars.get(previous))
                                            .is_some_and(|ch| *ch == '-');
                                if !touches_rule {
                                    return false;
                                }
                                if rule.trim_start
                                    && first_dash_offset.is_some_and(|first| *offset <= first)
                                {
                                    return false;
                                }
                                if rule.trim_end
                                    && after_last_dash_offset.is_some_and(|end| *offset >= end)
                                {
                                    return false;
                                }
                                true
                            })
                            .collect::<Vec<_>>()
                    };
                if let Some(caption) = &block.caption {
                    let source = block
                        .caption_source
                        .clone()
                        .unwrap_or_else(|| block.source.clone());
                    segments.push(LogicalTextSegment {
                        text: caption.clone(),
                        source,
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    });
                }
                for (row, rendered_cells) in block.rows.iter().zip(&rendered_rows) {
                    if row.rule_above {
                        if !segments.is_empty() {
                            segments.push(LogicalTextSegment {
                                text: "\n".to_string(),
                                source: block.source.clone(),
                                link_target: None,
                                table_rule: false,
                                table_rule_trim_start_pt: None,
                                table_rule_trim_end_pt: None,
                                table_vertical_rule_offsets: Vec::new(),
                            });
                        }
                        segments.push(LogicalTextSegment {
                            text: rule_text.clone(),
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: true,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: table_vertical_rule_offsets(),
                        });
                    }
                    for rule in &row.partial_rules_above {
                        if !segments.is_empty() {
                            segments.push(LogicalTextSegment {
                                text: "\n".to_string(),
                                source: block.source.clone(),
                                link_target: None,
                                table_rule: false,
                                table_rule_trim_start_pt: None,
                                table_rule_trim_end_pt: None,
                                table_vertical_rule_offsets: Vec::new(),
                            });
                        }
                        let rule_text = partial_rule_text(rule);
                        let table_vertical_rule_offsets =
                            table_vertical_rule_offsets_for_partial_rule(&rule_text, rule);
                        segments.push(LogicalTextSegment {
                            text: rule_text,
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: true,
                            table_rule_trim_start_pt: partial_rule_trim_pt(
                                rule.trim_start,
                                rule.trim_start_pt_milli,
                            ),
                            table_rule_trim_end_pt: partial_rule_trim_pt(
                                rule.trim_end,
                                rule.trim_end_pt_milli,
                            ),
                            table_vertical_rule_offsets,
                        });
                    }
                    if !segments.is_empty() {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        });
                    }
                    let mut row_text = String::new();
                    let mut row_vertical_rule_offsets = Vec::new();
                    let mut column_index = 0usize;
                    let left_rule_count = column_rule_before_count(0).max(
                        rendered_cells
                            .first()
                            .map(|cell| cell.rule_before_count)
                            .unwrap_or(0),
                    );
                    if left_rule_count > 0 {
                        row_vertical_rule_offsets.push((0, left_rule_count));
                    }
                    for (cell_index, cell) in rendered_cells.iter().enumerate() {
                        column_index = cell.column_index;
                        if cell_index > 0 {
                            let separator_start = row_text.chars().count();
                            let previous_column_index = column_index.saturating_sub(1);
                            let previous_cell_rule_after = rendered_cells
                                .get(cell_index.saturating_sub(1))
                                .map(|cell| cell.rule_after_count)
                                .unwrap_or(0);
                            let rule_count = column_rule_after_count(previous_column_index)
                                .max(column_rule_before_count(column_index))
                                .max(previous_cell_rule_after)
                                .max(cell.rule_before_count);
                            let target_separator_width =
                                separator_width(previous_column_index, column_index);
                            if rule_count > 0 {
                                for _ in 0..target_separator_width {
                                    row_text.push(' ');
                                }
                            } else if let Some(separator) = block
                                .columns
                                .get(previous_column_index)
                                .and_then(|column| column.separator_after.as_deref())
                            {
                                row_text.push_str(separator);
                                for _ in separator.chars().count()..target_separator_width {
                                    row_text.push(' ');
                                }
                            } else {
                                row_text.push_str(" | ");
                                for _ in 3..target_separator_width {
                                    row_text.push(' ');
                                }
                            }
                            if rule_count > 0 {
                                row_vertical_rule_offsets.push((
                                    separator_start + target_separator_width / 2,
                                    rule_count,
                                ));
                            }
                        }
                        let column_span = cell.column_span;
                        let end_column = (column_index + column_span).min(column_widths.len());
                        let mut spanned_width = column_widths[column_index..end_column]
                            .iter()
                            .sum::<usize>();
                        spanned_width += spanned_separator_width(column_index, end_column);
                        let text_width = cell.text.chars().count();
                        let available_padding = spanned_width.saturating_sub(text_width);
                        let alignment = cell
                            .alignment
                            .or_else(|| {
                                block
                                    .columns
                                    .get(column_index)
                                    .map(|column| column.alignment)
                            })
                            .unwrap_or(TableColumnAlignment::Left);
                        let (left_padding, right_padding) = if column_span == 1
                            && matches!(alignment, TableColumnAlignment::Decimal)
                        {
                            let (left_width, _) = if let Some(dot_index) = cell.text.find('.') {
                                (
                                    cell.text[..dot_index].chars().count(),
                                    cell.text[dot_index + 1..].chars().count(),
                                )
                            } else {
                                (text_width, 0)
                            };
                            let left_padding = decimal_left_widths
                                .get(column_index)
                                .copied()
                                .unwrap_or(0)
                                .saturating_sub(left_width);
                            (
                                left_padding,
                                spanned_width.saturating_sub(left_padding + text_width),
                            )
                        } else if matches!(alignment, TableColumnAlignment::Right) {
                            (available_padding, 0)
                        } else if matches!(alignment, TableColumnAlignment::Center) {
                            (
                                available_padding / 2,
                                available_padding - available_padding / 2,
                            )
                        } else {
                            (0, available_padding)
                        };
                        for _ in 0..left_padding {
                            row_text.push(' ');
                        }
                        row_text.push_str(&cell.text);
                        if cell_index + 1 < rendered_cells.len()
                            || block.width_spec.is_some()
                            || matches!(alignment, TableColumnAlignment::Decimal)
                        {
                            for _ in 0..right_padding {
                                row_text.push(' ');
                            }
                        }
                        column_index += column_span;
                    }
                    let right_rule_count = column_rule_after_count(column_index.saturating_sub(1))
                        .max(
                            rendered_cells
                                .last()
                                .map(|cell| cell.rule_after_count)
                                .unwrap_or(0),
                        );
                    if right_rule_count > 0 {
                        row_vertical_rule_offsets
                            .push((row_text.chars().count(), right_rule_count));
                    }
                    row_vertical_rule_offsets.sort_unstable_by_key(|(offset, _)| *offset);
                    let mut deduped_vertical_rule_offsets: Vec<(usize, u8)> = Vec::new();
                    for (offset, count) in row_vertical_rule_offsets {
                        if let Some((last_offset, last_count)) =
                            deduped_vertical_rule_offsets.last_mut()
                            && *last_offset == offset
                        {
                            *last_count = (*last_count).max(count);
                        } else {
                            deduped_vertical_rule_offsets.push((offset, count));
                        }
                    }
                    segments.push(LogicalTextSegment {
                        text: row_text,
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: deduped_vertical_rule_offsets,
                    });
                    if row.rule_below {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        });
                        segments.push(LogicalTextSegment {
                            text: rule_text.clone(),
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: true,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: table_vertical_rule_offsets(),
                        });
                    }
                    for rule in &row.partial_rules_below {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: false,
                            table_rule_trim_start_pt: None,
                            table_rule_trim_end_pt: None,
                            table_vertical_rule_offsets: Vec::new(),
                        });
                        let rule_text = partial_rule_text(rule);
                        let table_vertical_rule_offsets =
                            table_vertical_rule_offsets_for_partial_rule(&rule_text, rule);
                        segments.push(LogicalTextSegment {
                            text: rule_text,
                            source: block.source.clone(),
                            link_target: None,
                            table_rule: true,
                            table_rule_trim_start_pt: partial_rule_trim_pt(
                                rule.trim_start,
                                rule.trim_start_pt_milli,
                            ),
                            table_rule_trim_end_pt: partial_rule_trim_pt(
                                rule.trim_end,
                                rule.trim_end_pt_milli,
                            ),
                            table_vertical_rule_offsets,
                        });
                    }
                }
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments,
                    source: block.source.clone(),
                    font: FontRequest {
                        family: FontFamilyRequest::Mono,
                        series: FontSeries::Regular,
                        shape: FontShape::Upright,
                        size_pt: table_font_size_pt,
                        role: FontRole::Mono,
                    },
                    size_pt: table_font_size_pt,
                    line_height_pt: table_line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: table_side_indent_pt,
                    continuation_indent_pt: table_side_indent_pt,
                    right_indent_pt: table_side_indent_pt,
                    preserve_leading_whitespace: true,
                    full_width,
                }));
            }
            IrBlock::RawFallback(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: vec![LogicalTextSegment {
                        text: block
                            .normalized_visible_text
                            .clone()
                            .unwrap_or_else(|| block.source_excerpt.clone()),
                        source: block.source.clone(),
                        link_target: None,
                        table_rule: false,
                        table_rule_trim_start_pt: None,
                        table_rule_trim_end_pt: None,
                        table_vertical_rule_offsets: Vec::new(),
                    }],
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    line_height_pt: options.line_height_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                    right_indent_pt: 0.0,
                    preserve_leading_whitespace: false,
                    full_width: false,
                }));
            }
        }
    }

    let container_gap_pt = 4.0;
    let mut grouped_logical_items = Vec::new();
    let mut container_row = Vec::new();
    let mut container_row_width = 0.0_f32;
    for logical in logical_items {
        match logical {
            LogicalItem::Container(container) => {
                let next_width = if container_row.is_empty() {
                    container.width_pt
                } else {
                    container_row_width + container_gap_pt + container.width_pt
                };
                if !container_row.is_empty() && next_width > column_width_pt + 0.01 {
                    grouped_logical_items.push(LogicalItem::ContainerRow(std::mem::take(
                        &mut container_row,
                    )));
                    container_row_width = 0.0;
                }
                if !container_row.is_empty() {
                    container_row_width += container_gap_pt;
                }
                container_row_width += container.width_pt;
                container_row.push(container);
            }
            logical => {
                if !container_row.is_empty() {
                    grouped_logical_items.push(LogicalItem::ContainerRow(std::mem::take(
                        &mut container_row,
                    )));
                    container_row_width = 0.0;
                }
                grouped_logical_items.push(logical);
            }
        }
    }
    if !container_row.is_empty() {
        grouped_logical_items.push(LogicalItem::ContainerRow(container_row));
    }
    let logical_items = grouped_logical_items;

    let mut pages = Vec::new();
    let mut pending_labels = document_ir.labels.clone();
    pending_labels.sort_by(
        |left, right| match (&left.source.primary, &right.source.primary) {
            (ProvenanceSpan::File(left), ProvenanceSpan::File(right)) => left
                .path
                .cmp(&right.path)
                .then(left.start_utf8.cmp(&right.start_utf8)),
            (ProvenanceSpan::File(_), ProvenanceSpan::Generated(_)) => std::cmp::Ordering::Less,
            (ProvenanceSpan::Generated(_), ProvenanceSpan::File(_)) => std::cmp::Ordering::Greater,
            (ProvenanceSpan::Generated(left), ProvenanceSpan::Generated(right)) => {
                left.stable_id.cmp(&right.stable_id)
            }
        },
    );
    let mut page_content_occurrences = std::collections::BTreeMap::<String, usize>::new();
    let mut finish_page = |pages: &mut Vec<PageDisplayList>, mut pending: PendingPage| {
        if options.show_page_numbers {
            let page_number = pages.len() + 1;
            let text = page_number.to_string();
            let mut font = body_font.clone();
            font.size_pt = options.page_number_font_size_pt;
            let advance = text_advance_pt(&text, &font, options.page_number_font_size_pt);
            let x = options.margin_left_pt + (page_content_width_pt - advance) / 2.0;
            let y = (options.page_height_pt - options.margin_bottom_pt
                + options.page_number_offset_pt)
                .min(options.page_height_pt - options.page_number_font_size_pt);
            let source = SourceProvenance::generated(
                format!("page-number:{page_number}"),
                format!("generated page number {page_number}"),
            );
            pending.hash_input.push('\u{1f}');
            pending
                .hash_input
                .push_str(&format!("page-number:{page_number}:{x:.3}:{y:.3}"));
            pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                origin: Point { x, y },
                font,
                size_pt: options.page_number_font_size_pt,
                text,
                approximate_advance_pt: advance,
                glyphs: None,
                clusters: None,
                source,
            }));
        }
        let content_hash = blake3::hash(pending.hash_input.as_bytes())
            .to_hex()
            .to_string();
        let occurrence = page_content_occurrences
            .entry(content_hash.clone())
            .or_default();
        let page_id = blake3::hash(
            format!(
                "display-list:{content_hash}:{}:{}:{occurrence}",
                options.page_width_pt, options.page_height_pt
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        *occurrence += 1;
        pages.push(PageDisplayList {
            page_id,
            width_pt: options.page_width_pt,
            height_pt: options.page_height_pt,
            ops: pending.ops,
            source_spans: pending.source_spans,
            content_hash,
        });
    };
    let new_pending_page = || PendingPage {
        ops: Vec::new(),
        source_spans: Vec::new(),
        text: String::new(),
        hash_input: format!("options:{options:?}:font-metrics:basic-v1"),
    };
    let content_height_pt =
        (options.page_height_pt - options.margin_top_pt - options.margin_bottom_pt).max(1.0);
    let parse_graphic_dimension_pt =
        |raw_value: &str, allow_zero: bool, full_width: bool| -> Option<f32> {
            let accepts_dimension = |dimension: f32| {
                dimension.is_finite() && (dimension > 0.0 || (allow_zero && dimension >= 0.0))
            };
            let normalized = raw_value
                .trim()
                .trim_matches(|ch| ch == '{' || ch == '}')
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            if normalized.is_empty() {
                return None;
            }

            let line_width_pt = if full_width {
                page_content_width_pt
            } else {
                column_width_pt
            };
            let reference_dimensions = [
                ("\\hsize", line_width_pt),
                ("\\linewidth", line_width_pt),
                ("\\textwidth", page_content_width_pt),
                ("\\columnwidth", line_width_pt),
                ("\\paperwidth", options.page_width_pt),
                ("\\pagewidth", options.page_width_pt),
                ("\\vsize", content_height_pt),
                ("\\textheight", content_height_pt),
                ("\\paperheight", options.page_height_pt),
                ("\\pageheight", options.page_height_pt),
                ("\\fboxsep", 3.0),
            ];
            let unit_dimensions = [
                ("truept", 1.0),
                ("bp", 1.0),
                ("pt", 1.0),
                ("in", 72.0),
                ("cm", 72.0 / 2.54),
                ("mm", 72.0 / 25.4),
                ("pc", 12.0),
                ("em", options.body_font_size_pt),
                ("ex", options.body_font_size_pt * 0.5),
            ];
            let parse_dimension_atom = |atom: &str| -> Option<f32> {
                let atom = atom.trim();
                if atom.is_empty() {
                    return None;
                }

                for (name, reference_pt) in reference_dimensions {
                    if atom == name {
                        return Some(reference_pt);
                    }
                    if let Some(prefix) = atom.strip_suffix(name) {
                        let factor = prefix.strip_suffix('*').unwrap_or(prefix);
                        let factor = if factor.is_empty() {
                            Some(1.0)
                        } else {
                            factor.parse::<f32>().ok()
                        }?;
                        let dimension = reference_pt * factor;
                        if accepts_dimension(dimension) {
                            return Some(dimension);
                        }
                    }
                }

                for (unit, multiplier) in unit_dimensions {
                    if let Some(number) = atom.strip_suffix(unit) {
                        let dimension = number.parse::<f32>().ok()? * multiplier;
                        if accepts_dimension(dimension) {
                            return Some(dimension);
                        }
                    }
                }

                let dimension = atom.parse::<f32>().ok()?;
                accepts_dimension(dimension).then_some(dimension)
            };
            let parse_dimension_expression = |expression: &str| -> Option<f32> {
                let mut expression = expression;
                let is_dimexpr = if let Some(inner) = expression.strip_prefix("\\dimexpr") {
                    expression = inner.strip_suffix("\\relax").unwrap_or(inner);
                    true
                } else {
                    false
                };
                let mut total = 0.0;
                let mut sign = 1.0;
                let mut term_start = 0usize;
                let mut saw_operator = false;
                for (index, ch) in expression.char_indices() {
                    if ch != '+' && ch != '-' {
                        continue;
                    }
                    if index == term_start {
                        sign = if ch == '-' { -1.0 } else { 1.0 };
                        term_start = index + ch.len_utf8();
                        continue;
                    }
                    total += sign * parse_dimension_atom(&expression[term_start..index])?;
                    saw_operator = true;
                    sign = if ch == '-' { -1.0 } else { 1.0 };
                    term_start = index + ch.len_utf8();
                }
                if term_start >= expression.len() {
                    return None;
                }
                total += sign * parse_dimension_atom(&expression[term_start..])?;
                ((is_dimexpr || saw_operator) && accepts_dimension(total)).then_some(total)
            };

            parse_dimension_atom(&normalized).or_else(|| parse_dimension_expression(&normalized))
        };
    let parse_graphic_quad_pt = |raw_value: &str, full_width: bool| -> Option<[f32; 4]> {
        let normalized = raw_value
            .trim()
            .trim_matches(|ch| ch == '{' || ch == '}')
            .replace(',', " ");
        let parts = normalized.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 {
            return None;
        }

        Some([
            parse_graphic_dimension_pt(parts[0], true, full_width)?,
            parse_graphic_dimension_pt(parts[1], true, full_width)?,
            parse_graphic_dimension_pt(parts[2], true, full_width)?,
            parse_graphic_dimension_pt(parts[3], true, full_width)?,
        ])
    };
    let mut pending = new_pending_page();
    let mut pending_image_row: Option<PendingImageRow> = None;
    let mut column_index = 0usize;
    let first_page_top_pt = if document_ir
        .blocks
        .iter()
        .any(|block| matches!(block, IrBlock::TitleBlock(_)))
    {
        options.front_matter_top_pt.unwrap_or(options.margin_top_pt)
    } else {
        options.margin_top_pt
    };
    let mut y = first_page_top_pt;
    let mut column_start_y = first_page_top_pt;
    let record_source_spans = |source: &SourceProvenance, source_spans: &mut Vec<SourceSpan>| {
        if let ProvenanceSpan::File(span) = &source.primary {
            if !source_spans.contains(span) {
                source_spans.push(span.clone());
            }
        }
        for related in &source.related {
            if let ProvenanceSpan::File(span) = &related.span {
                if !source_spans.contains(span) {
                    source_spans.push(span.clone());
                }
            }
        }
        for frame in &source.expansion_stack {
            if let ProvenanceSpan::File(span) = &frame.call_span {
                if !source_spans.contains(span) {
                    source_spans.push(span.clone());
                }
            }
            if let Some(ProvenanceSpan::File(span)) = &frame.definition_span {
                if !source_spans.contains(span) {
                    source_spans.push(span.clone());
                }
            }
        }
    };
    let mut emit_due_destinations =
        |current_source: &SourceProvenance, point: Point, pending: &mut PendingPage| {
            let ProvenanceSpan::File(current_span) = &current_source.primary else {
                return;
            };
            let mut index = 0usize;
            while index < pending_labels.len() {
                let should_emit = match &pending_labels[index].source.primary {
                    ProvenanceSpan::File(label_span) => {
                        label_span.path == current_span.path
                            && label_span.start_utf8 <= current_span.start_utf8
                    }
                    ProvenanceSpan::Generated(_) => false,
                };
                if should_emit {
                    let label = pending_labels.remove(index);
                    record_source_spans(&label.source, &mut pending.source_spans);
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("dest:");
                    pending.hash_input.push_str(&label.key);
                    pending.ops.push(DrawOp::NamedDestination(Destination {
                        name: label.key,
                        point,
                        source: label.source,
                    }));
                } else {
                    index += 1;
                }
            }
        };

    for logical in logical_items {
        match logical {
            LogicalItem::Text(logical) => {
                let full_width = logical.full_width;
                if let Some(row) = pending_image_row.take() {
                    y = row.y + row.height_pt + row.gap_after_pt;
                }
                if full_width && column_index > 0 {
                    if !pending.ops.is_empty() {
                        finish_page(&mut pages, pending);
                        pending = new_pending_page();
                    }
                    column_index = 0;
                    column_start_y = options.margin_top_pt;
                    y = column_start_y;
                }
                let mut wrapped_lines = Vec::new();
                let mut current_line = Vec::new();
                let mut current_len = 0usize;
                let average_glyph_width_pt =
                    text_advance_pt("n", &logical.font, logical.size_pt).max(0.1);
                let text_area_width_pt = if full_width {
                    page_content_width_pt
                } else {
                    column_width_pt
                };
                let available_width_pt_for_line = |line_index: usize| {
                    let left_indent_pt = if line_index == 0 {
                        logical.first_line_indent_pt
                    } else {
                        logical.continuation_indent_pt
                    };
                    (text_area_width_pt - left_indent_pt - logical.right_indent_pt)
                        .max(average_glyph_width_pt)
                };
                let max_chars_per_line = options.max_chars_per_line.max(1);
                let push_segment_text =
                    |mut text: &str,
                     source: &SourceProvenance,
                     link_target: Option<&str>,
                     table_rule: bool,
                     table_rule_trim_start_pt: Option<f32>,
                     table_rule_trim_end_pt: Option<f32>,
                     table_vertical_rule_offsets: &[(usize, u8)],
                     current_line: &mut Vec<LogicalTextSegment>,
                     current_len: &mut usize,
                     wrapped_lines: &mut Vec<Vec<LogicalTextSegment>>| {
                        let mut consumed_chars = 0usize;
                        while !text.is_empty() {
                            if *current_len == 0 && !logical.preserve_leading_whitespace {
                                let trimmed = text.trim_start_matches(char::is_whitespace);
                                consumed_chars +=
                                    text[..text.len() - trimmed.len()].chars().count();
                                text = trimmed;
                                if text.is_empty() {
                                    break;
                                }
                            }
                            let current_width_pt = current_line
                                .iter()
                                .map(|segment| {
                                    text_advance_pt(&segment.text, &logical.font, logical.size_pt)
                                })
                                .sum::<f32>();
                            let available_width_pt =
                                available_width_pt_for_line(wrapped_lines.len());
                            let remaining_width_pt =
                                (available_width_pt - current_width_pt).max(0.0);
                            let remaining_line_chars =
                                max_chars_per_line.saturating_sub(*current_len);
                            let text_char_count = text.chars().count();
                            let width_fitting_chars = text
                                .char_indices()
                                .scan(0.0, |width, (index, ch)| {
                                    let next_width = *width
                                        + text_advance_pt(
                                            &text[index..index + ch.len_utf8()],
                                            &logical.font,
                                            logical.size_pt,
                                        );
                                    (next_width <= remaining_width_pt + 0.01).then(|| {
                                        *width = next_width;
                                        1usize
                                    })
                                })
                                .sum::<usize>();
                            if width_fitting_chars == 0 && !current_line.is_empty() {
                                wrapped_lines.push(std::mem::take(current_line));
                                *current_len = 0;
                                continue;
                            }
                            let mut take_chars = remaining_line_chars
                                .max(1)
                                .min(width_fitting_chars.max(1))
                                .min(text_char_count);
                            let mut wrap_after_chunk = false;
                            let can_wrap_at_words = !logical.preserve_leading_whitespace
                                && !table_rule
                                && table_vertical_rule_offsets.is_empty();
                            if can_wrap_at_words && take_chars > 0 && text_char_count > take_chars {
                                let first_word_chars =
                                    text.chars().take_while(|ch| !ch.is_whitespace()).count();
                                let line_ends_with_whitespace = current_line
                                    .last()
                                    .and_then(|segment| segment.text.chars().last())
                                    .is_some_and(char::is_whitespace);
                                if *current_len > 0
                                    && line_ends_with_whitespace
                                    && first_word_chars > take_chars
                                    && first_word_chars <= max_chars_per_line
                                    && text_advance_pt(
                                        &text[..text
                                            .char_indices()
                                            .nth(first_word_chars)
                                            .map(|(index, _)| index)
                                            .unwrap_or(text.len())],
                                        &logical.font,
                                        logical.size_pt,
                                    ) <= available_width_pt
                                {
                                    wrapped_lines.push(std::mem::take(current_line));
                                    *current_len = 0;
                                    continue;
                                }
                                if let Some(word_end) = text
                                    .chars()
                                    .take(take_chars)
                                    .enumerate()
                                    .filter_map(|(index, ch)| {
                                        (index > 0 && ch.is_whitespace()).then_some(index)
                                    })
                                    .last()
                                {
                                    take_chars = word_end + 1;
                                    wrap_after_chunk = true;
                                }
                            }
                            let split_byte = if take_chars == text.chars().count() {
                                text.len()
                            } else {
                                text.char_indices()
                                    .nth(take_chars)
                                    .map(|(index, _)| index)
                                    .unwrap_or(text.len())
                            };
                            let chunk = &text[..split_byte];
                            if !chunk.is_empty() {
                                let chunk_start_chars = consumed_chars;
                                let chunk_end_chars = chunk_start_chars + take_chars;
                                let is_final_chunk = split_byte == text.len();
                                let table_vertical_rule_offsets = table_vertical_rule_offsets
                                    .iter()
                                    .filter_map(|(offset, count)| {
                                        if *offset >= chunk_start_chars
                                            && (*offset < chunk_end_chars
                                                || (is_final_chunk && *offset == chunk_end_chars))
                                        {
                                            Some((*offset - chunk_start_chars, *count))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                current_line.push(LogicalTextSegment {
                                    text: chunk.to_string(),
                                    source: source.clone(),
                                    link_target: link_target.map(ToOwned::to_owned),
                                    table_rule,
                                    table_rule_trim_start_pt,
                                    table_rule_trim_end_pt,
                                    table_vertical_rule_offsets,
                                });
                                *current_len += take_chars;
                            }
                            consumed_chars += take_chars;
                            text = &text[split_byte..];
                            let current_width_pt = current_line
                                .iter()
                                .map(|segment| {
                                    text_advance_pt(&segment.text, &logical.font, logical.size_pt)
                                })
                                .sum::<f32>();
                            if wrap_after_chunk
                                || *current_len >= max_chars_per_line
                                || current_width_pt >= available_width_pt - 0.01
                            {
                                wrapped_lines.push(std::mem::take(current_line));
                                *current_len = 0;
                            }
                        }
                    };

                for segment in &logical.segments {
                    let mut remaining = segment.text.as_str();
                    let mut remaining_start_chars = 0usize;
                    while !remaining.is_empty() {
                        if let Some(newline_index) = remaining.find('\n') {
                            let before_newline = &remaining[..newline_index];
                            let before_newline_chars = before_newline.chars().count();
                            let table_vertical_rule_offsets = segment
                                .table_vertical_rule_offsets
                                .iter()
                                .filter_map(|(offset, count)| {
                                    if *offset >= remaining_start_chars
                                        && *offset <= remaining_start_chars + before_newline_chars
                                    {
                                        Some((*offset - remaining_start_chars, *count))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            push_segment_text(
                                before_newline,
                                &segment.source,
                                segment.link_target.as_deref(),
                                segment.table_rule,
                                segment.table_rule_trim_start_pt,
                                segment.table_rule_trim_end_pt,
                                &table_vertical_rule_offsets,
                                &mut current_line,
                                &mut current_len,
                                &mut wrapped_lines,
                            );
                            wrapped_lines.push(std::mem::take(&mut current_line));
                            current_len = 0;
                            remaining = &remaining[newline_index + 1..];
                            remaining_start_chars += before_newline_chars + 1;
                        } else {
                            let table_vertical_rule_offsets = segment
                                .table_vertical_rule_offsets
                                .iter()
                                .filter_map(|(offset, count)| {
                                    if *offset >= remaining_start_chars {
                                        Some((*offset - remaining_start_chars, *count))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            push_segment_text(
                                remaining,
                                &segment.source,
                                segment.link_target.as_deref(),
                                segment.table_rule,
                                segment.table_rule_trim_start_pt,
                                segment.table_rule_trim_end_pt,
                                &table_vertical_rule_offsets,
                                &mut current_line,
                                &mut current_len,
                                &mut wrapped_lines,
                            );
                            remaining = "";
                        }
                    }
                }
                if !current_line.is_empty() || wrapped_lines.is_empty() {
                    wrapped_lines.push(current_line);
                }
                for (line_index, line_segments) in wrapped_lines.into_iter().enumerate() {
                    let line_is_table_rule = !line_segments.is_empty()
                        && line_segments.iter().all(|segment| segment.table_rule);
                    let line_advance_pt = if line_is_table_rule {
                        0.0
                    } else {
                        logical.line_height_pt
                    };
                    if y + line_advance_pt > options.page_height_pt - options.margin_bottom_pt
                        && !pending.ops.is_empty()
                    {
                        if !full_width && column_index + 1 < column_count {
                            column_index += 1;
                        } else {
                            finish_page(&mut pages, pending);
                            pending = new_pending_page();
                            column_index = 0;
                            column_start_y = options.margin_top_pt;
                        }
                        y = column_start_y;
                    }
                    let line_width_pt = line_segments
                        .iter()
                        .filter(|segment| !segment.table_rule)
                        .map(|segment| {
                            text_advance_pt(&segment.text, &logical.font, logical.size_pt)
                        })
                        .sum::<f32>();
                    let column_left_pt = if full_width {
                        options.margin_left_pt
                    } else {
                        options.margin_left_pt
                            + column_index as f32 * (column_width_pt + column_gap_pt)
                    };
                    let line_x = column_left_pt
                        + if full_width {
                            ((page_content_width_pt - line_width_pt) / 2.0).max(0.0)
                        } else {
                            0.0
                        }
                        + if line_index == 0 {
                            logical.first_line_indent_pt
                        } else {
                            logical.continuation_indent_pt
                        };

                    let destination_source = line_segments
                        .first()
                        .map(|segment| &segment.source)
                        .unwrap_or(&logical.source);
                    emit_due_destinations(destination_source, Point { x: line_x, y }, &mut pending);
                    let visible_line_text = line_segments
                        .iter()
                        .filter(|segment| !segment.table_rule)
                        .map(|segment| segment.text.as_str())
                        .collect::<String>();
                    let line_has_text = !visible_line_text.is_empty() || line_segments.is_empty();
                    if line_has_text {
                        if !pending.text.is_empty() {
                            pending.text.push('\n');
                            pending.hash_input.push('\n');
                        }
                        pending.hash_input.push_str(&format!(
                            "\u{1e}text_run:{line_x:.3}:{y:.3}:{:?}:{:.3}\u{1f}",
                            logical.font, logical.size_pt
                        ));
                        pending.text.push_str(&visible_line_text);
                        pending.hash_input.push_str(&visible_line_text);
                    }

                    if line_segments.is_empty() {
                        record_source_spans(&logical.source, &mut pending.source_spans);
                        pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                            origin: Point { x: line_x, y },
                            text: String::new(),
                            font: logical.font.clone(),
                            size_pt: logical.size_pt,
                            approximate_advance_pt: 0.0,
                            glyphs: None,
                            clusters: None,
                            source: logical.source.clone(),
                        }));
                        y += logical.line_height_pt;
                        continue;
                    }

                    let mut x = line_x;
                    for segment in line_segments {
                        record_source_spans(&segment.source, &mut pending.source_spans);
                        let advance =
                            text_advance_pt(&segment.text, &logical.font, logical.size_pt);
                        let mut table_rule_rects = Vec::new();
                        let mut table_vertical_rule_rects = Vec::new();
                        if segment.table_rule {
                            let mut push_table_rule_rect =
                                |prefix_advance: f32, rule_advance: f32| {
                                    let trim_start = segment
                                        .table_rule_trim_start_pt
                                        .unwrap_or(0.0)
                                        .min(rule_advance);
                                    let trim_end = segment
                                        .table_rule_trim_end_pt
                                        .unwrap_or(0.0)
                                        .min((rule_advance - trim_start).max(0.0));
                                    let trimmed_rule_advance = rule_advance - trim_start - trim_end;
                                    if trimmed_rule_advance > 0.0 {
                                        table_rule_rects.push(Rect {
                                            x: x + prefix_advance + trim_start,
                                            y: (y - logical.size_pt).max(0.0),
                                            width: trimmed_rule_advance,
                                            height: 0.8,
                                        });
                                    }
                                };
                            let mut rule_start_byte = None;
                            for (byte_index, ch) in segment.text.char_indices() {
                                if ch == '-' {
                                    if rule_start_byte.is_none() {
                                        rule_start_byte = Some(byte_index);
                                    }
                                } else if let Some(start_byte) = rule_start_byte.take() {
                                    let prefix_advance = text_advance_pt(
                                        &segment.text[..start_byte],
                                        &logical.font,
                                        logical.size_pt,
                                    );
                                    let rule_advance = text_advance_pt(
                                        &segment.text[start_byte..byte_index],
                                        &logical.font,
                                        logical.size_pt,
                                    );
                                    push_table_rule_rect(prefix_advance, rule_advance);
                                }
                            }
                            if let Some(start_byte) = rule_start_byte {
                                let prefix_advance = text_advance_pt(
                                    &segment.text[..start_byte],
                                    &logical.font,
                                    logical.size_pt,
                                );
                                let rule_advance = text_advance_pt(
                                    &segment.text[start_byte..],
                                    &logical.font,
                                    logical.size_pt,
                                );
                                push_table_rule_rect(prefix_advance, rule_advance);
                            }
                        }
                        for (offset, count) in &segment.table_vertical_rule_offsets {
                            let prefix_byte = if *offset >= segment.text.chars().count() {
                                segment.text.len()
                            } else {
                                segment
                                    .text
                                    .char_indices()
                                    .nth(*offset)
                                    .map(|(index, _)| index)
                                    .unwrap_or(segment.text.len())
                            };
                            let prefix_advance = text_advance_pt(
                                &segment.text[..prefix_byte],
                                &logical.font,
                                logical.size_pt,
                            );
                            let rule_count = (*count).clamp(1, 4);
                            let rule_spacing = 1.2;
                            let first_shift =
                                -((rule_count.saturating_sub(1) as f32) * rule_spacing) / 2.0;
                            for rule_index in 0..rule_count {
                                table_vertical_rule_rects.push(Rect {
                                    x: x + prefix_advance
                                        + first_shift
                                        + rule_index as f32 * rule_spacing,
                                    y: (y - logical.size_pt).max(0.0),
                                    width: 0.8,
                                    height: if segment.table_rule {
                                        0.8
                                    } else {
                                        logical.line_height_pt
                                    },
                                });
                            }
                        }
                        if !segment.table_rule {
                            pending.hash_input.push('\u{1f}');
                            pending.hash_input.push_str(&format!(
                                "text_segment:{x:.3}:{advance:.3}:{}",
                                segment.text
                            ));
                            let clusters = approximate_text_clusters(&segment.text);
                            let source = segment.source;
                            pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                                origin: Point { x, y },
                                text: segment.text,
                                font: logical.font.clone(),
                                size_pt: logical.size_pt,
                                approximate_advance_pt: advance,
                                glyphs: None,
                                clusters,
                                source: source.clone(),
                            }));
                            if let Some(target) = segment.link_target {
                                let rect = Rect {
                                    x,
                                    y: (y - logical.size_pt).max(0.0),
                                    width: advance,
                                    height: logical.line_height_pt,
                                };
                                pending.hash_input.push('\u{1f}');
                                pending.hash_input.push_str(&format!(
                                    "link:{target}:{:.3}:{:.3}:{:.3}:{:.3}",
                                    rect.x, rect.y, rect.width, rect.height
                                ));
                                pending.ops.push(DrawOp::LinkAnnotation(LinkAnnotation {
                                    rect,
                                    target,
                                    source,
                                }));
                            }
                        }
                        for rect in table_rule_rects {
                            pending.hash_input.push('\u{1f}');
                            pending.hash_input.push_str(&format!(
                                "table_rule:{:.3}:{:.3}:{:.3}:{:.3}",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                            pending.ops.push(DrawOp::Rule(rect));
                        }
                        for rect in table_vertical_rule_rects {
                            pending.hash_input.push('\u{1f}');
                            pending.hash_input.push_str(&format!(
                                "table_vertical_rule:{:.3}:{:.3}:{:.3}:{:.3}",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                            let mut merged = false;
                            for op in pending.ops.iter_mut().rev() {
                                let DrawOp::Rule(previous) = op else {
                                    continue;
                                };
                                if (previous.x - rect.x).abs() <= 0.01
                                    && (previous.width - rect.width).abs() <= 0.01
                                    && previous.y <= rect.y + rect.height + 0.01
                                    && rect.y <= previous.y + previous.height + 0.01
                                {
                                    let bottom =
                                        (previous.y + previous.height).max(rect.y + rect.height);
                                    previous.y = previous.y.min(rect.y);
                                    previous.height = bottom - previous.y;
                                    merged = true;
                                    break;
                                }
                            }
                            if !merged {
                                pending.ops.push(DrawOp::Rule(rect));
                            }
                        }
                        x += advance;
                    }
                    y += line_advance_pt;
                }
                y += logical.gap_after_pt;
                if full_width {
                    column_start_y = y;
                }
            }
            LogicalItem::ContainerRow(containers) => {
                if let Some(row) = pending_image_row.take() {
                    y = row.y + row.height_pt + row.gap_after_pt;
                }
                let row_height_pt = containers
                    .iter()
                    .map(|container| container.height_pt)
                    .fold(options.line_height_pt, f32::max);
                if y + row_height_pt > options.page_height_pt - options.margin_bottom_pt
                    && !pending.ops.is_empty()
                {
                    if column_index + 1 < column_count {
                        column_index += 1;
                    } else {
                        finish_page(&mut pages, pending);
                        pending = new_pending_page();
                        column_index = 0;
                        column_start_y = options.margin_top_pt;
                    }
                    y = column_start_y;
                }
                let mut container_x = options.margin_left_pt
                    + column_index as f32 * (column_width_pt + column_gap_pt);
                for container in containers {
                    let alignment_offset_y = match container.alignment {
                        Some(LayoutAlignment::Top | LayoutAlignment::Stretch) => 0.0,
                        Some(LayoutAlignment::Bottom) => row_height_pt - container.height_pt,
                        Some(LayoutAlignment::Center) | None => {
                            (row_height_pt - container.height_pt) / 2.0
                        }
                    };
                    let container_y = y + alignment_offset_y;
                    emit_due_destinations(
                        &container.source,
                        Point {
                            x: container_x,
                            y: container_y,
                        },
                        &mut pending,
                    );
                    if !pending.text.is_empty() {
                        pending.text.push('\n');
                    }
                    pending.text.push_str("[layout container]");
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!(
                        "layout-container:{}:{:.3}:{:.3}:{:.3}:{:.3}:{}",
                        container.name,
                        container_x,
                        container_y,
                        container.width_pt,
                        container.height_pt,
                        container.content_hash
                    ));
                    record_source_spans(&container.source, &mut pending.source_spans);
                    for span in container.source_spans {
                        if !pending.source_spans.contains(&span) {
                            pending.source_spans.push(span);
                        }
                    }
                    for mut op in container.ops {
                        op.translate(container_x, container_y);
                        let source_and_point = match &op {
                            DrawOp::TextRun(run) => Some((&run.source, run.origin)),
                            DrawOp::Image(image) => Some((
                                &image.source,
                                Point {
                                    x: image.rect.x,
                                    y: image.rect.y,
                                },
                            )),
                            DrawOp::LinkAnnotation(link) => Some((
                                &link.source,
                                Point {
                                    x: link.rect.x,
                                    y: link.rect.y,
                                },
                            )),
                            DrawOp::Save
                            | DrawOp::Restore
                            | DrawOp::ClipRect(_)
                            | DrawOp::Rule(_)
                            | DrawOp::NamedDestination(_) => None,
                        };
                        if let Some((source, point)) = source_and_point {
                            emit_due_destinations(source, point, &mut pending);
                        }
                        pending.ops.push(op);
                    }
                    container_x += container.width_pt + container_gap_pt;
                }
                y += row_height_pt + options.block_gap_pt;
            }
            LogicalItem::Container(_) => {
                unreachable!("layout containers must be grouped before page placement")
            }
            LogicalItem::PageBreak => {
                pending_image_row = None;
                if !pending.ops.is_empty() {
                    finish_page(&mut pages, pending);
                    pending = new_pending_page();
                }
                column_index = 0;
                column_start_y = options.margin_top_pt;
                y = column_start_y;
            }
            LogicalItem::FullPageImage(logical) => {
                pending_image_row = None;
                if !pending.ops.is_empty() {
                    finish_page(&mut pages, pending);
                    pending = new_pending_page();
                }

                let (natural_width_pt, natural_height_pt) = logical
                    .asset_dimensions
                    .and_then(|dimensions| {
                        match (
                            dimensions.natural_width_pt_milli,
                            dimensions.natural_height_pt_milli,
                        ) {
                            (Some(width), Some(height)) if width > 0 && height > 0 => {
                                Some((width as f32 / 1000.0, height as f32 / 1000.0))
                            }
                            _ if dimensions.width_px > 0 && dimensions.height_px > 0 => {
                                Some((dimensions.width_px as f32, dimensions.height_px as f32))
                            }
                            _ => None,
                        }
                    })
                    .unwrap_or((options.page_width_pt, options.page_height_pt));
                let fit_scale = (options.page_width_pt / natural_width_pt)
                    .min(options.page_height_pt / natural_height_pt);
                let image_width = natural_width_pt * fit_scale;
                let image_height = natural_height_pt * fit_scale;
                let image_x = (options.page_width_pt - image_width) / 2.0;
                let image_y = (options.page_height_pt - image_height) / 2.0;

                emit_due_destinations(
                    &logical.source,
                    Point {
                        x: image_x,
                        y: image_y,
                    },
                    &mut pending,
                );
                pending.hash_input.push('\u{1f}');
                pending.hash_input.push_str(&format!(
                    "included-pdf-page:{}:{:?}:{:.3}:{:.3}:{image_width:.3}:{image_height:.3}",
                    logical.path, logical.page_selection, image_x, image_y
                ));
                if let Some(asset_hash) = &logical.asset_hash {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("asset-hash:");
                    pending.hash_input.push_str(asset_hash);
                }
                record_source_spans(&logical.source, &mut pending.source_spans);
                pending.ops.push(DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: image_x,
                        y: image_y,
                        width: image_width,
                        height: image_height,
                    },
                    asset_ref: logical.path,
                    asset_format: logical.asset_format,
                    page_selection: logical.page_selection,
                    asset_hash: logical.asset_hash,
                    natural_width_pt: Some(natural_width_pt),
                    natural_height_pt: Some(natural_height_pt),
                    crop: None,
                    scale: None,
                    rotation: None,
                    diagnostic: None,
                    source: logical.source,
                }));
                finish_page(&mut pages, pending);
                pending = new_pending_page();
                column_index = 0;
                column_start_y = options.margin_top_pt;
                y = column_start_y;
            }
            LogicalItem::Image(logical) => {
                let full_width = logical.full_width;
                let image_area_width_pt = if full_width {
                    page_content_width_pt
                } else {
                    column_width_pt
                };
                let (mut natural_image_width, mut natural_image_height) = if let Some(dimensions) =
                    logical.asset_dimensions
                {
                    let (mut natural_width, mut natural_height) =
                        if let (Some(width_pt_milli), Some(height_pt_milli)) = (
                            dimensions.natural_width_pt_milli,
                            dimensions.natural_height_pt_milli,
                        ) {
                            (
                                width_pt_milli as f32 / 1000.0,
                                height_pt_milli as f32 / 1000.0,
                            )
                        } else {
                            (dimensions.width_px as f32, dimensions.height_px as f32)
                        };
                    if dimensions.natural_width_pt_milli.is_none()
                        && dimensions.natural_height_pt_milli.is_none()
                        && let Some(density) = dimensions.density
                    {
                        let x_density_per_inch = match density.unit {
                            GraphicAssetDensityUnit::PixelsPerInch => density.x_density as f32,
                            GraphicAssetDensityUnit::PixelsPerCentimeter => {
                                density.x_density as f32 * 2.54
                            }
                            GraphicAssetDensityUnit::PixelsPerMeter => {
                                density.x_density as f32 * 0.0254
                            }
                        };
                        let y_density_per_inch = match density.unit {
                            GraphicAssetDensityUnit::PixelsPerInch => density.y_density as f32,
                            GraphicAssetDensityUnit::PixelsPerCentimeter => {
                                density.y_density as f32 * 2.54
                            }
                            GraphicAssetDensityUnit::PixelsPerMeter => {
                                density.y_density as f32 * 0.0254
                            }
                        };
                        if x_density_per_inch.is_finite()
                            && y_density_per_inch.is_finite()
                            && x_density_per_inch > 0.0
                            && y_density_per_inch > 0.0
                        {
                            natural_width = dimensions.width_px as f32 * 72.0 / x_density_per_inch;
                            natural_height =
                                dimensions.height_px as f32 * 72.0 / y_density_per_inch;
                        }
                    }
                    if natural_width.is_finite()
                        && natural_height.is_finite()
                        && natural_width > 0.0
                        && natural_height > 0.0
                    {
                        (natural_width, natural_height)
                    } else {
                        (image_area_width_pt, options.line_height_pt * 6.0)
                    }
                } else {
                    (image_area_width_pt, options.line_height_pt * 6.0)
                };
                let mut width_hint_pt = None;
                let mut height_hint_pt = None;
                let mut scale_hint = None;
                let mut x_scale_hint = None;
                let mut y_scale_hint = None;
                let mut natural_width_hint_pt = None;
                let mut natural_height_hint_pt = None;
                let mut keep_aspect_ratio = false;
                let mut trim = None;
                let mut viewport = None;
                let mut bb_llx_pt = None;
                let mut bb_lly_pt = None;
                let mut bb_urx_pt = None;
                let mut bb_ury_pt = None;
                let mut clip = false;
                let mut draft = false;
                let mut rotation_angle_degrees = None;
                let mut rotation_origin = None;
                if let Some(graphic_options) = &logical.options {
                    let mut option_parts = Vec::new();
                    let mut part_start = 0usize;
                    let mut brace_depth = 0usize;
                    for (index, ch) in graphic_options.char_indices() {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' if brace_depth > 0 => brace_depth -= 1,
                            ',' if brace_depth == 0 => {
                                option_parts.push(&graphic_options[part_start..index]);
                                part_start = index + ch.len_utf8();
                            }
                            _ => {}
                        }
                    }
                    option_parts.push(&graphic_options[part_start..]);

                    for part in option_parts {
                        let part = part.trim();
                        if part == "keepaspectratio" {
                            keep_aspect_ratio = true;
                            continue;
                        }
                        if part == "clip" {
                            clip = true;
                            continue;
                        }
                        if part == "draft" {
                            draft = true;
                            continue;
                        }
                        if part == "final" {
                            draft = false;
                            continue;
                        }
                        let Some((key, value)) = part.split_once('=') else {
                            continue;
                        };
                        match key.trim() {
                            "width" => {
                                width_hint_pt =
                                    parse_graphic_dimension_pt(value, false, full_width);
                            }
                            "height" | "totalheight" => {
                                height_hint_pt =
                                    parse_graphic_dimension_pt(value, false, full_width);
                            }
                            "natwidth" => {
                                natural_width_hint_pt =
                                    parse_graphic_dimension_pt(value, false, full_width);
                            }
                            "natheight" => {
                                natural_height_hint_pt =
                                    parse_graphic_dimension_pt(value, false, full_width);
                            }
                            "scale" => {
                                let scale = value
                                    .trim()
                                    .parse::<f32>()
                                    .ok()
                                    .filter(|value| value.is_finite() && *value != 0.0);
                                scale_hint = scale;
                            }
                            "xscale" => {
                                x_scale_hint = value
                                    .trim()
                                    .parse::<f32>()
                                    .ok()
                                    .filter(|value| value.is_finite() && *value != 0.0);
                            }
                            "yscale" => {
                                y_scale_hint = value
                                    .trim()
                                    .parse::<f32>()
                                    .ok()
                                    .filter(|value| value.is_finite() && *value != 0.0);
                            }
                            "keepaspectratio" => {
                                keep_aspect_ratio = !matches!(value.trim(), "false" | "0" | "off");
                            }
                            "trim" => {
                                if let Some([left, bottom, right, top]) =
                                    parse_graphic_quad_pt(value, full_width)
                                {
                                    trim = Some(ImageTrim {
                                        left_pt: left,
                                        bottom_pt: bottom,
                                        right_pt: right,
                                        top_pt: top,
                                    });
                                }
                            }
                            "viewport" | "bb" => {
                                if let Some([llx, lly, urx, ury]) =
                                    parse_graphic_quad_pt(value, full_width)
                                {
                                    viewport = Some(ImageViewport {
                                        llx_pt: llx,
                                        lly_pt: lly,
                                        urx_pt: urx,
                                        ury_pt: ury,
                                    });
                                }
                            }
                            "bbllx" => {
                                bb_llx_pt = parse_graphic_dimension_pt(value, true, full_width);
                            }
                            "bblly" => {
                                bb_lly_pt = parse_graphic_dimension_pt(value, true, full_width);
                            }
                            "bburx" => {
                                bb_urx_pt = parse_graphic_dimension_pt(value, true, full_width);
                            }
                            "bbury" => {
                                bb_ury_pt = parse_graphic_dimension_pt(value, true, full_width);
                            }
                            "clip" => {
                                clip = !matches!(value.trim(), "false" | "0" | "off");
                            }
                            "draft" => {
                                draft = !matches!(value.trim(), "false" | "0" | "off");
                            }
                            "final" => {
                                if !matches!(value.trim(), "false" | "0" | "off") {
                                    draft = false;
                                }
                            }
                            "angle" => {
                                rotation_angle_degrees = value
                                    .trim()
                                    .trim_matches(|ch| ch == '{' || ch == '}')
                                    .parse::<f32>()
                                    .ok()
                                    .filter(|angle| angle.is_finite());
                            }
                            "origin" => {
                                let origin = value
                                    .trim()
                                    .trim_matches(|ch| ch == '{' || ch == '}')
                                    .to_string();
                                if !origin.is_empty() {
                                    rotation_origin = Some(origin);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if viewport.is_none()
                    && let (Some(llx), Some(lly), Some(urx), Some(ury)) =
                        (bb_llx_pt, bb_lly_pt, bb_urx_pt, bb_ury_pt)
                    && urx > llx
                    && ury > lly
                {
                    viewport = Some(ImageViewport {
                        llx_pt: llx,
                        lly_pt: lly,
                        urx_pt: urx,
                        ury_pt: ury,
                    });
                }
                match (natural_width_hint_pt, natural_height_hint_pt) {
                    (Some(width), Some(height)) => {
                        natural_image_width = width;
                        natural_image_height = height;
                    }
                    (Some(width), None) => {
                        let ratio = width / natural_image_width;
                        if ratio.is_finite() && ratio > 0.0 {
                            natural_image_width = width;
                            natural_image_height = (natural_image_height * ratio).max(1.0);
                        }
                    }
                    (None, Some(height)) => {
                        let ratio = height / natural_image_height;
                        if ratio.is_finite() && ratio > 0.0 {
                            natural_image_width = (natural_image_width * ratio).max(1.0);
                            natural_image_height = height;
                        }
                    }
                    (None, None) => {}
                }
                let crop = (clip || trim.is_some() || viewport.is_some()).then_some(ImageCrop {
                    trim,
                    viewport,
                    clip,
                });
                let rotation = rotation_angle_degrees.map(|angle_degrees| ImageRotation {
                    angle_degrees,
                    origin: rotation_origin,
                });
                let scale =
                    (scale_hint.is_some() || x_scale_hint.is_some() || y_scale_hint.is_some())
                        .then(|| {
                            let base_scale = scale_hint.unwrap_or(1.0);
                            ImageScale {
                                x: x_scale_hint.unwrap_or(base_scale),
                                y: y_scale_hint.unwrap_or(base_scale),
                            }
                        });
                let (source_image_width, source_image_height) = if let Some(crop) = crop {
                    let (mut source_left, mut source_bottom, mut source_right, mut source_top) =
                        if let Some(viewport) = crop.viewport {
                            (
                                viewport.llx_pt,
                                viewport.lly_pt,
                                viewport.urx_pt,
                                viewport.ury_pt,
                            )
                        } else {
                            (0.0, 0.0, natural_image_width, natural_image_height)
                        };
                    if let Some(trim) = crop.trim {
                        source_left += trim.left_pt;
                        source_bottom += trim.bottom_pt;
                        source_right -= trim.right_pt;
                        source_top -= trim.top_pt;
                    }
                    let source_width = source_right - source_left;
                    let source_height = source_top - source_bottom;
                    if source_width.is_finite()
                        && source_height.is_finite()
                        && source_width > 0.0
                        && source_height > 0.0
                    {
                        (source_width, source_height)
                    } else {
                        (natural_image_width, natural_image_height)
                    }
                } else {
                    (natural_image_width, natural_image_height)
                };
                let fit_scale = (image_area_width_pt / source_image_width).min(1.0);
                let (default_image_width, default_image_height) = (
                    source_image_width * fit_scale,
                    source_image_height * fit_scale,
                );
                let (mut image_width, mut image_height) = match (width_hint_pt, height_hint_pt) {
                    (Some(width), Some(height)) if keep_aspect_ratio => {
                        let scale =
                            (width / default_image_width).min(height / default_image_height);
                        (
                            (default_image_width * scale).max(1.0),
                            (default_image_height * scale).max(1.0),
                        )
                    }
                    (Some(width), Some(height)) => (width, height),
                    (Some(width), None) => (
                        width,
                        (default_image_height * (width / default_image_width)).max(1.0),
                    ),
                    (None, Some(height)) => (
                        (default_image_width * (height / default_image_height)).max(1.0),
                        height,
                    ),
                    (None, None) => {
                        let scale = scale.unwrap_or(ImageScale { x: 1.0, y: 1.0 });
                        let x_scale = scale.x.abs();
                        let y_scale = scale.y.abs();
                        (
                            default_image_width * x_scale,
                            default_image_height * y_scale,
                        )
                    }
                };
                let bounds_scale = (image_area_width_pt / image_width)
                    .min(content_height_pt / image_height)
                    .min(1.0);
                if bounds_scale < 1.0 {
                    image_width = (image_width * bounds_scale).max(1.0);
                    image_height = (image_height * bounds_scale).max(1.0);
                }
                let required_height = image_height
                    + if logical.caption.is_some() {
                        options.line_height_pt
                    } else {
                        0.0
                    };
                let row_gap_pt = 4.0;
                let image_is_packable = !full_width
                    && width_hint_pt.is_some()
                    && image_width + row_gap_pt < column_width_pt;
                let can_join_pending_row = pending_image_row.as_ref().is_some_and(|row| {
                    row.packable
                        && image_is_packable
                        && row.used_width_pt + row_gap_pt + image_width <= column_width_pt + 0.01
                        && row.y + required_height
                            <= options.page_height_pt - options.margin_bottom_pt
                });
                let (image_x, image_y) = if can_join_pending_row {
                    let row = pending_image_row.as_mut().expect("pending image row");
                    let image_x = options.margin_left_pt
                        + column_index as f32 * (column_width_pt + column_gap_pt)
                        + row.used_width_pt
                        + row_gap_pt;
                    row.used_width_pt += row_gap_pt + image_width;
                    row.height_pt = row.height_pt.max(required_height);
                    row.gap_after_pt = row.gap_after_pt.max(logical.gap_after_pt);
                    (image_x, row.y)
                } else {
                    if let Some(row) = pending_image_row.take() {
                        y = row.y + row.height_pt + row.gap_after_pt;
                    }
                    if full_width && column_index > 0 {
                        if !pending.ops.is_empty() {
                            finish_page(&mut pages, pending);
                            pending = new_pending_page();
                        }
                        column_index = 0;
                        column_start_y = options.margin_top_pt;
                        y = column_start_y;
                    }
                    if y + required_height > options.page_height_pt - options.margin_bottom_pt
                        && !pending.ops.is_empty()
                    {
                        if !full_width && column_index + 1 < column_count {
                            column_index += 1;
                        } else {
                            finish_page(&mut pages, pending);
                            pending = new_pending_page();
                            column_index = 0;
                            column_start_y = options.margin_top_pt;
                        }
                        y = column_start_y;
                    }
                    let image_x = if full_width {
                        options.margin_left_pt
                    } else {
                        options.margin_left_pt
                            + column_index as f32 * (column_width_pt + column_gap_pt)
                    };
                    pending_image_row = Some(PendingImageRow {
                        y,
                        used_width_pt: image_width,
                        height_pt: required_height,
                        gap_after_pt: logical.gap_after_pt,
                        packable: image_is_packable,
                    });
                    (image_x, y)
                };
                if full_width {
                    column_index = 0;
                    column_start_y = image_y + required_height + logical.gap_after_pt;
                }

                if !pending.text.is_empty() {
                    pending.text.push('\n');
                    pending.hash_input.push('\n');
                }
                emit_due_destinations(
                    &logical.source,
                    Point {
                        x: image_x,
                        y: image_y,
                    },
                    &mut pending,
                );
                let image_text = format!("[image: {}]", logical.path);
                pending.text.push_str(&image_text);
                pending.hash_input.push_str(&image_text);
                if let Some(graphic_options) = &logical.options {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("graphic-options:");
                    pending.hash_input.push_str(graphic_options);
                }
                if let Some(asset_format) = logical.asset_format {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("asset-format:");
                    pending.hash_input.push_str(asset_format.as_str());
                }
                if let Some(page_selection) = &logical.page_selection {
                    pending.hash_input.push('\u{1f}');
                    pending
                        .hash_input
                        .push_str(&format!("page-selection:{page_selection:?}"));
                }
                if let Some(asset_hash) = &logical.asset_hash {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("asset-hash:");
                    pending.hash_input.push_str(asset_hash);
                }
                if let Some(dimensions) = logical.asset_dimensions {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!(
                        "asset-dimensions:{}:{}:{:?}:{:?}:{:?}",
                        dimensions.width_px,
                        dimensions.height_px,
                        dimensions.density,
                        dimensions.natural_width_pt_milli,
                        dimensions.natural_height_pt_milli
                    ));
                }
                if let Some(crop) = crop {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!("image-crop:{crop:?}"));
                }
                if let Some(rotation) = &rotation {
                    pending.hash_input.push('\u{1f}');
                    pending
                        .hash_input
                        .push_str(&format!("image-rotation:{rotation:?}"));
                }
                if let Some(scale) = &scale {
                    pending.hash_input.push('\u{1f}');
                    pending
                        .hash_input
                        .push_str(&format!("image-scale:{scale:?}"));
                }
                let image_diagnostic =
                    draft.then(|| format!("draft graphic asset {}", logical.path));
                if let Some(diagnostic) = &image_diagnostic {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("image-diagnostic:");
                    pending.hash_input.push_str(diagnostic);
                }
                pending.hash_input.push('\u{1f}');
                pending.hash_input.push_str(&format!(
                    "image-rect:{:.3}:{:.3}:{image_width:.3}:{image_height:.3}",
                    image_x, image_y
                ));
                record_source_spans(&logical.source, &mut pending.source_spans);
                pending.ops.push(DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: image_x,
                        y: image_y,
                        width: image_width,
                        height: image_height,
                    },
                    asset_ref: logical.path.clone(),
                    asset_format: logical.asset_format,
                    page_selection: logical.page_selection.clone(),
                    asset_hash: logical.asset_hash.clone(),
                    natural_width_pt: Some(natural_image_width),
                    natural_height_pt: Some(natural_image_height),
                    crop,
                    scale,
                    rotation,
                    diagnostic: image_diagnostic,
                    source: logical.source.clone(),
                }));

                if let Some(caption) = &logical.caption {
                    if !pending.text.is_empty() {
                        pending.text.push('\n');
                        pending.hash_input.push('\n');
                    }
                    pending.text.push_str(caption);
                    pending.hash_input.push_str(caption);
                    let caption_source = logical
                        .caption_source
                        .clone()
                        .unwrap_or_else(|| logical.source.clone());
                    let caption_advance =
                        text_advance_pt(caption, &body_font, options.body_font_size_pt);
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!(
                        "text_segment:{:.3}:{caption_advance:.3}:{}",
                        image_x, caption
                    ));
                    record_source_spans(&caption_source, &mut pending.source_spans);
                    pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                        origin: Point {
                            x: image_x,
                            y: image_y + image_height,
                        },
                        text: caption.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        approximate_advance_pt: caption_advance,
                        glyphs: None,
                        clusters: None,
                        source: caption_source,
                    }));
                }
            }
        }
    }
    if let Some(row) = pending_image_row.take() {
        y = row.y + row.height_pt + row.gap_after_pt;
    }
    drop(emit_due_destinations);
    for label in pending_labels.drain(..) {
        record_source_spans(&label.source, &mut pending.source_spans);
        pending.hash_input.push('\u{1f}');
        pending.hash_input.push_str("dest:");
        pending.hash_input.push_str(&label.key);
        let column_left_pt =
            options.margin_left_pt + column_index as f32 * (column_width_pt + column_gap_pt);
        pending.ops.push(DrawOp::NamedDestination(Destination {
            name: label.key,
            point: Point {
                x: column_left_pt,
                y,
            },
            source: label.source,
        }));
    }

    if pending.ops.is_empty() && pages.is_empty() {
        pending.text = String::new();
        pending.hash_input = format!("options:{options:?}:font-metrics:basic-v1");
        finish_page(&mut pages, pending);
    } else if !pending.ops.is_empty() {
        finish_page(&mut pages, pending);
    }

    pages
}

fn text_advance_pt(text: &str, font: &FontRequest, size_pt: f32) -> f32 {
    text.chars()
        .map(|ch| {
            let em_width = match font.family {
                FontFamilyRequest::Mono => 0.6,
                FontFamilyRequest::Math => {
                    if ch.is_whitespace() {
                        0.25
                    } else if ch.is_ascii_digit() {
                        0.5
                    } else {
                        0.62
                    }
                }
                FontFamilyRequest::Serif
                | FontFamilyRequest::Sans
                | FontFamilyRequest::Named(_) => {
                    if ch.is_whitespace() {
                        0.25
                    } else if matches!(ch, 'i' | 'j' | 'l' | 'I' | '!' | '|' | '\'' | '`') {
                        0.28
                    } else if matches!(ch, '.' | ',' | ';' | ':' | '-' | '/' | '\\') {
                        0.33
                    } else if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
                        0.38
                    } else if matches!(ch, 'm' | 'w' | 'M' | 'W') {
                        0.82
                    } else if ch.is_ascii_uppercase() {
                        0.68
                    } else if ch.is_ascii_digit() {
                        0.5
                    } else if ch.is_ascii() {
                        0.5
                    } else {
                        0.8
                    }
                }
            };
            let series_adjust = if font.series == FontSeries::Bold && !ch.is_whitespace() {
                1.04
            } else {
                1.0
            };
            em_width * series_adjust * size_pt
        })
        .sum()
}

fn approximate_text_clusters(text: &str) -> Option<Vec<TextCluster>> {
    if text.is_empty() {
        return None;
    }
    let glyph_count = text.chars().count() as u32;
    if text.len() == glyph_count as usize {
        return Some(vec![TextCluster {
            text_start_utf8: 0,
            text_end_utf8: text.len() as u32,
            glyph_start: 0,
            glyph_end: glyph_count,
        }]);
    }
    let mut clusters = Vec::new();
    for (glyph_index, (start, ch)) in text.char_indices().enumerate() {
        clusters.push(TextCluster {
            text_start_utf8: start as u32,
            text_end_utf8: (start + ch.len_utf8()) as u32,
            glyph_start: glyph_index as u32,
            glyph_end: glyph_index as u32 + 1,
        });
    }
    Some(clusters)
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        AbstractBlock, BibliographyBlock, BibliographyItemIr, CitationInline, CitationStyleHint,
        DisplayMathBlock, DocumentClassIr, DocumentIr, DocumentLayoutIntent, DrawOp, FontSeries,
        GraphicAssetDensity, GraphicAssetDensityUnit, GraphicAssetDimensions, GraphicAssetFormat,
        GraphicBlock, GraphicPageSelection, HeadingBlock, ImageCrop, ImageRotation, ImageScale,
        ImageTrim, ImageViewport, InlineNode, IrBlock, LabelDefinitionIr, LayoutAlignment,
        LayoutContainerBlock, LinkInline, ListBlock, ListItemIr, ListKind, PageBreakBlock,
        PageBreakKind, ParagraphBlock, Point, ProvenanceSpan, ReferenceInline, SourceProvenance,
        SourceSpan, TableBlock, TableCell, TableColumnAlignment, TableColumnSpec, TableRow,
        TableRuleSpan, TextCluster, TitleBlock,
    };

    use super::{PageDisplayListOptions, build_page_display_lists, parse_table_width_spec_pt};

    #[test]
    fn builds_positioned_text_runs_from_document_ir() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::TitleBlock(TitleBlock {
                    title: Some("A Paper".to_string()),
                    title_source: None,
                    authors: vec!["Ada Lovelace".to_string()],
                    author_sources: Vec::new(),
                    affiliations: Vec::new(),
                    affiliation_sources: Vec::new(),
                    correspondence: Vec::new(),
                    correspondence_sources: Vec::new(),
                    date: None,
                    date_source: None,
                    keywords: Vec::new(),
                    keyword_sources: Vec::new(),
                    pacs: Vec::new(),
                    pacs_sources: Vec::new(),
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "Hello world".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            PageDisplayListOptions::default(),
        );

        assert_eq!(display_lists.len(), 1);
        let text_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text_runs[0].text, "A Paper");
        assert_eq!(text_runs[0].font.role, tex_render_model::FontRole::Heading);
        assert_eq!(text_runs[1].text, "Ada Lovelace");
        assert_eq!(text_runs[2].text, "Hello world");
        assert_eq!(display_lists[0].source_spans.len(), 1);
    }

    #[test]
    fn table_display_list_text_aligns_columns_by_cell_width() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: Vec::new(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Longer".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Alpha".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"A     | Longer"), "{lines:?}");
        assert!(lines.contains(&"Alpha | B"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_honors_column_alignment_specs() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Center,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Long".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Left".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Wide".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "9".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"A    |  B   | Long"), "{lines:?}");
        assert!(lines.contains(&"Left | Wide |    9"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_aligns_decimal_columns() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Decimal,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "3.4".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "12".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "C".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "0.25".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"A |  3.4 "), "{lines:?}");
        assert!(lines.contains(&"B | 12   "), "{lines:?}");
        assert!(lines.contains(&"C |  0.25"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_uses_intercolumn_separators() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: Some("--".to_string()),
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "1".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "2".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"A--1"), "{lines:?}");
        assert!(lines.contains(&"B--2"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains(" | ")), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_uses_column_cell_hooks() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: Some("+".to_string()),
                        cell_suffix: Some("!".to_string()),
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "A".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "1".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"+A! | 1"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_uses_fixed_width_column_hints() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: Some(66_000),
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "A".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "1".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"A          | 1"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_stretches_flexible_columns_to_table_width_spec() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabularx".to_string(),
                width_spec: Some("\\textwidth".to_string()),
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Paragraph,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "Alpha".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "Beta".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let line = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .find(|line| line.starts_with("Alpha") && line.contains("Beta"))
            .expect("stretched table row");

        assert!(line.chars().count() > "Alpha | Beta".chars().count());
    }

    #[test]
    fn parses_simple_table_width_dimexpr_specs() {
        let options = PageDisplayListOptions::default();
        let content_width_pt = options.page_width_pt - options.margin_left_pt * 2.0;

        assert_eq!(
            parse_table_width_spec_pt("\\textwidth", content_width_pt, &options),
            Some(content_width_pt)
        );
        assert_eq!(
            parse_table_width_spec_pt(
                "\\dimexpr\\textwidth-36pt\\relax",
                content_width_pt,
                &options,
            ),
            Some(content_width_pt - 36.0)
        );
        assert_eq!(
            parse_table_width_spec_pt(
                "\\dimexpr\\textwidth-2\\tabcolsep\\relax",
                content_width_pt,
                &options,
            ),
            Some(content_width_pt - 12.0)
        );
    }

    #[test]
    fn table_display_list_text_stretches_simple_dimexpr_width_specs() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let build_line_len = |width_spec: Option<&str>| -> usize {
            let display_lists = build_page_display_lists(
                &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                    environment: "tabularx".to_string(),
                    width_spec: width_spec.map(ToOwned::to_owned),
                    columns: vec![
                        TableColumnSpec {
                            alignment: TableColumnAlignment::Left,
                            rule_before: false,
                            rule_before_count: 0,
                            rule_after: false,
                            rule_after_count: 0,
                            separator_after: None,
                            width_pt_milli: None,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableColumnSpec {
                            alignment: TableColumnAlignment::Paragraph,
                            rule_before: false,
                            rule_before_count: 0,
                            rule_after: false,
                            rule_after_count: 0,
                            separator_after: None,
                            width_pt_milli: None,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rows: vec![TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Alpha".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Beta".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    }],
                    caption: None,
                    caption_source: None,
                    source: source.clone(),
                })]),
                PageDisplayListOptions::default(),
            );
            display_lists[0]
                .ops
                .iter()
                .filter_map(|op| match op {
                    DrawOp::TextRun(run) => Some(run.text.as_str()),
                    _ => None,
                })
                .find(|line| line.starts_with("Alpha") && line.contains("Beta"))
                .expect("table row")
                .chars()
                .count()
        };

        let natural = build_line_len(None);
        let full = build_line_len(Some("\\textwidth"));
        let minus_absolute = build_line_len(Some("\\dimexpr\\textwidth-36pt\\relax"));
        let minus_register = build_line_len(Some("\\dimexpr\\textwidth-20\\tabcolsep\\relax"));

        assert!(minus_absolute > natural, "{natural} {minus_absolute}");
        assert!(minus_absolute < full, "{minus_absolute} {full}");
        assert!(minus_register > natural, "{natural} {minus_register}");
        assert!(minus_register < full, "{minus_register} {full}");
    }

    #[test]
    fn table_display_list_text_stretches_target_width_separators_when_no_flexible_columns() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular*".to_string(),
                width_spec: Some("120pt".to_string()),
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "Alpha".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "Beta".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let line = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .find(|line| line.starts_with("Alpha"))
            .expect("table row");

        assert!(line.chars().count() > "Alpha | Beta".chars().count());
        assert!(line.starts_with("Alpha | "), "{line:?}");
        assert!(line.ends_with("Beta"), "{line:?}");
    }

    #[test]
    fn table_display_list_textwidth_target_width_does_not_wrap_rows() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular*".to_string(),
                width_spec: Some("\\textwidth".to_string()),
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "Alpha".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "Beta".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let line = lines
            .iter()
            .find(|line| line.starts_with("Alpha"))
            .expect("table row");

        assert!(line.ends_with("Beta"), "{lines:?}");
        assert!(line.chars().count() > "Alpha | Beta".chars().count());
        assert!(
            !lines.iter().any(|line| *line == "Beta"),
            "target-width row wrapped: {lines:?}"
        );
    }

    #[test]
    fn resizebox_width_scales_oversized_full_width_table_without_wrapping() {
        let source = SourceProvenance::file("main.tex", 0, 96);
        let columns = (0..2)
            .map(|_| TableColumnSpec {
                alignment: TableColumnAlignment::Left,
                rule_before: false,
                rule_before_count: 0,
                rule_after: false,
                rule_after_count: 0,
                separator_after: None,
                width_pt_milli: None,
                cell_prefix: None,
                cell_suffix: None,
            })
            .collect();
        let cells = ["A".repeat(40), "B".repeat(40)]
            .into_iter()
            .map(|text| TableCell {
                text,
                column_span: None,
                row_span: None,
                alignment: None,
                rule_before_count: 0,
                rule_after_count: 0,
                cell_prefix: None,
                cell_suffix: None,
            })
            .collect();
        let options = PageDisplayListOptions {
            page_width_pt: 600.0,
            margin_left_pt: 50.0,
            column_count: 2,
            column_gap_pt: 20.0,
            body_font_size_pt: 10.0,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::FullWidthTable(TableBlock {
                environment: "tabular".to_string(),
                width_spec: Some("0.5\\textwidth".to_string()),
                columns,
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells,
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            options,
        );
        let row_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if run.text.contains('A') => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(row_runs.len(), 1, "scaled table row wrapped: {row_runs:?}");
        assert!(row_runs[0].size_pt < 10.0);
        assert!(row_runs[0].approximate_advance_pt <= 250.1);
        assert!(
            (row_runs[0].origin.x + row_runs[0].approximate_advance_pt / 2.0 - 300.0).abs() < 0.1
        );
    }

    #[test]
    fn table_display_list_emits_vertical_rule_ops_from_column_specs() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: true,
                        rule_before_count: 1,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "1".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "22".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(vertical_rules.len(), 3, "{vertical_rules:?}");
        assert!(
            vertical_rules
                .iter()
                .all(|rule| (rule.height - 28.0).abs() < 0.001),
            "{vertical_rules:?}"
        );
        assert!(lines.contains(&"A    1"), "{lines:?}");
        assert!(lines.contains(&"B   22"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains('|')), "{lines:?}");
    }

    #[test]
    fn table_display_list_vertical_rules_span_horizontal_rule_rows() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: true,
                        rule_before_count: 1,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: true,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "A".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "1".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: true,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let horizontal_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(horizontal_rules.len(), 2, "{horizontal_rules:?}");
        assert_eq!(vertical_rules.len(), 3, "{vertical_rules:?}");
        let top_rule_y = horizontal_rules
            .iter()
            .map(|rule| rule.y)
            .fold(f32::INFINITY, f32::min);
        let bottom_rule_bottom = horizontal_rules
            .iter()
            .map(|rule| rule.y + rule.height)
            .fold(0.0, f32::max);
        assert!(
            vertical_rules
                .iter()
                .all(|rule| (rule.y - top_rule_y).abs() < 0.001
                    && (rule.y + rule.height - bottom_rule_bottom).abs() < 0.001),
            "{vertical_rules:?} {horizontal_rules:?}"
        );
        assert!(
            vertical_rules
                .iter()
                .all(|rule| rule.height < 16.0 && rule.height > 14.0),
            "{vertical_rules:?}"
        );
        assert!(lines.contains(&"A   1"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains('|')), "{lines:?}");
    }

    #[test]
    fn table_display_list_vertical_rules_keep_internal_horizontal_rule_connections() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: true,
                        rule_before_count: 1,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: true,
                        rule_after_count: 1,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "1".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: true,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "22".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let horizontal_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(horizontal_rules.len(), 1, "{horizontal_rules:?}");
        assert_eq!(vertical_rules.len(), 3, "{vertical_rules:?}");
        assert!(
            vertical_rules
                .iter()
                .all(|rule| (rule.height - 28.0).abs() < 0.001),
            "{vertical_rules:?}"
        );
    }

    #[test]
    fn table_display_list_partial_rule_limits_vertical_rule_stubs_to_span() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let cell = |text: &str| TableCell {
            text: text.to_string(),
            column_span: None,
            row_span: None,
            alignment: None,
            rule_before_count: 0,
            rule_after_count: 0,
            cell_prefix: None,
            cell_suffix: None,
        };
        let column = |alignment| TableColumnSpec {
            alignment,
            rule_before: true,
            rule_before_count: 1,
            rule_after: true,
            rule_after_count: 1,
            separator_after: None,
            width_pt_milli: None,
            cell_prefix: None,
            cell_suffix: None,
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    column(TableColumnAlignment::Left),
                    column(TableColumnAlignment::Center),
                    column(TableColumnAlignment::Right),
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![cell("A"), cell("B"), cell("C")],
                        rule_below: false,
                        partial_rules_below: vec![TableRuleSpan {
                            start_column: 1,
                            end_column: 2,
                            trim_start: false,
                            trim_end: false,
                            trim_start_pt_milli: None,
                            trim_end_pt_milli: None,
                        }],
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![cell("D"), cell("E"), cell("F")],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let horizontal_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(horizontal_rules.len(), 1, "{horizontal_rules:?}");
        let horizontal_rule = horizontal_rules[0];
        let overlapping_vertical_rules = vertical_rules
            .iter()
            .filter(|rule| {
                rule.y <= horizontal_rule.y + horizontal_rule.height + 0.01
                    && rule.y + rule.height >= horizontal_rule.y - 0.01
                    && rule.x >= horizontal_rule.x - 0.01
                    && rule.x <= horizontal_rule.x + horizontal_rule.width + 0.01
            })
            .collect::<Vec<_>>();

        assert_eq!(
            overlapping_vertical_rules.len(),
            2,
            "{overlapping_vertical_rules:?} {horizontal_rule:?}"
        );
        assert!(
            overlapping_vertical_rules.iter().all(|rule| {
                rule.x >= horizontal_rule.x - 0.01
                    && rule.x <= horizontal_rule.x + horizontal_rule.width + 0.01
            }),
            "{overlapping_vertical_rules:?} {horizontal_rule:?}"
        );
    }

    #[test]
    fn table_display_list_trimmed_partial_rule_omits_trimmed_side_vertical_rule_stubs() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let cell = |text: &str| TableCell {
            text: text.to_string(),
            column_span: None,
            row_span: None,
            alignment: None,
            rule_before_count: 0,
            rule_after_count: 0,
            cell_prefix: None,
            cell_suffix: None,
        };
        let column = |alignment| TableColumnSpec {
            alignment,
            rule_before: true,
            rule_before_count: 1,
            rule_after: true,
            rule_after_count: 1,
            separator_after: None,
            width_pt_milli: None,
            cell_prefix: None,
            cell_suffix: None,
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    column(TableColumnAlignment::Left),
                    column(TableColumnAlignment::Center),
                    column(TableColumnAlignment::Right),
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![cell("A"), cell("B"), cell("C")],
                        rule_below: false,
                        partial_rules_below: vec![TableRuleSpan {
                            start_column: 0,
                            end_column: 1,
                            trim_start: true,
                            trim_end: true,
                            trim_start_pt_milli: None,
                            trim_end_pt_milli: None,
                        }],
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![cell("D"), cell("E"), cell("F")],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let horizontal_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.width > rect.height => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(horizontal_rules.len(), 1, "{horizontal_rules:?}");
        let horizontal_rule = horizontal_rules[0];
        let overlapping_vertical_rules = vertical_rules
            .iter()
            .filter(|rule| {
                rule.y <= horizontal_rule.y + horizontal_rule.height + 0.01
                    && rule.y + rule.height >= horizontal_rule.y - 0.01
                    && rule.x >= horizontal_rule.x - 0.01
                    && rule.x <= horizontal_rule.x + horizontal_rule.width + 0.01
            })
            .collect::<Vec<_>>();

        assert_eq!(
            overlapping_vertical_rules.len(),
            1,
            "{overlapping_vertical_rules:?} {horizontal_rule:?}"
        );
        assert!(
            overlapping_vertical_rules.iter().all(|rule| {
                rule.x >= horizontal_rule.x - 0.01
                    && rule.x <= horizontal_rule.x + horizontal_rule.width + 0.01
            }),
            "{overlapping_vertical_rules:?} {horizontal_rule:?}"
        );
    }

    #[test]
    fn table_display_list_emits_repeated_vertical_rule_ops_from_column_specs() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: true,
                        rule_before_count: 2,
                        rule_after: true,
                        rule_after_count: 2,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Right,
                        rule_before: true,
                        rule_before_count: 2,
                        rule_after: true,
                        rule_after_count: 2,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![TableRow {
                    rule_above: false,
                    partial_rules_above: Vec::new(),
                    cells: vec![
                        TableCell {
                            text: "A".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                        TableCell {
                            text: "1".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rule_below: false,
                    partial_rules_below: Vec::new(),
                }],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let vertical_rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) if rect.height > rect.width => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(vertical_rules.len(), 6, "{vertical_rules:?}");
        assert!(lines.contains(&"A   1"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains('|')), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_renders_horizontal_rules() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: Vec::new(),
                rows: vec![
                    TableRow {
                        rule_above: true,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Head".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Value".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: true,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: true,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(
            !lines.iter().any(|line| line.contains("------------")),
            "{lines:?}"
        );
        assert!(lines.contains(&"Head | Value"), "{lines:?}");
        assert!(lines.contains(&"A    | B"), "{lines:?}");
        let row_origins = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if !run.text.is_empty() => {
                    Some((run.text.as_str(), run.origin))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let head_y = row_origins
            .iter()
            .find_map(|(text, origin)| (*text == "Head | Value").then_some(origin.y))
            .expect("header row origin");
        let body_y = row_origins
            .iter()
            .find_map(|(text, origin)| (*text == "A    | B").then_some(origin.y))
            .expect("body row origin");
        assert!((body_y - head_y - 14.0).abs() < 0.001, "{row_origins:?}");
        let rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 3, "{rules:?}");
        assert!(rules.iter().all(|rule| rule.width > 70.0), "{rules:?}");
    }

    #[test]
    fn table_display_list_text_renders_partial_horizontal_rules() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: Vec::new(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Head".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Value".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: vec![TableRuleSpan {
                            start_column: 1,
                            end_column: 2,
                            trim_start: false,
                            trim_end: false,
                            trim_start_pt_milli: None,
                            trim_end_pt_milli: None,
                        }],
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "C".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"Head | Value | Tail"), "{lines:?}");
        assert!(
            !lines
                .iter()
                .any(|line| line.contains(".......") || line.contains("------------")),
            "{lines:?}"
        );
        assert!(lines.contains(&"A    | B     | C"), "{lines:?}");
        let rules = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rule(rect) => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 1, "{rules:?}");
        assert!(rules[0].x > PageDisplayListOptions::default().margin_left_pt);
    }

    #[test]
    fn table_display_list_partial_rule_uses_pt_trim_lengths() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let build = |trim_start_pt_milli: Option<u32>, trim_end_pt_milli: Option<u32>| {
            build_page_display_lists(
                &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                    environment: "tabular".to_string(),
                    width_spec: None,
                    columns: Vec::new(),
                    rows: vec![
                        TableRow {
                            rule_above: false,
                            partial_rules_above: Vec::new(),
                            cells: vec![
                                TableCell {
                                    text: "Head".to_string(),
                                    column_span: None,
                                    row_span: None,
                                    alignment: None,
                                    rule_before_count: 0,
                                    rule_after_count: 0,
                                    cell_prefix: None,
                                    cell_suffix: None,
                                },
                                TableCell {
                                    text: "Value".to_string(),
                                    column_span: None,
                                    row_span: None,
                                    alignment: None,
                                    rule_before_count: 0,
                                    rule_after_count: 0,
                                    cell_prefix: None,
                                    cell_suffix: None,
                                },
                            ],
                            rule_below: false,
                            partial_rules_below: vec![TableRuleSpan {
                                start_column: 0,
                                end_column: 1,
                                trim_start: trim_start_pt_milli.is_some(),
                                trim_end: trim_end_pt_milli.is_some(),
                                trim_start_pt_milli,
                                trim_end_pt_milli,
                            }],
                        },
                        TableRow {
                            rule_above: false,
                            partial_rules_above: Vec::new(),
                            cells: vec![
                                TableCell {
                                    text: "A".to_string(),
                                    column_span: None,
                                    row_span: None,
                                    alignment: None,
                                    rule_before_count: 0,
                                    rule_after_count: 0,
                                    cell_prefix: None,
                                    cell_suffix: None,
                                },
                                TableCell {
                                    text: "B".to_string(),
                                    column_span: None,
                                    row_span: None,
                                    alignment: None,
                                    rule_before_count: 0,
                                    rule_after_count: 0,
                                    cell_prefix: None,
                                    cell_suffix: None,
                                },
                            ],
                            rule_below: false,
                            partial_rules_below: Vec::new(),
                        },
                    ],
                    caption: None,
                    caption_source: None,
                    source: source.clone(),
                })]),
                PageDisplayListOptions::default(),
            )
        };
        let untrimmed = build(None, None);
        let trimmed = build(Some(2_500), Some(4_000));
        let horizontal_rule = |display_lists: &[tex_render_model::PageDisplayList]| {
            display_lists[0]
                .ops
                .iter()
                .find_map(|op| match op {
                    DrawOp::Rule(rect) if rect.width > rect.height => Some(rect.to_owned()),
                    _ => None,
                })
                .expect("horizontal partial rule")
        };
        let untrimmed_rule = horizontal_rule(&untrimmed);
        let trimmed_rule = horizontal_rule(&trimmed);
        let untrimmed_end = untrimmed_rule.x + untrimmed_rule.width;
        let trimmed_end = trimmed_rule.x + trimmed_rule.width;

        assert!(
            ((trimmed_rule.x - untrimmed_rule.x) - 2.5).abs() <= 0.01,
            "{trimmed_rule:?} {untrimmed_rule:?}"
        );
        assert!(
            ((untrimmed_end - trimmed_end) - 4.0).abs() <= 0.01,
            "{trimmed_rule:?} {untrimmed_rule:?}"
        );
    }

    #[test]
    fn table_display_list_text_uses_multicolumn_spans() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: Vec::new(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Wide".to_string(),
                                column_span: Some(2),
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "C".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"Wide  | Tail"), "{lines:?}");
        assert!(lines.contains(&"A | B | C"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_uses_visible_separator_widths_inside_multicolumn_spans() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: vec![
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: Some("------".to_string()),
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableColumnSpec {
                        alignment: TableColumnAlignment::Left,
                        rule_before: false,
                        rule_before_count: 0,
                        rule_after: false,
                        rule_after_count: 0,
                        separator_after: None,
                        width_pt_milli: None,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Wide".to_string(),
                                column_span: Some(2),
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Alpha".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Beta".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"Wide            | Tail"), "{lines:?}");
        assert!(lines.contains(&"Alpha------Beta | Tail"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_offsets_cells_below_multirow_spans() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                width_spec: None,
                columns: Vec::new(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Span".to_string(),
                                column_span: None,
                                row_span: Some(2),
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![TableCell {
                            text: "B".to_string(),
                            column_span: None,
                            row_span: None,
                            alignment: None,
                            rule_before_count: 0,
                            rule_after_count: 0,
                            cell_prefix: None,
                            cell_suffix: None,
                        }],
                        rule_below: false,
                        partial_rules_below: Vec::new(),
                    },
                ],
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let lines = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.contains(&"Span | A"), "{lines:?}");
        assert!(lines.contains(&"     | B"), "{lines:?}");
    }

    #[test]
    fn page_source_spans_include_expansion_stack_frames() {
        let call_span = SourceSpan {
            path: "main.tex".into(),
            start_utf8: 0,
            end_utf8: 9,
        };
        let definition_span = SourceSpan {
            path: "macros.tex".into(),
            start_utf8: 12,
            end_utf8: 40,
        };
        let source = SourceProvenance::file("main.tex", 20, 24).with_expansion_frame(
            tex_render_model::ExpansionFrame {
                call_span: ProvenanceSpan::File(call_span.clone()),
                definition_span: Some(ProvenanceSpan::File(definition_span.clone())),
                command_name: Some("mytext".to_string()),
            },
        );
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "Text".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert!(display_lists[0].source_spans.contains(&call_span));
        assert!(display_lists[0].source_spans.contains(&definition_span));
    }

    #[test]
    fn graphic_source_spans_include_expansion_stack_frames() {
        let call_span = SourceSpan {
            path: "main.tex".into(),
            start_utf8: 0,
            end_utf8: 18,
        };
        let definition_span = SourceSpan {
            path: "macros.tex".into(),
            start_utf8: 10,
            end_utf8: 52,
        };
        let source = SourceProvenance::file("main.tex", 24, 42).with_expansion_frame(
            tex_render_model::ExpansionFrame {
                call_span: ProvenanceSpan::File(call_span.clone()),
                definition_span: Some(ProvenanceSpan::File(definition_span.clone())),
                command_name: Some("mygraphic".to_string()),
            },
        );
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: None,
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert!(display_lists[0].source_spans.contains(&call_span));
        assert!(display_lists[0].source_spans.contains(&definition_span));
    }

    #[test]
    fn text_runs_include_approximate_text_clusters() {
        let source = SourceProvenance::file("main.tex", 0, 3);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "aé".to_string(),
                    source,
                }],
                source: SourceProvenance::file("main.tex", 0, 3),
            })]),
            PageDisplayListOptions::default(),
        );

        let run = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::TextRun(run) => Some(run),
                _ => None,
            })
            .expect("text run");

        assert_eq!(
            run.clusters.clone(),
            Some(vec![
                TextCluster {
                    text_start_utf8: 0,
                    text_end_utf8: 1,
                    glyph_start: 0,
                    glyph_end: 1,
                },
                TextCluster {
                    text_start_utf8: 1,
                    text_end_utf8: 3,
                    glyph_start: 1,
                    glyph_end: 2,
                }
            ])
        );
    }

    #[test]
    fn text_run_advances_use_basic_font_metrics() {
        let source = SourceProvenance::file("main.tex", 0, 7);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "WWW".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "iii".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            PageDisplayListOptions::default(),
        );

        let advances = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if run.text == "WWW" || run.text == "iii" => {
                    Some((run.text.as_str(), run.approximate_advance_pt))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let wide = advances
            .iter()
            .find_map(|(text, advance)| (*text == "WWW").then_some(*advance))
            .expect("wide advance");
        let narrow = advances
            .iter()
            .find_map(|(text, advance)| (*text == "iii").then_some(*advance))
            .expect("narrow advance");
        assert!(wide > narrow, "wide={wide} narrow={narrow}");
    }

    #[test]
    fn text_run_segmentation_affects_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 2);
        let combined = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "Wi".to_string(),
                    source: source.clone(),
                }],
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let split = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![
                    InlineNode::Text {
                        text: "W".to_string(),
                        source: source.clone(),
                    },
                    InlineNode::Text {
                        text: "i".to_string(),
                        source: source.clone(),
                    },
                ],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_ne!(combined[0].content_hash, split[0].content_hash);
        assert_ne!(combined[0].page_id, split[0].page_id);
    }

    #[test]
    fn page_ids_use_content_occurrence_not_absolute_page_index() {
        let paragraph = |text: &str, start_utf8: u32| {
            let source =
                SourceProvenance::file("main.tex", start_utf8, start_utf8 + text.len() as u32);
            IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: text.to_string(),
                    source: source.clone(),
                }],
                source,
            })
        };
        let options = PageDisplayListOptions {
            page_height_pt: 36.0,
            margin_top_pt: 4.0,
            margin_bottom_pt: 4.0,
            line_height_pt: 10.0,
            block_gap_pt: 10.0,
            ..PageDisplayListOptions::default()
        };

        let original = build_page_display_lists(
            &DocumentIr::new(vec![
                paragraph("first", 0),
                paragraph("tail", 10),
                paragraph("duplicate", 20),
                paragraph("duplicate", 30),
            ]),
            options.clone(),
        );
        let shifted = build_page_display_lists(
            &DocumentIr::new(vec![
                paragraph("first", 0),
                paragraph("inserted", 40),
                paragraph("tail", 10),
                paragraph("duplicate", 20),
                paragraph("duplicate", 30),
            ]),
            options,
        );

        assert_eq!(original.len(), 4);
        assert_eq!(shifted.len(), 5);

        let original_tail = original
            .iter()
            .find(|page| {
                page.ops.iter().any(|op| {
                    matches!(
                        op,
                        DrawOp::TextRun(run) if run.text == "tail"
                    )
                })
            })
            .expect("original tail page");
        let shifted_tail = shifted
            .iter()
            .find(|page| {
                page.ops.iter().any(|op| {
                    matches!(
                        op,
                        DrawOp::TextRun(run) if run.text == "tail"
                    )
                })
            })
            .expect("shifted tail page");
        assert_eq!(original_tail.content_hash, shifted_tail.content_hash);
        assert_eq!(original_tail.page_id, shifted_tail.page_id);

        let original_duplicates = original
            .iter()
            .filter(|page| {
                page.ops.iter().any(|op| {
                    matches!(
                        op,
                        DrawOp::TextRun(run) if run.text == "duplicate"
                    )
                })
            })
            .collect::<Vec<_>>();
        let shifted_duplicates = shifted
            .iter()
            .filter(|page| {
                page.ops.iter().any(|op| {
                    matches!(
                        op,
                        DrawOp::TextRun(run) if run.text == "duplicate"
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(original_duplicates.len(), 2);
        assert_eq!(shifted_duplicates.len(), 2);
        assert_eq!(
            original_duplicates[0].content_hash,
            original_duplicates[1].content_hash
        );
        assert_ne!(
            original_duplicates[0].page_id,
            original_duplicates[1].page_id
        );
        assert_eq!(
            original_duplicates[0].page_id,
            shifted_duplicates[0].page_id
        );
        assert_eq!(
            original_duplicates[1].page_id,
            shifted_duplicates[1].page_id
        );
    }

    #[test]
    fn preserves_inline_node_sources_in_text_runs() {
        let text_source = SourceProvenance::file("main.tex", 0, 4);
        let reference_source = SourceProvenance::file("main.tex", 9, 18);
        let citation_source = SourceProvenance::file("main.tex", 30, 33);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![
                    InlineNode::Text {
                        text: "See ".to_string(),
                        source: text_source,
                    },
                    InlineNode::Reference(ReferenceInline {
                        keys: vec!["sec:intro".to_string()],
                        command: "ref".to_string(),
                        resolved_target: Some("1".to_string()),
                        display_text: "1".to_string(),
                        source: reference_source,
                    }),
                    InlineNode::Text {
                        text: " and ".to_string(),
                        source: SourceProvenance::file("main.tex", 19, 24),
                    },
                    InlineNode::Citation(CitationInline {
                        keys: vec!["key".to_string()],
                        style_hint: CitationStyleHint::Parenthetical,
                        resolved_label: Some("[7]".to_string()),
                        display_text: "[7]".to_string(),
                        source: citation_source,
                    }),
                    InlineNode::Text {
                        text: ".".to_string(),
                        source: SourceProvenance::file("main.tex", 35, 36),
                    },
                ],
                source: SourceProvenance::file("main.tex", 0, 36),
            })]),
            PageDisplayListOptions::default(),
        );

        let text_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(text_runs.iter().any(|run| {
            run.text == "1"
                && matches!(
                    &run.source.primary,
                    tex_render_model::ProvenanceSpan::File(span)
                        if span.start_utf8 == 9 && span.end_utf8 == 18
                )
        }));
        assert!(text_runs.iter().any(|run| {
            run.text == "[7]"
                && matches!(
                        &run.source.primary,
                        tex_render_model::ProvenanceSpan::File(span)
                            if span.start_utf8 == 30 && span.end_utf8 == 33
                )
        }));
    }

    #[test]
    fn wrapped_lines_do_not_start_with_interword_space() {
        let source = SourceProvenance::file("main.tex", 0, 22);
        let options = PageDisplayListOptions {
            max_chars_per_line: 10,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![
                    InlineNode::Text {
                        text: "alpha".to_string(),
                        source: source.clone(),
                    },
                    InlineNode::Space {
                        source: source.clone(),
                    },
                    InlineNode::Text {
                        text: "beta".to_string(),
                        source: source.clone(),
                    },
                    InlineNode::Space {
                        source: source.clone(),
                    },
                    InlineNode::Text {
                        text: "gamma".to_string(),
                        source: source.clone(),
                    },
                ],
                source,
            })]),
            options.clone(),
        );

        let gamma = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "gamma" => Some(run),
            _ => None,
        });
        assert_eq!(gamma.map(|run| run.origin.x), Some(options.margin_left_pt));
    }

    #[test]
    fn wrapped_lines_prefer_word_boundaries() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let options = PageDisplayListOptions {
            max_chars_per_line: 8,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "alpha beta".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            options.clone(),
        );

        let text_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some((run.text.as_str(), run.origin.x)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            text_runs,
            vec![
                ("alpha ", options.margin_left_pt),
                ("beta", options.margin_left_pt),
            ]
        );
    }

    #[test]
    fn width_wrapping_moves_a_complete_word_to_the_next_line() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let options = PageDisplayListOptions {
            page_width_pt: 60.0,
            margin_left_pt: 10.0,
            max_chars_per_line: 100,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "alpha beta".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            options.clone(),
        );

        let text_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some((run.text.as_str(), run.origin.x)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            text_runs,
            vec![
                ("alpha ", options.margin_left_pt),
                ("beta", options.margin_left_pt),
            ]
        );
    }

    #[test]
    fn wraps_heading_text_by_approximate_available_width() {
        let source = SourceProvenance::file("main.tex", 0, 70);
        let options = PageDisplayListOptions::default();
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Heading(HeadingBlock {
                level: 1,
                number: None,
                content: vec![InlineNode::Text {
                    text: "x".repeat(70),
                    source,
                }],
                source: SourceProvenance::file("main.tex", 0, 70),
            })]),
            options.clone(),
        );

        let available_width = options.page_width_pt - options.margin_left_pt * 2.0;
        let heading_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(heading_runs.len() > 1);
        assert!(
            heading_runs
                .iter()
                .all(|run| run.approximate_advance_pt <= available_width)
        );
    }

    #[test]
    fn heading_numbers_survive_display_list_text() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Heading(HeadingBlock {
                level: 1,
                number: Some("1".to_string()),
                content: vec![InlineNode::Text {
                    text: "Intro".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        let text = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(text, "1 Intro");
    }

    #[test]
    fn normalized_math_text_survives_display_list_text() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::InlineMath {
                        raw_source: "\\alpha".to_string(),
                        normalized_text: Some("alpha".to_string()),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::DisplayMath(DisplayMathBlock {
                    raw_source: "\\beta".to_string(),
                    normalized_text: Some("beta".to_string()),
                    source,
                }),
            ]),
            PageDisplayListOptions::default(),
        );

        let text = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
        assert!(!text.contains("\\alpha"));
        assert!(!text.contains("\\beta"));
    }

    #[test]
    fn uses_title_field_sources_for_text_runs() {
        let block_source = SourceProvenance::file("main.tex", 40, 50);
        let title_source = SourceProvenance::file("main.tex", 7, 14);
        let author_source = SourceProvenance::file("main.tex", 24, 36);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::TitleBlock(TitleBlock {
                title: Some("A Paper".to_string()),
                title_source: Some(title_source),
                authors: vec!["Ada Lovelace".to_string()],
                author_sources: vec![author_source],
                affiliations: Vec::new(),
                affiliation_sources: Vec::new(),
                correspondence: Vec::new(),
                correspondence_sources: Vec::new(),
                date: None,
                date_source: None,
                keywords: Vec::new(),
                keyword_sources: Vec::new(),
                pacs: Vec::new(),
                pacs_sources: Vec::new(),
                source: block_source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::TextRun(run)
                    if run.text == "A Paper"
                        && matches!(
                            &run.source.primary,
                            tex_render_model::ProvenanceSpan::File(span)
                                if span.start_utf8 == 7 && span.end_utf8 == 14
                        )
            )
        }));
        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::TextRun(run)
                    if run.text == "Ada Lovelace"
                        && matches!(
                            &run.source.primary,
                            tex_render_model::ProvenanceSpan::File(span)
                                if span.start_utf8 == 24 && span.end_utf8 == 36
                        )
            )
        }));
    }

    #[test]
    fn derives_link_annotations_from_link_inline_nodes() {
        let link_source = SourceProvenance::file("main.tex", 6, 16);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Link(LinkInline {
                    target: "https://example.test/paper".to_string(),
                    display_text: "paper link".to_string(),
                    source: link_source,
                })],
                source: SourceProvenance::file("main.tex", 0, 16),
            })]),
            PageDisplayListOptions::default(),
        );

        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::TextRun(run) if run.text == "paper link"
            )
        }));
        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
            op,
            DrawOp::LinkAnnotation(link)
                if link.target == "https://example.test/paper"
                    && link.rect.width > 0.0
                    && matches!(
                        &link.source.primary,
                        tex_render_model::ProvenanceSpan::File(span)
                            if span.start_utf8 == 6 && span.end_utf8 == 16
                    )
            )
        }));
    }

    #[test]
    fn link_annotation_targets_affect_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let left = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Link(LinkInline {
                    target: "https://example.test/a".to_string(),
                    display_text: "paper".to_string(),
                    source: source.clone(),
                })],
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let right = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Link(LinkInline {
                    target: "https://example.test/b".to_string(),
                    display_text: "paper".to_string(),
                    source: source.clone(),
                })],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_ne!(left[0].content_hash, right[0].content_hash);
        assert_ne!(left[0].page_id, right[0].page_id);
    }

    #[test]
    fn link_annotation_geometry_affects_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let left = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![
                    InlineNode::Link(LinkInline {
                        target: "https://example.test".to_string(),
                        display_text: "A".to_string(),
                        source: source.clone(),
                    }),
                    InlineNode::Text {
                        text: "B".to_string(),
                        source: source.clone(),
                    },
                ],
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let right = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![
                    InlineNode::Text {
                        text: "A".to_string(),
                        source: source.clone(),
                    },
                    InlineNode::Link(LinkInline {
                        target: "https://example.test".to_string(),
                        display_text: "B".to_string(),
                        source: source.clone(),
                    }),
                ],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_ne!(left[0].content_hash, right[0].content_hash);
        assert_ne!(left[0].page_id, right[0].page_id);
    }

    #[test]
    fn layout_options_affect_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 5);
        let document = DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
            content: vec![InlineNode::Text {
                text: "hello".to_string(),
                source: source.clone(),
            }],
            source,
        })]);
        let default = build_page_display_lists(&document, PageDisplayListOptions::default());
        let larger_font = build_page_display_lists(
            &document,
            PageDisplayListOptions {
                body_font_size_pt: 13.0,
                ..PageDisplayListOptions::default()
            },
        );

        assert_ne!(default[0].content_hash, larger_font[0].content_hash);
        assert_ne!(default[0].page_id, larger_font[0].page_id);
    }

    #[test]
    fn text_run_style_affects_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 5);
        let paragraph = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "Intro".to_string(),
                    source: source.clone(),
                }],
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let heading = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Heading(HeadingBlock {
                level: 1,
                number: None,
                content: vec![InlineNode::Text {
                    text: "Intro".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_ne!(paragraph[0].content_hash, heading[0].content_hash);
        assert_ne!(paragraph[0].page_id, heading[0].page_id);
    }

    #[test]
    fn label_definitions_emit_named_destinations_near_following_content() {
        let label_source = SourceProvenance::file("main.tex", 5, 22);
        let paragraph_source = SourceProvenance::file("main.tex", 23, 28);
        let display_lists = build_page_display_lists(
            &DocumentIr::with_labels(
                vec![IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "hello".to_string(),
                        source: paragraph_source.clone(),
                    }],
                    source: paragraph_source,
                })],
                vec![LabelDefinitionIr {
                    key: "sec:intro".to_string(),
                    source: label_source,
                }],
            ),
            PageDisplayListOptions::default(),
        );

        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::NamedDestination(destination)
                    if destination.name == "sec:intro"
                        && destination.point.x == 72.0
                        && destination.point.y == 72.0
            )
        }));
    }

    #[test]
    fn paginates_when_text_runs_exceed_page_height() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "one".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "two".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "three".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            PageDisplayListOptions {
                page_height_pt: 46.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );

        assert_eq!(display_lists.len(), 2);
    }

    #[test]
    fn fills_columns_before_starting_a_new_page() {
        let source = SourceProvenance::file("main.tex", 0, 33);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "one\ntwo\nthree\nfour\nfive\nsix\nseven".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 50.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                column_count: 2,
                column_gap_pt: 20.0,
                max_chars_per_line: 100,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );

        assert_eq!(display_lists.len(), 2);
        let second_column = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "four" => Some(run.origin),
            _ => None,
        });
        let next_page = display_lists[1].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "seven" => Some(run.origin),
            _ => None,
        });

        assert_eq!(second_column, Some(Point { x: 110.0, y: 10.0 }));
        assert_eq!(next_page, Some(Point { x: 10.0, y: 10.0 }));
    }

    #[test]
    fn infers_two_column_profiles_from_document_class_intent() {
        let source = SourceProvenance::file("main.tex", 0, 43);
        let article = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["10pt".to_string(), "twocolumn".to_string()],
                source: source.clone(),
            }),
            Vec::new(),
        );
        let ieee = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "IEEEtran".to_string(),
                options: vec!["journal".to_string()],
                source: source.clone(),
            }),
            Vec::new(),
        );
        let explicit_one_column = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "IEEEtran".to_string(),
                options: vec!["journal".to_string(), "onecolumn".to_string()],
                source: source.clone(),
            }),
            Vec::new(),
        );
        let a4_article = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["11pt".to_string(), "a4paper".to_string()],
                source: source.clone(),
            }),
            Vec::new(),
        );
        let llncs = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "llncs".to_string(),
                options: vec!["runningheads".to_string()],
                source: source.clone(),
            }),
            Vec::new(),
        );
        let a4_llncs = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "llncs".to_string(),
                options: vec!["a4paper".to_string()],
                source,
            }),
            Vec::new(),
        );

        let article_options = PageDisplayListOptions::for_document_ir(&article);
        let ieee_options = PageDisplayListOptions::for_document_ir(&ieee);

        let plain_article = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: Vec::new(),
                source: SourceProvenance::file("plain.tex", 0, 23),
            }),
            Vec::new(),
        );
        let plain_article_options = PageDisplayListOptions::for_document_ir(&plain_article);
        let ten_point_article = DocumentIr::with_document_class_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["10pt".to_string()],
                source: SourceProvenance::file("ten-point.tex", 0, 28),
            }),
            Vec::new(),
        );
        let ten_point_article_options = PageDisplayListOptions::for_document_ir(&ten_point_article);

        assert_eq!(article_options.column_count, 2);
        assert_eq!(article_options.body_font_size_pt, 10.0);
        assert_eq!(article_options.line_height_pt, 12.0);
        assert!((article_options.column_gap_pt - 9.96).abs() < 0.02);
        assert!((article_options.margin_left_pt - 72.0).abs() < 0.2);
        assert_eq!(ieee_options.column_count, 2);
        assert_eq!(ieee_options.margin_left_pt, 49.5);
        assert_eq!(ieee_options.body_font_size_pt, 9.0);
        let a4_options = PageDisplayListOptions::for_document_ir(&a4_article);
        assert_eq!(a4_options.page_width_pt, 595.276);
        assert_eq!(a4_options.page_height_pt, 841.89);
        assert_eq!(a4_options.body_font_size_pt, 11.0);
        assert_eq!(a4_options.line_height_pt, 13.6);
        assert_eq!(plain_article_options.body_font_size_pt, 10.0);
        assert_eq!(plain_article_options.line_height_pt, 12.0);
        assert!((plain_article_options.margin_left_pt - 134.1).abs() < 0.2);
        assert!((plain_article_options.margin_top_pt - 135.5).abs() < 0.5);
        assert!((plain_article_options.paragraph_first_line_indent_pt - 14.94).abs() < 0.02);
        assert_eq!(plain_article_options.paragraph_gap_pt, Some(0.0));
        assert!(plain_article_options.show_page_numbers);
        assert_eq!(ten_point_article_options.body_font_size_pt, 10.0);
        assert_eq!(ten_point_article_options.line_height_pt, 12.0);
        let llncs_options = PageDisplayListOptions::for_document_ir(&llncs);
        assert_eq!(llncs_options.page_width_pt, 612.0);
        assert_eq!(llncs_options.page_height_pt, 792.0);
        assert!((llncs_options.margin_left_pt - 133.1).abs() < 0.1);
        assert!((llncs_options.margin_top_pt - 125.8).abs() < 0.1);
        assert!((llncs_options.margin_bottom_pt - 129.1).abs() < 0.1);
        assert!((llncs_options.paragraph_first_line_indent_pt - 14.94).abs() < 0.02);
        assert_eq!(llncs_options.paragraph_gap_pt, Some(0.0));
        assert_eq!(llncs_options.body_font_size_pt, 10.0);
        let a4_llncs_options = PageDisplayListOptions::for_document_ir(&a4_llncs);
        assert_eq!(a4_llncs_options.page_width_pt, 595.276);
        assert_eq!(a4_llncs_options.page_height_pt, 841.89);
        assert!((a4_llncs_options.margin_left_pt - 124.7).abs() < 0.1);
        assert_eq!(
            PageDisplayListOptions::for_document_ir(&explicit_one_column),
            PageDisplayListOptions {
                max_chars_per_line: usize::MAX,
                ..PageDisplayListOptions::default()
            }
        );
    }

    #[test]
    fn package_layout_profile_takes_precedence_over_article_defaults() {
        let source = SourceProvenance::file("main.tex", 0, 72);
        let document = DocumentIr::with_document_class_layout_and_labels(
            Vec::new(),
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["10pt".to_string(), "twocolumn".to_string()],
                source,
            }),
            Some(DocumentLayoutIntent {
                profile: Some("wacv".to_string()),
                text_width_pt_milli: Some(495_000),
                text_height_pt_milli: Some(639_000),
                margin_top_pt_milli: Some(72_000),
                column_count: Some(2),
                column_gap_pt_milli: Some(22_500),
                body_font_size_pt_milli: Some(10_000),
                line_height_pt_milli: Some(10_500),
                ..DocumentLayoutIntent::default()
            }),
            Vec::new(),
        );

        let options = PageDisplayListOptions::for_document_ir(&document);

        assert_eq!(options.column_count, 2);
        assert_eq!(options.line_height_pt, 10.5);
        assert_eq!(options.margin_left_pt, 58.5);
        assert_eq!(options.margin_top_pt, 72.0);
        assert_eq!(options.margin_bottom_pt, 81.0);
        assert_eq!(options.paragraph_first_line_indent_pt, 0.0);
        assert_eq!(options.paragraph_gap_pt, None);
        assert!(!options.show_page_numbers);
    }

    #[test]
    fn article_paragraphs_indent_except_immediately_after_headings() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let document = DocumentIr::with_document_class_and_labels(
            vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "opening".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Heading(HeadingBlock {
                    level: 1,
                    number: Some("1".to_string()),
                    content: vec![InlineNode::Text {
                        text: "Heading".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "after-heading".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "later".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
            ],
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["letterpaper".to_string()],
                source,
            }),
            Vec::new(),
        );
        let options = PageDisplayListOptions::for_document_ir(&document);
        let pages = build_page_display_lists(&document, options.clone());
        let origin_x = |text: &str| {
            pages[0].ops.iter().find_map(|op| match op {
                DrawOp::TextRun(run) if run.text == text => Some(run.origin.x),
                _ => None,
            })
        };

        assert_eq!(
            origin_x("opening"),
            Some(options.margin_left_pt + options.paragraph_first_line_indent_pt)
        );
        assert_eq!(origin_x("after-heading"), Some(options.margin_left_pt));
        assert_eq!(
            origin_x("later"),
            Some(options.margin_left_pt + options.paragraph_first_line_indent_pt)
        );
    }

    #[test]
    fn article_pages_include_centered_generated_page_numbers() {
        let source = SourceProvenance::file("main.tex", 0, 5);
        let document = DocumentIr::with_document_class_and_labels(
            vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: "hello".to_string(),
                    source: source.clone(),
                }],
                source: source.clone(),
            })],
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["letterpaper".to_string()],
                source,
            }),
            Vec::new(),
        );
        let options = PageDisplayListOptions::for_document_ir(&document);
        let pages = build_page_display_lists(&document, options.clone());
        let page_number = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run)
                if matches!(
                    &run.source.primary,
                    ProvenanceSpan::Generated(span) if span.stable_id == "page-number:1"
                ) =>
            {
                Some(run)
            }
            _ => None,
        });

        let page_number = page_number.expect("generated page number");
        assert_eq!(page_number.text, "1");
        assert!(page_number.origin.y > options.page_height_pt - options.margin_bottom_pt);
        let content_center =
            options.margin_left_pt + (options.page_width_pt - options.margin_left_pt * 2.0) / 2.0;
        assert!(
            (page_number.origin.x + page_number.approximate_advance_pt / 2.0 - content_center)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn article_front_matter_uses_standard_author_and_abstract_layout() {
        let source = SourceProvenance::file("main.tex", 0, 128);
        let document = DocumentIr::with_document_class_and_labels(
            vec![
                IrBlock::TitleBlock(TitleBlock {
                    title: Some("A Paper".to_string()),
                    title_source: Some(source.clone()),
                    authors: vec!["Ada Example".to_string(), "Grace Sample".to_string()],
                    author_sources: vec![source.clone(), source.clone()],
                    affiliations: Vec::new(),
                    affiliation_sources: Vec::new(),
                    correspondence: Vec::new(),
                    correspondence_sources: Vec::new(),
                    date: Some("15 July 2026".to_string()),
                    date_source: Some(source.clone()),
                    keywords: Vec::new(),
                    keyword_sources: Vec::new(),
                    pacs: Vec::new(),
                    pacs_sources: Vec::new(),
                    source: source.clone(),
                }),
                IrBlock::Abstract(AbstractBlock {
                    content: vec![InlineNode::Text {
                        text: "Compact abstract body.".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
            ],
            Some(DocumentClassIr {
                name: "article".to_string(),
                options: vec!["letterpaper".to_string()],
                source,
            }),
            Vec::new(),
        );
        let options = PageDisplayListOptions::for_document_ir(&document);
        let pages = build_page_display_lists(&document, options.clone());
        let text_run = |text: &str| {
            pages[0].ops.iter().find_map(|op| match op {
                DrawOp::TextRun(run) if run.text == text => Some(run),
                _ => None,
            })
        };

        let title = text_run("A Paper").expect("title run");
        let first_author = text_run("Ada Example").expect("first author run");
        let second_author = text_run("Grace Sample").expect("second author run");
        let date = text_run("15 July 2026").expect("date run");
        let abstract_heading = text_run("Abstract").expect("abstract heading run");
        let abstract_body = text_run("Compact abstract body.").expect("abstract body run");

        assert_eq!(title.font.series, FontSeries::Regular);
        assert_eq!(first_author.origin.y, second_author.origin.y);
        assert_eq!(
            first_author.size_pt,
            options.author_date_font_size_pt.unwrap()
        );
        assert!(
            second_author.origin.x
                > first_author.origin.x + first_author.approximate_advance_pt + 20.0
        );
        assert!(
            (date.origin.y
                - first_author.origin.y
                - options.line_height_pt
                - options.author_date_gap_pt)
                .abs()
                < 0.01
        );
        assert_eq!(abstract_heading.font.series, FontSeries::Bold);
        let content_center =
            options.margin_left_pt + (options.page_width_pt - options.margin_left_pt * 2.0) / 2.0;
        assert!(
            (abstract_heading.origin.x + abstract_heading.approximate_advance_pt / 2.0
                - content_center)
                .abs()
                < 0.01
        );
        assert!(
            (abstract_body.origin.x
                - options.margin_left_pt
                - options.abstract_indent_pt
                - options.abstract_first_line_indent_pt)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn title_text_uses_and_centers_within_the_full_page_width() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let options = PageDisplayListOptions {
            page_width_pt: 200.0,
            page_height_pt: 200.0,
            margin_left_pt: 10.0,
            margin_top_pt: 10.0,
            margin_bottom_pt: 10.0,
            column_count: 2,
            column_gap_pt: 20.0,
            max_chars_per_line: 100,
            title_font_size_pt: 10.0,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::TitleBlock(TitleBlock {
                title: Some("MMMMMMMMMM".to_string()),
                title_source: Some(source.clone()),
                authors: Vec::new(),
                author_sources: Vec::new(),
                affiliations: Vec::new(),
                affiliation_sources: Vec::new(),
                correspondence: Vec::new(),
                correspondence_sources: Vec::new(),
                date: None,
                date_source: None,
                keywords: Vec::new(),
                keyword_sources: Vec::new(),
                pacs: Vec::new(),
                pacs_sources: Vec::new(),
                source,
            })]),
            options,
        );
        let title_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if !run.text.is_empty() => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(title_runs.len(), 1);
        assert!(title_runs[0].approximate_advance_pt > 80.0);
        assert!(title_runs[0].origin.x > 10.0);
    }

    #[test]
    fn later_columns_start_below_full_width_front_matter() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let mut blocks = vec![IrBlock::TitleBlock(TitleBlock {
            title: Some("Title".to_string()),
            title_source: Some(source.clone()),
            authors: Vec::new(),
            author_sources: Vec::new(),
            affiliations: Vec::new(),
            affiliation_sources: Vec::new(),
            correspondence: Vec::new(),
            correspondence_sources: Vec::new(),
            date: None,
            date_source: None,
            keywords: Vec::new(),
            keyword_sources: Vec::new(),
            pacs: Vec::new(),
            pacs_sources: Vec::new(),
            source: source.clone(),
        })];
        blocks.extend((0..8).map(|index| {
            IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Text {
                    text: format!("p{index}"),
                    source: source.clone(),
                }],
                source: source.clone(),
            })
        }));
        let display_lists = build_page_display_lists(
            &DocumentIr::new(blocks),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 100.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                column_count: 2,
                column_gap_pt: 20.0,
                max_chars_per_line: 100,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );
        let second_column = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "p7" => Some(run.origin),
            _ => None,
        });

        assert_eq!(second_column, Some(Point { x: 110.0, y: 20.0 }));
    }

    #[test]
    fn full_width_graphics_span_columns_and_advance_both_column_origins() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::FullWidthGraphic(GraphicBlock {
                    path: "figures/wide.pdf".to_string(),
                    options: Some("width=\\textwidth".to_string()),
                    page_selection: None,
                    asset_format: Some(GraphicAssetFormat::Pdf),
                    asset_hash: None,
                    asset_dimensions: Some(GraphicAssetDimensions {
                        width_px: 180,
                        height_px: 20,
                        density: None,
                        natural_width_pt_milli: Some(180_000),
                        natural_height_pt_milli: Some(20_000),
                    }),
                    caption: Some("Wide caption".to_string()),
                    caption_source: Some(source.clone()),
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "one\ntwo\nthree\nfour\nfive\nsix".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 100.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                column_count: 2,
                column_gap_pt: 10.0,
                max_chars_per_line: 100,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                body_font_size_pt: 10.0,
                ..PageDisplayListOptions::default()
            },
        );

        let page = &display_lists[0];
        let image = page
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) => Some(image),
                _ => None,
            })
            .expect("full-width image");
        let caption = page
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::TextRun(run) if run.text == "Wide caption" => Some(run),
                _ => None,
            })
            .expect("full-width caption");
        let second_column = page.ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "six" => Some(run.origin),
            _ => None,
        });

        assert_eq!(image.rect.x, 10.0);
        assert!((image.rect.width - 180.0).abs() < 0.01);
        assert!(caption.origin.y >= image.rect.y + image.rect.height);
        assert_eq!(second_column, Some(Point { x: 105.0, y: 40.0 }));
    }

    #[test]
    fn moves_images_to_the_next_column() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "one\ntwo\nthree".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Graphic(GraphicBlock {
                    path: "figures/plot.png".to_string(),
                    options: None,
                    page_selection: None,
                    asset_format: Some(GraphicAssetFormat::Png),
                    asset_hash: None,
                    asset_dimensions: Some(GraphicAssetDimensions {
                        width_px: 20,
                        height_px: 10,
                        density: None,
                        natural_width_pt_milli: None,
                        natural_height_pt_milli: None,
                    }),
                    caption: None,
                    caption_source: None,
                    source,
                }),
            ]),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 50.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                column_count: 2,
                column_gap_pt: 20.0,
                max_chars_per_line: 100,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );
        let image = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
            _ => None,
        });

        assert_eq!(display_lists.len(), 1);
        assert_eq!(image.map(|image| image.rect.x), Some(110.0));
        assert_eq!(image.map(|image| image.rect.y), Some(10.0));
    }

    #[test]
    fn lays_out_sibling_containers_with_local_text_and_graphic_widths() {
        let container_source = SourceProvenance::file("main.tex", 0, 10);
        let source = SourceProvenance::file("main.tex", 30, 120);
        let expected_source = source.clone();
        let left = IrBlock::LayoutContainer(LayoutContainerBlock {
            name: "minipage".to_string(),
            width_spec: "0.55\\linewidth".to_string(),
            alignment: Some(LayoutAlignment::Top),
            height_spec: None,
            inner_alignment: None,
            children: vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "left".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::Graphic(GraphicBlock {
                    path: "figures/left.png".to_string(),
                    options: Some("width=0.5\\linewidth".to_string()),
                    page_selection: None,
                    asset_format: Some(GraphicAssetFormat::Png),
                    asset_hash: None,
                    asset_dimensions: Some(GraphicAssetDimensions {
                        width_px: 100,
                        height_px: 100,
                        density: None,
                        natural_width_pt_milli: None,
                        natural_height_pt_milli: None,
                    }),
                    caption: None,
                    caption_source: None,
                    source: source.clone(),
                }),
            ],
            source: container_source.clone(),
        });
        let right = IrBlock::LayoutContainer(LayoutContainerBlock {
            name: "minipage".to_string(),
            width_spec: "0.4\\linewidth".to_string(),
            alignment: Some(LayoutAlignment::Top),
            height_spec: None,
            inner_alignment: None,
            children: vec![IrBlock::Paragraph(ParagraphBlock {
                content: vec![InlineNode::Link(LinkInline {
                    target: "https://example.test/right".to_string(),
                    display_text: "right".to_string(),
                    source: source.clone(),
                })],
                source: source.clone(),
            })],
            source: container_source,
        });
        let pages = build_page_display_lists(
            &DocumentIr::with_labels(
                vec![left, right],
                vec![LabelDefinitionIr {
                    key: "fig:inside".to_string(),
                    source: SourceProvenance::file("main.tex", 20, 29),
                }],
            ),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 300.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );
        let left_text = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "left" => Some(run.origin),
            _ => None,
        });
        let right_text = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "right" => Some(run.origin),
            _ => None,
        });
        let image = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::Image(image) => Some(image.rect),
            _ => None,
        });
        let link = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::LinkAnnotation(link) if link.target == "https://example.test/right" => {
                Some(link)
            }
            _ => None,
        });
        let destination = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::NamedDestination(destination) if destination.name == "fig:inside" => {
                Some(destination.point)
            }
            _ => None,
        });

        assert_eq!(left_text, Some(Point { x: 10.0, y: 10.0 }));
        assert_eq!(right_text, Some(Point { x: 113.0, y: 10.0 }));
        assert_eq!(image.map(|rect| rect.x), Some(10.0));
        assert_eq!(image.map(|rect| rect.width), Some(49.5));
        assert!(image.is_some_and(|rect| rect.y > 10.0));
        assert!(
            link.is_some_and(|link| { link.rect.x == 113.0 && link.source == expected_source })
        );
        assert_eq!(destination, Some(Point { x: 10.0, y: 10.0 }));
    }

    #[test]
    fn resolves_nested_container_widths_against_the_nearest_parent() {
        let source = SourceProvenance::file("main.tex", 0, 100);
        let document = DocumentIr::new(vec![IrBlock::LayoutContainer(LayoutContainerBlock {
            name: "minipage".to_string(),
            width_spec: "0.5\\linewidth".to_string(),
            alignment: Some(LayoutAlignment::Top),
            height_spec: None,
            inner_alignment: None,
            children: vec![IrBlock::LayoutContainer(LayoutContainerBlock {
                name: "minipage".to_string(),
                width_spec: "0.5\\linewidth".to_string(),
                alignment: Some(LayoutAlignment::Top),
                height_spec: None,
                inner_alignment: None,
                children: vec![IrBlock::Graphic(GraphicBlock {
                    path: "figures/nested.png".to_string(),
                    options: Some("width=\\linewidth".to_string()),
                    page_selection: None,
                    asset_format: Some(GraphicAssetFormat::Png),
                    asset_hash: None,
                    asset_dimensions: Some(GraphicAssetDimensions {
                        width_px: 100,
                        height_px: 100,
                        density: None,
                        natural_width_pt_milli: None,
                        natural_height_pt_milli: None,
                    }),
                    caption: None,
                    caption_source: None,
                    source: source.clone(),
                })],
                source: source.clone(),
            })],
            source,
        })]);
        let pages = build_page_display_lists(
            &document,
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 300.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );
        let image = pages[0].ops.iter().find_map(|op| match op {
            DrawOp::Image(image) => Some(image.rect),
            _ => None,
        });

        assert_eq!(image.map(|rect| rect.x), Some(10.0));
        assert_eq!(image.map(|rect| rect.width), Some(45.0));
    }

    #[test]
    fn wraps_overflowing_sibling_containers_to_the_next_row() {
        let source = SourceProvenance::file("main.tex", 0, 100);
        let blocks = (0..3)
            .map(|index| {
                IrBlock::LayoutContainer(LayoutContainerBlock {
                    name: "minipage".to_string(),
                    width_spec: "0.48\\linewidth".to_string(),
                    alignment: Some(LayoutAlignment::Top),
                    height_spec: None,
                    inner_alignment: None,
                    children: vec![IrBlock::Paragraph(ParagraphBlock {
                        content: vec![InlineNode::Text {
                            text: format!("box-{index}"),
                            source: source.clone(),
                        }],
                        source: source.clone(),
                    })],
                    source: source.clone(),
                })
            })
            .collect();
        let pages = build_page_display_lists(
            &DocumentIr::new(blocks),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 300.0,
                margin_left_pt: 10.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                line_height_pt: 10.0,
                block_gap_pt: 0.0,
                ..PageDisplayListOptions::default()
            },
        );
        let origins = pages[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if run.text.starts_with("box-") => Some(run.origin),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(origins.len(), 3);
        assert_eq!(origins[0].y, origins[1].y);
        assert!(origins[1].x > origins[0].x);
        assert_eq!(origins[2].x, 10.0);
        assert!(origins[2].y > origins[0].y);
    }

    #[test]
    fn packs_only_adjacent_explicitly_sized_images() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let blocks = (0..3)
            .map(|index| {
                IrBlock::Graphic(GraphicBlock {
                    path: format!("figures/plot-{index}.png"),
                    options: Some("width=0.48\\linewidth".to_string()),
                    page_selection: None,
                    asset_format: Some(GraphicAssetFormat::Png),
                    asset_hash: None,
                    asset_dimensions: Some(GraphicAssetDimensions {
                        width_px: 100,
                        height_px: 100,
                        density: None,
                        natural_width_pt_milli: None,
                        natural_height_pt_milli: None,
                    }),
                    caption: None,
                    caption_source: None,
                    source: source.clone(),
                })
            })
            .collect::<Vec<_>>();
        let options = PageDisplayListOptions {
            page_width_pt: 200.0,
            page_height_pt: 300.0,
            margin_left_pt: 10.0,
            margin_top_pt: 10.0,
            margin_bottom_pt: 10.0,
            block_gap_pt: 0.0,
            ..PageDisplayListOptions::default()
        };
        let packed = build_page_display_lists(&DocumentIr::new(blocks), options.clone());
        let packed_rects = packed[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Image(image) => Some(image.rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let natural = build_page_display_lists(
            &DocumentIr::new(
                (0..2)
                    .map(|index| {
                        IrBlock::Graphic(GraphicBlock {
                            path: format!("figures/icon-{index}.png"),
                            options: None,
                            page_selection: None,
                            asset_format: Some(GraphicAssetFormat::Png),
                            asset_hash: None,
                            asset_dimensions: Some(GraphicAssetDimensions {
                                width_px: 20,
                                height_px: 10,
                                density: None,
                                natural_width_pt_milli: None,
                                natural_height_pt_milli: None,
                            }),
                            caption: None,
                            caption_source: None,
                            source: source.clone(),
                        })
                    })
                    .collect(),
            ),
            options.clone(),
        );
        let natural_rects = natural[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Image(image) => Some(image.rect),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(packed_rects.len(), 3);
        assert_eq!(packed_rects[0].y, packed_rects[1].y);
        assert!(packed_rects[1].x > packed_rects[0].x);
        assert_eq!(packed_rects[2].x, options.margin_left_pt);
        assert!(packed_rects[2].y > packed_rects[0].y);
        assert_eq!(natural_rects[0].x, natural_rects[1].x);
        assert!(natural_rects[1].y > natural_rects[0].y);
    }

    #[test]
    fn builds_image_ops_from_graphic_blocks() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let caption_related_span = tex_render_model::SourceSpan {
            path: "main.tex".into(),
            start_utf8: 39,
            end_utf8: 48,
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: Some("width=0.8\\linewidth".to_string()),
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
                caption: Some("Plot caption.".to_string()),
                caption_source: Some(SourceProvenance::file("main.tex", 25, 38).with_related(
                    tex_render_model::SourceSpanRole::EmitSite,
                    tex_render_model::ProvenanceSpan::File(caption_related_span.clone()),
                )),
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_eq!(display_lists.len(), 1);
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
                _ => None,
            })
            .expect("image op");
        assert_eq!(image.rect.x, 72.0);
        assert!((image.rect.width - 374.4).abs() < 0.01);
        assert!((image.rect.height - 67.2).abs() < 0.01);
        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::TextRun(run) if run.text == "Plot caption."
                    && matches!(
                        &run.source.primary,
                        tex_render_model::ProvenanceSpan::File(span)
                            if span.start_utf8 == 25 && span.end_utf8 == 38
                    )
            )
        }));
        assert!(
            display_lists[0]
                .source_spans
                .contains(&caption_related_span)
        );
        assert_eq!(display_lists[0].source_spans.len(), 3);
    }

    #[test]
    fn graphic_absolute_dimension_options_affect_image_rect_and_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: Some("width=5cm,height=2cm".to_string()),
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let different_width = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: Some("width=6cm,height=2cm".to_string()),
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - (5.0 * 72.0 / 2.54)).abs() < 0.01);
        assert!((image.rect.height - (2.0 * 72.0 / 2.54)).abs() < 0.01);
        assert_ne!(
            display_lists[0].content_hash,
            different_width[0].content_hash
        );
    }

    #[test]
    fn included_pdf_pages_are_centered_on_dedicated_pages() {
        let source = SourceProvenance::file("main.tex", 0, 48);
        let included_page = |page, width, height| {
            IrBlock::IncludedPdfPage(GraphicBlock {
                path: "paper.pdf".to_string(),
                options: Some("pages=1-last".to_string()),
                page_selection: Some(GraphicPageSelection {
                    page: Some(page),
                    pagebox: None,
                }),
                asset_format: Some(GraphicAssetFormat::Pdf),
                asset_hash: Some("blake3:paper".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: width,
                    height_px: height,
                    density: None,
                    natural_width_pt_milli: Some(width * 1000),
                    natural_height_pt_milli: Some(height * 1000),
                }),
                caption: None,
                caption_source: None,
                source: source.clone(),
            })
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![included_page(1, 100, 100), included_page(2, 200, 100)]),
            PageDisplayListOptions {
                page_width_pt: 200.0,
                page_height_pt: 300.0,
                ..PageDisplayListOptions::default()
            },
        );

        assert_eq!(display_lists.len(), 2);
        let first = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) => Some(image),
                _ => None,
            })
            .expect("first included page");
        assert_eq!(first.rect.x, 0.0);
        assert_eq!(first.rect.y, 50.0);
        assert_eq!(first.rect.width, 200.0);
        assert_eq!(first.rect.height, 200.0);
        let second = display_lists[1]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) => Some(image),
                _ => None,
            })
            .expect("second included page");
        assert_eq!(second.rect.x, 0.0);
        assert_eq!(second.rect.y, 100.0);
        assert_eq!(second.rect.width, 200.0);
        assert_eq!(second.rect.height, 100.0);
        assert_eq!(
            second
                .page_selection
                .as_ref()
                .and_then(|selection| selection.page),
            Some(2)
        );
    }

    #[test]
    fn graphic_dimexpr_dimension_options_affect_image_rect() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let options = PageDisplayListOptions::default();
        let content_width_pt = options.page_width_pt - options.margin_left_pt * 2.0;
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some(r"width=\dimexpr\textwidth-2\fboxsep\relax".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 200,
                    height_px: 100,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            options,
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");
        let expected_width = content_width_pt - 6.0;

        assert!((image.rect.width - expected_width).abs() < 0.01);
        assert!((image.rect.height - expected_width / 2.0).abs() < 0.01);
    }

    #[test]
    fn graphic_nonuniform_scale_options_affect_image_rect_and_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("scale=0.5,yscale=2".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 120,
                    height_px: 60,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let different_y_scale = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("scale=0.5,yscale=1".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 120,
                    height_px: 60,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 60.0).abs() < 0.01);
        assert!((image.rect.height - 120.0).abs() < 0.01);
        assert_eq!(image.scale, Some(ImageScale { x: 0.5, y: 2.0 }));
        assert_ne!(
            display_lists[0].content_hash,
            different_y_scale[0].content_hash
        );
    }

    #[test]
    fn graphic_asset_format_affects_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let pdf_display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.asset".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Pdf),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let svg_display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.asset".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Svg),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_ne!(
            pdf_display_lists[0].content_hash,
            svg_display_lists[0].content_hash
        );
        assert_ne!(pdf_display_lists[0].page_id, svg_display_lists[0].page_id);
    }

    #[test]
    fn graphic_asset_dimensions_affect_default_image_rect_and_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 120,
                    height_px: 60,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let without_dimensions = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 120.0).abs() < 0.01);
        assert!((image.rect.height - 60.0).abs() < 0.01);
        assert_ne!(
            display_lists[0].content_hash,
            without_dimensions[0].content_hash
        );
    }

    #[test]
    fn graphic_asset_density_affects_default_image_rect_and_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 288,
                    height_px: 144,
                    density: Some(GraphicAssetDensity {
                        x_density: 144,
                        y_density: 144,
                        unit: GraphicAssetDensityUnit::PixelsPerInch,
                    }),
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source: source.clone(),
            })]),
            PageDisplayListOptions::default(),
        );
        let without_density = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 288,
                    height_px: 144,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 144.0).abs() < 0.01);
        assert!((image.rect.height - 72.0).abs() < 0.01);
        assert_ne!(
            display_lists[0].content_hash,
            without_density[0].content_hash
        );
    }

    #[test]
    fn graphic_asset_natural_point_dimensions_override_pixel_dimensions() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/vector.svg".to_string(),
                options: None,
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Svg),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 640,
                    height_px: 480,
                    density: None,
                    natural_width_pt_milli: Some(144_000),
                    natural_height_pt_milli: Some(72_000),
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/vector.svg" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 144.0).abs() < 0.01);
        assert!((image.rect.height - 72.0).abs() < 0.01);
        assert_eq!(image.natural_width_pt, Some(144.0));
        assert_eq!(image.natural_height_pt, Some(72.0));
    }

    #[test]
    fn graphic_keepaspectratio_fits_within_width_and_height() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("width=100pt,height=100pt,keepaspectratio".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 400,
                    height_px: 200,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 100.0).abs() < 0.01);
        assert!((image.rect.height - 50.0).abs() < 0.01);
    }

    #[test]
    fn graphic_trim_and_clip_options_are_preserved_on_image_ops() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("trim=1pt 2pt 3pt 4pt,clip".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
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
                viewport: None,
                clip: true,
            })
        );
    }

    #[test]
    fn graphic_viewport_option_is_preserved_on_image_ops() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("viewport=0pt 0pt 120pt 60pt,clip=false".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
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
                trim: None,
                viewport: Some(ImageViewport {
                    llx_pt: 0.0,
                    lly_pt: 0.0,
                    urx_pt: 120.0,
                    ury_pt: 60.0,
                }),
                clip: false,
            })
        );
    }

    #[test]
    fn graphic_braced_comma_viewport_option_is_preserved_on_image_ops() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("viewport={0pt,0pt,120pt,60pt},clip".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
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
                trim: None,
                viewport: Some(ImageViewport {
                    llx_pt: 0.0,
                    lly_pt: 0.0,
                    urx_pt: 120.0,
                    ury_pt: 60.0,
                }),
                clip: true,
            })
        );
    }

    #[test]
    fn graphic_trim_affects_default_image_rect() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("trim=50pt 0pt 50pt 0pt,clip".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 200,
                    height_px: 100,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 100.0).abs() < 0.01);
        assert!((image.rect.height - 100.0).abs() < 0.01);
    }

    #[test]
    fn graphic_viewport_affects_default_image_rect() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("viewport=10pt 20pt 60pt 45pt,clip".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 200,
                    height_px: 100,
                    density: None,
                    natural_width_pt_milli: None,
                    natural_height_pt_milli: None,
                }),
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
                _ => None,
            })
            .expect("image op");

        assert!((image.rect.width - 50.0).abs() < 0.01);
        assert!((image.rect.height - 25.0).abs() < 0.01);
    }

    #[test]
    fn graphic_angle_and_origin_options_are_preserved_on_image_ops() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("width=100pt,angle=90,origin=c".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: None,
                caption: None,
                caption_source: None,
                source,
            })]),
            PageDisplayListOptions::default(),
        );
        let image = display_lists[0]
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Image(image) if image.asset_ref == "figures/plot.png" => Some(image),
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
    fn wraps_graphic_caption_text_runs() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: None,
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
                caption: Some("abcdefghi".to_string()),
                caption_source: Some(SourceProvenance::file("main.tex", 25, 34)),
                source,
            })]),
            PageDisplayListOptions {
                max_chars_per_line: 6,
                ..PageDisplayListOptions::default()
            },
        );

        let caption_runs = display_lists[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) if run.text == "abcdef" || run.text == "ghi" => {
                    Some(run.text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(caption_runs, vec!["abcdef", "ghi"]);
    }

    #[test]
    fn indents_wrapped_list_item_continuation_lines() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let options = PageDisplayListOptions {
            max_chars_per_line: 6,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::List(ListBlock {
                kind: ListKind::Unordered,
                items: vec![ListItemIr {
                    marker: "*".to_string(),
                    content: vec![InlineNode::Text {
                        text: "abcdefghi".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }],
                source,
            })]),
            options.clone(),
        );

        let continuation = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "efghi" => Some(run),
            _ => None,
        });
        assert_eq!(
            continuation.map(|run| run.origin.x),
            Some(options.margin_left_pt + options.list_continuation_indent_pt)
        );
    }

    #[test]
    fn indents_abstract_first_and_continuation_lines() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let options = PageDisplayListOptions {
            max_chars_per_line: 8,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Abstract(AbstractBlock {
                content: vec![InlineNode::Text {
                    text: "abcdefghi".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            options.clone(),
        );

        let heading = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "Abstract" => Some(run),
            _ => None,
        });
        let first_line = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "abcdefgh" => Some(run),
            _ => None,
        });
        let continuation = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "i" => Some(run),
            _ => None,
        });
        assert_eq!(
            heading.map(|run| run.origin.x),
            Some(options.margin_left_pt)
        );
        assert_eq!(
            first_line.map(|run| run.origin.x),
            Some(options.margin_left_pt + options.abstract_indent_pt)
        );
        assert_eq!(
            continuation.map(|run| run.origin.x),
            Some(options.margin_left_pt + options.abstract_indent_pt)
        );
    }

    #[test]
    fn indents_wrapped_bibliography_continuation_lines() {
        let source = SourceProvenance::file("main.tex", 0, 20);
        let options = PageDisplayListOptions {
            max_chars_per_line: 6,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Bibliography(BibliographyBlock {
                items: vec![BibliographyItemIr {
                    key: "key".to_string(),
                    label: Some("1".to_string()),
                    content: "abcdefghi".to_string(),
                    source: source.clone(),
                }],
                source,
            })]),
            options.clone(),
        );

        let continuation = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "abcdef" => Some(run),
            _ => None,
        });
        assert_eq!(
            continuation.map(|run| run.origin.x),
            Some(options.margin_left_pt + options.bibliography_continuation_indent_pt)
        );
    }

    #[test]
    fn leaves_block_gap_after_last_bibliography_item() {
        let source = SourceProvenance::file("main.tex", 0, 20);
        let options = PageDisplayListOptions {
            line_height_pt: 10.0,
            block_gap_pt: 5.0,
            ..PageDisplayListOptions::default()
        };
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Bibliography(BibliographyBlock {
                    items: vec![
                        BibliographyItemIr {
                            key: "one".to_string(),
                            label: Some("1".to_string()),
                            content: "First.".to_string(),
                            source: source.clone(),
                        },
                        BibliographyItemIr {
                            key: "two".to_string(),
                            label: Some("2".to_string()),
                            content: "Second.".to_string(),
                            source: source.clone(),
                        },
                    ],
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "After".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            options.clone(),
        );

        let after = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "After" => Some(run),
            _ => None,
        });
        assert_eq!(
            after.map(|run| run.origin.y),
            Some(options.margin_top_pt + options.line_height_pt * 2.0 + options.block_gap_pt)
        );
    }

    #[test]
    fn forced_page_break_starts_following_content_on_a_new_page() {
        let source = SourceProvenance::file("main.tex", 0, 20);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "Before".to_string(),
                        source: source.clone(),
                    }],
                    source: source.clone(),
                }),
                IrBlock::PageBreak(PageBreakBlock {
                    kind: PageBreakKind::NewPage,
                    source: source.clone(),
                }),
                IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: "After".to_string(),
                        source: source.clone(),
                    }],
                    source,
                }),
            ]),
            PageDisplayListOptions::default(),
        );

        assert_eq!(display_lists.len(), 2);
        assert!(
            display_lists[0]
                .ops
                .iter()
                .any(|op| matches!(op, DrawOp::TextRun(run) if run.text == "Before"))
        );
        assert!(
            display_lists[1]
                .ops
                .iter()
                .any(|op| matches!(op, DrawOp::TextRun(run) if run.text == "After"))
        );
    }
}
