use serde::{Deserialize, Serialize};

use crate::{
    CitationStyleHint, DocumentLayoutIntent, FootnoteCommandKind, FootnoteId,
    GraphicAssetDimensions, GraphicAssetFormat, GraphicPageSelection, LayoutAlignment, ListKind,
    PageBreakKind, RawFallbackEvent, SourceProvenance, TableColumnAlignment, TableColumnSpec,
    TableRuleSpan,
};

pub const DOCUMENT_IR_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentClassIr {
    pub name: String,
    pub options: Vec<String>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentIr {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_class: Option<DocumentClassIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<DocumentLayoutIntent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LabelDefinitionIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<FootnoteIr>,
    pub blocks: Vec<IrBlock>,
}

impl DocumentIr {
    pub fn new(blocks: Vec<IrBlock>) -> Self {
        Self::with_labels(blocks, Vec::new())
    }

    pub fn with_labels(blocks: Vec<IrBlock>, labels: Vec<LabelDefinitionIr>) -> Self {
        Self::with_document_class_and_labels(blocks, None, labels)
    }

    pub fn with_document_class_and_labels(
        blocks: Vec<IrBlock>,
        document_class: Option<DocumentClassIr>,
        labels: Vec<LabelDefinitionIr>,
    ) -> Self {
        Self::with_document_class_layout_and_labels(blocks, document_class, None, labels)
    }

    pub fn with_document_class_layout_and_labels(
        blocks: Vec<IrBlock>,
        document_class: Option<DocumentClassIr>,
        layout: Option<DocumentLayoutIntent>,
        labels: Vec<LabelDefinitionIr>,
    ) -> Self {
        Self {
            schema_version: DOCUMENT_IR_SCHEMA_VERSION,
            document_class,
            layout,
            labels,
            footnotes: Vec::new(),
            blocks,
        }
    }

    pub fn extracted_text(&self) -> String {
        let mut text = String::new();
        let mut pending_blocks = self.blocks.iter().rev().collect::<Vec<_>>();
        while let Some(block) = pending_blocks.pop() {
            if let IrBlock::LayoutContainer(container) = block {
                pending_blocks.extend(container.children.iter().rev());
                continue;
            }
            if matches!(block, IrBlock::PageBreak(_)) {
                continue;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            match block {
                IrBlock::TitleBlock(block) => {
                    if let Some(title) = &block.title {
                        text.push_str(title);
                    }
                    for author in &block.authors {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(author);
                    }
                    for author_note in &block.author_notes {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(author_note);
                    }
                    for affiliation in &block.affiliations {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(affiliation);
                    }
                    for correspondence in &block.correspondence {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(correspondence);
                    }
                    if let Some(date) = &block.date {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(date);
                    }
                    for keyword in &block.keywords {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(keyword);
                    }
                    for pacs in &block.pacs {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(pacs);
                    }
                }
                IrBlock::Abstract(block) => {
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::LineBreak { .. } => text.push('\n'),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
                            }
                            InlineNode::Link(link) => text.push_str(&link.display_text),
                            InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                                text.push_str(&anchor.marker)
                            }
                            InlineNode::FootnoteAnchor(_) => {}
                            InlineNode::InlineMath {
                                raw_source,
                                normalized_text,
                                ..
                            } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                            InlineNode::RawFallback(fallback) => {
                                if let Some(visible) = &fallback.normalized_visible_text {
                                    text.push_str(visible);
                                } else {
                                    text.push_str(&fallback.source_excerpt);
                                }
                            }
                        }
                    }
                }
                IrBlock::Heading(block) => {
                    if let Some(number) = &block.number {
                        text.push_str(number);
                        text.push(' ');
                    }
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::LineBreak { .. } => text.push('\n'),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
                            }
                            InlineNode::Link(link) => text.push_str(&link.display_text),
                            InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                                text.push_str(&anchor.marker)
                            }
                            InlineNode::FootnoteAnchor(_) => {}
                            InlineNode::InlineMath {
                                raw_source,
                                normalized_text,
                                ..
                            } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                            InlineNode::RawFallback(fallback) => {
                                if let Some(visible) = &fallback.normalized_visible_text {
                                    text.push_str(visible);
                                } else {
                                    text.push_str(&fallback.source_excerpt);
                                }
                            }
                        }
                    }
                }
                IrBlock::Paragraph(block) => {
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::LineBreak { .. } => text.push('\n'),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
                            }
                            InlineNode::Link(link) => text.push_str(&link.display_text),
                            InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                                text.push_str(&anchor.marker)
                            }
                            InlineNode::FootnoteAnchor(_) => {}
                            InlineNode::InlineMath {
                                raw_source,
                                normalized_text,
                                ..
                            } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                            InlineNode::RawFallback(fallback) => {
                                if let Some(visible) = &fallback.normalized_visible_text {
                                    text.push_str(visible);
                                } else {
                                    text.push_str(&fallback.source_excerpt);
                                }
                            }
                        }
                    }
                }
                IrBlock::Environment(block) => {
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::LineBreak { .. } => text.push('\n'),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
                            }
                            InlineNode::Link(link) => text.push_str(&link.display_text),
                            InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                                text.push_str(&anchor.marker)
                            }
                            InlineNode::FootnoteAnchor(_) => {}
                            InlineNode::InlineMath {
                                raw_source,
                                normalized_text,
                                ..
                            } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                            InlineNode::RawFallback(fallback) => {
                                if let Some(visible) = &fallback.normalized_visible_text {
                                    text.push_str(visible);
                                } else {
                                    text.push_str(&fallback.source_excerpt);
                                }
                            }
                        }
                    }
                }
                IrBlock::LayoutContainer(_) => unreachable!("layout containers are flattened"),
                IrBlock::List(block) => {
                    for (index, item) in block.items.iter().enumerate() {
                        if index > 0 {
                            text.push('\n');
                        }
                        text.push_str(&item.marker);
                        text.push(' ');
                        for node in &item.content {
                            match node {
                                InlineNode::Text { text: value, .. } => text.push_str(value),
                                InlineNode::Space { .. } => text.push(' '),
                                InlineNode::LineBreak { .. } => text.push('\n'),
                                InlineNode::Citation(citation) => {
                                    text.push_str(&citation.display_text)
                                }
                                InlineNode::Reference(reference) => {
                                    text.push_str(&reference.display_text)
                                }
                                InlineNode::Link(link) => text.push_str(&link.display_text),
                                InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                                    text.push_str(&anchor.marker)
                                }
                                InlineNode::FootnoteAnchor(_) => {}
                                InlineNode::InlineMath {
                                    raw_source,
                                    normalized_text,
                                    ..
                                } => {
                                    text.push_str(normalized_text.as_deref().unwrap_or(raw_source))
                                }
                                InlineNode::RawFallback(fallback) => {
                                    if let Some(visible) = &fallback.normalized_visible_text {
                                        text.push_str(visible);
                                    } else {
                                        text.push_str(&fallback.source_excerpt);
                                    }
                                }
                            }
                        }
                    }
                }
                IrBlock::DisplayMath(block) => text.push_str(
                    block
                        .normalized_text
                        .as_deref()
                        .unwrap_or(&block.raw_source),
                ),
                IrBlock::Bibliography(block) => {
                    for (index, item) in block.items.iter().enumerate() {
                        if index > 0 {
                            text.push('\n');
                        }
                        text.push_str(&item.content);
                    }
                }
                IrBlock::Graphic(block) | IrBlock::FullWidthGraphic(block) => {
                    if let Some(caption) = &block.caption {
                        text.push_str(caption);
                    }
                }
                IrBlock::IncludedPdfPage(_) => {}
                IrBlock::PageBreak(_) => {}
                IrBlock::Table(block) | IrBlock::FullWidthTable(block) => {
                    text.push_str(&block.visible_text())
                }
                IrBlock::RawFallback(block) => {
                    if let Some(visible) = &block.normalized_visible_text {
                        text.push_str(visible);
                    } else {
                        text.push_str(&block.source_excerpt);
                    }
                }
            }
        }
        for footnote in &self.footnotes {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&footnote.marker);
            text.push(' ');
            for node in &footnote.content {
                match node {
                    InlineNode::Text { text: value, .. } => text.push_str(value),
                    InlineNode::Space { .. } => text.push(' '),
                    InlineNode::LineBreak { .. } => text.push('\n'),
                    InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                    InlineNode::Reference(reference) => text.push_str(&reference.display_text),
                    InlineNode::Link(link) => text.push_str(&link.display_text),
                    InlineNode::FootnoteAnchor(anchor) if anchor.draw_reference => {
                        text.push_str(&anchor.marker)
                    }
                    InlineNode::FootnoteAnchor(_) => {}
                    InlineNode::InlineMath {
                        raw_source,
                        normalized_text,
                        ..
                    } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                    InlineNode::RawFallback(fallback) => {
                        if let Some(visible) = &fallback.normalized_visible_text {
                            text.push_str(visible);
                        } else {
                            text.push_str(&fallback.source_excerpt);
                        }
                    }
                }
            }
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrBlock {
    TitleBlock(TitleBlock),
    Abstract(AbstractBlock),
    Heading(HeadingBlock),
    Paragraph(ParagraphBlock),
    Environment(EnvironmentBlock),
    LayoutContainer(LayoutContainerBlock),
    List(ListBlock),
    DisplayMath(DisplayMathBlock),
    Bibliography(BibliographyBlock),
    Graphic(GraphicBlock),
    FullWidthGraphic(GraphicBlock),
    IncludedPdfPage(GraphicBlock),
    PageBreak(PageBreakBlock),
    Table(TableBlock),
    FullWidthTable(TableBlock),
    RawFallback(RawFallbackIr),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<SourceProvenance>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_sources: Vec<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_note_sources: Vec<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affiliations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affiliation_sources: Vec<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correspondence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correspondence_sources: Vec<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_source: Option<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_sources: Vec<SourceProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pacs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pacs_sources: Vec<SourceProvenance>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbstractBlock {
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingBlock {
    pub level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphBlock {
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentBlock {
    pub name: String,
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutContainerBlock {
    pub name: String,
    pub width_spec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<LayoutAlignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_spec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_alignment: Option<LayoutAlignment>,
    pub children: Vec<IrBlock>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBlock {
    #[serde(rename = "list_kind")]
    pub kind: ListKind,
    pub items: Vec<ListItemIr>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItemIr {
    pub marker: String,
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayMathBlock {
    pub raw_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<MathNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MathNode {
    Row {
        children: Vec<MathNode>,
    },
    Atom {
        text: String,
        atom_kind: MathAtomKind,
    },
    LargeOperator {
        operator: MathLargeOperator,
    },
    Fraction {
        numerator: Box<MathNode>,
        denominator: Box<MathNode>,
    },
    Scripts {
        base: Box<MathNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscript: Option<Box<MathNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        superscript: Option<Box<MathNode>>,
        placement: MathScriptPlacement,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathAtomKind {
    Identifier,
    Number,
    Operator,
    Relation,
    Delimiter,
    Punctuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathLargeOperator {
    Sum,
    Product,
    Integral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathScriptPlacement {
    Side,
    Limits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyBlock {
    pub items: Vec<BibliographyItemIr>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyItemIr {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub content: String,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicBlock {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_selection: Option<GraphicPageSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_format: Option<GraphicAssetFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_dimensions: Option<GraphicAssetDimensions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_source: Option<SourceProvenance>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageBreakBlock {
    #[serde(rename = "break_kind")]
    pub kind: PageBreakKind,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableBlock {
    pub environment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_spec: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<TableColumnSpec>,
    pub rows: Vec<TableRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_source: Option<SourceProvenance>,
    pub source: SourceProvenance,
}

impl TableBlock {
    pub fn visible_text(&self) -> String {
        let mut text = String::new();
        if let Some(caption) = &self.caption {
            text.push_str(caption);
        }
        for row in &self.rows {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(
                &row.cells
                    .iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    #[serde(default)]
    pub rule_above: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partial_rules_above: Vec<TableRuleSpan>,
    pub cells: Vec<TableCell>,
    #[serde(default)]
    pub rule_below: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partial_rules_below: Vec<TableRuleSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_span: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_span: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TableColumnAlignment>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rule_before_count: u8,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rule_after_count: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_suffix: Option<String>,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFallbackIr {
    pub source_excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_visible_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub reason: crate::FallbackReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_source_artifact: Option<String>,
    #[serde(default)]
    pub truncated: bool,
    pub source: SourceProvenance,
}

impl RawFallbackIr {
    pub fn from_event(event: &RawFallbackEvent, source: SourceProvenance) -> Self {
        Self {
            source_excerpt: event.source_excerpt.clone(),
            expanded_text: event.expanded_text.clone(),
            normalized_visible_text: event.normalized_visible_text.clone(),
            environment: event.environment.clone(),
            reason: event.reason,
            source_hash: event.source_hash.clone(),
            full_source_artifact: event.full_source_artifact.clone(),
            truncated: event.truncated,
            source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelDefinitionIr {
    pub key: String,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InlineNode {
    Text {
        text: String,
        source: SourceProvenance,
    },
    Space {
        source: SourceProvenance,
    },
    LineBreak {
        source: SourceProvenance,
    },
    Citation(CitationInline),
    Reference(ReferenceInline),
    Link(LinkInline),
    FootnoteAnchor(FootnoteAnchor),
    InlineMath {
        raw_source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        normalized_text: Option<String>,
        source: SourceProvenance,
    },
    RawFallback(RawFallbackIr),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteAnchor {
    pub note_id: FootnoteId,
    pub marker: String,
    pub draw_reference: bool,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteIr {
    pub note_id: FootnoteId,
    pub marker: String,
    pub command: FootnoteCommandKind,
    pub content: Vec<InlineNode>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationInline {
    pub keys: Vec<String>,
    pub style_hint: CitationStyleHint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_label: Option<String>,
    pub display_text: String,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceInline {
    pub keys: Vec<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_target: Option<String>,
    pub display_text: String,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInline {
    pub target: String,
    pub display_text: String,
    pub source: SourceProvenance,
}

#[cfg(test)]
mod tests {
    use super::{
        DisplayMathBlock, DocumentIr, HeadingBlock, InlineNode, IrBlock, ListBlock, ListItemIr,
        PageBreakBlock, ParagraphBlock,
    };
    use crate::{ListKind, PageBreakKind, SourceProvenance};

    #[test]
    fn extracted_text_includes_heading_numbers() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let document = DocumentIr::new(vec![IrBlock::Heading(HeadingBlock {
            level: 1,
            number: Some("1".to_string()),
            content: vec![InlineNode::Text {
                text: "Intro".to_string(),
                source: source.clone(),
            }],
            source,
        })]);

        assert_eq!(document.extracted_text(), "1 Intro");
    }

    #[test]
    fn extracted_text_prefers_normalized_math_text() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let document = DocumentIr::new(vec![
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
                structure: None,
                source,
            }),
        ]);

        assert_eq!(document.extracted_text(), "alpha\nbeta");
    }

    #[test]
    fn list_block_json_uses_distinct_list_kind_field() {
        let source = SourceProvenance::file("main.tex", 0, 16);
        let document = DocumentIr::new(vec![IrBlock::List(ListBlock {
            kind: ListKind::Ordered,
            items: vec![ListItemIr {
                marker: "1.".to_string(),
                content: vec![InlineNode::Text {
                    text: "First".to_string(),
                    source: source.clone(),
                }],
                source: source.clone(),
            }],
            source,
        })]);

        let encoded = serde_json::to_string(&document).expect("serialize document IR");
        let decoded: DocumentIr = serde_json::from_str(&encoded).expect("deserialize document IR");

        assert!(encoded.contains("\"kind\":\"list\""));
        assert!(encoded.contains("\"list_kind\":\"ordered\""));
        assert_eq!(decoded, document);
    }

    #[test]
    fn page_break_block_roundtrips_with_a_non_conflicting_payload_field() {
        let document = DocumentIr::new(vec![IrBlock::PageBreak(PageBreakBlock {
            kind: PageBreakKind::ClearDoublePage,
            source: SourceProvenance::file("main.tex", 0, 16),
        })]);

        let encoded = serde_json::to_string(&document).expect("serialize document IR");
        let decoded: DocumentIr = serde_json::from_str(&encoded).expect("deserialize document IR");

        assert!(encoded.contains("\"kind\":\"page_break\""));
        assert!(encoded.contains("\"break_kind\":\"clear_double_page\""));
        assert_eq!(decoded, document);
    }
}
