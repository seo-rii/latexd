use std::{
    collections::{HashMap, HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventId, EventProducer, HeadingEvent, ProvenanceSpan, RenderEvent, RenderEventEnvelope,
    SemanticConfidence, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{ControlSequenceId, Token};

use crate::{Vm, input::QueueItem, semantic_inline::ExecutedInlineEventMark};

#[derive(Debug, Default)]
pub(super) struct SemanticHeadingState {
    scanner_event_ids: HashSet<EventId>,
    executed_events: Vec<RenderEventEnvelope>,
    marker_actions: HashMap<ControlSequenceId, ExecutedHeadingCapture>,
    next_marker_id: u64,
}

#[derive(Debug)]
struct ExecutedHeadingCapture {
    level: u8,
    numbered: bool,
    source: SourceProvenance,
    producer: EventProducer,
    output_start: usize,
    diagnostic_mark: usize,
    text_event_mark: usize,
    inline_event_mark: ExecutedInlineEventMark,
    math_event_mark: usize,
    heading_event_mark: usize,
}

impl Vm<'_> {
    pub(super) fn mark_scanner_heading_event(&mut self, event_id: EventId) {
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
            level,
            numbered,
            source,
            producer,
            output_start: self.output.len(),
            diagnostic_mark: self.diagnostics.len(),
            text_event_mark: self.executed_text_event_mark(),
            inline_event_mark: self.executed_inline_event_mark(),
            math_event_mark: self.executed_math_event_mark(),
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
        let raw_text = self
            .output
            .get(capture.output_start..)
            .unwrap_or_default()
            .replace("[?]", "\u{e000}");
        let text = crate::normalize_latex_text_with_inline_placeholders(&raw_text)
            .replace('\u{e000}', "[?]");
        self.rollback_executed_text_events(capture.text_event_mark);
        self.rollback_executed_inline_events(capture.inline_event_mark);
        self.rollback_executed_math_events(capture.math_event_mark);
        self.semantic_heading
            .executed_events
            .truncate(capture.heading_event_mark);
        self.finish_executed_block_content();

        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let lossy = self.diagnostics.len() > capture.diagnostic_mark;
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

        let scanner_events = mem::take(&mut self.render_events);
        let mut reconciled = Vec::with_capacity(scanner_events.len() + executed.len());
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
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
        self.render_events = reconciled;
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
                                            && existing.meta.event_id > event.meta.event_id))))
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
