use tex_render_model::{
    BibliographyBlock, DocumentIr, DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries,
    FontShape, InlineNode, IrBlock, LinkAnnotation, PageDisplayList, Point, PositionedImage,
    PositionedTextRun, ProvenanceSpan, Rect, SourceProvenance, SourceSpan,
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
                    raw_source, source, ..
                } => {
                    segments.push(LogicalTextSegment {
                        text: raw_source.clone(),
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
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    segments: inline_segments(&block.content),
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
                        text: block.raw_source.clone(),
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
        hash_input: format!("options:{options:?}"),
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
                let available_width_pt =
                    (options.page_width_pt - options.margin_left_pt * 2.0 - widest_indent)
                        .max(logical.size_pt * 0.5);
                let width_limited_chars =
                    (available_width_pt / (logical.size_pt * 0.5).max(0.1)).floor() as usize;
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
                        let advance = segment.text.chars().count() as f32 * logical.size_pt * 0.5;
                        let source = segment.source;
                        pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                            origin: Point { x, y },
                            text: segment.text,
                            font: logical.font.clone(),
                            size_pt: logical.size_pt,
                            approximate_advance_pt: advance,
                            glyphs: None,
                            clusters: None,
                            source: source.clone(),
                        }));
                        if let Some(target) = segment.link_target {
                            pending.hash_input.push('\u{1f}');
                            pending.hash_input.push_str("link:");
                            pending.hash_input.push_str(&target);
                            pending.ops.push(DrawOp::LinkAnnotation(LinkAnnotation {
                                rect: Rect {
                                    x,
                                    y: (y - logical.size_pt).max(0.0),
                                    width: advance,
                                    height: options.line_height_pt,
                                },
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
                let image_width = options.page_width_pt - options.margin_left_pt * 2.0;
                let image_height = options.line_height_pt * 6.0;
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
                let image_text = format!("[image: {}]", logical.path);
                pending.text.push_str(&image_text);
                pending.hash_input.push_str(&image_text);
                if let ProvenanceSpan::File(span) = &logical.source.primary {
                    if !pending.source_spans.contains(span) {
                        pending.source_spans.push(span.clone());
                    }
                }
                for related in &logical.source.related {
                    if let ProvenanceSpan::File(span) = &related.span {
                        if !pending.source_spans.contains(span) {
                            pending.source_spans.push(span.clone());
                        }
                    }
                }
                pending.ops.push(DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: options.margin_left_pt,
                        y,
                        width: image_width,
                        height: image_height,
                    },
                    asset_ref: logical.path.clone(),
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
                    if let ProvenanceSpan::File(span) = &caption_source.primary {
                        if !pending.source_spans.contains(span) {
                            pending.source_spans.push(span.clone());
                        }
                    }
                    pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                        origin: Point {
                            x: options.margin_left_pt,
                            y,
                        },
                        text: caption.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        approximate_advance_pt: caption.chars().count() as f32
                            * options.body_font_size_pt
                            * 0.5,
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

    if pending.ops.is_empty() && pages.is_empty() {
        pending.text = String::new();
        pending.hash_input = format!("options:{options:?}");
        finish_page(&mut pages, page_index, pending);
    } else if !pending.ops.is_empty() {
        finish_page(&mut pages, page_index, pending);
    }

    pages
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        AbstractBlock, BibliographyBlock, BibliographyItemIr, CitationInline, CitationStyleHint,
        DocumentIr, DrawOp, GraphicBlock, HeadingBlock, InlineNode, IrBlock, LinkInline, ListBlock,
        ListItemIr, ListKind, ParagraphBlock, ReferenceInline, SourceProvenance, TitleBlock,
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
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: Some("width=0.8\\linewidth".to_string()),
                caption: Some("Plot caption.".to_string()),
                caption_source: Some(SourceProvenance::file("main.tex", 25, 38)),
                source,
            })]),
            PageDisplayListOptions::default(),
        );

        assert_eq!(display_lists.len(), 1);
        assert!(display_lists[0].ops.iter().any(|op| {
            matches!(
                op,
                DrawOp::Image(image) if image.asset_ref == "figures/plot.pdf"
                    && image.rect.x == 72.0
                    && image.rect.width == 468.0
            )
        }));
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
        assert_eq!(display_lists[0].source_spans.len(), 2);
    }

    #[test]
    fn wraps_graphic_caption_text_runs() {
        let source = SourceProvenance::file("main.tex", 0, 24);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![IrBlock::Graphic(GraphicBlock {
                path: "figures/plot.pdf".to_string(),
                options: None,
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
