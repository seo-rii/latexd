use serde::{Deserialize, Serialize};

use crate::{CitationStyleHint, RawFallbackEvent, SourceProvenance};

pub const DOCUMENT_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentIr {
    pub schema_version: u32,
    pub blocks: Vec<IrBlock>,
}

impl DocumentIr {
    pub fn new(blocks: Vec<IrBlock>) -> Self {
        Self {
            schema_version: DOCUMENT_IR_SCHEMA_VERSION,
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
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
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
                }
                IrBlock::Heading(block) => {
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
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
                }
                IrBlock::Paragraph(block) => {
                    for node in &block.content {
                        match node {
                            InlineNode::Text { text: value, .. } => text.push_str(value),
                            InlineNode::Space { .. } => text.push(' '),
                            InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                            InlineNode::Reference(reference) => {
                                text.push_str(&reference.display_text)
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
                }
                IrBlock::DisplayMath(block) => text.push_str(&block.raw_source),
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
    DisplayMath(DisplayMathBlock),
    Bibliography(BibliographyBlock),
    Graphic(GraphicBlock),
    RawFallback(RawFallbackIr),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
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
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_source: Option<SourceProvenance>,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFallbackIr {
    pub source_excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_visible_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub reason: crate::FallbackReason,
    pub source: SourceProvenance,
}

impl RawFallbackIr {
    pub fn from_event(event: &RawFallbackEvent, source: SourceProvenance) -> Self {
        Self {
            source_excerpt: event.source_excerpt.clone(),
            normalized_visible_text: event.normalized_visible_text.clone(),
            environment: event.environment.clone(),
            reason: event.reason,
            source,
        }
    }
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
    Citation(CitationInline),
    Reference(ReferenceInline),
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
