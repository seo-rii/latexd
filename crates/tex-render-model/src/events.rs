use serde::{Deserialize, Serialize};

use crate::{CitationStyleHint, GeneratedBy, SourceProvenance};

pub type EventId = u64;

pub const RENDER_EVENT_SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderEventEnvelope {
    pub event: RenderEvent,
    pub meta: EventMeta,
}

impl RenderEventEnvelope {
    pub fn new(event_id: EventId, event: RenderEvent, mut source: SourceProvenance) -> Self {
        let (confidence, producer) = match &event {
            RenderEvent::RawFallback(_) => {
                source = source.with_generated_by(GeneratedBy::Fallback);
                (SemanticConfidence::Fallback, EventProducer::Fallback)
            }
            RenderEvent::Diagnostic(_) => (SemanticConfidence::Low, EventProducer::Unknown),
            _ => (SemanticConfidence::High, EventProducer::Command),
        };
        let mode_hint = event.default_mode_hint();
        Self {
            event,
            meta: EventMeta {
                event_id,
                source,
                mode_hint,
                confidence,
                producer,
            },
        }
    }

    pub fn with_mode_hint(mut self, mode_hint: ModeHint) -> Self {
        self.meta.mode_hint = mode_hint;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    pub event_id: EventId,
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
    SetDocumentMetadata(SetDocumentMetadataEvent),
    FlushTitleBlock(FlushTitleBlockEvent),
    BeginBlock(BeginBlockEvent),
    EndBlock(EndBlockEvent),
    Heading(HeadingEvent),
    InlineCitation(InlineCitationEvent),
    InlineReference(InlineReferenceEvent),
    InlineLink(InlineLinkEvent),
    LabelDefinition(LabelDefinitionEvent),
    ListItem(ListItemEvent),
    BibliographyItem(BibliographyItemEvent),
    GraphicRef(GraphicRefEvent),
    Caption(CaptionEvent),
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
            Self::SetDocumentMetadata(_) => ModeHint::Preamble,
            Self::FlushTitleBlock(_) => ModeHint::Vertical,
            Self::BeginBlock(_) | Self::EndBlock(_) => ModeHint::Vertical,
            Self::Heading(_) => ModeHint::Vertical,
            Self::ListItem(_) => ModeHint::Vertical,
            Self::InlineCitation(_) => ModeHint::Horizontal,
            Self::BibliographyItem(_) => ModeHint::Vertical,
            Self::InlineReference(_) | Self::InlineLink(_) => ModeHint::Horizontal,
            Self::GraphicRef(_) | Self::Caption(_) => ModeHint::Vertical,
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
pub struct SetDocumentMetadataEvent {
    pub field: MetadataField,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataField {
    Title,
    Author,
    Date,
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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockKind {
    Abstract,
    Bibliography,
    Figure,
    Table,
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
    pub asset_format: Option<GraphicAssetFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_dimensions: Option<GraphicAssetDimensions>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionEvent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathSourceEvent {
    pub raw_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCellSpanEvent {
    pub row_index: usize,
    pub column_index: usize,
    pub column_span: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_span: Option<usize>,
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
    Paragraph,
    Unknown,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
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
        EndBlockEvent, EventMeta, EventProducer, FallbackReason, FlushTitleBlockEvent, GeneratedBy,
        GraphicAssetFormat, GraphicRefEvent, HeadingEvent, InlineCitationEvent, InlineLinkEvent,
        InlineReferenceEvent, LabelDefinitionEvent, LineBreakEvent, LineBreakReason, ListItemEvent,
        MathSourceEvent, MetadataField, ModeHint, ParagraphBreakEvent, ParagraphBreakReason,
        RawFallbackEvent, RenderDiagnosticEvent, RenderEvent, RenderEventEnvelope,
        RenderEventStream, SemanticConfidence, SetDocumentMetadataEvent, SourceProvenance,
        SpaceEvent, SpaceKind, TextEvent,
    };

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
                    event_id: 1,
                    source: SourceProvenance::file("main.tex", 0, 10),
                    mode_hint: ModeHint::Preamble,
                    confidence: SemanticConfidence::High,
                    producer: EventProducer::Command,
                },
            }],
        );
        let encoded = serde_json::to_string_pretty(&stream).expect("encode stream");

        assert!(encoded.contains("\"schema_version\": 1"));
        assert!(!encoded.contains("\"event_id\": 0"));
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
                asset_format: Some(GraphicAssetFormat::Pdf),
                asset_hash: None,
                asset_dimensions: None,
            }),
            SourceProvenance::file("main.tex", 230, 278),
        );
        let caption = RenderEventEnvelope::new(
            14,
            RenderEvent::Caption(CaptionEvent {
                text: "Plot caption.".to_string(),
            }),
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
}
