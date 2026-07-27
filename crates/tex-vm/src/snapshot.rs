use std::collections::{BTreeMap, BTreeSet, HashMap};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tex_lexer::{Mouth, MouthSnapshot};
use tex_render_model::{
    CaptionInlinePlaceholderEvent, CaptionKind, EventId, EventProducer, ListKind,
    RenderEventEnvelope, SourceProvenance, TableCellEvent, TableColumnSpec, TableRowEvent,
};
use tex_tokens::CatCode;

use crate::{diagnostic::VmDiagnostic, outcome::VmModuleTrace};

pub const VM_CONTINUATION_SAFETY_SCHEMA_VERSION: u32 = 2;
pub const VM_SEMANTIC_CAPTURE_SCHEMA_VERSION: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmReplayFrame {
    pub path: Utf8PathBuf,
    pub source_offset_utf8: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmModuleCheckpointKind {
    #[default]
    Enter,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmModuleCheckpoint {
    pub kind: VmModuleCheckpointKind,
    pub module_path: Utf8PathBuf,
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
    pub output_start_utf8: u32,
    pub snapshot: VmSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmContinuationBlocker {
    UnverifiedSnapshot,
    OpenGroup,
    OpenConditional,
    ActiveInput,
    PendingGlobalPrefix,
    RenderEventSink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmContinuationSafety {
    pub schema_version: u32,
    pub blockers: Vec<VmContinuationBlocker>,
}

impl VmContinuationSafety {
    pub fn is_safe(&self) -> bool {
        self.schema_version == VM_CONTINUATION_SAFETY_SCHEMA_VERSION && self.blockers.is_empty()
    }
}

impl Default for VmContinuationSafety {
    fn default() -> Self {
        Self {
            schema_version: VM_CONTINUATION_SAFETY_SCHEMA_VERSION,
            blockers: vec![VmContinuationBlocker::UnverifiedSnapshot],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticSinkSnapshot {
    #[serde(default)]
    pub events: Vec<RenderEventEnvelope>,
    #[serde(default = "default_next_render_event_id")]
    pub next_event_id: EventId,
    #[serde(default = "default_next_render_event_id")]
    pub batch_start_event_id: EventId,
    #[serde(default)]
    pub epoch: u64,
}

impl VmSemanticSinkSnapshot {
    pub fn is_restorable(&self) -> bool {
        let mut event_ids = BTreeSet::new();
        self.next_event_id >= 1
            && self.batch_start_event_id >= 1
            && self.batch_start_event_id <= self.next_event_id
            && self.events.iter().all(|event| {
                event.meta.event_id >= self.batch_start_event_id
                    && event.meta.event_id < self.next_event_id
                    && event_ids.insert(event.meta.event_id)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticCaptureSnapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub source_buffers: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub execution_in_document: bool,
    #[serde(default)]
    pub execution_no_hyper_depth: u32,
    #[serde(default)]
    pub math: VmSemanticMathSnapshot,
    #[serde(default)]
    pub text: VmSemanticTextSnapshot,
    #[serde(default)]
    pub graphic: VmSemanticGraphicSnapshot,
    #[serde(default)]
    pub list: VmSemanticListSnapshot,
    #[serde(default)]
    pub environment: VmSemanticEnvironmentSnapshot,
    #[serde(default)]
    pub table: VmSemanticTableSnapshot,
    #[serde(default)]
    pub inline: VmSemanticInlineSnapshot,
    #[serde(default)]
    pub heading: VmSemanticHeadingSnapshot,
    #[serde(default)]
    pub caption: VmSemanticCaptionSnapshot,
}

impl VmSemanticCaptureSnapshot {
    pub fn is_restorable(&self) -> bool {
        let active_math_source_is_valid = self.math.active_capture.as_ref().is_none_or(|capture| {
            let Some(source) = self.source_buffers.get(&capture.source_path) else {
                return false;
            };
            let invocation_start = capture.invocation_start_utf8 as usize;
            let content_start = capture.content_start_utf8 as usize;
            self.execution_in_document
                && invocation_start <= content_start
                && content_start <= source.len()
                && source.is_char_boundary(invocation_start)
                && source.is_char_boundary(content_start)
        });
        let active_text_source_is_valid = self.text.active_capture.as_ref().is_none_or(|capture| {
            let Some(path) = &capture.literal_path else {
                return true;
            };
            let Some(source) = self.source_buffers.get(path) else {
                return false;
            };
            let end_utf8 = capture.end_utf8 as usize;
            end_utf8 <= source.len() && source.is_char_boundary(end_utf8)
        });
        self.schema_version == VM_SEMANTIC_CAPTURE_SCHEMA_VERSION
            && self.math.is_restorable()
            && self.text.is_restorable()
            && self.graphic.is_restorable()
            && self.list.is_restorable()
            && self.environment.is_restorable()
            && self.table.is_restorable()
            && self.inline.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
            )
            && self.heading.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && self.caption.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && active_math_source_is_valid
            && active_text_source_is_valid
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticMathSnapshot {
    #[serde(default)]
    pub scanner_dollar_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_invocations: Vec<VmSemanticMathInvocationSnapshot>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_capture: Option<VmExecutedMathCaptureSnapshot>,
}

impl VmSemanticMathSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        let scanner_event_ids = self
            .scanner_dollar_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        values_are_unique_nonzero(&self.scanner_dollar_event_ids)
            && values_are_unique(&self.executed_invocations)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && self
                .active_capture
                .as_ref()
                .is_none_or(VmExecutedMathCaptureSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VmSemanticMathInvocationSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedMathCaptureSnapshot {
    pub display: bool,
    pub raw_source: String,
    pub source_path: Utf8PathBuf,
    pub invocation_start_utf8: u32,
    pub content_start_utf8: u32,
}

impl VmExecutedMathCaptureSnapshot {
    fn is_restorable(&self) -> bool {
        self.invocation_start_utf8 <= self.content_start_utf8
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticTextSnapshot {
    #[serde(default)]
    pub scanner_slots: Vec<VmScannerTextSlotSnapshot>,
    #[serde(default)]
    pub suppressed_ranges: Vec<VmSuppressedSourceRangeSnapshot>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_capture: Option<VmExecutedTextCaptureSnapshot>,
    #[serde(default)]
    pub paragraph_has_content: bool,
    #[serde(default)]
    pub space_run_active: bool,
    #[serde(default)]
    pub marker_actions: Vec<VmExpansionMarkerSnapshot>,
    #[serde(default)]
    pub expansion_stack: Vec<VmExpansionContextSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticTextSnapshot {
    pub fn is_restorable(&self) -> bool {
        let scanner_event_ids = self
            .scanner_slots
            .iter()
            .flat_map(|slot| slot.event_ids.iter().copied())
            .collect::<Vec<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        let marker_names = self
            .marker_actions
            .iter()
            .map(|marker| marker.control_sequence.clone())
            .collect::<Vec<_>>();
        let expansion_ids = self
            .expansion_stack
            .iter()
            .map(|context| context.marker_id)
            .collect::<Vec<_>>();
        let expansion_id_set = expansion_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut action_state = BTreeMap::<u64, (bool, bool)>::new();
        for marker in &self.marker_actions {
            let marker_id = marker.action.marker_id();
            if marker_id >= self.next_marker_id {
                return false;
            }
            let state = action_state.entry(marker_id).or_default();
            match &marker.action {
                VmExpansionMarkerActionSnapshot::Begin { .. } if state.0 => return false,
                VmExpansionMarkerActionSnapshot::Begin { .. } => state.0 = true,
                VmExpansionMarkerActionSnapshot::End { .. } if state.1 => return false,
                VmExpansionMarkerActionSnapshot::End { .. } => state.1 = true,
            }
        }
        let marker_state_is_valid = action_state
            .iter()
            .all(|(marker_id, (has_begin, has_end))| {
                if expansion_id_set.contains(marker_id) {
                    !has_begin && *has_end
                } else {
                    *has_begin && *has_end
                }
            })
            && expansion_ids
                .iter()
                .all(|marker_id| action_state.contains_key(marker_id));
        marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&expansion_ids)
            && expansion_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && marker_state_is_valid
            && self
                .scanner_slots
                .iter()
                .all(VmScannerTextSlotSnapshot::is_restorable)
            && values_are_unique_nonzero(&scanner_event_ids)
            && self
                .suppressed_ranges
                .iter()
                .all(VmSuppressedSourceRangeSnapshot::is_restorable)
            && values_are_unique_nonzero(&executed_event_ids)
            && self
                .active_capture
                .as_ref()
                .is_none_or(VmExecutedTextCaptureSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExpansionMarkerSnapshot {
    pub control_sequence: String,
    pub action: VmExpansionMarkerActionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmExpansionMarkerActionSnapshot {
    Begin { context: VmExpansionContextSnapshot },
    End { marker_id: u64 },
}

impl VmExpansionMarkerActionSnapshot {
    fn marker_id(&self) -> u64 {
        match self {
            Self::Begin { context } => context.marker_id,
            Self::End { marker_id } => *marker_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExpansionContextSnapshot {
    pub marker_id: u64,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticInlineSnapshot {
    #[serde(default)]
    pub scanner_citation_event_ids: Vec<EventId>,
    #[serde(default)]
    pub scanner_reference_event_ids: Vec<EventId>,
    #[serde(default)]
    pub scanner_link_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_citations: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_references: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_links: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub caption_placeholders: Vec<CaptionInlinePlaceholderEvent>,
    #[serde(default)]
    pub active_link_actions: Vec<VmActiveLinkCaptureSnapshot>,
    #[serde(default)]
    pub next_link_marker_id: u64,
}

impl VmSemanticInlineSnapshot {
    pub fn is_restorable(&self, text_event_count: usize, math_event_count: usize) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let citation_event_count = u64::try_from(self.executed_citations.len()).unwrap_or(u64::MAX);
        let reference_event_count =
            u64::try_from(self.executed_references.len()).unwrap_or(u64::MAX);
        let link_event_count = u64::try_from(self.executed_links.len()).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_citation_event_ids
            .iter()
            .chain(&self.scanner_reference_event_ids)
            .chain(&self.scanner_link_event_ids)
            .copied()
            .collect::<Vec<_>>();
        let executed_event_ids = self
            .executed_citations
            .iter()
            .chain(&self.executed_references)
            .chain(&self.executed_links)
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        let executed_event_id_set = executed_event_ids.iter().copied().collect::<BTreeSet<_>>();
        let marker_names = self
            .active_link_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_link_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && scanner_event_ids
                .iter()
                .all(|event_id| !executed_event_id_set.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_link_marker_id)
            && self.active_link_actions.iter().all(|capture| {
                capture.text_event_mark <= text_event_count
                    && capture.citation_event_mark <= citation_event_count
                    && capture.reference_event_mark <= reference_event_count
                    && capture.link_event_mark <= link_event_count
                    && capture.math_event_mark <= math_event_count
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveLinkCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub command: String,
    pub target: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub text_event_mark: u64,
    pub citation_event_mark: u64,
    pub reference_event_mark: u64,
    pub link_event_mark: u64,
    pub math_event_mark: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedInlineEventMarkSnapshot {
    pub citations: u64,
    pub references: u64,
    pub links: u64,
    pub caption_placeholders: u64,
}

impl VmExecutedInlineEventMarkSnapshot {
    fn is_restorable(&self, inline: &VmSemanticInlineSnapshot) -> bool {
        self.citations <= u64::try_from(inline.executed_citations.len()).unwrap_or(u64::MAX)
            && self.references
                <= u64::try_from(inline.executed_references.len()).unwrap_or(u64::MAX)
            && self.links <= u64::try_from(inline.executed_links.len()).unwrap_or(u64::MAX)
            && self.caption_placeholders
                <= u64::try_from(inline.caption_placeholders.len()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticHeadingSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_heading_actions: Vec<VmActiveHeadingCaptureSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticHeadingSnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        let marker_names = self
            .active_heading_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_heading_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && self.active_heading_actions.iter().all(|capture| {
                capture.text_event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.heading_event_mark
                        <= u64::try_from(self.executed_events.len()).unwrap_or(u64::MAX)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveHeadingCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub level: u8,
    pub numbered: bool,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub lossy_before_restore: bool,
    pub text_event_mark: u64,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub heading_event_mark: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticCaptionSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_caption_actions: Vec<VmActiveCaptionCaptureSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticCaptionSnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        let marker_names = self
            .active_caption_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_caption_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && self.active_caption_actions.iter().all(|capture| {
                capture.text_event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.caption_event_mark
                        <= u64::try_from(self.executed_events.len()).unwrap_or(u64::MAX)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveCaptionCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub numbered: bool,
    pub caption_kind: Option<CaptionKind>,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub lossy_before_restore: bool,
    pub text_event_mark: u64,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub caption_event_mark: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmScannerTextSlotSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub event_ids: Vec<EventId>,
}

impl VmScannerTextSlotSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8 && values_are_unique_nonzero(&self.event_ids)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSuppressedSourceRangeSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

impl VmSuppressedSourceRangeSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTextCaptureSnapshot {
    pub text: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub literal_path: Option<Utf8PathBuf>,
    pub end_utf8: u32,
}

impl VmExecutedTextCaptureSnapshot {
    fn is_restorable(&self) -> bool {
        self.literal_path.is_none()
            || matches!(&self.source.primary, tex_render_model::ProvenanceSpan::File(span)
                if Some(&span.path) == self.literal_path.as_ref()
                    && span.start_utf8 <= span.end_utf8
                    && span.end_utf8 == self.end_utf8)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticGraphicSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub overridden_invocations: Vec<VmGraphicInvocationRangeSnapshot>,
}

impl VmSemanticGraphicSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && self
                .overridden_invocations
                .iter()
                .all(VmGraphicInvocationRangeSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmGraphicInvocationRangeSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

impl VmGraphicInvocationRangeSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticListSnapshot {
    #[serde(default)]
    pub scanner_item_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_items: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_lists: Vec<ListKind>,
}

impl VmSemanticListSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_items
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_item_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticEnvironmentSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventId>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
}

impl VmSemanticEnvironmentSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticTableSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventId>,
    #[serde(default)]
    pub open_tables: Vec<VmExecutedTableFrameSnapshot>,
    #[serde(default)]
    pub executed_tables: Vec<VmExecutedTableSnapshot>,
    #[serde(default)]
    pub structured_events: bool,
}

impl VmSemanticTableSnapshot {
    pub fn is_restorable(&self) -> bool {
        let native_event_ids = self
            .executed_tables
            .iter()
            .filter_map(|table| table.native_event.as_ref())
            .map(|event| event.meta.event_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&native_event_ids)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTableFrameSnapshot {
    pub environment: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub width_spec: Option<String>,
    pub columns: Vec<TableColumnSpec>,
    pub rows: Vec<TableRowEvent>,
    pub current_cells: Vec<TableCellEvent>,
    pub current_text: String,
    pub current_source: Option<SourceProvenance>,
    pub row_source: Option<SourceProvenance>,
    pub row_started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTableSnapshot {
    pub environment: String,
    pub source: SourceProvenance,
    pub native_event: Option<RenderEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSnapshot {
    #[serde(default)]
    pub continuation_safety: VmContinuationSafety,
    #[serde(default)]
    pub input_continuation: Option<VmInputContinuationSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_sink: Option<VmSemanticSinkSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_capture: Option<VmSemanticCaptureSnapshot>,
    #[serde(default)]
    pub diagnostics: Vec<VmDiagnostic>,
    #[serde(default)]
    pub transcript: Vec<String>,
    #[serde(default)]
    pub module_traces: Vec<VmModuleTrace>,
    pub scopes: Vec<HashMap<String, SnapshotMeaning>>,
    pub registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub dimen_registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub skip_registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub token_registers: BTreeMap<u32, Vec<SnapshotToken>>,
    #[serde(default)]
    pub catcodes: BTreeMap<char, CatCode>,
    #[serde(default = "default_next_count_register")]
    pub next_count_register: u32,
    #[serde(default = "default_next_dimen_register")]
    pub next_dimen_register: u32,
    #[serde(default = "default_next_skip_register")]
    pub next_skip_register: u32,
    #[serde(default = "default_next_toks_register")]
    pub next_toks_register: u32,
    #[serde(default = "default_next_read_stream")]
    pub next_read_stream: u32,
    #[serde(default = "default_next_write_stream")]
    pub next_write_stream: u32,
    pub loaded_modules: Vec<Utf8PathBuf>,
    pub include_only: Option<Vec<Utf8PathBuf>>,
    #[serde(default)]
    pub aftergroup_tokens: Vec<Vec<SnapshotToken>>,
    #[serde(default)]
    pub after_assignment_token: Option<SnapshotToken>,
    #[serde(default)]
    pub at_end_document_hooks: Vec<Vec<SnapshotToken>>,
    #[serde(default)]
    pub tempswa: bool,
    #[serde(default = "default_filesw")]
    pub filesw: bool,
    #[serde(default)]
    pub in_at: bool,
    #[serde(default)]
    pub negate_next_conditional: bool,
    #[serde(default)]
    pub provided_files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub provided_packages: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub provided_classes: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub loaded_package_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub loaded_class_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub pending_package_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub pending_class_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub graphic_paths: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub graphic_extensions: Vec<String>,
    #[serde(default)]
    pub graphic_default_options: Option<String>,
    #[serde(default)]
    pub epsf_pending_options: Option<String>,
    #[serde(default)]
    pub counter_resets: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub read_stream_lines: BTreeMap<u32, Vec<String>>,
    #[serde(default)]
    pub read_stream_eof: BTreeMap<u32, bool>,
    #[serde(default)]
    pub legacy_math_output_active: bool,
    #[serde(default)]
    pub legacy_math_pending_word_boundary: bool,
    #[serde(default)]
    pub legacy_math_text_wrapper_restore_scope_depth: Option<usize>,
    #[serde(default)]
    pub legacy_math_script_boundary_scope_depths: Vec<usize>,
    #[serde(default)]
    pub legacy_output_last_char: Option<char>,
    #[serde(default)]
    pub legacy_text_script_boundary_pending: bool,
}

fn default_next_render_event_id() -> EventId {
    1
}

fn values_are_unique_nonzero(values: &[EventId]) -> bool {
    values.iter().all(|value| *value >= 1) && values_are_unique(values)
}

fn values_are_unique<T: Ord + Clone>(values: &[T]) -> bool {
    values.iter().cloned().collect::<BTreeSet<_>>().len() == values.len()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmInputContinuationSnapshot {
    pub queue: Vec<VmQueueItemSnapshot>,
    pub source_stack: Vec<VmActiveSourceFrameSnapshot>,
    pub last_token_end_utf8: u32,
}

impl VmInputContinuationSnapshot {
    pub fn is_restorable(&self) -> bool {
        !self.source_stack.is_empty()
            && self
                .queue
                .iter()
                .filter(|item| matches!(item, VmQueueItemSnapshot::CharacterSource { .. }))
                .count()
                == self.source_stack.len()
            && self.queue.iter().all(|item| match item {
                VmQueueItemSnapshot::Token { token } => token.start_utf8 <= token.end_utf8,
                VmQueueItemSnapshot::CharacterSource { mouth } => Mouth::restore(mouth).is_some(),
                VmQueueItemSnapshot::ModuleEnd {
                    source_start_utf8,
                    source_end_utf8,
                    ..
                } => source_start_utf8 <= source_end_utf8,
            })
    }

    pub fn matches_character_sources<'a>(
        &self,
        sources: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        let mut sources = sources.into_iter();
        for mouth in self.queue.iter().filter_map(|item| match item {
            VmQueueItemSnapshot::CharacterSource { mouth } => Some(mouth),
            VmQueueItemSnapshot::Token { .. } | VmQueueItemSnapshot::ModuleEnd { .. } => None,
        }) {
            if sources.next() != Some(mouth.input()) {
                return false;
            }
        }
        sources.next().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmQueueItemSnapshot {
    Token {
        token: SnapshotToken,
    },
    CharacterSource {
        mouth: MouthSnapshot,
    },
    ModuleEnd {
        path: Utf8PathBuf,
        source_start_utf8: u32,
        source_end_utf8: u32,
        output_start_utf8: u32,
        checkpoint: Option<VmPendingModuleCheckpointSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmPendingModuleCheckpointSnapshot {
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveSourceFrameSnapshot {
    pub path: Utf8PathBuf,
    pub return_to_parent: Option<VmReplayFrame>,
    pub global_definition_base_scope: Option<usize>,
    pub module_kind: Option<VmActiveModuleKindSnapshot>,
    pub catcode_overrides: BTreeMap<char, CatCode>,
    pub suppressed_catcode_overrides: BTreeMap<char, usize>,
    pub end_hooks: Vec<Vec<SnapshotToken>>,
    pub module_options: Option<VmActiveModuleOptionsSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmActiveModuleKindSnapshot {
    Package,
    Class,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveModuleOptionsSnapshot {
    pub default_options: Vec<String>,
    pub passed_options: Vec<String>,
    pub forwarded_options: Vec<String>,
    pub declared_options: BTreeMap<String, Vec<SnapshotToken>>,
    pub default_option_body: Option<Vec<SnapshotToken>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotMeaning {
    Macro {
        parameter_count: u8,
        #[serde(default)]
        optional_first_argument_default: Option<Vec<SnapshotToken>>,
        body: Vec<SnapshotToken>,
    },
    Primitive {
        name: String,
    },
    Token {
        token: SnapshotToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotToken {
    pub kind: SnapshotTokenKind,
    #[serde(default)]
    pub start_utf8: u32,
    #[serde(default)]
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotTokenKind {
    ControlSequence { name: String },
    Character { ch: char, catcode: CatCode },
}

pub(crate) fn default_next_count_register() -> u32 {
    256
}

pub(crate) fn default_next_dimen_register() -> u32 {
    256
}

pub(crate) fn default_next_skip_register() -> u32 {
    256
}

pub(crate) fn default_next_toks_register() -> u32 {
    0
}

pub(crate) fn default_next_read_stream() -> u32 {
    0
}

pub(crate) fn default_next_write_stream() -> u32 {
    16
}

fn default_filesw() -> bool {
    true
}
