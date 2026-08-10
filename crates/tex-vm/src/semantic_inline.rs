use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    CaptionInlinePlaceholderEvent, EventProducer, EventSequence, InlineCitationEvent,
    InlineLinkEvent, InlineReferenceEvent, LabelDefinitionEvent, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SemanticConfidence, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{ControlSequenceId, Token};

use crate::{
    Vm, citation_style_hint_for_command,
    input::QueueItem,
    snapshot::{
        VmActiveLinkCaptureSnapshot, VmExecutedInlineEventMarkSnapshot, VmExecutionAnchor,
        VmSemanticInlineSnapshot, VmSuppressedSourceRangeSnapshot,
    },
};

#[derive(Debug, Default)]
pub(super) struct SemanticInlineState {
    scanner_citation_event_ids: HashSet<EventSequence>,
    scanner_reference_event_ids: HashSet<EventSequence>,
    scanner_link_event_ids: HashSet<EventSequence>,
    scanner_label_event_ids: HashSet<EventSequence>,
    executed_citations: Vec<RenderEventEnvelope>,
    executed_references: Vec<RenderEventEnvelope>,
    executed_links: Vec<RenderEventEnvelope>,
    executed_labels: Vec<RenderEventEnvelope>,
    overridden_label_invocations: Vec<LabelInvocationRange>,
    caption_placeholders: Vec<CaptionInlinePlaceholderEvent>,
    link_marker_actions: HashMap<ControlSequenceId, ExecutedLinkCapture>,
    next_link_marker_id: u64,
}

#[derive(Debug, Clone)]
struct LabelInvocationRange {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
    execution_anchor: VmExecutionAnchor,
}

#[derive(Debug)]
struct ExecutedLinkCapture {
    marker_id: u64,
    command: String,
    target: String,
    source: SourceProvenance,
    producer: EventProducer,
    text_prefix: String,
    output_start: usize,
    text_event_mark: usize,
    citation_event_mark: usize,
    reference_event_mark: usize,
    link_event_mark: usize,
    math_event_mark: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ExecutedInlineEventMark {
    citations: usize,
    references: usize,
    links: usize,
    labels: usize,
    caption_placeholders: usize,
}

impl ExecutedInlineEventMark {
    pub(super) fn snapshot(self) -> VmExecutedInlineEventMarkSnapshot {
        VmExecutedInlineEventMarkSnapshot {
            citations: self.citations.try_into().unwrap_or(u64::MAX),
            references: self.references.try_into().unwrap_or(u64::MAX),
            links: self.links.try_into().unwrap_or(u64::MAX),
            labels: self.labels.try_into().unwrap_or(u64::MAX),
            caption_placeholders: self.caption_placeholders.try_into().unwrap_or(u64::MAX),
        }
    }

    pub(super) fn restore(snapshot: &VmExecutedInlineEventMarkSnapshot) -> Self {
        Self {
            citations: snapshot
                .citations
                .try_into()
                .expect("validated citation event mark"),
            references: snapshot
                .references
                .try_into()
                .expect("validated reference event mark"),
            links: snapshot
                .links
                .try_into()
                .expect("validated link event mark"),
            labels: snapshot
                .labels
                .try_into()
                .expect("validated label event mark"),
            caption_placeholders: snapshot
                .caption_placeholders
                .try_into()
                .expect("validated caption placeholder mark"),
        }
    }
}

impl Vm<'_> {
    pub(super) fn semantic_inline_snapshot(&self) -> VmSemanticInlineSnapshot {
        let sorted_event_ids = |event_ids: &HashSet<EventSequence>| {
            let mut event_ids = event_ids.iter().copied().collect::<Vec<_>>();
            event_ids.sort_unstable();
            event_ids
        };
        let mut active_link_actions = self
            .semantic_inline
            .link_marker_actions
            .iter()
            .map(|(name, capture)| {
                let mut visible_output_prefix = capture.text_prefix.clone();
                visible_output_prefix.push_str(
                    self.output
                        .get(capture.output_start..)
                        .expect("active link output cursor must be valid"),
                );
                VmActiveLinkCaptureSnapshot {
                    control_sequence: self.interner.resolve(*name).unwrap_or("").to_string(),
                    marker_id: capture.marker_id,
                    command: capture.command.clone(),
                    target: capture.target.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    visible_output_prefix,
                    text_event_mark: capture.text_event_mark.try_into().unwrap_or(u64::MAX),
                    citation_event_mark: capture.citation_event_mark.try_into().unwrap_or(u64::MAX),
                    reference_event_mark: capture
                        .reference_event_mark
                        .try_into()
                        .unwrap_or(u64::MAX),
                    link_event_mark: capture.link_event_mark.try_into().unwrap_or(u64::MAX),
                    math_event_mark: capture.math_event_mark.try_into().unwrap_or(u64::MAX),
                }
            })
            .collect::<Vec<_>>();
        active_link_actions
            .sort_by(|left, right| left.control_sequence.cmp(&right.control_sequence));
        VmSemanticInlineSnapshot {
            scanner_citation_event_ids: sorted_event_ids(
                &self.semantic_inline.scanner_citation_event_ids,
            ),
            scanner_reference_event_ids: sorted_event_ids(
                &self.semantic_inline.scanner_reference_event_ids,
            ),
            scanner_link_event_ids: sorted_event_ids(&self.semantic_inline.scanner_link_event_ids),
            scanner_label_event_ids: sorted_event_ids(
                &self.semantic_inline.scanner_label_event_ids,
            ),
            executed_citations: self.semantic_inline.executed_citations.clone(),
            executed_references: self.semantic_inline.executed_references.clone(),
            executed_links: self.semantic_inline.executed_links.clone(),
            executed_labels: self.semantic_inline.executed_labels.clone(),
            overridden_label_invocations: self
                .semantic_inline
                .overridden_label_invocations
                .iter()
                .map(|invocation| VmSuppressedSourceRangeSnapshot {
                    path: invocation.path.clone(),
                    start_utf8: invocation.start_utf8,
                    end_utf8: invocation.end_utf8,
                    execution_anchor: invocation.execution_anchor.clone(),
                })
                .collect(),
            caption_placeholders: self.semantic_inline.caption_placeholders.clone(),
            active_link_actions,
            next_link_marker_id: self.semantic_inline.next_link_marker_id,
        }
    }

    pub(super) fn restore_semantic_inline_snapshot(&mut self, snapshot: &VmSemanticInlineSnapshot) {
        self.semantic_inline.scanner_citation_event_ids = snapshot
            .scanner_citation_event_ids
            .iter()
            .copied()
            .collect();
        self.semantic_inline.scanner_reference_event_ids = snapshot
            .scanner_reference_event_ids
            .iter()
            .copied()
            .collect();
        self.semantic_inline.scanner_link_event_ids =
            snapshot.scanner_link_event_ids.iter().copied().collect();
        self.semantic_inline.scanner_label_event_ids =
            snapshot.scanner_label_event_ids.iter().copied().collect();
        self.semantic_inline.executed_citations = snapshot.executed_citations.clone();
        self.semantic_inline.executed_references = snapshot.executed_references.clone();
        self.semantic_inline.executed_links = snapshot.executed_links.clone();
        self.semantic_inline.executed_labels = snapshot.executed_labels.clone();
        self.semantic_inline.overridden_label_invocations = snapshot
            .overridden_label_invocations
            .iter()
            .map(|invocation| LabelInvocationRange {
                path: invocation.path.clone(),
                start_utf8: invocation.start_utf8,
                end_utf8: invocation.end_utf8,
                execution_anchor: invocation.execution_anchor.clone(),
            })
            .collect();
        self.semantic_inline.caption_placeholders = snapshot.caption_placeholders.clone();
        self.semantic_inline.link_marker_actions.clear();
        for capture in &snapshot.active_link_actions {
            let name = self.interner.intern(&capture.control_sequence);
            self.semantic_inline.link_marker_actions.insert(
                name,
                ExecutedLinkCapture {
                    marker_id: capture.marker_id,
                    command: capture.command.clone(),
                    target: capture.target.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    text_prefix: capture.visible_output_prefix.clone(),
                    output_start: self.output.len(),
                    text_event_mark: capture
                        .text_event_mark
                        .try_into()
                        .expect("validated text event mark"),
                    citation_event_mark: capture
                        .citation_event_mark
                        .try_into()
                        .expect("validated citation event mark"),
                    reference_event_mark: capture
                        .reference_event_mark
                        .try_into()
                        .expect("validated reference event mark"),
                    link_event_mark: capture
                        .link_event_mark
                        .try_into()
                        .expect("validated link event mark"),
                    math_event_mark: capture
                        .math_event_mark
                        .try_into()
                        .expect("validated math event mark"),
                },
            );
        }
        self.semantic_inline.next_link_marker_id = snapshot.next_link_marker_id;
    }

    pub(super) fn mark_scanner_citation_event(&mut self, event_id: EventSequence) {
        self.semantic_inline
            .scanner_citation_event_ids
            .insert(event_id);
    }

    pub(super) fn mark_scanner_reference_event(&mut self, event_id: EventSequence) {
        self.semantic_inline
            .scanner_reference_event_ids
            .insert(event_id);
    }

    pub(super) fn mark_scanner_link_event(&mut self, event_id: EventSequence) {
        self.semantic_inline.scanner_link_event_ids.insert(event_id);
    }

    pub(super) fn mark_scanner_label_event(&mut self, event_id: EventSequence) {
        self.semantic_inline
            .scanner_label_event_ids
            .insert(event_id);
    }

    pub(super) fn record_overridden_label_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if command_name != "label"
            || !self.render_event_capture
            || !self.execution_in_document
            || end_utf8 <= start_utf8
        {
            return;
        }
        let path = self.current_execution_source_path();
        let execution_anchor = self.current_execution_anchor();
        self.semantic_inline
            .overridden_label_invocations
            .push(LabelInvocationRange {
                path,
                start_utf8,
                end_utf8,
                execution_anchor,
            });
    }

    pub(super) fn executed_inline_event_mark(&self) -> ExecutedInlineEventMark {
        ExecutedInlineEventMark {
            citations: self.semantic_inline.executed_citations.len(),
            references: self.semantic_inline.executed_references.len(),
            links: self.semantic_inline.executed_links.len(),
            labels: self.semantic_inline.executed_labels.len(),
            caption_placeholders: self.semantic_inline.caption_placeholders.len(),
        }
    }

    pub(super) fn rollback_executed_inline_events(&mut self, mark: ExecutedInlineEventMark) {
        self.semantic_inline
            .executed_citations
            .truncate(mark.citations);
        self.semantic_inline
            .executed_references
            .truncate(mark.references);
        self.semantic_inline.executed_links.truncate(mark.links);
        self.semantic_inline.executed_labels.truncate(mark.labels);
        self.semantic_inline
            .caption_placeholders
            .truncate(mark.caption_placeholders);
    }

    pub(super) fn take_executed_inline_events_since(
        &mut self,
        mark: ExecutedInlineEventMark,
    ) -> Vec<RenderEventEnvelope> {
        let mut events = self
            .semantic_inline
            .executed_citations
            .split_off(mark.citations);
        events.extend(
            self.semantic_inline
                .executed_references
                .split_off(mark.references),
        );
        events.extend(self.semantic_inline.executed_links.split_off(mark.links));
        events.extend(self.semantic_inline.executed_labels.split_off(mark.labels));
        self.semantic_inline
            .caption_placeholders
            .truncate(mark.caption_placeholders);
        events
    }

    pub(super) fn caption_inline_placeholders_since(
        &self,
        mark: ExecutedInlineEventMark,
    ) -> Vec<CaptionInlinePlaceholderEvent> {
        self.semantic_inline.caption_placeholders[mark.caption_placeholders..].to_vec()
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
        let event_id = self.render_events.allocate_event_sequence();
        let citation = InlineCitationEvent {
            keys,
            style_hint: citation_style_hint_for_command(&command),
            command,
        };
        let envelope = RenderEventEnvelope::with_origin(
            event_id,
            RenderEvent::InlineCitation(citation.clone()),
            source,
            producer,
            SemanticConfidence::High,
        );
        self.semantic_inline.executed_citations.push(envelope);
        self.semantic_inline
            .caption_placeholders
            .push(CaptionInlinePlaceholderEvent::Citation(citation));
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
        let event_id = self.render_events.allocate_event_sequence();
        let reference = InlineReferenceEvent { keys, command };
        let envelope = RenderEventEnvelope::with_origin(
            event_id,
            RenderEvent::InlineReference(reference.clone()),
            source,
            producer,
            SemanticConfidence::High,
        );
        self.semantic_inline.executed_references.push(envelope);
        self.semantic_inline
            .caption_placeholders
            .push(CaptionInlinePlaceholderEvent::Reference(reference));
        self.mark_executed_inline_content();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_executed_label(
        &mut self,
        command: String,
        key: String,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        key_start_utf8: u32,
        key_end_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        let path = self.current_execution_source_path();
        let (mut source, producer) =
            self.executed_inline_source(invocation_start_utf8, invocation_end_utf8);
        let invocation_span = if producer == EventProducer::Macro {
            source.primary.clone()
        } else {
            ProvenanceSpan::File(SourceSpan {
                path: path.clone(),
                start_utf8: invocation_start_utf8,
                end_utf8: invocation_end_utf8,
            })
        };
        let key_span = ProvenanceSpan::File(SourceSpan {
            path,
            start_utf8: key_start_utf8,
            end_utf8: key_end_utf8,
        });
        let key_belongs_to_invocation = match (&source.primary, &key_span) {
            (ProvenanceSpan::File(invocation), ProvenanceSpan::File(key)) => {
                invocation.path == key.path
                    && key.start_utf8 >= invocation.start_utf8
                    && key.end_utf8 <= invocation.end_utf8
            }
            _ => false,
        };
        if key_belongs_to_invocation {
            source.primary = key_span;
        }
        source
            .related
            .retain(|related| related.role != SourceSpanRole::Invocation);
        source = source.with_related(SourceSpanRole::Invocation, invocation_span);

        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::with_origin(
            event_id,
            RenderEvent::LabelDefinition(LabelDefinitionEvent { key, command }),
            source,
            producer,
            SemanticConfidence::High,
        );
        self.semantic_inline.executed_labels.push(envelope);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn queue_executed_link(
        &mut self,
        command: String,
        target: String,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        target_start_utf8: u32,
        target_end_utf8: u32,
        visible_start_utf8: u32,
        visible_end_utf8: u32,
        visible_tokens: Vec<Token>,
        queue: &mut VecDeque<QueueItem>,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            for token in visible_tokens.into_iter().rev() {
                self.push_token_front(queue, token);
            }
            return;
        }

        self.flush_executed_text_capture();
        let (mut source, producer) =
            self.executed_inline_source(visible_start_utf8, visible_end_utf8);
        let path = self.current_execution_source_path();
        source.related.retain(|related| {
            related.role != SourceSpanRole::Invocation
                || !matches!(
                    &related.span,
                    ProvenanceSpan::File(span)
                        if span.path == path
                            && span.start_utf8 == visible_start_utf8
                            && span.end_utf8 == visible_end_utf8
                )
        });
        source = source
            .with_related(
                SourceSpanRole::ArgumentContent,
                ProvenanceSpan::File(SourceSpan {
                    path: path.clone(),
                    start_utf8: visible_start_utf8,
                    end_utf8: visible_end_utf8,
                }),
            )
            .with_related(
                SourceSpanRole::Invocation,
                ProvenanceSpan::File(SourceSpan {
                    path: path.clone(),
                    start_utf8: invocation_start_utf8,
                    end_utf8: invocation_end_utf8,
                }),
            )
            .with_related(
                SourceSpanRole::Argument,
                ProvenanceSpan::File(SourceSpan {
                    path,
                    start_utf8: target_start_utf8,
                    end_utf8: target_end_utf8,
                }),
            );
        let marker_id = self.semantic_inline.next_link_marker_id;
        self.semantic_inline.next_link_marker_id += 1;
        let marker_name = self
            .interner
            .intern(&format!("latexd@semantic@link@end@{marker_id}"));
        let text_event_mark = self.executed_text_event_mark();
        let citation_event_mark = self.semantic_inline.executed_citations.len();
        let reference_event_mark = self.semantic_inline.executed_references.len();
        let link_event_mark = self.semantic_inline.executed_links.len();
        let math_event_mark = self.executed_math_events.len();
        self.semantic_inline.link_marker_actions.insert(
            marker_name,
            ExecutedLinkCapture {
                marker_id,
                command,
                target,
                source,
                producer,
                text_prefix: String::new(),
                output_start: self.output.len(),
                text_event_mark,
                citation_event_mark,
                reference_event_mark,
                link_event_mark,
                math_event_mark,
            },
        );

        self.push_token_front(
            queue,
            Token::control_sequence(
                marker_name,
                invocation_start_utf8 as usize,
                invocation_end_utf8 as usize,
            ),
        );
        for token in visible_tokens.into_iter().rev() {
            self.push_token_front(queue, token);
        }
    }

    pub(super) fn execute_semantic_link_marker(&mut self, name: ControlSequenceId) -> bool {
        let Some(capture) = self.semantic_inline.link_marker_actions.remove(&name) else {
            return false;
        };
        self.flush_executed_text_capture();
        let mut text = capture.text_prefix;
        text.push_str(self.output.get(capture.output_start..).unwrap_or_default());
        self.rollback_executed_text_events(capture.text_event_mark);
        self.semantic_inline
            .executed_citations
            .truncate(capture.citation_event_mark);
        self.semantic_inline
            .executed_references
            .truncate(capture.reference_event_mark);
        self.semantic_inline
            .executed_links
            .truncate(capture.link_event_mark);
        self.executed_math_events.truncate(capture.math_event_mark);

        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::with_origin(
            event_id,
            RenderEvent::InlineLink(InlineLinkEvent {
                target: capture.target,
                text,
                command: capture.command,
            }),
            capture.source,
            capture.producer,
            SemanticConfidence::High,
        );
        self.semantic_inline.executed_links.push(envelope);
        self.mark_executed_inline_content();
        true
    }

    pub(super) fn reconcile_executed_inline_events(&mut self) {
        let citation_ids = mem::take(&mut self.semantic_inline.scanner_citation_event_ids);
        let citations = mem::take(&mut self.semantic_inline.executed_citations);
        self.reconcile_scanner_inline_events(citation_ids, citations, &[]);

        let reference_ids = mem::take(&mut self.semantic_inline.scanner_reference_event_ids);
        let references = mem::take(&mut self.semantic_inline.executed_references);
        self.reconcile_scanner_inline_events(reference_ids, references, &[]);

        let link_ids = mem::take(&mut self.semantic_inline.scanner_link_event_ids);
        let links = mem::take(&mut self.semantic_inline.executed_links);
        self.reconcile_scanner_inline_events(link_ids, links, &[]);

        let label_ids = mem::take(&mut self.semantic_inline.scanner_label_event_ids);
        let labels = mem::take(&mut self.semantic_inline.executed_labels);
        let overridden_labels = mem::take(&mut self.semantic_inline.overridden_label_invocations);
        self.reconcile_scanner_inline_events(label_ids, labels, &overridden_labels);
        self.semantic_inline.caption_placeholders.clear();
    }

    fn reconcile_scanner_inline_events(
        &mut self,
        scanner_ids: HashSet<EventSequence>,
        mut executed: Vec<RenderEventEnvelope>,
        overridden_label_invocations: &[LabelInvocationRange],
    ) {
        if scanner_ids.is_empty() && executed.is_empty() && overridden_label_invocations.is_empty()
        {
            return;
        }

        let mut reconciled = Vec::with_capacity(self.render_events.len() + executed.len());
        let scanner_events = self.render_events.take_events();
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.sequence) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed.iter().position(|candidate| {
                inline_payload_matches(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
            });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                executed_event.meta.sequence = scanner_event.meta.sequence;
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                if !source
                    .related
                    .iter()
                    .any(|related| related.role == SourceSpanRole::Invocation)
                {
                    source.related.extend(
                        executed_source
                            .related
                            .iter()
                            .filter(|related| related.role == SourceSpanRole::Invocation)
                            .cloned(),
                    );
                }
                if !executed_source.expansion_stack.is_empty()
                    && matches!(executed_event.event, RenderEvent::LabelDefinition(_))
                {
                    let scanner_expansion_stack = mem::take(&mut source.expansion_stack);
                    source.expansion_stack = executed_source.expansion_stack;
                    for frame in &mut source.expansion_stack {
                        if frame.definition_span.is_none()
                            && let Some(scanner_frame) =
                                scanner_expansion_stack.iter().find(|scanner_frame| {
                                    scanner_frame.command_name == frame.command_name
                                })
                        {
                            frame.definition_span = scanner_frame.definition_span.clone();
                        }
                    }
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                } else if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            } else if !self.semantic_source_is_suppressed(&scanner_event.meta.source)
                && !provenance_overlaps_label_invocation(
                    &scanner_event.meta.source,
                    overridden_label_invocations,
                )
            {
                reconciled.push(scanner_event);
            }
        }

        executed.retain(|event| !self.semantic_source_is_suppressed(&event.meta.source));
        insert_unmatched_inline_events(&mut reconciled, executed);
        self.render_events.replace_events(reconciled);
    }

    pub(super) fn reconcile_embedded_executed_inline_events(&mut self) {
        let mut executed = Vec::new();
        let mut events = Vec::with_capacity(self.render_events.len());
        let mut footnote_depth = 0usize;
        for event in self.render_events.drain(..) {
            if matches!(event.event, RenderEvent::BeginFootnote(_)) {
                footnote_depth += 1;
                events.push(event);
                continue;
            }
            if matches!(event.event, RenderEvent::EndFootnote(_)) {
                footnote_depth = footnote_depth.saturating_sub(1);
                events.push(event);
                continue;
            }
            if matches!(
                event.event,
                RenderEvent::InlineCitation(_)
                    | RenderEvent::InlineReference(_)
                    | RenderEvent::InlineLink(_)
            ) && matches!(
                event.meta.producer,
                EventProducer::Primitive | EventProducer::Macro
            ) && footnote_depth == 0
            {
                executed.push(event);
            } else {
                events.push(event);
            }
        }
        self.replace_embedded_inline_placeholders(&mut events, &mut executed);
        executed.retain(|event| !recovery_container_represents(&events, event));
        insert_unmatched_inline_events(&mut events, executed);
        let first_event_id = self.render_events.batch_start_event_sequence();
        let mut next_batch_event_id = first_event_id;
        for event in &mut events {
            if event.meta.sequence < first_event_id {
                continue;
            }
            event.meta.sequence = next_batch_event_id;
            next_batch_event_id = next_batch_event_id.saturating_add(1);
        }
        let next_event_sequence = self
            .render_events
            .next_event_sequence()
            .max(next_batch_event_id);
        self.render_events
            .set_next_event_sequence(next_event_sequence);
        self.render_events.replace_events(events);
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
                let Some((event_path, event_start, event_end)) = embedded_event_anchor(event)
                else {
                    continue;
                };
                if event_path != path || event_start < start_utf8 || event_end > end_utf8 {
                    continue;
                }
                let prefix_end = (event_start - start_utf8) as usize;
                let replacement = match &event.event {
                    RenderEvent::InlineCitation(_) | RenderEvent::InlineReference(_) => {
                        let placeholder_ordinal =
                            crate::caption_inline_placeholders(&raw_source[..prefix_end]).len();
                        placeholder_offsets
                            .get(placeholder_ordinal)
                            .copied()
                            .map(|offset| (offset, "[?]".len()))
                    }
                    RenderEvent::InlineLink(link) => {
                        let visible_prefix = crate::normalize_latex_text_with_inline_placeholders(
                            &raw_source[..prefix_end],
                        );
                        let search_start = visible_prefix.len().min(text.text.len());
                        text.text
                            .get(search_start..)
                            .and_then(|suffix| suffix.find(&link.text))
                            .map(|relative| (search_start + relative, link.text.len()))
                            .or_else(|| {
                                text.text
                                    .rfind(&link.text)
                                    .map(|offset| (offset, link.text.len()))
                            })
                    }
                    _ => None,
                };
                let Some((replacement_offset, replacement_len)) = replacement else {
                    continue;
                };
                if !replacements.contains_key(&replacement_offset) {
                    replacements.insert(replacement_offset, (replacement_len, event.clone()));
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
            for (replacement_offset, (replacement_len, event)) in replacements {
                push_text_fragment(
                    &mut reconciled,
                    &scanner_event,
                    &text.text[cursor..replacement_offset],
                );
                reconciled.push(event);
                cursor = replacement_offset + replacement_len;
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
        (RenderEvent::InlineLink(left), RenderEvent::InlineLink(right)) => {
            left.target == right.target && left.text == right.text && left.command == right.command
        }
        (RenderEvent::LabelDefinition(left), RenderEvent::LabelDefinition(right)) => {
            left.key == right.key && left.command == right.command
        }
        _ => false,
    }
}

fn recovery_container_represents(
    events: &[RenderEventEnvelope],
    inline_event: &RenderEventEnvelope,
) -> bool {
    events.iter().any(|event| {
        (matches!(
            event.meta.producer,
            EventProducer::ScannerRecovery | EventProducer::Fallback
        ) || matches!(event.event, RenderEvent::InlineLink(_)))
            && event_anchor_is_contained_by(inline_event, &event.meta.source)
            && match &event.event {
                RenderEvent::Text(text) => {
                    inline_container_text_represents(&text.text, inline_event)
                }
                RenderEvent::Heading(heading) => {
                    inline_container_text_represents(&heading.text, inline_event)
                }
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

fn provenance_overlaps_label_invocation(
    source: &SourceProvenance,
    invocations: &[LabelInvocationRange],
) -> bool {
    provenance_spans(source).any(|span| {
        invocations.iter().any(|invocation| {
            span.path == invocation.path
                && span.start_utf8 < invocation.end_utf8
                && invocation.start_utf8 < span.end_utf8
        })
    })
}

fn inline_container_text_represents(text: &str, inline_event: &RenderEventEnvelope) -> bool {
    match &inline_event.event {
        RenderEvent::InlineLink(link) => text.contains(&link.text),
        RenderEvent::InlineCitation(_) | RenderEvent::InlineReference(_) => text.contains("[?]"),
        _ => false,
    }
}

fn embedded_event_anchor(event: &RenderEventEnvelope) -> Option<(Utf8PathBuf, u32, u32)> {
    if matches!(event.event, RenderEvent::InlineLink(_))
        && let Some(span) = event.meta.source.related.iter().find_map(|related| {
            (related.role == SourceSpanRole::ArgumentContent)
                .then_some(&related.span)
                .and_then(|span| match span {
                    ProvenanceSpan::File(span) => Some(span),
                    ProvenanceSpan::Generated(_) => None,
                })
        })
    {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    if matches!(event.event, RenderEvent::InlineLink(_))
        && let ProvenanceSpan::File(span) = &event.meta.source.primary
    {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    event_anchor(event)
}

fn event_anchor(event: &RenderEventEnvelope) -> Option<(Utf8PathBuf, u32, u32)> {
    if event.meta.producer == EventProducer::Macro
        && matches!(event.event, RenderEvent::InlineLink(_))
        && let Some(ProvenanceSpan::File(span)) = event
            .meta
            .source
            .expansion_stack
            .last()
            .map(|frame| &frame.call_span)
    {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    if matches!(
        event.event,
        RenderEvent::InlineCitation(_)
            | RenderEvent::InlineReference(_)
            | RenderEvent::InlineLink(_)
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
