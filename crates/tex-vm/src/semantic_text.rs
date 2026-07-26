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

use crate::{Vm, input::QueueItem};

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
}

#[derive(Debug)]
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
    pub(super) fn record_scanner_text_slot(
        &mut self,
        path: &Utf8Path,
        start_utf8: u32,
        end_utf8: u32,
        first_event_id: EventId,
    ) {
        let event_ids = (first_event_id..self.next_render_event_id).collect::<Vec<_>>();
        if event_ids.is_empty() {
            return;
        }
        self.semantic_text.scanner_slots.push(ScannerTextSlot {
            path: path.to_owned(),
            start_utf8,
            end_utf8,
            event_ids,
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
        });
    }

    pub(super) fn record_suppressed_source_range(&mut self, start_utf8: u32, end_utf8: u32) {
        if !self.render_event_capture || end_utf8 <= start_utf8 {
            return;
        }
        self.semantic_text
            .suppressed_ranges
            .push(SuppressedSourceRange {
                path: self.current_execution_source_path(),
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
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::Text(TextEvent { text: capture.text }),
            capture.source,
        );
        envelope.meta.producer = capture.producer;
        self.semantic_text.executed_events.push(envelope);
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

    pub(super) fn reconcile_executed_text_events(&mut self) {
        self.flush_executed_text_capture();
        let slots = mem::take(&mut self.semantic_text.scanner_slots);
        let suppressed_ranges = mem::take(&mut self.semantic_text.suppressed_ranges);
        let executed = mem::take(&mut self.semantic_text.executed_events);
        if slots.is_empty() {
            return;
        }

        let mut events_by_slot = vec![Vec::<RenderEventEnvelope>::new(); slots.len()];
        let mut unmatched = Vec::new();
        for event in executed {
            if let Some(index) = slots
                .iter()
                .position(|slot| event_belongs_to_slot(&event, slot))
            {
                events_by_slot[index].push(event);
            } else {
                unmatched.push(event);
            }
        }
        for (slot, replacements) in slots.iter().zip(events_by_slot.iter_mut()) {
            let originals = self
                .render_events
                .iter()
                .filter(|event| slot.event_ids.contains(&event.meta.event_id))
                .cloned()
                .collect::<Vec<_>>();
            let matching_originals = if event_payloads_match(&originals, replacements) {
                Some(originals.as_slice())
            } else if originals.first().is_some_and(|event| {
                matches!(
                    event.event,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    })
                )
            }) && event_payloads_match(&originals[1..], replacements)
            {
                Some(&originals[1..])
            } else {
                None
            };
            if let Some(matching_originals) = matching_originals {
                for (original, replacement) in
                    matching_originals.iter().zip(replacements.iter_mut())
                {
                    replacement.meta.event_id = original.meta.event_id;
                    replacement.meta.source = original.meta.source.clone();
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
        for (index, event) in reconciled.iter_mut().enumerate() {
            event.meta.event_id = index as EventId + 1;
        }
        self.next_render_event_id = reconciled.len() as EventId + 1;
        self.render_events = reconciled;
    }

    fn push_executed_text_event(&mut self, event: RenderEvent, start_utf8: u32, end_utf8: u32) {
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
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

    fn current_execution_source_path(&self) -> Utf8PathBuf {
        self.source_stack
            .last()
            .map(|frame| frame.path.clone())
            .or_else(|| self.entry_source_path.clone())
            .unwrap_or_else(|| Utf8PathBuf::from("texput.tex"))
    }
}

fn event_belongs_to_slot(event: &RenderEventEnvelope, slot: &ScannerTextSlot) -> bool {
    let ProvenanceSpan::File(SourceSpan {
        path,
        start_utf8,
        end_utf8,
    }) = &event.meta.source.primary
    else {
        return false;
    };
    path == &slot.path && *start_utf8 >= slot.start_utf8 && *end_utf8 <= slot.end_utf8
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
