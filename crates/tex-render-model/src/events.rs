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
        Self {
            event,
            meta: EventMeta {
                event_id,
                source,
                mode_hint: ModeHint::Unknown,
                confidence,
                producer,
            },
        }
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
    #[serde(default)]
    pub truncated: bool,
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
        BeginBlockEvent, BlockKind, EndBlockEvent, EventMeta, EventProducer, FallbackReason,
        GeneratedBy, GraphicAssetFormat, MetadataField, ModeHint, RawFallbackEvent,
        RenderDiagnosticEvent, RenderEvent, RenderEventEnvelope, RenderEventStream,
        SemanticConfidence, SetDocumentMetadataEvent, SourceProvenance, SpaceEvent, SpaceKind,
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
