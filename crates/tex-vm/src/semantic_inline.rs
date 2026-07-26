use std::{
    collections::{BTreeMap, HashSet},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    CaptionInlinePlaceholderEvent, EventId, EventProducer, InlineCitationEvent,
    InlineReferenceEvent, ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance,
    SourceSpan, SourceSpanRole,
};

use crate::{Vm, citation_style_hint_for_command};

#[derive(Debug, Default)]
pub(super) struct SemanticInlineState {
    scanner_citation_event_ids: HashSet<EventId>,
    scanner_reference_event_ids: HashSet<EventId>,
    executed_citations: Vec<RenderEventEnvelope>,
    executed_references: Vec<RenderEventEnvelope>,
}

impl Vm<'_> {
    pub(super) fn mark_scanner_citation_event(&mut self, event_id: EventId) {
        self.semantic_inline
            .scanner_citation_event_ids
            .insert(event_id);
    }

    pub(super) fn mark_scanner_reference_event(&mut self, event_id: EventId) {
        self.semantic_inline
            .scanner_reference_event_ids
            .insert(event_id);
    }

    pub(super) fn emit_executed_citation(
        &mut self,
        command: String,
        keys: Vec<String>,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        let (source, producer) = self.executed_inline_source(start_utf8, end_utf8);
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::InlineCitation(InlineCitationEvent {
                keys,
                style_hint: citation_style_hint_for_command(&command),
                command,
            }),
            source,
        );
        envelope.meta.producer = producer;
        self.semantic_inline.executed_citations.push(envelope);
        self.mark_executed_inline_content();
    }

    pub(super) fn emit_executed_reference(
        &mut self,
        command: String,
        keys: Vec<String>,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        let (source, producer) = self.executed_inline_source(start_utf8, end_utf8);
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::InlineReference(InlineReferenceEvent { keys, command }),
            source,
        );
        envelope.meta.producer = producer;
        self.semantic_inline.executed_references.push(envelope);
        self.mark_executed_inline_content();
    }

    pub(super) fn reconcile_executed_inline_events(&mut self) {
        let citation_ids = mem::take(&mut self.semantic_inline.scanner_citation_event_ids);
        let citations = mem::take(&mut self.semantic_inline.executed_citations);
        self.reconcile_scanner_inline_events(citation_ids, citations);

        let reference_ids = mem::take(&mut self.semantic_inline.scanner_reference_event_ids);
        let references = mem::take(&mut self.semantic_inline.executed_references);
        self.reconcile_scanner_inline_events(reference_ids, references);
    }

    fn reconcile_scanner_inline_events(
        &mut self,
        scanner_ids: HashSet<EventId>,
        mut executed: Vec<RenderEventEnvelope>,
    ) {
        if scanner_ids.is_empty() && executed.is_empty() {
            return;
        }

        let mut reconciled = Vec::with_capacity(self.render_events.len() + executed.len());
        let scanner_events = mem::take(&mut self.render_events);
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed.iter().position(|candidate| {
                inline_payload_matches(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
            });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                executed_event.meta.event_id = scanner_event.meta.event_id;
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                source.related.extend(
                    executed_source
                        .related
                        .iter()
                        .filter(|related| related.role == SourceSpanRole::Invocation)
                        .cloned(),
                );
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
        insert_unmatched_inline_events(&mut reconciled, executed);
        self.render_events = reconciled;
    }

    pub(super) fn reconcile_embedded_executed_inline_events(&mut self) {
        let mut executed = Vec::new();
        let mut events = Vec::with_capacity(self.render_events.len());
        for event in self.render_events.drain(..) {
            if matches!(
                event.event,
                RenderEvent::InlineCitation(_) | RenderEvent::InlineReference(_)
            ) && matches!(
                event.meta.producer,
                EventProducer::Primitive | EventProducer::Macro
            ) {
                executed.push(event);
            } else {
                events.push(event);
            }
        }
        self.replace_embedded_inline_placeholders(&mut events, &mut executed);
        executed.retain(|event| !recovery_container_represents(&events, event));
        insert_unmatched_inline_events(&mut events, executed);
        for (index, event) in events.iter_mut().enumerate() {
            event.meta.event_id = index as EventId + 1;
        }
        self.next_render_event_id = events.len() as EventId + 1;
        self.render_events = events;
    }

    fn replace_embedded_inline_placeholders(
        &self,
        events: &mut Vec<RenderEventEnvelope>,
        executed: &mut Vec<RenderEventEnvelope>,
    ) {
        let mut reconciled = Vec::with_capacity(events.len() + executed.len());
        for scanner_event in events.drain(..) {
            let RenderEvent::Text(text) = &scanner_event.event else {
                reconciled.push(scanner_event);
                continue;
            };
            let Some((path, start_utf8, end_utf8)) = event_anchor(&scanner_event) else {
                reconciled.push(scanner_event);
                continue;
            };
            let Some(source) = self.render_event_sources.get(&path) else {
                reconciled.push(scanner_event);
                continue;
            };
            let Some(raw_source) = source.get(start_utf8 as usize..end_utf8 as usize) else {
                reconciled.push(scanner_event);
                continue;
            };
            let placeholder_offsets = text
                .text
                .match_indices("[?]")
                .map(|(offset, _)| offset)
                .collect::<Vec<_>>();
            if placeholder_offsets.is_empty() {
                reconciled.push(scanner_event);
                continue;
            }

            let mut replacements = BTreeMap::new();
            let mut consumed = Vec::new();
            for (index, event) in executed.iter().enumerate() {
                let Some((event_path, event_start, event_end)) = event_anchor(event) else {
                    continue;
                };
                if event_path != path || event_start < start_utf8 || event_end > end_utf8 {
                    continue;
                }
                let prefix_end = (event_start - start_utf8) as usize;
                let placeholder_ordinal =
                    crate::caption_inline_placeholders(&raw_source[..prefix_end]).len();
                let Some(placeholder_offset) =
                    placeholder_offsets.get(placeholder_ordinal).copied()
                else {
                    continue;
                };
                if !replacements.contains_key(&placeholder_offset) {
                    replacements.insert(placeholder_offset, event.clone());
                    consumed.push(index);
                }
            }
            if replacements.is_empty() {
                reconciled.push(scanner_event);
                continue;
            }
            for index in consumed.into_iter().rev() {
                executed.remove(index);
            }

            let mut cursor = 0;
            for (placeholder_offset, event) in replacements {
                push_text_fragment(
                    &mut reconciled,
                    &scanner_event,
                    &text.text[cursor..placeholder_offset],
                );
                reconciled.push(event);
                cursor = placeholder_offset + "[?]".len();
            }
            push_text_fragment(&mut reconciled, &scanner_event, &text.text[cursor..]);
        }
        *events = reconciled;
    }

    fn executed_inline_source(
        &self,
        start_utf8: u32,
        end_utf8: u32,
    ) -> (SourceProvenance, EventProducer) {
        let (mut source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        let invocation_span = match &source.primary {
            ProvenanceSpan::File(primary)
                if producer == EventProducer::Macro
                    && start_utf8 >= primary.start_utf8
                    && end_utf8 <= primary.end_utf8 =>
            {
                Some(SourceSpan {
                    path: primary.path.clone(),
                    start_utf8,
                    end_utf8,
                })
            }
            _ => None,
        };
        if let Some(invocation_span) = invocation_span {
            source = source.with_related(
                SourceSpanRole::Invocation,
                ProvenanceSpan::File(invocation_span),
            );
        }
        (source, producer)
    }
}

fn inline_payload_matches(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::InlineCitation(left), RenderEvent::InlineCitation(right)) => {
            left.keys == right.keys && left.style_hint == right.style_hint
        }
        (RenderEvent::InlineReference(left), RenderEvent::InlineReference(right)) => {
            left.keys == right.keys && left.command == right.command
        }
        _ => false,
    }
}

fn recovery_container_represents(
    events: &[RenderEventEnvelope],
    inline_event: &RenderEventEnvelope,
) -> bool {
    events.iter().any(|event| {
        matches!(
            event.meta.producer,
            EventProducer::ScannerRecovery | EventProducer::Fallback
        ) && event_anchor_is_contained_by(inline_event, &event.meta.source)
            && match &event.event {
                RenderEvent::Text(text) => text.text.contains("[?]"),
                RenderEvent::Heading(heading) => heading.text.contains("[?]"),
                RenderEvent::InlineLink(link) => link.text.contains("[?]"),
                RenderEvent::BibliographyItem(item) => item.text.contains("[?]"),
                RenderEvent::Caption(caption) => {
                    caption.text.contains("[?]")
                        || caption.inline_placeholders.iter().any(|placeholder| {
                            match (placeholder, &inline_event.event) {
                                (
                                    CaptionInlinePlaceholderEvent::Citation(embedded),
                                    RenderEvent::InlineCitation(actual),
                                ) => {
                                    embedded.keys == actual.keys
                                        && embedded.style_hint == actual.style_hint
                                }
                                (
                                    CaptionInlinePlaceholderEvent::Reference(embedded),
                                    RenderEvent::InlineReference(actual),
                                ) => embedded.keys == actual.keys,
                                _ => false,
                            }
                        })
                }
                RenderEvent::RawFallback(fallback) => fallback
                    .normalized_visible_text
                    .as_deref()
                    .is_some_and(|text| text.contains("[?]")),
                _ => false,
            }
    })
}

fn event_anchor_is_contained_by(event: &RenderEventEnvelope, source: &SourceProvenance) -> bool {
    let Some((path, start_utf8, end_utf8)) = event_anchor(event) else {
        return false;
    };
    provenance_spans(source)
        .any(|span| span.path == path && start_utf8 >= span.start_utf8 && end_utf8 <= span.end_utf8)
}

fn insert_unmatched_inline_events(
    events: &mut Vec<RenderEventEnvelope>,
    executed: Vec<RenderEventEnvelope>,
) {
    for event in executed {
        let Some((path, start_utf8, end_utf8)) = event_anchor(&event) else {
            continue;
        };
        events.retain(|existing| {
            !matches!(existing.event, RenderEvent::RawFallback(_))
                || !event_starts_at(existing, &path, start_utf8)
        });
        let insertion = events
            .iter()
            .position(|existing| {
                event_anchor(existing).is_some_and(|(existing_path, existing_start, _)| {
                    existing_path == path
                        && (existing_start > start_utf8
                            || (existing_start == start_utf8 && event_end(existing) >= end_utf8))
                })
            })
            .unwrap_or(events.len());
        events.insert(insertion, event);
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
    if matches!(
        event.event,
        RenderEvent::InlineCitation(_) | RenderEvent::InlineReference(_)
    ) && let Some(span) = event.meta.source.related.iter().find_map(|related| {
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

fn event_end(event: &RenderEventEnvelope) -> u32 {
    event_anchor(event).map_or(0, |(_, _, end)| end)
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

fn push_text_fragment(
    events: &mut Vec<RenderEventEnvelope>,
    template: &RenderEventEnvelope,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    let mut event = template.clone();
    event.event = RenderEvent::Text(tex_render_model::TextEvent {
        text: text.to_string(),
    });
    events.push(event);
}
