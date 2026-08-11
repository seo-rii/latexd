use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{CitationStyleHint, GeneratedBy, SourceProvenance};

pub type EventSequence = u64;
pub type FootnoteId = u64;

pub const RENDER_EVENT_SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderEventStream {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    pub events: Vec<RenderEventEnvelope>,
}

impl RenderEventStream {
    pub fn new(case: impl Into<Option<String>>, events: Vec<RenderEventEnvelope>) -> Self {
        Self {
            schema_version: RENDER_EVENT_SCHEMA_VERSION,
            case: case.into(),
            events,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBuildContext {
    sequence: EventSequence,
    source: SourceProvenance,
}

impl EventBuildContext {
    pub fn new(sequence: EventSequence, source: SourceProvenance) -> Self {
        Self { sequence, source }
    }

    pub fn sequence(&self) -> EventSequence {
        self.sequence
    }

    pub fn source(&self) -> &SourceProvenance {
        &self.source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryConfidence {
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventOrigin(EventOriginKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventOriginKind {
    Primitive,
    Macro,
    ScannerRecovery(RecoveryConfidence),
    Lossy,
    UnknownLow,
    RawFallback,
    DiagnosticUnknown,
    DiagnosticScannerRecovery,
}

impl EventOrigin {
    pub const fn primitive() -> Self {
        Self(EventOriginKind::Primitive)
    }

    pub const fn macro_expansion() -> Self {
        Self(EventOriginKind::Macro)
    }

    pub const fn scanner_recovery(confidence: RecoveryConfidence) -> Self {
        Self(EventOriginKind::ScannerRecovery(confidence))
    }

    /// Preserves the current `fallback`/`low` wire projection for lossy execution.
    ///
    /// Changing this projection to fallback confidence requires a separate
    /// reconciliation and downstream compatibility audit.
    pub const fn lossy() -> Self {
        Self(EventOriginKind::Lossy)
    }

    pub const fn unknown_low() -> Self {
        Self(EventOriginKind::UnknownLow)
    }

    pub const fn raw_fallback() -> Self {
        Self(EventOriginKind::RawFallback)
    }

    pub const fn diagnostic_unknown() -> Self {
        Self(EventOriginKind::DiagnosticUnknown)
    }

    pub const fn diagnostic_scanner_recovery() -> Self {
        Self(EventOriginKind::DiagnosticScannerRecovery)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidEventOrigin;

impl fmt::Display for InvalidEventOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("event origin is incompatible with the render event kind")
    }
}

impl std::error::Error for InvalidEventOrigin {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderEventEnvelope {
    pub event: RenderEvent,
    pub meta: EventMeta,
}

impl RenderEventEnvelope {
    /// Creates an envelope using the legacy metadata defaults for the event kind.
    pub fn new(sequence: EventSequence, event: RenderEvent, source: SourceProvenance) -> Self {
        let (confidence, producer) = match &event {
            RenderEvent::RawFallback(_) => (SemanticConfidence::Fallback, EventProducer::Fallback),
            RenderEvent::Diagnostic(_) => (SemanticConfidence::Low, EventProducer::Unknown),
            _ => (SemanticConfidence::High, EventProducer::Command),
        };
        Self::with_origin(sequence, event, source, producer, confidence)
    }

    /// Creates an envelope with producer and confidence declared at the emission boundary.
    pub fn with_origin(
        sequence: EventSequence,
        event: RenderEvent,
        mut source: SourceProvenance,
        producer: EventProducer,
        confidence: SemanticConfidence,
    ) -> Self {
        if matches!(event, RenderEvent::RawFallback(_)) {
            source = source.with_generated_by(GeneratedBy::Fallback);
        }
        let mode_hint = event.default_mode_hint();
        Self {
            event,
            meta: EventMeta {
                sequence,
                source,
                mode_hint,
                confidence,
                producer,
            },
        }
    }

    /// Creates an envelope from a validated semantic origin without changing
    /// the serialized producer/confidence representation.
    pub fn try_from_origin(
        event: RenderEvent,
        context: EventBuildContext,
        origin: EventOrigin,
    ) -> Result<Self, InvalidEventOrigin> {
        let (producer, confidence) = match (&event, origin.0) {
            (RenderEvent::RawFallback(_), EventOriginKind::RawFallback) => {
                (EventProducer::Fallback, SemanticConfidence::Fallback)
            }
            (RenderEvent::Diagnostic(_), EventOriginKind::DiagnosticUnknown) => {
                (EventProducer::Unknown, SemanticConfidence::Low)
            }
            (RenderEvent::Diagnostic(_), EventOriginKind::DiagnosticScannerRecovery) => {
                (EventProducer::ScannerRecovery, SemanticConfidence::Low)
            }
            (RenderEvent::RawFallback(_) | RenderEvent::Diagnostic(_), _) => {
                return Err(InvalidEventOrigin);
            }
            (_, EventOriginKind::Primitive) => (EventProducer::Primitive, SemanticConfidence::High),
            (_, EventOriginKind::Macro) => (EventProducer::Macro, SemanticConfidence::High),
            (_, EventOriginKind::ScannerRecovery(RecoveryConfidence::Medium)) => {
                (EventProducer::ScannerRecovery, SemanticConfidence::Medium)
            }
            (_, EventOriginKind::ScannerRecovery(RecoveryConfidence::Low)) => {
                (EventProducer::ScannerRecovery, SemanticConfidence::Low)
            }
            (_, EventOriginKind::Lossy) => (EventProducer::Fallback, SemanticConfidence::Low),
            (_, EventOriginKind::UnknownLow) => (EventProducer::Unknown, SemanticConfidence::Low),
            (
                _,
                EventOriginKind::RawFallback
                | EventOriginKind::DiagnosticUnknown
                | EventOriginKind::DiagnosticScannerRecovery,
            ) => return Err(InvalidEventOrigin),
        };
        let EventBuildContext { sequence, source } = context;
        Ok(Self::with_origin(
            sequence, event, source, producer, confidence,
        ))
    }

    pub fn with_mode_hint(mut self, mode_hint: ModeHint) -> Self {
        self.meta.mode_hint = mode_hint;
        self
    }

    pub fn from_scanner_recovery(
        sequence: EventSequence,
        event: RenderEvent,
        source: SourceProvenance,
    ) -> Self {
        let origin = match &event {
            RenderEvent::RawFallback(_) => EventOrigin::raw_fallback(),
            RenderEvent::Diagnostic(_) => EventOrigin::diagnostic_unknown(),
            _ => EventOrigin::scanner_recovery(RecoveryConfidence::Medium),
        };
        Self::try_from_origin(event, EventBuildContext::new(sequence, source), origin)
            .expect("scanner recovery chooses an origin compatible with the event kind")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    #[serde(alias = "event_id")]
    pub sequence: EventSequence,
    pub source: SourceProvenance,
    pub mode_hint: ModeHint,
    pub confidence: SemanticConfidence,
    pub producer: EventProducer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeHint {
    Horizontal,
    Vertical,
    Math,
    Preamble,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticConfidence {
    High,
    Medium,
    Low,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventProducer {
    Primitive,
    Macro,
    Command,
    Shim,
    BblParser,
    ScannerRecovery,
    Fallback,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderEvent {
    Text(TextEvent),
    Space(SpaceEvent),
    LineBreak(LineBreakEvent),
    ParagraphBreak(ParagraphBreakEvent),
    DocumentClass(DocumentClassEvent),
    SetDocumentLayout(DocumentLayoutIntent),
    PageBreak(PageBreakEvent),
    SetDocumentMetadata(SetDocumentMetadataEvent),
    FlushTitleBlock(FlushTitleBlockEvent),
    BeginBlock(BeginBlockEvent),
    EndBlock(EndBlockEvent),
    BeginLayoutContainer(BeginLayoutContainerEvent),
    EndLayoutContainer(EndLayoutContainerEvent),
    Heading(HeadingEvent),
    InlineCitation(InlineCitationEvent),
    InlineReference(InlineReferenceEvent),
    InlineLink(InlineLinkEvent),
    BeginFootnote(BeginFootnoteEvent),
    EndFootnote(EndFootnoteEvent),
    FootnoteMark(FootnoteMarkEvent),
    LabelDefinition(LabelDefinitionEvent),
    ListItem(ListItemEvent),
    BibliographyItem(BibliographyItemEvent),
    GraphicRef(GraphicRefEvent),
    IncludePdf(GraphicRefEvent),
    Caption(CaptionEvent),
    Table(TableEvent),
    InlineMath(MathSourceEvent),
    DisplayMath(MathSourceEvent),
    RawFallback(RawFallbackEvent),
    Diagnostic(RenderDiagnosticEvent),
}

impl RenderEvent {
    pub fn default_mode_hint(&self) -> ModeHint {
        match self {
            Self::Text(_) | Self::Space(_) => ModeHint::Horizontal,
            Self::LineBreak(_) => ModeHint::Horizontal,
            Self::ParagraphBreak(_) => ModeHint::Vertical,
            Self::DocumentClass(_) | Self::SetDocumentLayout(_) | Self::SetDocumentMetadata(_) => {
                ModeHint::Preamble
            }
            Self::PageBreak(_) => ModeHint::Vertical,
            Self::FlushTitleBlock(_) => ModeHint::Vertical,
            Self::BeginBlock(_)
            | Self::EndBlock(_)
            | Self::BeginLayoutContainer(_)
            | Self::EndLayoutContainer(_) => ModeHint::Vertical,
            Self::Heading(_) => ModeHint::Vertical,
            Self::ListItem(_) => ModeHint::Vertical,
            Self::InlineCitation(_) => ModeHint::Horizontal,
            Self::BibliographyItem(_) => ModeHint::Vertical,
            Self::InlineReference(_)
            | Self::InlineLink(_)
            | Self::BeginFootnote(_)
            | Self::EndFootnote(_)
            | Self::FootnoteMark(_) => ModeHint::Horizontal,
            Self::GraphicRef(_) | Self::IncludePdf(_) | Self::Caption(_) | Self::Table(_) => {
                ModeHint::Vertical
            }
            Self::InlineMath(_) | Self::DisplayMath(_) => ModeHint::Math,
            Self::LabelDefinition(_) | Self::RawFallback(_) | Self::Diagnostic(_) => {
                ModeHint::Unknown
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEvent {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceEvent {
    #[serde(rename = "space_kind")]
    pub kind: SpaceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceKind {
    Interword,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginFootnoteEvent {
    pub note_id: FootnoteId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    pub command: FootnoteCommandKind,
    pub draw_reference: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndFootnoteEvent {
    pub note_id: FootnoteId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteMarkEvent {
    pub note_id: FootnoteId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FootnoteCommandKind {
    Footnote,
    FootnoteText,
    TableFootnote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineBreakEvent {
    pub reason: LineBreakReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineBreakReason {
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphBreakEvent {
    pub reason: ParagraphBreakReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphBreakReason {
    BlankLine,
    ParCommand,
    EndBlock,
    StructuralCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentClassEvent {
    pub name: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocumentLayoutIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_width_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_height_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_width_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_height_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_left_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_top_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_matter_top_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_count: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_gap_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_font_size_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_font_size_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_font_size_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_gap_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstract_indent_pt_milli: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageBreakEvent {
    #[serde(rename = "break_kind")]
    pub kind: PageBreakKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageBreakKind {
    NewPage,
    ClearPage,
    ClearDoublePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetDocumentMetadataEvent {
    pub field: MetadataField,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataField {
    Title,
    Author,
    AuthorNote,
    Affiliation,
    Correspondence,
    Date,
    Keywords,
    Pacs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlushTitleBlockEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginBlockEvent {
    pub block: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndBlockEvent {
    pub block: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginLayoutContainerEvent {
    pub name: String,
    pub width_spec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<LayoutAlignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_spec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_alignment: Option<LayoutAlignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndLayoutContainerEvent {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAlignment {
    Top,
    Center,
    Bottom,
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockKind {
    Abstract,
    Bibliography,
    Figure,
    FullWidthFigure,
    Table,
    FullWidthTable,
    List { list_kind: ListKind },
    Environment { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    Unordered,
    Ordered,
    Description,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingEvent {
    pub level: u8,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineCitationEvent {
    pub keys: Vec<String>,
    pub command: String,
    pub style_hint: CitationStyleHint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineReferenceEvent {
    pub keys: Vec<String>,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineLinkEvent {
    pub target: String,
    pub text: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelDefinitionEvent {
    pub key: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItemEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyItemEvent {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_hint: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicRefEvent {
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
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphicPageSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagebox: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicAssetDimensions {
    pub width_px: u32,
    pub height_px: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<GraphicAssetDensity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_width_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_height_pt_milli: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicAssetDensity {
    pub x_density: u32,
    pub y_density: u32,
    pub unit: GraphicAssetDensityUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicAssetDensityUnit {
    PixelsPerInch,
    PixelsPerCentimeter,
    PixelsPerMeter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicAssetFormat {
    Pdf,
    Eps,
    Svg,
    Png,
    Jpeg,
}

impl GraphicAssetFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Eps => "eps",
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let extension = camino::Utf8Path::new(path)
            .extension()?
            .to_ascii_lowercase();
        match extension.as_str() {
            "pdf" => Some(Self::Pdf),
            "eps" | "ps" => Some(Self::Eps),
            "svg" => Some(Self::Svg),
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(Self::Png);
        }
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Some(Self::Jpeg);
        }
        if bytes.starts_with(b"%PDF-") {
            return Some(Self::Pdf);
        }
        if bytes.starts_with(b"%!PS") {
            return Some(Self::Eps);
        }

        let prefix = &bytes[..bytes.len().min(1024)];
        let text = std::str::from_utf8(prefix).ok()?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim_start();
        if text.contains("<svg") {
            return Some(Self::Svg);
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionEvent {
    pub text: String,
    pub numbered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_kind: Option<CaptionKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inline_placeholders: Vec<CaptionInlinePlaceholderEvent>,
}

impl CaptionEvent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            numbered: true,
            caption_kind: None,
            inline_placeholders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionKind {
    Figure,
    Table,
    Algorithm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptionInlinePlaceholderEvent {
    Citation(InlineCitationEvent),
    Reference(InlineReferenceEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathSourceEvent {
    pub raw_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableEvent {
    pub environment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_spec: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<TableColumnSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<TableRowEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowEvent {
    #[serde(default, skip_serializing_if = "is_false")]
    pub rule_above: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partial_rules_above: Vec<TableRuleSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<TableCellEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceProvenance>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rule_below: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partial_rules_below: Vec<TableRuleSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCellEvent {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceProvenance>,
    #[serde(default = "one_usize", skip_serializing_if = "is_one_usize")]
    pub column_span: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFallbackEvent {
    pub source_excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_visible_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub reason: FallbackReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_source_artifact: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_rules: Vec<TableRuleEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_cell_spans: Vec<TableCellSpanEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_columns: Vec<TableColumnSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_width_spec: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRuleEvent {
    pub row_index: usize,
    pub position: TableRulePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_span: Option<TableRuleSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableRulePosition {
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRuleSpan {
    pub start_column: usize,
    pub end_column: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trim_start: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trim_end: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_start_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_end_pt_milli: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCellSpanEvent {
    pub row_index: usize,
    pub column_index: usize,
    pub column_span: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableColumnSpec {
    pub alignment: TableColumnAlignment,
    #[serde(default)]
    pub rule_before: bool,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rule_before_count: u8,
    #[serde(default)]
    pub rule_after: bool,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rule_after_count: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_pt_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_suffix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableColumnAlignment {
    Left,
    Center,
    Right,
    Decimal,
    Paragraph,
    Unknown,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn one_usize() -> usize {
    1
}

fn is_one_usize(value: &usize) -> bool {
    *value == 1
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    UnsupportedCommand,
    UnsupportedEnvironment,
    MissingAsset,
    UnsafeExpansion,
    TooLarge,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiagnosticEvent {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use crate::{
        BeginBlockEvent, BibliographyItemEvent, BlockKind, CaptionEvent, CitationStyleHint,
        EndBlockEvent, EventBuildContext, EventMeta, EventOrigin, EventProducer, FallbackReason,
        FlushTitleBlockEvent, GeneratedBy, GraphicAssetFormat, GraphicRefEvent, HeadingEvent,
        InlineCitationEvent, InlineLinkEvent, InlineReferenceEvent, LabelDefinitionEvent,
        LineBreakEvent, LineBreakReason, ListItemEvent, MathSourceEvent, MetadataField, ModeHint,
        PageBreakEvent, PageBreakKind, ParagraphBreakEvent, ParagraphBreakReason, RawFallbackEvent,
        RecoveryConfidence, RenderDiagnosticEvent, RenderEvent, RenderEventEnvelope,
        RenderEventStream, SemanticConfidence, SetDocumentMetadataEvent, SourceProvenance,
        SpaceEvent, SpaceKind, TableCellEvent, TableColumnAlignment, TableColumnSpec, TableEvent,
        TableRowEvent, TableRuleSpan, TextEvent,
    };

    fn raw_fallback_event() -> RenderEvent {
        RenderEvent::RawFallback(RawFallbackEvent {
            source_excerpt: "\\begin{unknownenv}x\\end{unknownenv}".to_string(),
            expanded_text: None,
            normalized_visible_text: Some("x".to_string()),
            environment: Some("unknownenv".to_string()),
            reason: FallbackReason::UnsupportedEnvironment,
            source_hash: None,
            full_source_artifact: None,
            table_rules: Vec::new(),
            table_cell_spans: Vec::new(),
            table_columns: Vec::new(),
            table_width_spec: None,
            truncated: false,
        })
    }

    fn diagnostic_event() -> RenderEvent {
        RenderEvent::Diagnostic(RenderDiagnosticEvent {
            message: "missing input missing.tex".to_string(),
        })
    }

    #[test]
    fn stream_schema_version_is_top_level() {
        let stream = RenderEventStream::new(
            Some("case".to_string()),
            vec![RenderEventEnvelope {
                event: RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::Title,
                    value: "A Paper".to_string(),
                }),
                meta: EventMeta {
                    sequence: 1,
                    source: SourceProvenance::file("main.tex", 0, 10),
                    mode_hint: ModeHint::Preamble,
                    confidence: SemanticConfidence::High,
                    producer: EventProducer::Command,
                },
            }],
        );
        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");

        assert!(encoded.contains(&format!("\"schema_version\": {}", stream.schema_version)));
        assert!(encoded.contains("\"sequence\": 1"));
        assert!(!encoded.contains("\"event_id\""));
    }

    #[test]
    fn event_meta_accepts_the_legacy_event_id_field() {
        let envelope = RenderEventEnvelope::new(
            7,
            RenderEvent::Text(TextEvent {
                text: "legacy".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 6),
        );
        let mut encoded = serde_json::to_value(&envelope).expect("encode event");
        let meta = encoded
            .get_mut("meta")
            .and_then(serde_json::Value::as_object_mut)
            .expect("event metadata");
        let sequence = meta.remove("sequence").expect("event sequence");
        meta.insert("event_id".to_string(), sequence);

        let decoded: RenderEventEnvelope =
            serde_json::from_value(encoded).expect("decode legacy event");

        assert_eq!(decoded.meta.sequence, 7);
        let reencoded = serde_json::to_value(decoded).expect("re-encode event");
        assert_eq!(reencoded["meta"]["sequence"], serde_json::json!(7));
        assert!(reencoded["meta"].get("event_id").is_none());
    }

    #[test]
    fn space_event_uses_non_conflicting_payload_field() {
        let stream = RenderEventStream::new(
            Some("case".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::Space(SpaceEvent {
                    kind: SpaceKind::Interword,
                }),
                SourceProvenance::file("main.tex", 0, 1),
            )],
        );
        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");

        assert!(encoded.contains("\"kind\": \"space\""));
        assert!(encoded.contains("\"space_kind\": \"interword\""));
    }

    #[test]
    fn page_break_event_roundtrips_with_a_non_conflicting_payload_field() {
        let stream = RenderEventStream::new(
            Some("page-break".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::PageBreak(PageBreakEvent {
                    kind: PageBreakKind::ClearPage,
                }),
                SourceProvenance::file("main.tex", 0, 10),
            )],
        );

        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");
        let decoded: RenderEventStream = serde_json::from_str(&encoded).expect("decode stream");

        assert!(encoded.contains("\"kind\": \"page_break\""));
        assert!(encoded.contains("\"break_kind\": \"clear_page\""));
        assert_eq!(decoded, stream);
    }

    #[test]
    fn block_boundary_events_use_separate_payload_types_without_changing_json_shape() {
        let stream = RenderEventStream::new(
            Some("block-boundary".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::Abstract,
                    }),
                    SourceProvenance::file("main.tex", 0, 16),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::EndBlock(EndBlockEvent {
                        block: BlockKind::Abstract,
                    }),
                    SourceProvenance::file("main.tex", 17, 31),
                ),
            ],
        );

        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");
        assert!(encoded.contains("\"kind\": \"begin_block\""));
        assert!(encoded.contains("\"kind\": \"end_block\""));
        assert_eq!(encoded.matches("\"kind\": \"abstract\"").count(), 2);

        let decoded: RenderEventStream = serde_json::from_str(&encoded).expect("decode stream");
        assert_eq!(decoded, stream);
    }

    #[test]
    fn structured_table_event_roundtrips_without_fallback_fields() {
        let stream = RenderEventStream::new(
            Some("table".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::Table(TableEvent {
                    environment: "tabular".to_string(),
                    width_spec: Some("\\textwidth".to_string()),
                    columns: vec![
                        TableColumnSpec {
                            alignment: TableColumnAlignment::Left,
                            rule_before: true,
                            rule_before_count: 1,
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
                            rule_after: true,
                            rule_after_count: 1,
                            separator_after: None,
                            width_pt_milli: None,
                            cell_prefix: None,
                            cell_suffix: None,
                        },
                    ],
                    rows: vec![TableRowEvent {
                        rule_above: true,
                        partial_rules_above: Vec::new(),
                        cells: vec![
                            TableCellEvent {
                                text: "Alpha".to_string(),
                                source: None,
                                column_span: 1,
                                row_span: None,
                                alignment: None,
                                rule_before_count: 0,
                                rule_after_count: 0,
                                cell_prefix: None,
                                cell_suffix: None,
                            },
                            TableCellEvent {
                                text: "1".to_string(),
                                source: None,
                                column_span: 1,
                                row_span: Some(2),
                                alignment: Some(TableColumnAlignment::Right),
                                rule_before_count: 0,
                                rule_after_count: 1,
                                cell_prefix: None,
                                cell_suffix: Some("!".to_string()),
                            },
                        ],
                        source: None,
                        rule_below: false,
                        partial_rules_below: vec![TableRuleSpan {
                            start_column: 1,
                            end_column: 2,
                            trim_start: false,
                            trim_end: true,
                            trim_start_pt_milli: None,
                            trim_end_pt_milli: Some(500),
                        }],
                    }],
                    caption: Some("Measurements".to_string()),
                }),
                SourceProvenance::file("main.tex", 0, 80),
            )],
        );

        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");
        let decoded: RenderEventStream = serde_json::from_str(&encoded).expect("decode stream");

        assert!(encoded.contains("\"kind\": \"table\""));
        assert!(encoded.contains("\"environment\": \"tabular\""));
        assert!(!encoded.contains("\"source_excerpt\""));
        assert_eq!(decoded, stream);
        assert_eq!(stream.events[0].meta.mode_hint, ModeHint::Vertical);
    }

    #[test]
    fn structured_table_event_roundtrips_nested_source_provenance() {
        let row_source = serde_json::json!({
            "primary": {
                "kind": "file",
                "path": "main.tex",
                "start_utf8": 10,
                "end_utf8": 20
            },
            "related": [],
            "expansion_stack": [],
            "generated_by": "source",
            "expansion_stack_truncated": false
        });
        let encoded = serde_json::json!({
            "environment": "tabular",
            "rows": [{
                "cells": [{
                    "text": "Alpha",
                    "source": {
                        "primary": {
                            "kind": "file",
                            "path": "main.tex",
                            "start_utf8": 10,
                            "end_utf8": 15
                        },
                        "related": [],
                        "expansion_stack": [],
                        "generated_by": "source",
                        "expansion_stack_truncated": false
                    }
                }],
                "source": row_source
            }]
        });

        let event: TableEvent =
            serde_json::from_value(encoded.clone()).expect("decode table event");
        let reencoded = serde_json::to_value(event).expect("encode table event");

        assert_eq!(reencoded, encoded);
    }

    #[test]
    fn raw_fallback_envelope_defaults_to_fallback_metadata() {
        let envelope = RenderEventEnvelope::new(
            1,
            RenderEvent::RawFallback(RawFallbackEvent {
                source_excerpt: "\\begin{unknownenv}x\\end{unknownenv}".to_string(),
                expanded_text: None,
                normalized_visible_text: Some("x".to_string()),
                environment: Some("unknownenv".to_string()),
                reason: FallbackReason::UnsupportedEnvironment,
                source_hash: None,
                full_source_artifact: None,
                table_rules: Vec::new(),
                table_cell_spans: Vec::new(),
                table_columns: Vec::new(),
                table_width_spec: None,
                truncated: false,
            }),
            SourceProvenance::file("main.tex", 0, 35),
        );

        assert_eq!(envelope.meta.producer, EventProducer::Fallback);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Fallback);
        assert_eq!(envelope.meta.source.generated_by, GeneratedBy::Fallback);
    }

    #[test]
    fn diagnostic_envelope_defaults_to_low_confidence_unknown_producer() {
        let envelope = RenderEventEnvelope::new(
            1,
            RenderEvent::Diagnostic(RenderDiagnosticEvent {
                message: "missing input missing.tex".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 21),
        );

        assert_eq!(envelope.meta.producer, EventProducer::Unknown);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Low);
        assert_eq!(envelope.meta.source.generated_by, GeneratedBy::Source);
    }

    #[test]
    fn explicit_origin_constructor_uses_declared_producer_and_confidence() {
        let envelope = RenderEventEnvelope::with_origin(
            1,
            RenderEvent::Text(TextEvent {
                text: "Expanded".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 8),
            EventProducer::Macro,
            SemanticConfidence::Medium,
        );

        assert_eq!(envelope.meta.producer, EventProducer::Macro);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Medium);
        assert_eq!(envelope.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(envelope.meta.source.generated_by, GeneratedBy::Source);
    }

    #[test]
    fn envelope_builder_can_override_mode_hint_without_rebuilding_metadata() {
        let envelope = RenderEventEnvelope::new(
            1,
            RenderEvent::Text(TextEvent {
                text: "A Paper".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 15),
        )
        .with_mode_hint(ModeHint::Horizontal);

        assert_eq!(envelope.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(envelope.meta.producer, EventProducer::Command);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::High);
    }

    #[test]
    fn scanner_recovery_envelope_has_explicit_origin_and_confidence() {
        let envelope = RenderEventEnvelope::from_scanner_recovery(
            1,
            RenderEvent::Text(TextEvent {
                text: "Recovered".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 9),
        );

        assert_eq!(envelope.meta.producer, EventProducer::ScannerRecovery);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Medium);
    }

    #[test]
    fn scanner_recovery_raw_fallback_keeps_fallback_origin() {
        let envelope = RenderEventEnvelope::from_scanner_recovery(
            1,
            raw_fallback_event(),
            SourceProvenance::file("main.tex", 0, 35),
        );

        assert_eq!(envelope.meta.producer, EventProducer::Fallback);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Fallback);
        assert_eq!(envelope.meta.source.generated_by, GeneratedBy::Fallback);
    }

    #[test]
    fn scanner_recovery_diagnostic_keeps_unknown_low_origin() {
        let envelope = RenderEventEnvelope::from_scanner_recovery(
            1,
            diagnostic_event(),
            SourceProvenance::file("main.tex", 0, 21),
        );

        assert_eq!(envelope.meta.producer, EventProducer::Unknown);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Low);
        assert_eq!(envelope.meta.source.generated_by, GeneratedBy::Source);
    }

    #[test]
    fn typed_origin_constructor_enforces_event_kind_policy() {
        let cases = [
            (
                EventOrigin::primitive(),
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                EventOrigin::macro_expansion(),
                EventProducer::Macro,
                SemanticConfidence::High,
            ),
            (
                EventOrigin::scanner_recovery(RecoveryConfidence::Medium),
                EventProducer::ScannerRecovery,
                SemanticConfidence::Medium,
            ),
            (
                EventOrigin::scanner_recovery(RecoveryConfidence::Low),
                EventProducer::ScannerRecovery,
                SemanticConfidence::Low,
            ),
            (
                EventOrigin::lossy(),
                EventProducer::Fallback,
                SemanticConfidence::Low,
            ),
            (
                EventOrigin::unknown_low(),
                EventProducer::Unknown,
                SemanticConfidence::Low,
            ),
        ];
        for (origin, producer, confidence) in cases {
            let envelope = RenderEventEnvelope::try_from_origin(
                RenderEvent::Text(TextEvent {
                    text: "Visible".to_string(),
                }),
                EventBuildContext::new(1, SourceProvenance::file("main.tex", 0, 7)),
                origin,
            )
            .expect("ordinary event origin should be valid");

            assert_eq!(envelope.meta.producer, producer);
            assert_eq!(envelope.meta.confidence, confidence);
            assert_eq!(envelope.meta.mode_hint, ModeHint::Horizontal);
        }

        let raw_fallback = RenderEventEnvelope::try_from_origin(
            raw_fallback_event(),
            EventBuildContext::new(2, SourceProvenance::file("main.tex", 8, 43)),
            EventOrigin::raw_fallback(),
        )
        .expect("raw fallback origin should be valid");
        assert_eq!(raw_fallback.meta.producer, EventProducer::Fallback);
        assert_eq!(raw_fallback.meta.confidence, SemanticConfidence::Fallback);
        assert_eq!(raw_fallback.meta.source.generated_by, GeneratedBy::Fallback);

        let diagnostic = RenderEventEnvelope::try_from_origin(
            diagnostic_event(),
            EventBuildContext::new(3, SourceProvenance::file("main.tex", 44, 65)),
            EventOrigin::diagnostic_unknown(),
        )
        .expect("unknown diagnostic origin should be valid");
        assert_eq!(diagnostic.meta.producer, EventProducer::Unknown);
        assert_eq!(diagnostic.meta.confidence, SemanticConfidence::Low);

        let scanner_diagnostic = RenderEventEnvelope::try_from_origin(
            diagnostic_event(),
            EventBuildContext::new(4, SourceProvenance::file("main.tex", 66, 87)),
            EventOrigin::diagnostic_scanner_recovery(),
        )
        .expect("scanner diagnostic origin should be valid");
        assert_eq!(
            scanner_diagnostic.meta.producer,
            EventProducer::ScannerRecovery
        );
        assert_eq!(scanner_diagnostic.meta.confidence, SemanticConfidence::Low);

        assert!(
            RenderEventEnvelope::try_from_origin(
                raw_fallback_event(),
                EventBuildContext::new(5, SourceProvenance::file("main.tex", 88, 123)),
                EventOrigin::primitive(),
            )
            .is_err()
        );
        assert!(
            RenderEventEnvelope::try_from_origin(
                diagnostic_event(),
                EventBuildContext::new(6, SourceProvenance::file("main.tex", 124, 145)),
                EventOrigin::macro_expansion(),
            )
            .is_err()
        );
        assert!(
            RenderEventEnvelope::try_from_origin(
                RenderEvent::Text(TextEvent {
                    text: "Visible".to_string(),
                }),
                EventBuildContext::new(7, SourceProvenance::file("main.tex", 146, 153)),
                EventOrigin::raw_fallback(),
            )
            .is_err()
        );
    }

    #[test]
    fn envelope_new_applies_event_default_mode_hints() {
        let metadata = RenderEventEnvelope::new(
            1,
            RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                field: MetadataField::Title,
                value: "A Paper".to_string(),
            }),
            SourceProvenance::file("main.tex", 0, 15),
        );
        let flush_title = RenderEventEnvelope::new(
            2,
            RenderEvent::FlushTitleBlock(FlushTitleBlockEvent),
            SourceProvenance::file("main.tex", 30, 40),
        );
        let inline_math = RenderEventEnvelope::new(
            3,
            RenderEvent::InlineMath(MathSourceEvent {
                raw_source: "x^2".to_string(),
                normalized_text: None,
            }),
            SourceProvenance::file("main.tex", 50, 53),
        );
        let display_math = RenderEventEnvelope::new(
            4,
            RenderEvent::DisplayMath(MathSourceEvent {
                raw_source: "y^2".to_string(),
                normalized_text: None,
            }),
            SourceProvenance::file("main.tex", 60, 63),
        );
        let heading = RenderEventEnvelope::new(
            5,
            RenderEvent::Heading(HeadingEvent {
                level: 1,
                text: "Intro".to_string(),
                number: None,
            }),
            SourceProvenance::file("main.tex", 70, 75),
        );
        let citation = RenderEventEnvelope::new(
            6,
            RenderEvent::InlineCitation(InlineCitationEvent {
                keys: vec!["key".to_string()],
                command: "cite".to_string(),
                style_hint: CitationStyleHint::Parenthetical,
            }),
            SourceProvenance::file("main.tex", 80, 90),
        );
        let text = RenderEventEnvelope::new(
            7,
            RenderEvent::Text(TextEvent {
                text: "Hello".to_string(),
            }),
            SourceProvenance::file("main.tex", 100, 105),
        );
        let space = RenderEventEnvelope::new(
            8,
            RenderEvent::Space(SpaceEvent {
                kind: SpaceKind::Interword,
            }),
            SourceProvenance::file("main.tex", 105, 106),
        );
        let begin_block = RenderEventEnvelope::new(
            9,
            RenderEvent::BeginBlock(BeginBlockEvent {
                block: BlockKind::Abstract,
            }),
            SourceProvenance::file("main.tex", 110, 126),
        );
        let end_block = RenderEventEnvelope::new(
            10,
            RenderEvent::EndBlock(EndBlockEvent {
                block: BlockKind::Abstract,
            }),
            SourceProvenance::file("main.tex", 140, 154),
        );
        let reference = RenderEventEnvelope::new(
            11,
            RenderEvent::InlineReference(InlineReferenceEvent {
                keys: vec!["sec:intro".to_string()],
                command: "ref".to_string(),
            }),
            SourceProvenance::file("main.tex", 160, 175),
        );
        let link = RenderEventEnvelope::new(
            12,
            RenderEvent::InlineLink(InlineLinkEvent {
                target: "https://example.test".to_string(),
                text: "example".to_string(),
                command: "href".to_string(),
            }),
            SourceProvenance::file("main.tex", 180, 220),
        );
        let graphic = RenderEventEnvelope::new(
            13,
            RenderEvent::GraphicRef(GraphicRefEvent {
                path: "figures/plot.pdf".to_string(),
                options: Some("width=5cm".to_string()),
                page_selection: None,
                asset_format: Some(GraphicAssetFormat::Pdf),
                asset_hash: None,
                asset_dimensions: None,
            }),
            SourceProvenance::file("main.tex", 230, 278),
        );
        let caption = RenderEventEnvelope::new(
            14,
            RenderEvent::Caption(CaptionEvent::new("Plot caption.")),
            SourceProvenance::file("main.tex", 290, 303),
        );
        let bibliography_item = RenderEventEnvelope::new(
            15,
            RenderEvent::BibliographyItem(BibliographyItemEvent {
                key: "ref".to_string(),
                label_hint: None,
                text: "Author. Title.".to_string(),
            }),
            SourceProvenance::file("main.tex", 310, 340),
        );
        let line_break = RenderEventEnvelope::new(
            16,
            RenderEvent::LineBreak(LineBreakEvent {
                reason: LineBreakReason::Explicit,
            }),
            SourceProvenance::file("main.tex", 350, 352),
        );
        let paragraph_break = RenderEventEnvelope::new(
            17,
            RenderEvent::ParagraphBreak(ParagraphBreakEvent {
                reason: ParagraphBreakReason::ParCommand,
            }),
            SourceProvenance::file("main.tex", 360, 364),
        );
        let list_item = RenderEventEnvelope::new(
            18,
            RenderEvent::ListItem(ListItemEvent {
                marker: Some("Custom".to_string()),
            }),
            SourceProvenance::file("main.tex", 370, 383),
        );
        let label_definition = RenderEventEnvelope::new(
            19,
            RenderEvent::LabelDefinition(LabelDefinitionEvent {
                key: "sec:intro".to_string(),
                command: "label".to_string(),
            }),
            SourceProvenance::file("main.tex", 390, 408),
        );
        let raw_fallback = RenderEventEnvelope::new(
            20,
            RenderEvent::RawFallback(RawFallbackEvent {
                source_excerpt: "\\begin{unknownenv}x\\end{unknownenv}".to_string(),
                expanded_text: None,
                normalized_visible_text: Some("x".to_string()),
                environment: Some("unknownenv".to_string()),
                reason: FallbackReason::UnsupportedEnvironment,
                source_hash: None,
                full_source_artifact: None,
                table_rules: Vec::new(),
                table_cell_spans: Vec::new(),
                table_columns: Vec::new(),
                table_width_spec: None,
                truncated: false,
            }),
            SourceProvenance::file("main.tex", 420, 455),
        );
        let diagnostic = RenderEventEnvelope::new(
            21,
            RenderEvent::Diagnostic(RenderDiagnosticEvent {
                message: "missing input missing.tex".to_string(),
            }),
            SourceProvenance::file("main.tex", 460, 481),
        );

        assert_eq!(metadata.meta.mode_hint, ModeHint::Preamble);
        assert_eq!(flush_title.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(inline_math.meta.mode_hint, ModeHint::Math);
        assert_eq!(display_math.meta.mode_hint, ModeHint::Math);
        assert_eq!(heading.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(citation.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(text.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(space.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(begin_block.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(end_block.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(reference.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(link.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(graphic.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(caption.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(bibliography_item.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(line_break.meta.mode_hint, ModeHint::Horizontal);
        assert_eq!(paragraph_break.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(list_item.meta.mode_hint, ModeHint::Vertical);
        assert_eq!(label_definition.meta.mode_hint, ModeHint::Unknown);
        assert_eq!(raw_fallback.meta.mode_hint, ModeHint::Unknown);
        assert_eq!(diagnostic.meta.mode_hint, ModeHint::Unknown);
    }

    #[test]
    fn graphic_asset_format_is_derived_from_known_path_extensions() {
        assert_eq!(
            GraphicAssetFormat::from_path("figures/plot.PDF"),
            Some(GraphicAssetFormat::Pdf)
        );
        assert_eq!(
            GraphicAssetFormat::from_path("figures/plot.eps"),
            Some(GraphicAssetFormat::Eps)
        );
        assert_eq!(
            GraphicAssetFormat::from_path("figures/vector.svg"),
            Some(GraphicAssetFormat::Svg)
        );
        assert_eq!(
            GraphicAssetFormat::from_path("figures/photo.jpg"),
            Some(GraphicAssetFormat::Jpeg)
        );
        assert_eq!(GraphicAssetFormat::from_path("figures/plot"), None);
    }

    #[test]
    fn graphic_asset_format_is_detected_from_renderer_input_bytes() {
        assert_eq!(
            GraphicAssetFormat::from_bytes(b"\x89PNG\r\n\x1a\nrest"),
            Some(GraphicAssetFormat::Png)
        );
        assert_eq!(
            GraphicAssetFormat::from_bytes(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(GraphicAssetFormat::Jpeg)
        );
        assert_eq!(
            GraphicAssetFormat::from_bytes(b"%PDF-1.7\n\xff\x00"),
            Some(GraphicAssetFormat::Pdf)
        );
        assert_eq!(
            GraphicAssetFormat::from_bytes(b"%!PS-Adobe-3.0 EPSF-3.0\n"),
            Some(GraphicAssetFormat::Eps)
        );
        assert_eq!(
            GraphicAssetFormat::from_bytes(b"<?xml version=\"1.0\"?>\n<svg/>"),
            Some(GraphicAssetFormat::Svg)
        );
        assert_eq!(GraphicAssetFormat::from_bytes(b"not an asset"), None);
    }
}
