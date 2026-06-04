use serde::{Deserialize, Serialize};

use crate::{
    CitationStyleHint, GraphicAssetDimensions, GraphicAssetFormat, ListKind, RawFallbackEvent,
    SourceProvenance, TableColumnAlignment, TableColumnSpec, TableRuleSpan,
};

pub const DOCUMENT_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentIr {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LabelDefinitionIr>,
    pub blocks: Vec<IrBlock>,
}

impl DocumentIr {
    pub fn new(blocks: Vec<IrBlock>) -> Self {
        Self::with_labels(blocks, Vec::new())
    }

    pub fn with_labels(blocks: Vec<IrBlock>, labels: Vec<LabelDefinitionIr>) -> Self {
        Self {
            schema_version: DOCUMENT_IR_SCHEMA_VERSION,
            labels,
            blocks,
        }
    }

    pub fn extracted_text(&self) -> String {
        let mut text = String::new();
        for block in &self.blocks {
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
                    if let Some(date) = &block.date {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(date);
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
                IrBlock::Graphic(block) => {
                    if let Some(caption) = &block.caption {
                        text.push_str(caption);
                    }
                }
                IrBlock::Table(block) => text.push_str(&block.visible_text()),
                IrBlock::RawFallback(block) => {
                    if let Some(visible) = &block.normalized_visible_text {
                        text.push_str(visible);
                    } else {
                        text.push_str(&block.source_excerpt);
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
    List(ListBlock),
    DisplayMath(DisplayMathBlock),
    Bibliography(BibliographyBlock),
    Graphic(GraphicBlock),
    Table(TableBlock),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_source: Option<SourceProvenance>,
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
pub struct ListBlock {
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
    pub source: SourceProvenance,
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
pub struct TableBlock {
    pub environment: String,
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
    InlineMath {
        raw_source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        normalized_text: Option<String>,
        source: SourceProvenance,
    },
    RawFallback(RawFallbackIr),
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
    use super::{DisplayMathBlock, DocumentIr, HeadingBlock, InlineNode, IrBlock, ParagraphBlock};
    use crate::SourceProvenance;

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
                source,
            }),
        ]);

        assert_eq!(document.extracted_text(), "alpha\nbeta");
    }
}
