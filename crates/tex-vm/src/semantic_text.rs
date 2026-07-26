use std::{
    collections::{HashMap, HashSet},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    EventId, EventProducer, ParagraphBreakEvent, ParagraphBreakReason, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SourceProvenance, SourceSpan, SpaceEvent, SpaceKind, TextEvent,
};

use crate::Vm;

#[derive(Debug, Default)]
pub(super) struct SemanticTextState {
    scanner_slots: Vec<ScannerTextSlot>,
    suppressed_ranges: Vec<SuppressedSourceRange>,
    executed_events: Vec<RenderEventEnvelope>,
    capture: Option<ExecutedTextCapture>,
    paragraph_has_content: bool,
    space_run_active: bool,
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
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
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

    pub(super) fn capture_executed_text_character(
        &mut self,
        ch: char,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.can_capture_executed_text() || (start_utf8 == 0 && end_utf8 == 0) {
            return;
        }
        let path = self.current_execution_source_path();
        let can_extend = self
            .semantic_text
            .capture
            .as_ref()
            .is_some_and(|capture| capture.path == path && start_utf8 >= capture.end_utf8);
        if !can_extend {
            self.flush_executed_text_capture();
            self.semantic_text.capture = Some(ExecutedTextCapture {
                text: String::new(),
                path,
                start_utf8,
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
            SourceProvenance::file(capture.path, capture.start_utf8, capture.end_utf8),
        );
        envelope.meta.producer = EventProducer::Primitive;
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
        for event in executed {
            if let Some(index) = slots
                .iter()
                .position(|slot| event_belongs_to_slot(&event, slot))
            {
                events_by_slot[index].push(event);
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
        for (index, event) in reconciled.iter_mut().enumerate() {
            event.meta.event_id = index as EventId + 1;
        }
        self.next_render_event_id = reconciled.len() as EventId + 1;
        self.render_events = reconciled;
    }

    fn push_executed_text_event(&mut self, event: RenderEvent, start_utf8: u32, end_utf8: u32) {
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            event,
            SourceProvenance::file(self.current_execution_source_path(), start_utf8, end_utf8),
        );
        envelope.meta.producer = EventProducer::Primitive;
        self.semantic_text.executed_events.push(envelope);
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
