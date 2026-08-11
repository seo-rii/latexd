use std::{collections::HashSet, mem};

use tex_render_model::{
    EventBuildContext, EventProducer, EventSequence, FlushTitleBlockEvent, MetadataField,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope, SetDocumentMetadataEvent, SourceProvenance,
    SourceSpan, SourceSpanRole,
};
use tex_tokens::Token;

use crate::{
    Vm, author_metadata_ranges,
    command::Meaning,
    normalize_author_metadata, normalize_latex_text_with_inline_placeholders,
    semantic_reconciliation::{call_invocation_primary_anchor, source_locations_overlap},
    semantic_text::event_origin_for_executed_producer,
    snapshot::VmSemanticFrontMatterSnapshot,
};

#[derive(Debug, Default)]
pub(super) struct SemanticFrontMatterState {
    scanner_event_ids: HashSet<EventSequence>,
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

    pub(super) fn mark_scanner_front_matter_event(&mut self, event_id: EventSequence) {
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
        if matches!(
            command_name,
            "title"
                | "author"
                | "date"
                | "affil"
                | "affiliation"
                | "institute"
                | "email"
                | "keywords"
                | "pacs"
                | "maketitle"
                | "icmltitle"
                | "icmlauthor"
                | "icmlaffiliation"
                | "icmlcorrespondingauthor"
                | "icmlkeywords"
                | "printAffiliationsAndNotice"
        ) {
            self.record_suppressed_source_range(start_utf8, end_utf8);
        }
    }

    pub(super) fn record_overridden_front_matter_macro_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
        expansion_is_empty: bool,
    ) {
        self.record_overridden_front_matter_invocation(command_name, start_utf8, end_utf8);
        if !expansion_is_empty && matches!(command_name, "maketitle" | "printAffiliationsAndNotice")
        {
            self.emit_executed_compat_flush_title_block(start_utf8, end_utf8);
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
                    let Some(scope_index) =
                        self.control_sequences.visible_layer_index(command_name)
                    else {
                        continue;
                    };
                    let Some(Meaning::Macro(definition)) =
                        self.control_sequences.get_mut_at(scope_index, command_name)
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
                self.control_sequences.insert_at(
                    scope_index,
                    command_name.to_string(),
                    Meaning::Macro(definition),
                );
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
        self.push_executed_flush_title_block(source, producer);
    }

    fn emit_executed_compat_flush_title_block(&mut self, start_utf8: u32, end_utf8: u32) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        self.finish_executed_block_content();
        let source =
            SourceProvenance::file(self.current_execution_source_path(), start_utf8, end_utf8);
        self.push_executed_flush_title_block(source, EventProducer::Macro);
    }

    fn push_executed_flush_title_block(
        &mut self,
        source: SourceProvenance,
        producer: EventProducer,
    ) {
        if self
            .semantic_front_matter
            .executed_events
            .iter()
            .any(|event| {
                matches!(event.event, RenderEvent::FlushTitleBlock(_))
                    && source_locations_overlap(&event.meta.source, &source)
            })
        {
            return;
        }
        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::try_from_origin(
            RenderEvent::FlushTitleBlock(FlushTitleBlockEvent),
            EventBuildContext::new(event_id, source),
            event_origin_for_executed_producer(producer),
        )
        .expect("executed title flushes use an origin valid for ordinary events");
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
            if !scanner_ids.contains(&scanner_event.meta.sequence) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed
                .iter()
                .position(|candidate| {
                    front_matter_payloads_match(candidate, &scanner_event)
                        && source_locations_overlap(
                            &candidate.meta.source,
                            &scanner_event.meta.source,
                        )
                })
                .or_else(|| {
                    executed.iter().position(|candidate| {
                        front_matter_kinds_match(candidate, &scanner_event)
                            && source_locations_overlap(
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
                executed_event.meta.sequence = scanner_event.meta.sequence;
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
        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::try_from_origin(
            event,
            EventBuildContext::new(event_id, source),
            event_origin_for_executed_producer(producer),
        )
        .expect("executed front matter uses an origin valid for ordinary events");
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

fn insert_unmatched_front_matter_events(
    events: &mut Vec<RenderEventEnvelope>,
    executed: Vec<RenderEventEnvelope>,
) {
    for event in executed {
        let Some((path, start_utf8, end_utf8)) = call_invocation_primary_anchor(&event.meta.source)
        else {
            continue;
        };
        let insertion = events
            .iter()
            .position(|existing| {
                call_invocation_primary_anchor(&existing.meta.source).is_some_and(
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
