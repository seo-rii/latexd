use tex_render_model::{
    BibliographyBlock, Destination, DocumentIr, DrawOp, FontFamilyRequest, FontRequest, FontRole,
    FontSeries, FontShape, ImageCrop, ImageTrim, ImageViewport, InlineNode, IrBlock,
    LinkAnnotation, PageDisplayList, Point, PositionedImage, PositionedTextRun, ProvenanceSpan,
    Rect, SourceProvenance, SourceSpan, TableRuleSpan, TextCluster,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PageDisplayListOptions {
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub margin_left_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub abstract_indent_pt: f32,
    pub list_continuation_indent_pt: f32,
    pub bibliography_continuation_indent_pt: f32,
    pub max_chars_per_line: usize,
    pub line_height_pt: f32,
    pub block_gap_pt: f32,
    pub body_font_size_pt: f32,
    pub heading_font_size_pt: f32,
    pub title_font_size_pt: f32,
}

impl Default for PageDisplayListOptions {
    fn default() -> Self {
        Self {
            page_width_pt: 612.0,
            page_height_pt: 792.0,
            margin_left_pt: 72.0,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            abstract_indent_pt: 18.0,
            list_continuation_indent_pt: 18.0,
            bibliography_continuation_indent_pt: 24.0,
            max_chars_per_line: 72,
            line_height_pt: 14.0,
            block_gap_pt: 7.0,
            body_font_size_pt: 11.0,
            heading_font_size_pt: 15.0,
            title_font_size_pt: 18.0,
        }
    }
}

struct PendingPage {
    ops: Vec<DrawOp>,
    source_spans: Vec<SourceSpan>,
    text: String,
    hash_input: String,
}

#[derive(Clone)]
struct LogicalTextSegment {
    text: String,
    source: SourceProvenance,
    link_target: Option<String>,
}

struct LogicalTextRun {
    segments: Vec<LogicalTextSegment>,
    source: SourceProvenance,
    font: FontRequest,
    size_pt: f32,
    gap_after_pt: f32,
    first_line_indent_pt: f32,
    continuation_indent_pt: f32,
}

struct LogicalImage {
    path: String,
    options: Option<String>,
    asset_format: Option<tex_render_model::GraphicAssetFormat>,
    asset_hash: Option<String>,
    asset_dimensions: Option<tex_render_model::GraphicAssetDimensions>,
    caption: Option<String>,
    caption_source: Option<SourceProvenance>,
    source: SourceProvenance,
    gap_after_pt: f32,
}

enum LogicalItem {
    Text(LogicalTextRun),
    Image(LogicalImage),
}

pub fn build_page_display_lists(
    document_ir: &DocumentIr,
    options: PageDisplayListOptions,
) -> Vec<PageDisplayList> {
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
        series: FontSeries::Bold,
        shape: FontShape::Upright,
        size_pt: options.title_font_size_pt,
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
                    });
                }
                InlineNode::Space { source } => {
                    segments.push(LogicalTextSegment {
                        text: " ".to_string(),
                        source: source.clone(),
                        link_target: None,
                    });
                }
                InlineNode::LineBreak { source } => {
                    segments.push(LogicalTextSegment {
                        text: "\n".to_string(),
                        source: source.clone(),
                        link_target: None,
                    });
                }
                InlineNode::Citation(citation) => {
                    segments.push(LogicalTextSegment {
                        text: citation.display_text.clone(),
                        source: citation.source.clone(),
                        link_target: None,
                    });
                }
                InlineNode::Reference(reference) => {
                    segments.push(LogicalTextSegment {
                        text: reference.display_text.clone(),
                        source: reference.source.clone(),
                        link_target: None,
                    });
                }
                InlineNode::Link(link) => {
                    segments.push(LogicalTextSegment {
                        text: link.display_text.clone(),
                        source: link.source.clone(),
                        link_target: Some(link.target.clone()),
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
                    });
                }
            }
        }
        segments
    };

    let mut logical_items = Vec::new();
    for block in &document_ir.blocks {
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
                        }],
                        source,
                        font: title_font.clone(),
                        size_pt: options.title_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                    }));
                }
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
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: 0.0,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
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
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                    }));
                }
            }
            IrBlock::Abstract(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: options.abstract_indent_pt,
                    continuation_indent_pt: options.abstract_indent_pt,
                }));
            }
            IrBlock::Heading(block) => {
                let mut segments = Vec::new();
                if let Some(number) = &block.number {
                    segments.push(LogicalTextSegment {
                        text: number.clone(),
                        source: block.source.clone(),
                        link_target: None,
                    });
                    segments.push(LogicalTextSegment {
                        text: " ".to_string(),
                        source: block.source.clone(),
                        link_target: None,
                    });
                }
                segments.extend(inline_segments(&block.content));
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments,
                    source: block.source.clone(),
                    font: heading_font.clone(),
                    size_pt: options.heading_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                }));
            }
            IrBlock::Paragraph(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                }));
            }
            IrBlock::Environment(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                }));
            }
            IrBlock::List(block) => {
                for (index, item) in block.items.iter().enumerate() {
                    let mut segments = vec![LogicalTextSegment {
                        text: format!("{} ", item.marker),
                        source: item.source.clone(),
                        link_target: None,
                    }];
                    segments.extend(inline_segments(&item.content));
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments,
                        source: item.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: if index + 1 == block.items.len() {
                            options.block_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.list_continuation_indent_pt,
                    }));
                }
                if block.items.is_empty() {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: Vec::new(),
                        source: block.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.list_continuation_indent_pt,
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
                    }],
                    source: block.source.clone(),
                    font: math_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
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
                        }],
                        source: item.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: if index + 1 == items.len() {
                            options.block_gap_pt
                        } else {
                            0.0
                        },
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.bibliography_continuation_indent_pt,
                    }));
                }
                if items.is_empty() {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        segments: Vec::new(),
                        source: source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: options.bibliography_continuation_indent_pt,
                    }));
                }
            }
            IrBlock::Graphic(block) => {
                logical_items.push(LogicalItem::Image(LogicalImage {
                    path: block.path.clone(),
                    options: block.options.clone(),
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
                        }],
                        source,
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                        first_line_indent_pt: 0.0,
                        continuation_indent_pt: 0.0,
                    }));
                }
            }
            IrBlock::Table(block) => {
                let mut segments = Vec::new();
                let mut column_widths = Vec::new();
                for row in &block.rows {
                    let mut column_index = 0usize;
                    for cell in &row.cells {
                        let column_span = cell.column_span.unwrap_or(1).max(1);
                        while column_index + column_span > column_widths.len() {
                            column_widths.push(0usize);
                        }
                        if column_span == 1 {
                            column_widths[column_index] =
                                column_widths[column_index].max(cell.text.chars().count());
                        }
                        column_index += column_span;
                    }
                }
                for row in &block.rows {
                    let mut column_index = 0usize;
                    for cell in &row.cells {
                        let column_span = cell.column_span.unwrap_or(1).max(1);
                        let end_column = (column_index + column_span).min(column_widths.len());
                        let mut spanned_width = column_widths[column_index..end_column]
                            .iter()
                            .sum::<usize>();
                        spanned_width += end_column.saturating_sub(column_index + 1) * 3;
                        let text_width = cell.text.chars().count();
                        if column_span > 1 && text_width > spanned_width && end_column > 0 {
                            column_widths[end_column - 1] += text_width - spanned_width;
                        }
                        column_index += column_span;
                    }
                }
                let rule_width =
                    column_widths.iter().sum::<usize>() + column_widths.len().saturating_sub(1) * 3;
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
                    for width in &column_widths[..start_column] {
                        start_offset += *width + 3;
                    }
                    let mut end_offset = start_offset;
                    for column in start_column..=end_column {
                        end_offset += column_widths[column];
                        if column < end_column {
                            end_offset += 3;
                        }
                    }
                    // Leading spaces are trimmed by line wrapping, so use visible filler
                    // for non-spanned columns in this readable table fallback.
                    let mut chars = vec!['.'; rule_width.max(3)];
                    for index in start_offset..end_offset.min(chars.len()) {
                        chars[index] = '-';
                    }
                    chars.into_iter().collect::<String>()
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
                    });
                }
                for row in &block.rows {
                    if row.rule_above {
                        if !segments.is_empty() {
                            segments.push(LogicalTextSegment {
                                text: "\n".to_string(),
                                source: block.source.clone(),
                                link_target: None,
                            });
                        }
                        segments.push(LogicalTextSegment {
                            text: rule_text.clone(),
                            source: block.source.clone(),
                            link_target: None,
                        });
                    }
                    for rule in &row.partial_rules_above {
                        if !segments.is_empty() {
                            segments.push(LogicalTextSegment {
                                text: "\n".to_string(),
                                source: block.source.clone(),
                                link_target: None,
                            });
                        }
                        segments.push(LogicalTextSegment {
                            text: partial_rule_text(rule),
                            source: block.source.clone(),
                            link_target: None,
                        });
                    }
                    if !segments.is_empty() {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                        });
                    }
                    let mut row_text = String::new();
                    let mut column_index = 0usize;
                    for (cell_index, cell) in row.cells.iter().enumerate() {
                        if cell_index > 0 {
                            row_text.push_str(" | ");
                        }
                        let column_span = cell.column_span.unwrap_or(1).max(1);
                        let end_column = (column_index + column_span).min(column_widths.len());
                        let mut spanned_width = column_widths[column_index..end_column]
                            .iter()
                            .sum::<usize>();
                        spanned_width += end_column.saturating_sub(column_index + 1) * 3;
                        row_text.push_str(&cell.text);
                        if cell_index + 1 < row.cells.len() {
                            for _ in 0..spanned_width.saturating_sub(cell.text.chars().count()) {
                                row_text.push(' ');
                            }
                        }
                        column_index += column_span;
                    }
                    segments.push(LogicalTextSegment {
                        text: row_text,
                        source: block.source.clone(),
                        link_target: None,
                    });
                    if row.rule_below {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                        });
                        segments.push(LogicalTextSegment {
                            text: rule_text.clone(),
                            source: block.source.clone(),
                            link_target: None,
                        });
                    }
                    for rule in &row.partial_rules_below {
                        segments.push(LogicalTextSegment {
                            text: "\n".to_string(),
                            source: block.source.clone(),
                            link_target: None,
                        });
                        segments.push(LogicalTextSegment {
                            text: partial_rule_text(rule),
                            source: block.source.clone(),
                            link_target: None,
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
                        size_pt: options.body_font_size_pt,
                        role: FontRole::Mono,
                    },
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
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
                    }],
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                    first_line_indent_pt: 0.0,
                    continuation_indent_pt: 0.0,
                }));
            }
        }
    }

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
    let finish_page =
        |pages: &mut Vec<PageDisplayList>, page_index: usize, pending: PendingPage| {
            let content_hash = blake3::hash(pending.hash_input.as_bytes())
                .to_hex()
                .to_string();
            let page_id = blake3::hash(
                format!(
                    "display-list:{page_index}:{}:{}:{content_hash}",
                    options.page_width_pt, options.page_height_pt
                )
                .as_bytes(),
            )
            .to_hex()
            .to_string();
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
    let content_width_pt = (options.page_width_pt - options.margin_left_pt * 2.0).max(1.0);
    let content_height_pt =
        (options.page_height_pt - options.margin_top_pt - options.margin_bottom_pt).max(1.0);
    let parse_graphic_dimension_pt = |raw_value: &str, allow_zero: bool| -> Option<f32> {
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

        for (name, reference_pt) in [
            ("\\linewidth", content_width_pt),
            ("\\textwidth", content_width_pt),
            ("\\columnwidth", content_width_pt),
            ("\\textheight", content_height_pt),
            ("\\paperheight", options.page_height_pt),
        ] {
            if normalized == name {
                return Some(reference_pt);
            }
            if let Some(prefix) = normalized.strip_suffix(name) {
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

        for (unit, multiplier) in [
            ("truept", 1.0),
            ("bp", 1.0),
            ("pt", 1.0),
            ("in", 72.0),
            ("cm", 72.0 / 2.54),
            ("mm", 72.0 / 25.4),
            ("pc", 12.0),
            ("em", options.body_font_size_pt),
            ("ex", options.body_font_size_pt * 0.5),
        ] {
            if let Some(number) = normalized.strip_suffix(unit) {
                let dimension = number.parse::<f32>().ok()? * multiplier;
                if accepts_dimension(dimension) {
                    return Some(dimension);
                }
            }
        }

        let dimension = normalized.parse::<f32>().ok()?;
        accepts_dimension(dimension).then_some(dimension)
    };
    let parse_graphic_quad_pt = |raw_value: &str| -> Option<[f32; 4]> {
        let normalized = raw_value.trim().trim_matches(|ch| ch == '{' || ch == '}');
        let parts = normalized.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 {
            return None;
        }

        Some([
            parse_graphic_dimension_pt(parts[0], true)?,
            parse_graphic_dimension_pt(parts[1], true)?,
            parse_graphic_dimension_pt(parts[2], true)?,
            parse_graphic_dimension_pt(parts[3], true)?,
        ])
    };
    let mut pending = new_pending_page();
    let mut page_index = 0usize;
    let mut y = options.margin_top_pt;
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
                let mut wrapped_lines = Vec::new();
                let mut current_line = Vec::new();
                let mut current_len = 0usize;
                let widest_indent = logical
                    .first_line_indent_pt
                    .max(logical.continuation_indent_pt);
                let average_glyph_width_pt =
                    text_advance_pt("n", &logical.font, logical.size_pt).max(0.1);
                let available_width_pt =
                    (options.page_width_pt - options.margin_left_pt * 2.0 - widest_indent)
                        .max(average_glyph_width_pt);
                let width_limited_chars =
                    (available_width_pt / average_glyph_width_pt).floor() as usize;
                let max_chars_per_line = options
                    .max_chars_per_line
                    .max(1)
                    .min(width_limited_chars.max(1));
                let push_segment_text =
                    |mut text: &str,
                     source: &SourceProvenance,
                     link_target: Option<&str>,
                     current_line: &mut Vec<LogicalTextSegment>,
                     current_len: &mut usize,
                     wrapped_lines: &mut Vec<Vec<LogicalTextSegment>>| {
                        while !text.is_empty() {
                            if *current_len == 0 {
                                text = text.trim_start_matches(char::is_whitespace);
                                if text.is_empty() {
                                    break;
                                }
                            }
                            let remaining_line_chars =
                                max_chars_per_line.saturating_sub(*current_len);
                            let take_chars = remaining_line_chars.max(1).min(text.chars().count());
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
                                current_line.push(LogicalTextSegment {
                                    text: chunk.to_string(),
                                    source: source.clone(),
                                    link_target: link_target.map(ToOwned::to_owned),
                                });
                                *current_len += take_chars;
                            }
                            text = &text[split_byte..];
                            if *current_len >= max_chars_per_line {
                                wrapped_lines.push(std::mem::take(current_line));
                                *current_len = 0;
                            }
                        }
                    };

                for segment in &logical.segments {
                    let mut remaining = segment.text.as_str();
                    while !remaining.is_empty() {
                        if let Some(newline_index) = remaining.find('\n') {
                            let before_newline = &remaining[..newline_index];
                            push_segment_text(
                                before_newline,
                                &segment.source,
                                segment.link_target.as_deref(),
                                &mut current_line,
                                &mut current_len,
                                &mut wrapped_lines,
                            );
                            wrapped_lines.push(std::mem::take(&mut current_line));
                            current_len = 0;
                            remaining = &remaining[newline_index + 1..];
                        } else {
                            push_segment_text(
                                remaining,
                                &segment.source,
                                segment.link_target.as_deref(),
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
                    let line_x = options.margin_left_pt
                        + if line_index == 0 {
                            logical.first_line_indent_pt
                        } else {
                            logical.continuation_indent_pt
                        };
                    if y + options.line_height_pt
                        > options.page_height_pt - options.margin_bottom_pt
                        && !pending.ops.is_empty()
                    {
                        finish_page(&mut pages, page_index, pending);
                        page_index += 1;
                        pending = new_pending_page();
                        y = options.margin_top_pt;
                    }

                    if !pending.text.is_empty() {
                        pending.text.push('\n');
                        pending.hash_input.push('\n');
                    }
                    pending.hash_input.push_str(&format!(
                        "\u{1e}text_run:{line_x:.3}:{y:.3}:{:?}:{:.3}\u{1f}",
                        logical.font, logical.size_pt
                    ));
                    let destination_source = line_segments
                        .first()
                        .map(|segment| &segment.source)
                        .unwrap_or(&logical.source);
                    emit_due_destinations(destination_source, Point { x: line_x, y }, &mut pending);
                    let line_text = line_segments
                        .iter()
                        .map(|segment| segment.text.as_str())
                        .collect::<String>();
                    pending.text.push_str(&line_text);
                    pending.hash_input.push_str(&line_text);

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
                        y += options.line_height_pt;
                        continue;
                    }

                    let mut x = line_x;
                    for segment in line_segments {
                        record_source_spans(&segment.source, &mut pending.source_spans);
                        let advance =
                            text_advance_pt(&segment.text, &logical.font, logical.size_pt);
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
                                height: options.line_height_pt,
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
                        x += advance;
                    }
                    y += options.line_height_pt;
                }
                y += logical.gap_after_pt;
            }
            LogicalItem::Image(logical) => {
                let (natural_image_width, natural_image_height) =
                    if let Some(dimensions) = logical.asset_dimensions {
                        let natural_width = dimensions.width_px as f32;
                        let natural_height = dimensions.height_px as f32;
                        if natural_width.is_finite()
                            && natural_height.is_finite()
                            && natural_width > 0.0
                            && natural_height > 0.0
                        {
                            (natural_width, natural_height)
                        } else {
                            (content_width_pt, options.line_height_pt * 6.0)
                        }
                    } else {
                        (content_width_pt, options.line_height_pt * 6.0)
                    };
                let mut width_hint_pt = None;
                let mut height_hint_pt = None;
                let mut scale_hint = None;
                let mut keep_aspect_ratio = false;
                let mut trim = None;
                let mut viewport = None;
                let mut clip = false;
                if let Some(graphic_options) = &logical.options {
                    for part in graphic_options.split(',') {
                        let part = part.trim();
                        if part == "keepaspectratio" {
                            keep_aspect_ratio = true;
                            continue;
                        }
                        if part == "clip" {
                            clip = true;
                            continue;
                        }
                        let Some((key, value)) = part.split_once('=') else {
                            continue;
                        };
                        match key.trim() {
                            "width" => width_hint_pt = parse_graphic_dimension_pt(value, false),
                            "height" | "totalheight" => {
                                height_hint_pt = parse_graphic_dimension_pt(value, false);
                            }
                            "scale" => {
                                let scale = value
                                    .trim()
                                    .parse::<f32>()
                                    .ok()
                                    .filter(|value| value.is_finite() && *value > 0.0);
                                scale_hint = scale;
                            }
                            "keepaspectratio" => {
                                keep_aspect_ratio = !matches!(value.trim(), "false" | "0" | "off");
                            }
                            "trim" => {
                                if let Some([left, bottom, right, top]) =
                                    parse_graphic_quad_pt(value)
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
                                if let Some([llx, lly, urx, ury]) = parse_graphic_quad_pt(value) {
                                    viewport = Some(ImageViewport {
                                        llx_pt: llx,
                                        lly_pt: lly,
                                        urx_pt: urx,
                                        ury_pt: ury,
                                    });
                                }
                            }
                            "clip" => {
                                clip = !matches!(value.trim(), "false" | "0" | "off");
                            }
                            _ => {}
                        }
                    }
                }
                let crop = (clip || trim.is_some() || viewport.is_some()).then_some(ImageCrop {
                    trim,
                    viewport,
                    clip,
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
                let fit_scale = (content_width_pt / source_image_width).min(1.0);
                let (default_image_width, default_image_height) = (
                    source_image_width * fit_scale,
                    source_image_height * fit_scale,
                );
                let (image_width, image_height) = match (width_hint_pt, height_hint_pt) {
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
                        let scale = scale_hint.unwrap_or(1.0);
                        (default_image_width * scale, default_image_height * scale)
                    }
                };
                let required_height = image_height
                    + if logical.caption.is_some() {
                        options.line_height_pt
                    } else {
                        0.0
                    };
                if y + required_height > options.page_height_pt - options.margin_bottom_pt
                    && !pending.ops.is_empty()
                {
                    finish_page(&mut pages, page_index, pending);
                    page_index += 1;
                    pending = new_pending_page();
                    y = options.margin_top_pt;
                }

                if !pending.text.is_empty() {
                    pending.text.push('\n');
                    pending.hash_input.push('\n');
                }
                emit_due_destinations(
                    &logical.source,
                    Point {
                        x: options.margin_left_pt,
                        y,
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
                if let Some(asset_hash) = &logical.asset_hash {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str("asset-hash:");
                    pending.hash_input.push_str(asset_hash);
                }
                if let Some(dimensions) = logical.asset_dimensions {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!(
                        "asset-dimensions:{}:{}",
                        dimensions.width_px, dimensions.height_px
                    ));
                }
                if let Some(crop) = crop {
                    pending.hash_input.push('\u{1f}');
                    pending.hash_input.push_str(&format!("image-crop:{crop:?}"));
                }
                pending.hash_input.push('\u{1f}');
                pending.hash_input.push_str(&format!(
                    "image-rect:{:.3}:{:.3}:{image_width:.3}:{image_height:.3}",
                    options.margin_left_pt, y
                ));
                record_source_spans(&logical.source, &mut pending.source_spans);
                pending.ops.push(DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: options.margin_left_pt,
                        y,
                        width: image_width,
                        height: image_height,
                    },
                    asset_ref: logical.path.clone(),
                    asset_format: logical.asset_format,
                    asset_hash: logical.asset_hash.clone(),
                    crop,
                    source: logical.source.clone(),
                }));
                y += image_height;

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
                        options.margin_left_pt, caption
                    ));
                    record_source_spans(&caption_source, &mut pending.source_spans);
                    pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                        origin: Point {
                            x: options.margin_left_pt,
                            y,
                        },
                        text: caption.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        approximate_advance_pt: caption_advance,
                        glyphs: None,
                        clusters: None,
                        source: caption_source,
                    }));
                    y += options.line_height_pt;
                }
                y += logical.gap_after_pt;
            }
        }
    }
    drop(emit_due_destinations);
    for label in pending_labels.drain(..) {
        record_source_spans(&label.source, &mut pending.source_spans);
        pending.hash_input.push('\u{1f}');
        pending.hash_input.push_str("dest:");
        pending.hash_input.push_str(&label.key);
        pending.ops.push(DrawOp::NamedDestination(Destination {
            name: label.key,
            point: Point {
                x: options.margin_left_pt,
                y,
            },
            source: label.source,
        }));
    }

    if pending.ops.is_empty() && pages.is_empty() {
        pending.text = String::new();
        pending.hash_input = format!("options:{options:?}:font-metrics:basic-v1");
        finish_page(&mut pages, page_index, pending);
    } else if !pending.ops.is_empty() {
        finish_page(&mut pages, page_index, pending);
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
        DisplayMathBlock, DocumentIr, DrawOp, GraphicAssetDimensions, GraphicAssetFormat,
        GraphicBlock, HeadingBlock, ImageCrop, ImageTrim, ImageViewport, InlineNode, IrBlock,
        LabelDefinitionIr, LinkInline, ListBlock, ListItemIr, ListKind, ParagraphBlock,
        ProvenanceSpan, ReferenceInline, SourceProvenance, SourceSpan, TableBlock, TableCell,
        TableRow, TableRuleSpan, TextCluster, TitleBlock,
    };

    use super::{PageDisplayListOptions, build_page_display_lists};

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
                    date: None,
                    date_source: None,
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
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "Longer".to_string(),
                                column_span: None,
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
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
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
    fn table_display_list_text_renders_horizontal_rules() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                rows: vec![
                    TableRow {
                        rule_above: true,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Head".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "Value".to_string(),
                                column_span: None,
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
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
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

        assert_eq!(
            lines.iter().filter(|line| **line == "------------").count(),
            3,
            "{lines:?}"
        );
        assert!(lines.contains(&"Head | Value"), "{lines:?}");
        assert!(lines.contains(&"A    | B"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_renders_partial_horizontal_rules() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Head".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "Value".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
                            },
                        ],
                        rule_below: false,
                        partial_rules_below: vec![TableRuleSpan {
                            start_column: 1,
                            end_column: 2,
                        }],
                    },
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "A".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "C".to_string(),
                                column_span: None,
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
        assert!(lines.contains(&".......------------"), "{lines:?}");
        assert!(lines.contains(&"A    | B     | C"), "{lines:?}");
    }

    #[test]
    fn table_display_list_text_uses_multicolumn_spans() {
        let source = SourceProvenance::file("main.tex", 0, 64);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Table(TableBlock {
                environment: "tabular".to_string(),
                rows: vec![
                    TableRow {
                        rule_above: false,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCell {
                                text: "Wide".to_string(),
                                column_span: Some(2),
                            },
                            TableCell {
                                text: "Tail".to_string(),
                                column_span: None,
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
                            },
                            TableCell {
                                text: "B".to_string(),
                                column_span: None,
                            },
                            TableCell {
                                text: "C".to_string(),
                                column_span: None,
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
                date: None,
                date_source: None,
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
    fn graphic_asset_format_affects_page_content_hash() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let pdf_display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.asset".to_string(),
                options: None,
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
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 120,
                    height_px: 60,
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
    fn graphic_keepaspectratio_fits_within_width_and_height() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("width=100pt,height=100pt,keepaspectratio".to_string()),
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: Some("blake3:asset".to_string()),
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 400,
                    height_px: 200,
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
    fn graphic_trim_affects_default_image_rect() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.png".to_string(),
                options: Some("trim=50pt 0pt 50pt 0pt,clip".to_string()),
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 200,
                    height_px: 100,
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
                asset_format: Some(GraphicAssetFormat::Png),
                asset_hash: None,
                asset_dimensions: Some(GraphicAssetDimensions {
                    width_px: 200,
                    height_px: 100,
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
    fn wraps_graphic_caption_text_runs() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: None,
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
            max_chars_per_line: 6,
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

        let first_line = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "abcdef" => Some(run),
            _ => None,
        });
        let continuation = display_lists[0].ops.iter().find_map(|op| match op {
            DrawOp::TextRun(run) if run.text == "ghi" => Some(run),
            _ => None,
        });
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
            DrawOp::TextRun(run) if run.text == "cdefgh" => Some(run),
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
}
