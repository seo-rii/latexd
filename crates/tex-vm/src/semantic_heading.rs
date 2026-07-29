use std::{
    collections::{HashMap, HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventProducer, EventSequence, HeadingEvent, ProvenanceSpan, RenderEvent, RenderEventEnvelope,
    SemanticConfidence, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{ControlSequenceId, Token};

use crate::{
    Vm,
    input::QueueItem,
    semantic_transaction::ExecutedSemanticEventMark,
    snapshot::{VmActiveHeadingCaptureSnapshot, VmSemanticHeadingSnapshot},
};

#[derive(Debug, Default)]
pub(super) struct SemanticHeadingState {
    scanner_event_ids: HashSet<EventSequence>,
    executed_events: Vec<RenderEventEnvelope>,
    marker_actions: HashMap<ControlSequenceId, ExecutedHeadingCapture>,
    next_marker_id: u64,
}

#[derive(Debug)]
struct ExecutedHeadingCapture {
    marker_id: u64,
    level: u8,
    numbered: bool,
    source: SourceProvenance,
    producer: EventProducer,
    text_prefix: String,
    output_start: usize,
    lossy_prefix: bool,
    diagnostic_mark: usize,
    event_mark: ExecutedSemanticEventMark,
    heading_event_mark: usize,
}

impl Vm<'_> {
    pub(super) fn semantic_heading_snapshot(&self) -> VmSemanticHeadingSnapshot {
        let mut scanner_event_ids = self
            .semantic_heading
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        let mut active_heading_actions = self
            .semantic_heading
            .marker_actions
            .iter()
            .map(|(name, capture)| {
                let mut visible_output_prefix = capture.text_prefix.clone();
                visible_output_prefix.push_str(
                    self.output
                        .get(capture.output_start..)
                        .expect("active heading output cursor must be valid"),
                );
                VmActiveHeadingCaptureSnapshot {
                    control_sequence: self.interner.resolve(*name).unwrap_or("").to_string(),
                    marker_id: capture.marker_id,
                    level: capture.level,
                    numbered: capture.numbered,
                    source: capture.source.clone(),
                    producer: capture.producer,
                    visible_output_prefix,
                    lossy_before_restore: capture.lossy_prefix
                        || self.diagnostics.len() > capture.diagnostic_mark,
                    text_event_mark: capture
                        .event_mark
                        .text_event_mark()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    inline_event_mark: capture.event_mark.inline_event_mark().snapshot(),
                    math_event_mark: capture
                        .event_mark
                        .math_event_mark()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    heading_event_mark: capture.heading_event_mark.try_into().unwrap_or(u64::MAX),
                }
            })
            .collect::<Vec<_>>();
        active_heading_actions
            .sort_by(|left, right| left.control_sequence.cmp(&right.control_sequence));
        VmSemanticHeadingSnapshot {
            scanner_event_ids,
            executed_events: self.semantic_heading.executed_events.clone(),
            active_heading_actions,
            next_marker_id: self.semantic_heading.next_marker_id,
        }
    }

    pub(super) fn restore_semantic_heading_snapshot(
        &mut self,
        snapshot: &VmSemanticHeadingSnapshot,
    ) {
        self.semantic_heading.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_heading.executed_events = snapshot.executed_events.clone();
        self.semantic_heading.marker_actions.clear();
        for capture in &snapshot.active_heading_actions {
            let name = self.interner.intern(&capture.control_sequence);
            self.semantic_heading.marker_actions.insert(
                name,
                ExecutedHeadingCapture {
                    marker_id: capture.marker_id,
                    level: capture.level,
                    numbered: capture.numbered,
                    source: capture.source.clone(),
                    producer: capture.producer,
                    text_prefix: capture.visible_output_prefix.clone(),
                    output_start: self.output.len(),
                    lossy_prefix: capture.lossy_before_restore,
                    diagnostic_mark: self.diagnostics.len(),
                    event_mark: ExecutedSemanticEventMark::from_parts(
                        capture
                            .text_event_mark
                            .try_into()
                            .expect("validated text event mark"),
                        crate::semantic_inline::ExecutedInlineEventMark::restore(
                            &capture.inline_event_mark,
                        ),
                        capture
                            .math_event_mark
                            .try_into()
                            .expect("validated math event mark"),
                    ),
                    heading_event_mark: capture
                        .heading_event_mark
                        .try_into()
                        .expect("validated heading event mark"),
                },
            );
        }
        self.semantic_heading.next_marker_id = snapshot.next_marker_id;
    }

    pub(super) fn mark_scanner_heading_event(&mut self, event_id: EventSequence) {
        self.semantic_heading.scanner_event_ids.insert(event_id);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn queue_executed_heading(
        &mut self,
        level: u8,
        numbered: bool,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        title_start_utf8: u32,
        title_end_utf8: u32,
        title_tokens: Vec<Token>,
        queue: &mut VecDeque<QueueItem>,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            for token in title_tokens.into_iter().rev() {
                self.push_token_front(queue, token);
            }
            return;
        }

        self.finish_executed_block_content();
        let (mut source, producer) =
            self.executed_semantic_source(title_start_utf8, title_end_utf8);
        let path = self.current_execution_source_path();
        if producer == EventProducer::Primitive {
            source = source
                .with_related(
                    SourceSpanRole::ArgumentContent,
                    ProvenanceSpan::File(SourceSpan {
                        path: path.clone(),
                        start_utf8: title_start_utf8,
                        end_utf8: title_end_utf8,
                    }),
                )
                .with_related(
                    SourceSpanRole::Invocation,
                    ProvenanceSpan::File(SourceSpan {
                        path,
                        start_utf8: invocation_start_utf8,
                        end_utf8: invocation_end_utf8,
                    }),
                );
        }

        let marker_id = self.semantic_heading.next_marker_id;
        self.semantic_heading.next_marker_id += 1;
        let marker_name = self
            .interner
            .intern(&format!("latexd@semantic@heading@end@{marker_id}"));
        let capture = ExecutedHeadingCapture {
            marker_id,
            level,
            numbered,
            source,
            producer,
            text_prefix: String::new(),
            output_start: self.output.len(),
            lossy_prefix: false,
            diagnostic_mark: self.diagnostics.len(),
            event_mark: self.mark_executed_semantic_events(),
            heading_event_mark: self.semantic_heading.executed_events.len(),
        };
        self.semantic_heading
            .marker_actions
            .insert(marker_name, capture);

        self.push_token_front(
            queue,
            Token::control_sequence(
                marker_name,
                invocation_start_utf8 as usize,
                invocation_end_utf8 as usize,
            ),
        );
        for token in title_tokens.into_iter().rev() {
            self.push_token_front(queue, token);
        }
    }

    pub(super) fn execute_semantic_heading_marker(&mut self, name: ControlSequenceId) -> bool {
        let Some(capture) = self.semantic_heading.marker_actions.remove(&name) else {
            return false;
        };
        self.flush_executed_text_capture();
        let mut raw_text = capture.text_prefix.clone();
        raw_text.push_str(self.output.get(capture.output_start..).unwrap_or_default());
        let raw_text = raw_text.replace("[?]", "\u{e000}");
        let text = crate::normalize_latex_text_with_inline_placeholders(&raw_text)
            .replace('\u{e000}', "[?]");
        self.rollback_executed_semantic_events(capture.event_mark);
        self.semantic_heading
            .executed_events
            .truncate(capture.heading_event_mark);
        self.finish_executed_block_content();

        let event_id = self.render_events.allocate_event_sequence();
        let lossy = capture.lossy_prefix || self.diagnostics.len() > capture.diagnostic_mark;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::Heading(HeadingEvent {
                level: capture.level,
                text,
                number: capture.numbered.then(String::new),
            }),
            capture.source,
        );
        if lossy {
            envelope.meta.producer = EventProducer::Fallback;
            envelope.meta.confidence = SemanticConfidence::Low;
        } else {
            envelope.meta.producer = capture.producer;
        }
        self.semantic_heading.executed_events.push(envelope);
        true
    }

    pub(super) fn reconcile_executed_heading_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_heading.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_heading.executed_events);
        if scanner_ids.is_empty() && executed.is_empty() {
            return;
        }

        let scanner_events = self.render_events.take_events();
        let mut reconciled = Vec::with_capacity(scanner_events.len() + executed.len());
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.sequence) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed
                .iter()
                .position(|candidate| {
                    candidate.meta.producer != EventProducer::Fallback
                        && heading_payloads_match(candidate, &scanner_event)
                        && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
                })
                .or_else(|| {
                    executed.iter().position(|candidate| {
                        candidate.meta.producer != EventProducer::Fallback
                            && heading_levels_match(candidate, &scanner_event)
                            && provenance_overlaps(
                                &candidate.meta.source,
                                &scanner_event.meta.source,
                            )
                    })
                });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                let payloads_match = heading_payloads_match(&executed_event, &scanner_event);
                let executed_source = executed_event.meta.source;
                let mut source = if payloads_match {
                    scanner_event.meta.source
                } else {
                    executed_source.clone()
                };
                if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            } else if !self.semantic_source_is_suppressed(&scanner_event.meta.source) {
                reconciled.push(scanner_event);
            }
        }

        executed.retain(|event| !self.semantic_source_is_suppressed(&event.meta.source));
        executed.retain(|event| {
            event.meta.producer != EventProducer::Fallback
                || !reconciled.iter().any(|existing| {
                    heading_levels_match(event, existing)
                        && provenance_overlaps(&event.meta.source, &existing.meta.source)
                })
        });
        insert_unmatched_heading_events(&mut reconciled, executed);
        renumber_heading_events(&mut reconciled);
        self.render_events.replace_events(reconciled);
    }
}

fn heading_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::Heading(left), RenderEvent::Heading(right))
            if left.level == right.level && left.text == right.text
    )
}

fn heading_levels_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::Heading(left), RenderEvent::Heading(right)) if left.level == right.level
    )
}

fn insert_unmatched_heading_events(
    events: &mut Vec<RenderEventEnvelope>,
    executed: Vec<RenderEventEnvelope>,
) {
    for event in executed {
        let Some((path, start_utf8, end_utf8)) = event_anchor(&event) else {
            continue;
        };
        let insertion = events
            .iter()
            .position(|existing| {
                event_anchor(existing).is_some_and(
                    |(existing_path, existing_start, existing_end)| {
                        existing_path == path
                            && (existing_start > start_utf8
                                || (existing_start == start_utf8
                                    && (existing_end > end_utf8
                                        || (existing_end == end_utf8
                                            && existing.meta.sequence > event.meta.sequence))))
                    },
                )
            })
            .unwrap_or(events.len());
        events.insert(insertion, event);
    }
}

fn renumber_heading_events(events: &mut [RenderEventEnvelope]) {
    let mut counters = [0u32; 6];
    for event in events {
        let RenderEvent::Heading(heading) = &mut event.event else {
            continue;
        };
        if heading.number.is_none() {
            continue;
        }
        let level = usize::from(heading.level).min(counters.len() - 1);
        counters[level] += 1;
        counters[level + 1..].fill(0);
        heading.number = Some(
            counters[..=level]
                .iter()
                .skip_while(|counter| **counter == 0)
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("."),
        );
    }
}

fn provenance_overlaps(left: &SourceProvenance, right: &SourceProvenance) -> bool {
    provenance_spans(left).any(|left_span| {
        provenance_spans(right).any(|right_span| {
            left_span.path == right_span.path
                && left_span.start_utf8 < right_span.end_utf8
                && right_span.start_utf8 < left_span.end_utf8
        })
    })
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
    if let Some(span) = event.meta.source.related.iter().find_map(|related| {
        if related.role != SourceSpanRole::Invocation {
            return None;
        }
        match &related.span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        }
    }) {
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
        .chain(
            source
                .expansion_stack
                .iter()
                .flat_map(|frame| std::iter::once(&frame.call_span).chain(&frame.definition_span)),
        )
        .filter_map(|span| match span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        })
}
