use std::{collections::HashSet, mem};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventId, EventProducer, FlushTitleBlockEvent, MetadataField, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SetDocumentMetadataEvent, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::Token;

use crate::{
    Vm, author_metadata_ranges, command::Meaning, normalize_author_metadata,
    normalize_latex_text_with_inline_placeholders, snapshot::VmSemanticFrontMatterSnapshot,
};

#[derive(Debug, Default)]
pub(super) struct SemanticFrontMatterState {
    scanner_event_ids: HashSet<EventId>,
    executed_events: Vec<RenderEventEnvelope>,
}

impl Vm<'_> {
    pub(super) fn semantic_front_matter_snapshot(&self) -> VmSemanticFrontMatterSnapshot {
        let mut scanner_event_ids = self
            .semantic_front_matter
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        VmSemanticFrontMatterSnapshot {
            scanner_event_ids,
            executed_events: self.semantic_front_matter.executed_events.clone(),
        }
    }

    pub(super) fn restore_semantic_front_matter_snapshot(
        &mut self,
        snapshot: &VmSemanticFrontMatterSnapshot,
    ) {
        self.semantic_front_matter.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_front_matter.executed_events = snapshot.executed_events.clone();
    }

    pub(super) fn mark_scanner_front_matter_event(&mut self, event_id: EventId) {
        self.semantic_front_matter
            .scanner_event_ids
            .insert(event_id);
    }

    pub(super) fn record_overridden_front_matter_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if matches!(command_name, "title" | "author" | "date" | "maketitle") {
            self.record_suppressed_source_range(start_utf8, end_utf8);
        }
    }

    pub(super) fn emit_executed_document_metadata(
        &mut self,
        field: MetadataField,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        value_tokens: Vec<Token>,
    ) -> Vec<String> {
        let argument_start_utf8 = value_tokens
            .first()
            .map_or(invocation_end_utf8, |token| token.span.start);
        let tokens_to_source =
            |tokens: &[Token], interner: &tex_tokens::ControlSequenceInterner| {
                let mut source = String::new();
                let mut source_ranges = Vec::with_capacity(tokens.len());
                for token in tokens {
                    let start = source.len();
                    match token.kind {
                        tex_tokens::TokenKind::Character { ch, .. } => source.push(ch),
                        tex_tokens::TokenKind::ControlSequence { name } => {
                            source.push('\\');
                            let name = interner.resolve(name).unwrap_or_default();
                            source.push_str(name);
                            let end = source.len();
                            if name.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '@') {
                                source.push(' ');
                            }
                            source_ranges.push((start, end));
                            continue;
                        }
                    }
                    source_ranges.push((start, source.len()));
                }
                (source, source_ranges)
            };
        let (raw_value, token_source_ranges) = tokens_to_source(&value_tokens, self.interner);
        let ranges = if field == MetadataField::Author {
            author_metadata_ranges(&raw_value)
        } else {
            vec![(0, raw_value.len())]
        };
        let mut visible_values = Vec::new();

        for (value_start, value_end) in ranges {
            let value = raw_value[value_start..value_end].trim();
            if value.is_empty() {
                continue;
            }
            let token_start = token_source_ranges
                .iter()
                .position(|(_, end)| *end > value_start)
                .unwrap_or(value_tokens.len());
            let token_end = token_source_ranges
                .iter()
                .rposition(|(start, _)| *start < value_end)
                .map_or(token_start, |index| index + 1);
            let segment_tokens = value_tokens[token_start..token_end].to_vec();
            let segment_start_utf8 = segment_tokens
                .first()
                .map_or(argument_start_utf8, |token| token.span.start);
            let segment_end_utf8 = segment_tokens
                .last()
                .map_or(segment_start_utf8, |token| token.span.end);
            let mut protected_definitions = Vec::new();
            if field == MetadataField::Author {
                for command_name in ["and", "thanks"] {
                    let Some(scope_index) = self
                        .scopes
                        .iter()
                        .rposition(|scope| scope.contains_key(command_name))
                    else {
                        continue;
                    };
                    let Some(Meaning::Macro(definition)) =
                        self.scopes[scope_index].get_mut(command_name)
                    else {
                        continue;
                    };
                    if definition.flags.protected {
                        continue;
                    }
                    let original = definition.clone();
                    definition.flags.protected = true;
                    protected_definitions.push((scope_index, command_name, original));
                }
            }
            let expanded = self.fully_expand_tokens(segment_tokens);
            for (scope_index, command_name, definition) in protected_definitions {
                self.scopes[scope_index]
                    .insert(command_name.to_string(), Meaning::Macro(definition));
            }
            let (expanded_source, _) = tokens_to_source(&expanded, self.interner);
            let expanded_ranges = if field == MetadataField::Author {
                author_metadata_ranges(&expanded_source)
            } else {
                vec![(0, expanded_source.len())]
            };
            for (expanded_start, expanded_end) in expanded_ranges {
                let expanded_value = expanded_source[expanded_start..expanded_end].trim();
                if expanded_value.is_empty() {
                    continue;
                }
                if field == MetadataField::Author {
                    let (author, notes) = normalize_author_metadata(expanded_value);
                    if !author.is_empty() {
                        visible_values.push(author.clone());
                        if self.render_event_capture {
                            self.push_executed_front_matter_event(
                                RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                                    field,
                                    value: author,
                                }),
                                invocation_start_utf8,
                                invocation_end_utf8,
                                segment_start_utf8,
                                segment_end_utf8,
                            );
                        }
                    }
                    for (note, _, _) in notes {
                        if self.render_event_capture {
                            self.push_executed_front_matter_event(
                                RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                                    field: MetadataField::AuthorNote,
                                    value: note,
                                }),
                                invocation_start_utf8,
                                invocation_end_utf8,
                                segment_start_utf8,
                                segment_end_utf8,
                            );
                        }
                    }
                } else {
                    let value = normalize_latex_text_with_inline_placeholders(expanded_value);
                    if !value.is_empty() {
                        visible_values.push(value.clone());
                        if self.render_event_capture {
                            self.push_executed_front_matter_event(
                                RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                                    field,
                                    value,
                                }),
                                invocation_start_utf8,
                                invocation_end_utf8,
                                segment_start_utf8,
                                segment_end_utf8,
                            );
                        }
                    }
                }
            }
        }
        visible_values
    }

    pub(super) fn emit_executed_flush_title_block(&mut self, start_utf8: u32, end_utf8: u32) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        self.finish_executed_block_content();
        let (source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        let event_id = self.render_events.allocate_event_id();
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::FlushTitleBlock(FlushTitleBlockEvent),
            source,
        );
        envelope.meta.producer = producer;
        self.semantic_front_matter.executed_events.push(envelope);
    }

    pub(super) fn reconcile_executed_front_matter_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_front_matter.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_front_matter.executed_events);
        if scanner_ids.is_empty() && executed.is_empty() {
            return;
        }

        let scanner_events = self.render_events.take_events();
        let mut reconciled = Vec::with_capacity(scanner_events.len() + executed.len());
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed
                .iter()
                .position(|candidate| {
                    front_matter_payloads_match(candidate, &scanner_event)
                        && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
                })
                .or_else(|| {
                    executed.iter().position(|candidate| {
                        front_matter_kinds_match(candidate, &scanner_event)
                            && provenance_overlaps(
                                &candidate.meta.source,
                                &scanner_event.meta.source,
                            )
                    })
                });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                let payloads_match = front_matter_payloads_match(&executed_event, &scanner_event);
                let executed_source = executed_event.meta.source;
                let mut source = if payloads_match {
                    scanner_event.meta.source
                } else {
                    executed_source.clone()
                };
                if source.related.is_empty() {
                    source.related = executed_source.related.clone();
                }
                if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.event_id = scanner_event.meta.event_id;
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            }
        }

        insert_unmatched_front_matter_events(&mut reconciled, executed);
        self.render_events.replace_events(reconciled);
    }

    fn push_executed_front_matter_event(
        &mut self,
        event: RenderEvent,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        value_start_utf8: u32,
        value_end_utf8: u32,
    ) {
        let (mut source, producer) =
            self.executed_semantic_source(value_start_utf8, value_end_utf8);
        if producer == EventProducer::Primitive {
            let path = self.current_execution_source_path();
            source = source
                .with_related(
                    SourceSpanRole::ArgumentContent,
                    ProvenanceSpan::File(SourceSpan {
                        path: path.clone(),
                        start_utf8: value_start_utf8,
                        end_utf8: value_end_utf8,
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
        let event_id = self.render_events.allocate_event_id();
        let mut envelope = RenderEventEnvelope::new(event_id, event, source);
        envelope.meta.producer = producer;
        self.semantic_front_matter.executed_events.push(envelope);
    }
}

fn front_matter_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::SetDocumentMetadata(left), RenderEvent::SetDocumentMetadata(right)) => {
            left.field == right.field && left.value == right.value
        }
        (RenderEvent::FlushTitleBlock(_), RenderEvent::FlushTitleBlock(_)) => true,
        _ => false,
    }
}

fn front_matter_kinds_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::SetDocumentMetadata(left), RenderEvent::SetDocumentMetadata(right)) => {
            left.field == right.field
        }
        (RenderEvent::FlushTitleBlock(_), RenderEvent::FlushTitleBlock(_)) => true,
        _ => false,
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

fn insert_unmatched_front_matter_events(
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
