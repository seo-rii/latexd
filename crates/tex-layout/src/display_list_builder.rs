use tex_render_model::{
    BibliographyBlock, DocumentIr, DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries,
    FontShape, InlineNode, IrBlock, PageDisplayList, Point, PositionedImage, PositionedTextRun,
    ProvenanceSpan, Rect, SourceProvenance, SourceSpan,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PageDisplayListOptions {
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub margin_left_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
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
}

struct LogicalTextRun {
    text: String,
    source: SourceProvenance,
    font: FontRequest,
    size_pt: f32,
    gap_after_pt: f32,
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

    let mut logical_items = Vec::new();
    for block in &document_ir.blocks {
        match block {
            IrBlock::TitleBlock(block) => {
                if let Some(title) = &block.title {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        text: title.clone(),
                        source: block.source.clone(),
                        font: title_font.clone(),
                        size_pt: options.title_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                    }));
                }
                for author in &block.authors {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        text: author.clone(),
                        source: block.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: 0.0,
                    }));
                }
                if let Some(date) = &block.date {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        text: date.clone(),
                        source: block.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: options.block_gap_pt,
                    }));
                }
            }
            IrBlock::Abstract(block) => {
                let mut text = String::new();
                for node in &block.content {
                    match node {
                        InlineNode::Text { text: value, .. } => text.push_str(value),
                        InlineNode::Space { .. } => text.push(' '),
                        InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                        InlineNode::Reference(reference) => {
                            text.push_str(&reference.display_text);
                        }
                        InlineNode::InlineMath { raw_source, .. } => text.push_str(raw_source),
                        InlineNode::RawFallback(fallback) => {
                            if let Some(visible) = &fallback.normalized_visible_text {
                                text.push_str(visible);
                            } else {
                                text.push_str(&fallback.source_excerpt);
                            }
                        }
                    }
                }
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    text,
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                }));
            }
            IrBlock::Heading(block) => {
                let mut text = String::new();
                for node in &block.content {
                    match node {
                        InlineNode::Text { text: value, .. } => text.push_str(value),
                        InlineNode::Space { .. } => text.push(' '),
                        InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                        InlineNode::Reference(reference) => {
                            text.push_str(&reference.display_text);
                        }
                        InlineNode::InlineMath { raw_source, .. } => text.push_str(raw_source),
                        InlineNode::RawFallback(fallback) => {
                            if let Some(visible) = &fallback.normalized_visible_text {
                                text.push_str(visible);
                            } else {
                                text.push_str(&fallback.source_excerpt);
                            }
                        }
                    }
                }
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    text,
                    source: block.source.clone(),
                    font: heading_font.clone(),
                    size_pt: options.heading_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                }));
            }
            IrBlock::Paragraph(block) => {
                let mut text = String::new();
                for node in &block.content {
                    match node {
                        InlineNode::Text { text: value, .. } => text.push_str(value),
                        InlineNode::Space { .. } => text.push(' '),
                        InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                        InlineNode::Reference(reference) => {
                            text.push_str(&reference.display_text);
                        }
                        InlineNode::InlineMath { raw_source, .. } => text.push_str(raw_source),
                        InlineNode::RawFallback(fallback) => {
                            if let Some(visible) = &fallback.normalized_visible_text {
                                text.push_str(visible);
                            } else {
                                text.push_str(&fallback.source_excerpt);
                            }
                        }
                    }
                }
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    text,
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                }));
            }
            IrBlock::DisplayMath(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    text: block.raw_source.clone(),
                    source: block.source.clone(),
                    font: math_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                }));
            }
            IrBlock::Bibliography(block) => {
                let BibliographyBlock { items, source } = block;
                for item in items {
                    let text = if let Some(label) = &item.label {
                        format!("[{label}] {}", item.content)
                    } else {
                        item.content.clone()
                    };
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        text,
                        source: item.source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: 0.0,
                    }));
                }
                if items.is_empty() {
                    logical_items.push(LogicalItem::Text(LogicalTextRun {
                        text: String::new(),
                        source: source.clone(),
                        font: body_font.clone(),
                        size_pt: options.body_font_size_pt,
                        gap_after_pt: 0.0,
                    }));
                }
            }
            IrBlock::Graphic(block) => {
                logical_items.push(LogicalItem::Image(LogicalImage {
                    path: block.path.clone(),
                    caption: block.caption.clone(),
                    caption_source: block.caption_source.clone(),
                    source: block.source.clone(),
                    gap_after_pt: options.block_gap_pt,
                }));
            }
            IrBlock::RawFallback(block) => {
                logical_items.push(LogicalItem::Text(LogicalTextRun {
                    text: block
                        .normalized_visible_text
                        .clone()
                        .unwrap_or_else(|| block.source_excerpt.clone()),
                    source: block.source.clone(),
                    font: body_font.clone(),
                    size_pt: options.body_font_size_pt,
                    gap_after_pt: options.block_gap_pt,
                }));
            }
        }
    }

    let mut pages = Vec::new();
    let finish_page =
        |pages: &mut Vec<PageDisplayList>, page_index: usize, pending: PendingPage| {
            let content_hash = blake3::hash(pending.text.as_bytes()).to_hex().to_string();
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
    let mut pending = PendingPage {
        ops: Vec::new(),
        source_spans: Vec::new(),
        text: String::new(),
    };
    let mut page_index = 0usize;
    let mut y = options.margin_top_pt;

    for logical in logical_items {
        match logical {
            LogicalItem::Text(logical) => {
                let mut wrapped_lines = Vec::new();
                for raw_line in logical.text.lines() {
                    if raw_line.trim().is_empty() {
                        wrapped_lines.push(String::new());
                        continue;
                    }
                    let mut current = String::new();
                    for word in raw_line.split_whitespace() {
                        let candidate_len = if current.is_empty() {
                            word.len()
                        } else {
                            current.len() + 1 + word.len()
                        };
                        if candidate_len > options.max_chars_per_line && !current.is_empty() {
                            wrapped_lines.push(current);
                            current = word.to_string();
                        } else {
                            if !current.is_empty() {
                                current.push(' ');
                            }
                            current.push_str(word);
                        }
                    }
                    if !current.is_empty() {
                        wrapped_lines.push(current);
                    }
                }
                if wrapped_lines.is_empty() {
                    wrapped_lines.push(logical.text);
                }

                for line in wrapped_lines {
                    if y + options.line_height_pt
                        > options.page_height_pt - options.margin_bottom_pt
                        && !pending.ops.is_empty()
                    {
                        finish_page(&mut pages, page_index, pending);
                        page_index += 1;
                        pending = PendingPage {
                            ops: Vec::new(),
                            source_spans: Vec::new(),
                            text: String::new(),
                        };
                        y = options.margin_top_pt;
                    }

                    if !pending.text.is_empty() {
                        pending.text.push('\n');
                    }
                    pending.text.push_str(&line);
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
                    pending.ops.push(DrawOp::TextRun(PositionedTextRun {
                        origin: Point {
                            x: options.margin_left_pt,
                            y,
                        },
                        text: line.clone(),
                        font: logical.font.clone(),
                        size_pt: logical.size_pt,
                        approximate_advance_pt: line.chars().count() as f32 * logical.size_pt * 0.5,
                        glyphs: None,
                        clusters: None,
                        source: logical.source.clone(),
                    }));
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
                    pending = PendingPage {
                        ops: Vec::new(),
                        source_spans: Vec::new(),
                        text: String::new(),
                    };
                    y = options.margin_top_pt;
                }

                if !pending.text.is_empty() {
                    pending.text.push('\n');
                }
                pending.text.push_str(&format!("[image: {}]", logical.path));
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
                    }
                    pending.text.push_str(caption);
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
        finish_page(&mut pages, page_index, pending);
    } else if !pending.ops.is_empty() {
        finish_page(&mut pages, page_index, pending);
    }

    pages
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        DocumentIr, DrawOp, GraphicBlock, InlineNode, IrBlock, ParagraphBlock, SourceProvenance,
        TitleBlock,
    };

    use super::{PageDisplayListOptions, build_page_display_lists};

    #[test]
    fn builds_positioned_text_runs_from_document_ir() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let display_lists = build_page_display_lists(
            &DocumentIr::new(vec![
                IrBlock::TitleBlock(TitleBlock {
                    title: Some("A Paper".to_string()),
                    authors: vec!["Ada Lovelace".to_string()],
                    date: None,
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
}
