use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    BeginFootnoteEvent, EndFootnoteEvent, EventId, EventProducer, FootnoteCommandKind,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{ControlSequenceId, Token};

use crate::{
    Vm,
    input::QueueItem,
    semantic_inline::ExecutedInlineEventMark,
    semantic_text::ExecutedTextFlowMark,
    snapshot::{
        VmActiveFootnoteCaptureSnapshot, VmScannerFootnoteSlotSnapshot, VmSemanticFootnoteSnapshot,
    },
};

#[derive(Debug, Default)]
pub(super) struct SemanticFootnoteState {
    scanner_slots: Vec<ScannerFootnoteSlot>,
    completed_transactions: Vec<Vec<RenderEventEnvelope>>,
    marker_actions: HashMap<ControlSequenceId, ActiveFootnoteCapture>,
    next_marker_id: u64,
}

#[derive(Debug)]
struct ScannerFootnoteSlot {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
    event_ids: Vec<EventId>,
}

#[derive(Debug)]
struct ActiveFootnoteCapture {
    marker_id: u64,
    begin_event: RenderEventEnvelope,
    text_flow_mark: ExecutedTextFlowMark,
    inline_event_mark: ExecutedInlineEventMark,
    math_event_mark: usize,
    transaction_mark: usize,
}

impl Vm<'_> {
    pub(super) fn semantic_footnote_snapshot(&self) -> VmSemanticFootnoteSnapshot {
        let mut scanner_slots = self
            .semantic_footnote
            .scanner_slots
            .iter()
            .map(|slot| VmScannerFootnoteSlotSnapshot {
                path: slot.path.clone(),
                start_utf8: slot.start_utf8,
                end_utf8: slot.end_utf8,
                event_ids: slot.event_ids.clone(),
            })
            .collect::<Vec<_>>();
        scanner_slots.sort_by(|left, right| {
            (&left.path, left.start_utf8, left.end_utf8).cmp(&(
                &right.path,
                right.start_utf8,
                right.end_utf8,
            ))
        });
        let mut active_actions = self
            .semantic_footnote
            .marker_actions
            .iter()
            .map(|(name, capture)| VmActiveFootnoteCaptureSnapshot {
                control_sequence: self.interner.resolve(*name).unwrap_or("").to_string(),
                marker_id: capture.marker_id,
                begin_event: capture.begin_event.clone(),
                text_flow_mark: capture.text_flow_mark.snapshot(),
                inline_event_mark: capture.inline_event_mark.snapshot(),
                math_event_mark: capture.math_event_mark.try_into().unwrap_or(u64::MAX),
                transaction_mark: capture.transaction_mark.try_into().unwrap_or(u64::MAX),
            })
            .collect::<Vec<_>>();
        active_actions.sort_by(|left, right| left.control_sequence.cmp(&right.control_sequence));
        VmSemanticFootnoteSnapshot {
            scanner_slots,
            completed_transactions: self.semantic_footnote.completed_transactions.clone(),
            active_actions,
            next_marker_id: self.semantic_footnote.next_marker_id,
        }
    }

    pub(super) fn restore_semantic_footnote_snapshot(
        &mut self,
        snapshot: &VmSemanticFootnoteSnapshot,
    ) {
        self.semantic_footnote.scanner_slots = snapshot
            .scanner_slots
            .iter()
            .map(|slot| ScannerFootnoteSlot {
                path: slot.path.clone(),
                start_utf8: slot.start_utf8,
                end_utf8: slot.end_utf8,
                event_ids: slot.event_ids.clone(),
            })
            .collect();
        self.semantic_footnote.completed_transactions = snapshot.completed_transactions.clone();
        self.semantic_footnote.marker_actions.clear();
        for capture in &snapshot.active_actions {
            let name = self.interner.intern(&capture.control_sequence);
            self.semantic_footnote.marker_actions.insert(
                name,
                ActiveFootnoteCapture {
                    marker_id: capture.marker_id,
                    begin_event: capture.begin_event.clone(),
                    text_flow_mark: ExecutedTextFlowMark::restore(&capture.text_flow_mark),
                    inline_event_mark: ExecutedInlineEventMark::restore(&capture.inline_event_mark),
                    math_event_mark: capture
                        .math_event_mark
                        .try_into()
                        .expect("validated footnote math event mark"),
                    transaction_mark: capture
                        .transaction_mark
                        .try_into()
                        .expect("validated footnote transaction mark"),
                },
            );
        }
        self.semantic_footnote.next_marker_id = snapshot.next_marker_id;
    }

    pub(super) fn record_scanner_footnote_slot(
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
        self.semantic_footnote
            .scanner_slots
            .push(ScannerFootnoteSlot {
                path: path.to_owned(),
                start_utf8,
                end_utf8,
                event_ids,
            });
    }

    pub(super) fn record_overridden_footnote_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if command_name == "footnote" {
            self.record_suppressed_source_range(start_utf8, end_utf8);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn queue_executed_footnote(
        &mut self,
        command: FootnoteCommandKind,
        marker: Option<String>,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        content_start_utf8: u32,
        content_end_utf8: u32,
        content_tokens: Vec<Token>,
        queue: &mut VecDeque<QueueItem>,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            for token in content_tokens.into_iter().rev() {
                self.push_token_front(queue, token);
            }
            return;
        }

        self.separate_executed_inline_content();
        let (mut source, producer) =
            self.executed_semantic_source(content_start_utf8, content_end_utf8);
        if producer == EventProducer::Primitive {
            let path = self.current_execution_source_path();
            source =
                SourceProvenance::file(path.clone(), invocation_start_utf8, invocation_end_utf8)
                    .with_related(
                        SourceSpanRole::ArgumentContent,
                        ProvenanceSpan::File(SourceSpan {
                            path,
                            start_utf8: content_start_utf8,
                            end_utf8: content_end_utf8,
                        }),
                    );
        }

        let event_id = self.render_events.allocate_event_id();
        let note_id = event_id;
        let mut begin_event = RenderEventEnvelope::new(
            event_id,
            RenderEvent::BeginFootnote(BeginFootnoteEvent {
                note_id,
                marker,
                command,
                draw_reference: command != FootnoteCommandKind::FootnoteText,
            }),
            source,
        );
        begin_event.meta.producer = producer;

        let marker_id = self.semantic_footnote.next_marker_id;
        self.semantic_footnote.next_marker_id += 1;
        let marker_name = self
            .interner
            .intern(&format!("latexd@semantic@footnote@end@{marker_id}"));
        let text_flow_mark = self.executed_text_flow_mark();
        let inline_event_mark = self.executed_inline_event_mark();
        let math_event_mark = self.executed_math_event_mark();
        let transaction_mark = self.semantic_footnote.completed_transactions.len();
        self.semantic_footnote.marker_actions.insert(
            marker_name,
            ActiveFootnoteCapture {
                marker_id,
                begin_event,
                text_flow_mark,
                inline_event_mark,
                math_event_mark,
                transaction_mark,
            },
        );
        self.output.push(' ');
        self.legacy_output_last_char = Some(' ');

        self.push_token_front(
            queue,
            Token::control_sequence(
                marker_name,
                invocation_start_utf8 as usize,
                invocation_end_utf8 as usize,
            ),
        );
        for token in content_tokens.into_iter().rev() {
            self.push_token_front(queue, token);
        }
    }

    pub(super) fn execute_semantic_footnote_marker(&mut self, name: ControlSequenceId) -> bool {
        let Some(capture) = self.semantic_footnote.marker_actions.remove(&name) else {
            return false;
        };

        let mut body_events = self.take_executed_text_events_since(capture.text_flow_mark);
        body_events.extend(self.take_executed_inline_events_since(capture.inline_event_mark));
        body_events.extend(self.take_executed_math_events_since(capture.math_event_mark));
        for transaction in self
            .semantic_footnote
            .completed_transactions
            .split_off(capture.transaction_mark)
        {
            body_events.extend(transaction);
        }
        body_events.sort_by_key(|event| event.meta.event_id);

        let note_id = match capture.begin_event.event {
            RenderEvent::BeginFootnote(ref begin) => begin.note_id,
            _ => unreachable!("footnote capture must begin with a footnote event"),
        };
        let event_id = self.render_events.allocate_event_id();
        let mut end_event = RenderEventEnvelope::new(
            event_id,
            RenderEvent::EndFootnote(EndFootnoteEvent { note_id }),
            capture.begin_event.meta.source.clone(),
        );
        end_event.meta.producer = capture.begin_event.meta.producer;

        let mut transaction = Vec::with_capacity(body_events.len() + 2);
        transaction.push(capture.begin_event);
        transaction.extend(body_events);
        transaction.push(end_event);
        self.semantic_footnote
            .completed_transactions
            .push(transaction);
        self.output.push(' ');
        self.legacy_output_last_char = Some(' ');
        self.mark_executed_inline_content();
        true
    }

    pub(super) fn reconcile_executed_footnote_events(&mut self) {
        let slots = mem::take(&mut self.semantic_footnote.scanner_slots);
        let mut transactions = mem::take(&mut self.semantic_footnote.completed_transactions);

        for slot in slots {
            let original_events = self
                .render_events
                .iter()
                .filter(|event| slot.event_ids.contains(&event.meta.event_id))
                .cloned()
                .collect::<Vec<_>>();
            let scanner_begin = original_events.iter().find_map(|event| match &event.event {
                RenderEvent::BeginFootnote(begin) => Some((begin, event.meta.source.clone())),
                _ => None,
            });
            let scanner_end_source = original_events.iter().find_map(|event| {
                matches!(event.event, RenderEvent::EndFootnote(_))
                    .then(|| event.meta.source.clone())
            });
            let matching = scanner_begin.as_ref().and_then(|(scanner_begin, _)| {
                transactions.iter().position(|transaction| {
                    let Some(executed_begin) =
                        transaction.iter().find_map(|event| match &event.event {
                            RenderEvent::BeginFootnote(begin) => Some(begin),
                            _ => None,
                        })
                    else {
                        return false;
                    };
                    if executed_begin.command != scanner_begin.command {
                        return false;
                    }
                    transaction.iter().any(|event| {
                        provenance_spans(&event.meta.source).any(|span| {
                            span.path == slot.path
                                && span.start_utf8 < slot.end_utf8
                                && slot.start_utf8 < span.end_utf8
                        })
                    })
                })
            });

            if let Some(index) = matching {
                let mut transaction = transactions.remove(index);
                let (scanner_note_id, scanner_begin_source) =
                    scanner_begin.expect("matching transaction requires scanner begin");
                let scanner_note_id = scanner_note_id.note_id;
                let executed_note_id = transaction
                    .iter()
                    .find_map(|event| match &event.event {
                        RenderEvent::BeginFootnote(begin) => Some(begin.note_id),
                        _ => None,
                    })
                    .expect("executed footnote transaction begin");
                for event in &mut transaction {
                    match &mut event.event {
                        RenderEvent::BeginFootnote(begin) if begin.note_id == executed_note_id => {
                            begin.note_id = scanner_note_id;
                            event.meta.source = scanner_begin_source.clone();
                        }
                        RenderEvent::EndFootnote(end) if end.note_id == executed_note_id => {
                            end.note_id = scanner_note_id;
                            if let Some(source) = &scanner_end_source {
                                event.meta.source = source.clone();
                            }
                        }
                        _ => {}
                    }
                }
                let removed_event_ids = original_events
                    .iter()
                    .map(|event| event.meta.event_id)
                    .collect::<BTreeSet<_>>();
                let _ = self
                    .render_events
                    .replace_transaction(&removed_event_ids, transaction);
            } else if original_events
                .iter()
                .any(|event| self.semantic_source_is_suppressed(&event.meta.source))
            {
                let removed_event_ids = original_events
                    .iter()
                    .map(|event| event.meta.event_id)
                    .collect::<BTreeSet<_>>();
                let _ = self
                    .render_events
                    .replace_transaction(&removed_event_ids, Vec::new());
            }
        }

        let mut events = self.render_events.take_events();
        for transaction in transactions {
            let Some((path, start_utf8, end_utf8)) = transaction.first().and_then(event_anchor)
            else {
                continue;
            };
            events.retain(|event| {
                !matches!(event.event, RenderEvent::RawFallback(_))
                    || !provenance_spans(&event.meta.source)
                        .any(|span| span.path == path && span.start_utf8 == start_utf8)
            });
            let insertion = events
                .iter()
                .position(|event| {
                    event_anchor(event).is_some_and(|(event_path, event_start, event_end)| {
                        event_path == path
                            && (event_start > start_utf8
                                || (event_start == start_utf8 && event_end >= end_utf8))
                    })
                })
                .unwrap_or(events.len());
            events.splice(insertion..insertion, transaction);
        }
        self.render_events.replace_events(events);
    }
}

fn event_anchor(event: &RenderEventEnvelope) -> Option<(Utf8PathBuf, u32, u32)> {
    if event.meta.producer == EventProducer::Macro
        && let Some(ProvenanceSpan::File(span)) = event
            .meta
            .source
            .expansion_stack
            .last()
            .map(|frame| &frame.call_span)
    {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    match &event.meta.source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8, span.end_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}

fn provenance_spans(source: &SourceProvenance) -> impl Iterator<Item = &SourceSpan> {
    std::iter::once(&source.primary)
        .chain(source.related.iter().map(|related| &related.span))
        .chain(source.expansion_stack.iter().map(|frame| &frame.call_span))
        .filter_map(|span| match span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        })
}
