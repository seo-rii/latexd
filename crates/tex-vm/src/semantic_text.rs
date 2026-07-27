use std::{
    collections::{HashMap, HashSet, VecDeque},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    EventId, EventProducer, ExpansionFrame, ParagraphBreakEvent, ParagraphBreakReason,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpan, SpaceEvent,
    SpaceKind, TextEvent,
};
use tex_tokens::{ControlSequenceId, Token};

use crate::{
    Vm,
    input::QueueItem,
    snapshot::{
        VmExecutedTextCaptureSnapshot, VmExpansionContextSnapshot, VmExpansionMarkerActionSnapshot,
        VmExpansionMarkerSnapshot, VmScannerTextSlotSnapshot, VmSemanticTextSnapshot,
        VmSuppressedSourceRangeSnapshot,
    },
};

#[derive(Debug, Default)]
pub(super) struct SemanticTextState {
    scanner_slots: Vec<ScannerTextSlot>,
    suppressed_ranges: Vec<SuppressedSourceRange>,
    executed_events: Vec<RenderEventEnvelope>,
    capture: Option<ExecutedTextCapture>,
    paragraph_has_content: bool,
    space_run_active: bool,
    marker_actions: HashMap<ControlSequenceId, ExpansionMarkerAction>,
    expansion_stack: Vec<ExpansionContext>,
    next_marker_id: u64,
}

#[derive(Debug)]
struct ScannerTextSlot {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
    event_ids: Vec<EventId>,
    preserve_leading_space: bool,
}

#[derive(Debug, Clone)]
struct SuppressedSourceRange {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
}

#[derive(Debug)]
struct ExecutedTextCapture {
    text: String,
    source: SourceProvenance,
    producer: EventProducer,
    literal_path: Option<Utf8PathBuf>,
    end_utf8: u32,
}

#[derive(Debug, Clone)]
struct ExpansionContext {
    id: u64,
    source: SourceProvenance,
}

#[derive(Debug)]
enum ExpansionMarkerAction {
    Begin(ExpansionContext),
    End(u64),
}

impl Vm<'_> {
    pub(super) fn semantic_text_snapshot(&self) -> VmSemanticTextSnapshot {
        let mut marker_actions = self
            .semantic_text
            .marker_actions
            .iter()
            .map(|(name, action)| VmExpansionMarkerSnapshot {
                control_sequence: self.interner.resolve(*name).unwrap_or("").to_string(),
                action: match action {
                    ExpansionMarkerAction::Begin(context) => {
                        VmExpansionMarkerActionSnapshot::Begin {
                            context: VmExpansionContextSnapshot {
                                marker_id: context.id,
                                source: context.source.clone(),
                            },
                        }
                    }
                    ExpansionMarkerAction::End(marker_id) => VmExpansionMarkerActionSnapshot::End {
                        marker_id: *marker_id,
                    },
                },
            })
            .collect::<Vec<_>>();
        marker_actions.sort_by(|left, right| left.control_sequence.cmp(&right.control_sequence));
        VmSemanticTextSnapshot {
            scanner_slots: self
                .semantic_text
                .scanner_slots
                .iter()
                .map(|slot| VmScannerTextSlotSnapshot {
                    path: slot.path.clone(),
                    start_utf8: slot.start_utf8,
                    end_utf8: slot.end_utf8,
                    event_ids: slot.event_ids.clone(),
                    preserve_leading_space: slot.preserve_leading_space,
                })
                .collect(),
            suppressed_ranges: self
                .semantic_text
                .suppressed_ranges
                .iter()
                .map(|range| VmSuppressedSourceRangeSnapshot {
                    path: range.path.clone(),
                    start_utf8: range.start_utf8,
                    end_utf8: range.end_utf8,
                })
                .collect(),
            executed_events: self.semantic_text.executed_events.clone(),
            active_capture: self.semantic_text.capture.as_ref().map(|capture| {
                VmExecutedTextCaptureSnapshot {
                    text: capture.text.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    literal_path: capture.literal_path.clone(),
                    end_utf8: capture.end_utf8,
                }
            }),
            paragraph_has_content: self.semantic_text.paragraph_has_content,
            space_run_active: self.semantic_text.space_run_active,
            marker_actions,
            expansion_stack: self
                .semantic_text
                .expansion_stack
                .iter()
                .map(|context| VmExpansionContextSnapshot {
                    marker_id: context.id,
                    source: context.source.clone(),
                })
                .collect(),
            next_marker_id: self.semantic_text.next_marker_id,
        }
    }

    pub(super) fn restore_semantic_text_snapshot(&mut self, snapshot: &VmSemanticTextSnapshot) {
        self.semantic_text.scanner_slots = snapshot
            .scanner_slots
            .iter()
            .map(|slot| ScannerTextSlot {
                path: slot.path.clone(),
                start_utf8: slot.start_utf8,
                end_utf8: slot.end_utf8,
                event_ids: slot.event_ids.clone(),
                preserve_leading_space: slot.preserve_leading_space,
            })
            .collect();
        self.semantic_text.suppressed_ranges = snapshot
            .suppressed_ranges
            .iter()
            .map(|range| SuppressedSourceRange {
                path: range.path.clone(),
                start_utf8: range.start_utf8,
                end_utf8: range.end_utf8,
            })
            .collect();
        self.semantic_text.executed_events = snapshot.executed_events.clone();
        self.semantic_text.capture =
            snapshot
                .active_capture
                .as_ref()
                .map(|capture| ExecutedTextCapture {
                    text: capture.text.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    literal_path: capture.literal_path.clone(),
                    end_utf8: capture.end_utf8,
                });
        self.semantic_text.paragraph_has_content = snapshot.paragraph_has_content;
        self.semantic_text.space_run_active = snapshot.space_run_active;
        self.semantic_text.marker_actions.clear();
        for marker in &snapshot.marker_actions {
            let name = self.interner.intern(&marker.control_sequence);
            let action = match &marker.action {
                VmExpansionMarkerActionSnapshot::Begin { context } => {
                    ExpansionMarkerAction::Begin(ExpansionContext {
                        id: context.marker_id,
                        source: context.source.clone(),
                    })
                }
                VmExpansionMarkerActionSnapshot::End { marker_id } => {
                    ExpansionMarkerAction::End(*marker_id)
                }
            };
            self.semantic_text.marker_actions.insert(name, action);
        }
        self.semantic_text.expansion_stack = snapshot
            .expansion_stack
            .iter()
            .map(|context| ExpansionContext {
                id: context.marker_id,
                source: context.source.clone(),
            })
            .collect();
        self.semantic_text.next_marker_id = snapshot.next_marker_id;
    }

    pub(super) fn record_scanner_text_slot(
        &mut self,
        path: &Utf8Path,
        start_utf8: u32,
        end_utf8: u32,
        first_event_id: EventId,
    ) {
        let event_ids = (first_event_id..self.render_events.next_event_id()).collect::<Vec<_>>();
        if event_ids.is_empty() {
            return;
        }
        self.semantic_text.scanner_slots.push(ScannerTextSlot {
            path: path.to_owned(),
            start_utf8,
            end_utf8,
            event_ids,
            preserve_leading_space: start_utf8 == 0 && self.semantic_text.paragraph_has_content,
        });
    }

    pub(super) fn record_scanner_par_event(
        &mut self,
        path: &Utf8Path,
        start_utf8: u32,
        end_utf8: u32,
        event_id: EventId,
    ) {
        self.semantic_text.scanner_slots.push(ScannerTextSlot {
            path: path.to_owned(),
            start_utf8,
            end_utf8,
            event_ids: vec![event_id],
            preserve_leading_space: false,
        });
    }

    pub(super) fn record_suppressed_source_range(&mut self, start_utf8: u32, end_utf8: u32) {
        if !self.render_event_capture || end_utf8 <= start_utf8 {
            return;
        }
        let expansion_call_range =
            self.semantic_text
                .expansion_stack
                .last()
                .and_then(|expansion| match &expansion.source.primary {
                    ProvenanceSpan::File(span) => {
                        Some((span.path.clone(), span.start_utf8, span.end_utf8))
                    }
                    ProvenanceSpan::Generated(_) => None,
                });
        self.record_suppressed_source_range_for_path(
            self.current_execution_source_path(),
            start_utf8,
            end_utf8,
        );
        if let Some((path, start_utf8, end_utf8)) = expansion_call_range {
            self.record_suppressed_source_range_for_path(path, start_utf8, end_utf8);
        }
    }

    pub(super) fn record_suppressed_source_range_for_path(
        &mut self,
        path: Utf8PathBuf,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture || end_utf8 <= start_utf8 {
            return;
        }
        self.semantic_text
            .suppressed_ranges
            .push(SuppressedSourceRange {
                path,
                start_utf8,
                end_utf8,
            });
    }

    pub(super) fn queue_macro_expansion(
        &mut self,
        command_name: &str,
        call_start_utf8: u32,
        call_end_utf8: u32,
        expanded: Vec<Token>,
        queue: &mut VecDeque<QueueItem>,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            for token in expanded.into_iter().rev() {
                self.push_token_front(queue, token);
            }
            return;
        }

        self.separate_executed_inline_content();
        let path = self.current_execution_source_path();
        let call_span = ProvenanceSpan::File(SourceSpan {
            path: path.clone(),
            start_utf8: call_start_utf8,
            end_utf8: call_end_utf8,
        });
        let expansion_frame = ExpansionFrame {
            call_span,
            definition_span: None,
            command_name: Some(command_name.to_string()),
        };
        let source = self
            .semantic_text
            .expansion_stack
            .last()
            .map(|context| context.source.clone())
            .unwrap_or_else(|| SourceProvenance::file(path, call_start_utf8, call_end_utf8))
            .with_expansion_frame(expansion_frame);
        let marker_id = self.semantic_text.next_marker_id;
        self.semantic_text.next_marker_id += 1;
        let begin_name = self
            .interner
            .intern(&format!("latexd@semantic@begin@{marker_id}"));
        let end_name = self
            .interner
            .intern(&format!("latexd@semantic@end@{marker_id}"));
        self.semantic_text.marker_actions.insert(
            begin_name,
            ExpansionMarkerAction::Begin(ExpansionContext {
                id: marker_id,
                source,
            }),
        );
        self.semantic_text
            .marker_actions
            .insert(end_name, ExpansionMarkerAction::End(marker_id));

        self.push_token_front(
            queue,
            Token::control_sequence(end_name, call_start_utf8 as usize, call_end_utf8 as usize),
        );
        for token in expanded.into_iter().rev() {
            self.push_token_front(queue, token);
        }
        self.push_token_front(
            queue,
            Token::control_sequence(begin_name, call_start_utf8 as usize, call_end_utf8 as usize),
        );
    }

    pub(super) fn execute_semantic_expansion_marker(&mut self, name: ControlSequenceId) -> bool {
        let Some(action) = self.semantic_text.marker_actions.remove(&name) else {
            return false;
        };
        self.separate_executed_inline_content();
        match action {
            ExpansionMarkerAction::Begin(context) => {
                self.semantic_text.expansion_stack.push(context);
            }
            ExpansionMarkerAction::End(id) => {
                if self
                    .semantic_text
                    .expansion_stack
                    .last()
                    .is_some_and(|context| context.id == id)
                {
                    self.semantic_text.expansion_stack.pop();
                }
            }
        }
        true
    }

    pub(super) fn capture_executed_text_character(
        &mut self,
        ch: char,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if self.capture_executed_table_character(ch, start_utf8, end_utf8) {
            return;
        }
        if !self.can_capture_executed_text()
            || (start_utf8 == 0 && end_utf8 == 0 && self.semantic_text.expansion_stack.is_empty())
        {
            return;
        }
        let (source, producer, literal_path) = self.executed_text_source(start_utf8, end_utf8);
        let can_extend = self.semantic_text.capture.as_ref().is_some_and(|capture| {
            capture.producer == producer
                && if let Some(path) = &literal_path {
                    capture.literal_path.as_ref() == Some(path) && start_utf8 >= capture.end_utf8
                } else {
                    capture.source == source
                }
        });
        if !can_extend {
            self.flush_executed_text_capture();
            self.semantic_text.capture = Some(ExecutedTextCapture {
                text: String::new(),
                source,
                producer,
                literal_path,
                end_utf8,
            });
        }
        let capture = self
            .semantic_text
            .capture
            .as_mut()
            .expect("text capture was initialized");
        capture.text.push(ch);
        capture.end_utf8 = end_utf8;
        if capture.literal_path.is_some()
            && let ProvenanceSpan::File(span) = &mut capture.source.primary
        {
            span.end_utf8 = end_utf8;
        }
        self.semantic_text.paragraph_has_content = true;
        self.semantic_text.space_run_active = false;
    }

    pub(super) fn capture_executed_space(&mut self, start_utf8: u32, end_utf8: u32) {
        if self.capture_executed_table_space() {
            return;
        }
        self.flush_executed_text_capture();
        if !self.can_capture_executed_text()
            || !self.semantic_text.paragraph_has_content
            || self.semantic_text.space_run_active
        {
            return;
        }
        self.push_executed_text_event(
            RenderEvent::Space(SpaceEvent {
                kind: SpaceKind::Interword,
            }),
            start_utf8,
            end_utf8,
        );
        self.semantic_text.space_run_active = true;
    }

    pub(super) fn capture_executed_paragraph_break(&mut self, start_utf8: u32, end_utf8: u32) {
        self.flush_executed_text_capture();
        if !self.can_capture_executed_text() || !self.semantic_text.paragraph_has_content {
            return;
        }
        if self
            .semantic_text
            .executed_events
            .last()
            .is_some_and(|event| matches!(event.event, RenderEvent::Space(_)))
        {
            self.semantic_text.executed_events.pop();
        }
        self.semantic_text.space_run_active = false;
        let path = self.current_execution_source_path();
        let reason = if self
            .render_event_sources
            .get(&path)
            .and_then(|source| source.as_bytes().get(start_utf8 as usize))
            == Some(&b'\\')
        {
            ParagraphBreakReason::ParCommand
        } else {
            ParagraphBreakReason::BlankLine
        };
        self.push_executed_text_event(
            RenderEvent::ParagraphBreak(ParagraphBreakEvent { reason }),
            start_utf8,
            end_utf8,
        );
        self.semantic_text.paragraph_has_content = false;
    }

    pub(super) fn flush_executed_text_capture(&mut self) {
        let Some(capture) = self.semantic_text.capture.take() else {
            return;
        };
        if capture.text.is_empty() {
            return;
        }
        let event_id = self.render_events.allocate_event_id();
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::Text(TextEvent { text: capture.text }),
            capture.source,
        );
        envelope.meta.producer = capture.producer;
        self.semantic_text.executed_events.push(envelope);
    }

    pub(super) fn executed_text_event_mark(&mut self) -> usize {
        self.flush_executed_text_capture();
        self.semantic_text.executed_events.len()
    }

    pub(super) fn rollback_executed_text_events(&mut self, mark: usize) {
        self.flush_executed_text_capture();
        self.semantic_text.executed_events.truncate(mark);
    }

    pub(super) fn mark_executed_inline_content(&mut self) {
        if self.render_event_capture && self.execution_in_document {
            self.flush_executed_text_capture();
            self.semantic_text.paragraph_has_content = true;
            self.semantic_text.space_run_active = false;
        }
    }

    pub(super) fn separate_executed_inline_content(&mut self) {
        self.flush_executed_text_capture();
        self.semantic_text.space_run_active = false;
    }

    pub(super) fn finish_executed_block_content(&mut self) {
        self.flush_executed_text_capture();
        self.semantic_text.paragraph_has_content = false;
        self.semantic_text.space_run_active = false;
    }

    pub(super) fn executed_semantic_source(
        &self,
        start_utf8: u32,
        end_utf8: u32,
    ) -> (SourceProvenance, EventProducer) {
        let (source, producer, _) = self.executed_text_source(start_utf8, end_utf8);
        (source, producer)
    }

    pub(super) fn semantic_source_is_suppressed(&self, source: &SourceProvenance) -> bool {
        provenance_spans(source).any(|span| {
            self.semantic_text.suppressed_ranges.iter().any(|range| {
                span.path == range.path
                    && span.start_utf8 < range.end_utf8
                    && range.start_utf8 < span.end_utf8
            })
        })
    }

    pub(super) fn reconcile_executed_text_events(&mut self) {
        self.flush_executed_text_capture();
        let mut slots = mem::take(&mut self.semantic_text.scanner_slots);
        attach_trailing_scanner_spaces_to_eof_slots(
            &mut slots,
            &self.render_events,
            &self.render_event_sources,
        );
        let suppressed_ranges = self.semantic_text.suppressed_ranges.clone();
        let executed = mem::take(&mut self.semantic_text.executed_events);
        if slots.is_empty() {
            insert_unmatched_macro_events(&mut self.render_events, executed);
            return;
        }

        let mut events_by_slot = vec![Vec::<RenderEventEnvelope>::new(); slots.len()];
        let mut unmatched = Vec::new();
        for event in executed {
            if let Some(index) = slots.iter().position(|slot| {
                event_belongs_to_slot(
                    &event,
                    slot,
                    self.render_event_sources
                        .get(&slot.path)
                        .map(|source| source.len() as u32),
                )
            }) {
                events_by_slot[index].push(event);
            } else {
                unmatched.push(event);
            }
        }
        for (slot, replacements) in slots.iter().zip(events_by_slot.iter_mut()) {
            if self.executed_environment_covers_source_range(
                &slot.path,
                slot.start_utf8,
                slot.end_utf8,
            ) {
                continue;
            }
            let originals = self
                .render_events
                .iter()
                .filter(|event| slot.event_ids.contains(&event.meta.event_id))
                .cloned()
                .collect::<Vec<_>>();
            let leading_space = originals.first().filter(|event| {
                matches!(
                    event.event,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    })
                )
            });
            let trailing_eof_space = originals.last().filter(|event| {
                event.meta.producer == EventProducer::ScannerRecovery
                    && matches!(
                        event.event,
                        RenderEvent::Space(SpaceEvent {
                            kind: SpaceKind::Interword,
                        })
                    )
                    && matches!(
                        &event.meta.source.primary,
                        ProvenanceSpan::File(SourceSpan {
                            path,
                            start_utf8,
                            end_utf8,
                        }) if path == &slot.path
                            && *start_utf8 == slot.end_utf8
                            && *end_utf8 == slot.end_utf8
                    )
            });
            let matching_originals = event_payloads_match(&originals, replacements).then_some((
                originals.as_slice(),
                false,
                false,
            ));
            let matching_originals = matching_originals.or_else(|| {
                [(false, true), (true, false), (true, true)]
                    .into_iter()
                    .find_map(|(strip_leading, strip_trailing)| {
                        if (strip_leading && leading_space.is_none())
                            || (strip_trailing && trailing_eof_space.is_none())
                        {
                            return None;
                        }
                        let payload_start = usize::from(strip_leading);
                        let payload_end =
                            originals.len().saturating_sub(usize::from(strip_trailing));
                        originals
                            .get(payload_start..payload_end)
                            .filter(|originals| event_payloads_match(originals, replacements))
                            .map(|originals| (originals, strip_leading, strip_trailing))
                    })
            });
            if let Some((matching_originals, stripped_leading, stripped_trailing)) =
                matching_originals
            {
                for (original, replacement) in
                    matching_originals.iter().zip(replacements.iter_mut())
                {
                    replacement.meta.event_id = original.meta.event_id;
                    let original_has_extent = provenance_spans(&original.meta.source)
                        .any(|span| span.start_utf8 < span.end_utf8);
                    if original_has_extent {
                        replacement.meta.source = original.meta.source.clone();
                    }
                }
                if stripped_leading
                    && slot.preserve_leading_space
                    && let Some(leading_space) = leading_space
                {
                    let mut leading_space = leading_space.clone();
                    leading_space.meta.producer = EventProducer::Primitive;
                    leading_space.meta.confidence = tex_render_model::SemanticConfidence::High;
                    replacements.insert(0, leading_space);
                }
                if stripped_trailing && let Some(trailing_eof_space) = trailing_eof_space {
                    replacements.push(trailing_eof_space.clone());
                }
            } else if !suppressed_ranges
                .iter()
                .any(|range| slot_overlaps_suppressed_range(slot, range))
            {
                *replacements = originals;
            }
        }

        let scanner_event_ids = slots
            .iter()
            .flat_map(|slot| slot.event_ids.iter().copied())
            .collect::<HashSet<_>>();
        let replacement_by_first_id = slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.event_ids
                    .first()
                    .copied()
                    .map(|event_id| (event_id, index))
            })
            .collect::<HashMap<_, _>>();
        let mut reconciled = Vec::with_capacity(self.render_events.len());
        for event in self.render_events.drain(..) {
            if let Some(index) = replacement_by_first_id.get(&event.meta.event_id) {
                reconciled.append(&mut events_by_slot[*index]);
            }
            if !scanner_event_ids.contains(&event.meta.event_id) {
                reconciled.push(event);
            }
        }
        insert_unmatched_macro_events(&mut reconciled, unmatched);
        self.render_events.replace_events(reconciled);
    }

    pub(super) fn clear_semantic_suppression_ranges(&mut self) {
        self.semantic_text.suppressed_ranges.clear();
    }

    fn push_executed_text_event(&mut self, event: RenderEvent, start_utf8: u32, end_utf8: u32) {
        let event_id = self.render_events.allocate_event_id();
        let (source, producer, _) = self.executed_text_source(start_utf8, end_utf8);
        let mut envelope = RenderEventEnvelope::new(event_id, event, source);
        envelope.meta.producer = producer;
        self.semantic_text.executed_events.push(envelope);
    }

    fn executed_text_source(
        &self,
        start_utf8: u32,
        end_utf8: u32,
    ) -> (SourceProvenance, EventProducer, Option<Utf8PathBuf>) {
        if let Some(expansion) = self.semantic_text.expansion_stack.last() {
            return (expansion.source.clone(), EventProducer::Macro, None);
        }
        let path = self.current_execution_source_path();
        (
            SourceProvenance::file(path.clone(), start_utf8, end_utf8),
            EventProducer::Primitive,
            Some(path),
        )
    }

    fn can_capture_executed_text(&self) -> bool {
        self.render_event_capture
            && self.execution_in_document
            && self.executed_math_capture.is_none()
    }

    pub(super) fn current_execution_source_path(&self) -> Utf8PathBuf {
        self.source_stack
            .last()
            .map(|frame| frame.path.clone())
            .or_else(|| self.entry_source_path.clone())
            .unwrap_or_else(|| Utf8PathBuf::from("texput.tex"))
    }
}

fn event_belongs_to_slot(
    event: &RenderEventEnvelope,
    slot: &ScannerTextSlot,
    saved_source_len: Option<u32>,
) -> bool {
    let ProvenanceSpan::File(SourceSpan {
        path,
        start_utf8,
        end_utf8,
    }) = &event.meta.source.primary
    else {
        return false;
    };
    path == &slot.path
        && ((*start_utf8 >= slot.start_utf8 && *end_utf8 <= slot.end_utf8)
            || (matches!(
                event.event,
                RenderEvent::Space(SpaceEvent {
                    kind: SpaceKind::Interword,
                })
            ) && *start_utf8 == slot.end_utf8
                && *end_utf8 == start_utf8.saturating_add(1)))
        && (end_utf8 <= &slot.end_utf8 || saved_source_len == Some(slot.end_utf8))
}

fn attach_trailing_scanner_spaces_to_eof_slots(
    slots: &mut [ScannerTextSlot],
    events: &[RenderEventEnvelope],
    sources: &HashMap<Utf8PathBuf, String>,
) {
    let mut assigned_event_ids = slots
        .iter()
        .flat_map(|slot| slot.event_ids.iter().copied())
        .collect::<HashSet<_>>();
    for slot in slots {
        if sources.get(&slot.path).map(|source| source.len() as u32) != Some(slot.end_utf8) {
            continue;
        }
        let Some(candidate_id) = slot
            .event_ids
            .last()
            .copied()
            .and_then(|event_id| event_id.checked_add(1))
        else {
            continue;
        };
        if assigned_event_ids.contains(&candidate_id) {
            continue;
        }
        let is_trailing_scanner_space = events.iter().any(|event| {
            event.meta.event_id == candidate_id
                && event.meta.producer == EventProducer::ScannerRecovery
                && matches!(
                    event.event,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    })
                )
                && matches!(
                    &event.meta.source.primary,
                    ProvenanceSpan::File(SourceSpan {
                        path,
                        start_utf8,
                        end_utf8,
                    }) if path == &slot.path
                        && *start_utf8 == slot.end_utf8
                        && *end_utf8 == slot.end_utf8
                )
        });
        if is_trailing_scanner_space {
            slot.event_ids.push(candidate_id);
            assigned_event_ids.insert(candidate_id);
        }
    }
}

fn slot_overlaps_suppressed_range(slot: &ScannerTextSlot, range: &SuppressedSourceRange) -> bool {
    slot.path == range.path && slot.start_utf8 < range.end_utf8 && range.start_utf8 < slot.end_utf8
}

fn event_payloads_match(
    originals: &[RenderEventEnvelope],
    replacements: &[RenderEventEnvelope],
) -> bool {
    originals.len() == replacements.len()
        && originals
            .iter()
            .zip(replacements)
            .all(|(original, replacement)| original.event == replacement.event)
}

fn insert_unmatched_macro_events(
    events: &mut Vec<RenderEventEnvelope>,
    unmatched: Vec<RenderEventEnvelope>,
) {
    let mut index = 0;
    while index < unmatched.len() {
        let Some(anchor) = event_anchor(&unmatched[index]) else {
            index += 1;
            continue;
        };
        if unmatched[index].meta.producer != EventProducer::Macro {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < unmatched.len()
            && unmatched[end].meta.producer == EventProducer::Macro
            && event_anchor(&unmatched[end]).as_ref() == Some(&anchor)
        {
            end += 1;
        }
        let has_scanner_semantics = events.iter().any(|event| {
            event_overlaps_anchor(event, &anchor.0, anchor.1, anchor.2)
                && !matches!(event.event, RenderEvent::RawFallback(_))
                && matches!(
                    event.meta.producer,
                    EventProducer::ScannerRecovery | EventProducer::Fallback
                )
        });
        if has_scanner_semantics {
            index = end;
            continue;
        }
        events.retain(|event| {
            !matches!(event.event, RenderEvent::RawFallback(_))
                || !event_starts_at(event, &anchor.0, anchor.1)
        });
        let insertion = events
            .iter()
            .position(|event| {
                event_anchor(event)
                    .is_some_and(|(path, start, _)| path == anchor.0 && start >= anchor.1)
            })
            .unwrap_or(events.len());
        events.splice(insertion..insertion, unmatched[index..end].iter().cloned());
        index = end;
    }
}

fn event_anchor(event: &RenderEventEnvelope) -> Option<(Utf8PathBuf, u32, u32)> {
    match &event.meta.source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8, span.end_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}

fn event_overlaps_anchor(
    event: &RenderEventEnvelope,
    path: &Utf8Path,
    start_utf8: u32,
    end_utf8: u32,
) -> bool {
    provenance_spans(&event.meta.source)
        .any(|span| span.path == path && span.start_utf8 < end_utf8 && start_utf8 < span.end_utf8)
}

fn event_starts_at(event: &RenderEventEnvelope, path: &Utf8Path, start_utf8: u32) -> bool {
    provenance_spans(&event.meta.source)
        .any(|span| span.path == path && span.start_utf8 == start_utf8)
}

fn provenance_spans(source: &SourceProvenance) -> impl Iterator<Item = &SourceSpan> {
    std::iter::once(&source.primary)
        .chain(source.related.iter().map(|related| &related.span))
        .filter_map(|span| match span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        })
}

#[cfg(test)]
mod tests {
    use tex_tokens::ControlSequenceInterner;

    use super::*;

    #[test]
    fn eof_text_slot_claims_adjacent_zero_width_scanner_space() {
        let path = Utf8PathBuf::from("body.tex");
        let mut slots = vec![ScannerTextSlot {
            path: path.clone(),
            start_utf8: 0,
            end_utf8: 4,
            event_ids: vec![1],
            preserve_leading_space: false,
        }];
        let mut trailing_space = RenderEventEnvelope::new(
            2,
            RenderEvent::Space(SpaceEvent {
                kind: SpaceKind::Interword,
            }),
            SourceProvenance::file(path.clone(), 4, 4),
        );
        trailing_space.meta.producer = EventProducer::ScannerRecovery;
        let sources = HashMap::from([(path, "word".to_string())]);

        attach_trailing_scanner_spaces_to_eof_slots(&mut slots, &[trailing_space], &sources);

        assert_eq!(slots[0].event_ids, vec![1, 2]);
    }

    #[test]
    fn restored_preamble_reconciles_a_leading_body_fragment_space() {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        vm.set_entry_source_path("main.tex");
        vm.enable_render_event_capture();
        let preamble = vm.run_plain(r"\documentclass{article}\begin{document}");
        vm.set_render_event_prefix(preamble.render_events);
        let snapshot = vm.snapshot();

        let mut restored = Vm::restore(&mut interner, &snapshot);
        restored.set_entry_source_path("main.tex");
        restored.enable_render_event_capture();
        let outcome = restored.run_plain_fragment("\nAlpha Beta", true);
        let visible = outcome
            .render_events
            .iter()
            .filter_map(|event| match &event.event {
                RenderEvent::Text(text) => Some(text.text.as_str()),
                RenderEvent::Space(_) => Some(" "),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(visible, "Alpha Beta", "{:#?}", outcome.render_events);
        assert!(outcome.render_events.iter().all(|event| {
            !matches!(event.event, RenderEvent::Text(_) | RenderEvent::Space(_))
                || event.meta.producer != EventProducer::ScannerRecovery
        }));
    }
}
