use std::{
    collections::{HashMap, HashSet},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    BibliographyItemEvent, EventId, EventProducer, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SemanticConfidence, SourceProvenance, SourceSpan, SourceSpanRole,
};

use crate::{
    Vm,
    semantic_transaction::ExecutedSemanticEventMark,
    snapshot::{
        VmActiveBibliographyCaptureSnapshot, VmBibliographyNestedSemanticSnapshot,
        VmEventExecutionAnchorSnapshot, VmExecutionAnchor, VmSemanticBibliographySnapshot,
    },
};

#[derive(Debug, Default)]
pub(super) struct SemanticBibliographyState {
    scanner_event_ids: HashSet<EventId>,
    scanner_event_anchors: HashMap<EventId, VmExecutionAnchor>,
    executed_events: Vec<RenderEventEnvelope>,
    executed_event_anchors: HashMap<EventId, VmExecutionAnchor>,
    environment: BibliographyEnvironmentFrame,
}

#[derive(Debug, Default)]
struct BibliographyEnvironmentFrame {
    depth: usize,
    active_item: Option<BibliographyItemTransaction>,
}

#[derive(Debug)]
struct BibliographyItemTransaction {
    key: String,
    label_hint: Option<String>,
    source: SourceProvenance,
    producer: EventProducer,
    execution_anchor: VmExecutionAnchor,
    visible_output_prefix: String,
    output_start: usize,
    lossy_prefix: bool,
    diagnostic_mark: usize,
    event_mark: ExecutedSemanticEventMark,
    nested_semantics: VmBibliographyNestedSemanticSnapshot,
}

impl Vm<'_> {
    pub(super) fn semantic_bibliography_snapshot(&self) -> VmSemanticBibliographySnapshot {
        let mut scanner_event_ids = self
            .semantic_bibliography
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        let mut scanner_event_anchors = self
            .semantic_bibliography
            .scanner_event_anchors
            .iter()
            .map(
                |(event_id, execution_anchor)| VmEventExecutionAnchorSnapshot {
                    event_id: *event_id,
                    execution_anchor: execution_anchor.clone(),
                },
            )
            .collect::<Vec<_>>();
        scanner_event_anchors.sort_by_key(|anchor| anchor.event_id);
        let mut executed_event_anchors = self
            .semantic_bibliography
            .executed_event_anchors
            .iter()
            .map(
                |(event_id, execution_anchor)| VmEventExecutionAnchorSnapshot {
                    event_id: *event_id,
                    execution_anchor: execution_anchor.clone(),
                },
            )
            .collect::<Vec<_>>();
        executed_event_anchors.sort_by_key(|anchor| anchor.event_id);
        let active_item = self
            .semantic_bibliography
            .environment
            .active_item
            .as_ref()
            .map(|capture| {
                let mut visible_output_prefix = capture.visible_output_prefix.clone();
                visible_output_prefix.push_str(
                    self.output
                        .get(capture.output_start..)
                        .expect("active bibliography output cursor must be valid"),
                );
                VmActiveBibliographyCaptureSnapshot {
                    key: capture.key.clone(),
                    label_hint: capture.label_hint.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    execution_anchor: capture.execution_anchor.clone(),
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
                    nested_semantics: capture.nested_semantics.clone(),
                }
            });
        VmSemanticBibliographySnapshot {
            scanner_event_ids,
            scanner_event_anchors,
            executed_events: self.semantic_bibliography.executed_events.clone(),
            executed_event_anchors,
            environment_depth: self
                .semantic_bibliography
                .environment
                .depth
                .try_into()
                .unwrap_or(u64::MAX),
            active_item,
        }
    }

    pub(super) fn restore_semantic_bibliography_snapshot(
        &mut self,
        snapshot: &VmSemanticBibliographySnapshot,
    ) {
        self.semantic_bibliography.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_bibliography.scanner_event_anchors = snapshot
            .scanner_event_anchors
            .iter()
            .map(|anchor| (anchor.event_id, anchor.execution_anchor.clone()))
            .collect();
        self.semantic_bibliography.executed_events = snapshot.executed_events.clone();
        self.semantic_bibliography.executed_event_anchors = snapshot
            .executed_event_anchors
            .iter()
            .map(|anchor| (anchor.event_id, anchor.execution_anchor.clone()))
            .collect();
        self.semantic_bibliography.environment.depth = snapshot
            .environment_depth
            .try_into()
            .expect("validated bibliography environment depth");
        self.semantic_bibliography.environment.active_item =
            snapshot
                .active_item
                .as_ref()
                .map(|capture| BibliographyItemTransaction {
                    key: capture.key.clone(),
                    label_hint: capture.label_hint.clone(),
                    source: capture.source.clone(),
                    producer: capture.producer,
                    execution_anchor: capture.execution_anchor.clone(),
                    visible_output_prefix: capture.visible_output_prefix.clone(),
                    output_start: self.output.len(),
                    lossy_prefix: capture.lossy_before_restore,
                    diagnostic_mark: self.diagnostics.len(),
                    event_mark: ExecutedSemanticEventMark::from_parts(
                        capture
                            .text_event_mark
                            .try_into()
                            .expect("validated bibliography text event mark"),
                        crate::semantic_inline::ExecutedInlineEventMark::restore(
                            &capture.inline_event_mark,
                        ),
                        capture
                            .math_event_mark
                            .try_into()
                            .expect("validated bibliography math event mark"),
                    ),
                    nested_semantics: capture.nested_semantics.clone(),
                });
    }

    pub(super) fn begin_executed_bibliography_environment(&mut self) {
        if self.render_event_capture && self.execution_in_document {
            self.semantic_bibliography.environment.depth += 1;
        }
    }

    pub(super) fn end_executed_bibliography_environment(&mut self, end_utf8: u32) {
        self.finish_executed_bibliography_item();
        self.close_executed_text_authority(end_utf8);
        self.semantic_bibliography.environment.depth = self
            .semantic_bibliography
            .environment
            .depth
            .saturating_sub(1);
    }

    pub(super) fn finish_executed_bibliography_document(&mut self, end_utf8: u32) {
        self.finish_executed_bibliography_item();
        self.close_executed_text_authority(end_utf8);
        self.semantic_bibliography.environment.depth = 0;
    }

    pub(super) fn mark_scanner_bibliography_event(&mut self, event_id: EventId) {
        let execution_anchor = self.current_scanner_execution_anchor();
        self.semantic_bibliography
            .scanner_event_ids
            .insert(event_id);
        self.semantic_bibliography
            .scanner_event_anchors
            .insert(event_id, execution_anchor);
    }

    pub(super) fn record_overridden_bibliography_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if command_name == "bibitem" {
            self.record_suppressed_source_range(start_utf8, end_utf8);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_executed_bibliography_item(
        &mut self,
        key: String,
        label_hint: Option<String>,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        key_start_utf8: u32,
        key_end_utf8: u32,
        lossy_label: bool,
    ) {
        self.finish_executed_bibliography_item();
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        if self.semantic_bibliography.environment.depth == 0 {
            self.record_suppressed_source_range(invocation_start_utf8, invocation_end_utf8);
            self.force_executed_text_after(invocation_end_utf8);
            return;
        }

        self.finish_executed_block_content();
        let (mut source, producer) =
            self.executed_semantic_source(invocation_start_utf8, invocation_end_utf8);
        let path = self.current_execution_source_path();
        if producer == EventProducer::Primitive {
            source =
                SourceProvenance::file(path.clone(), invocation_start_utf8, invocation_end_utf8);
        }
        if key_end_utf8 > key_start_utf8 {
            source = source.with_related(
                SourceSpanRole::CitationKey,
                ProvenanceSpan::File(SourceSpan {
                    path,
                    start_utf8: key_start_utf8,
                    end_utf8: key_end_utf8,
                }),
            );
        }

        let nested_semantics = self.capture_bibliography_nested_semantics();
        let execution_anchor = self.current_execution_anchor();
        self.semantic_bibliography.environment.active_item = Some(BibliographyItemTransaction {
            key,
            label_hint,
            source,
            producer,
            execution_anchor,
            visible_output_prefix: String::new(),
            output_start: self.output.len(),
            lossy_prefix: lossy_label,
            diagnostic_mark: self.diagnostics.len(),
            event_mark: self.mark_executed_semantic_events(),
            nested_semantics,
        });
    }

    pub(super) fn finish_executed_bibliography_item(&mut self) {
        let Some(capture) = self.semantic_bibliography.environment.active_item.take() else {
            return;
        };

        self.flush_executed_text_capture();
        let mut raw_text = capture.visible_output_prefix;
        raw_text.push_str(self.output.get(capture.output_start..).unwrap_or_default());
        let raw_text = raw_text.replace('[', "\u{e000}").replace(']', "\u{e001}");
        let text = crate::normalize_latex_text_with_inline_placeholders(&raw_text)
            .replace('\u{e000}', "[")
            .replace('\u{e001}', "]");

        let nested_projection_loss =
            self.capture_bibliography_nested_semantics() != capture.nested_semantics;
        let event_projection_loss = self.rollback_executed_semantic_events(capture.event_mark);
        self.restore_bibliography_nested_semantics(&capture.nested_semantics);
        self.finish_executed_block_content();

        let event_id = self.render_events.allocate_event_id();
        let execution_anchor = capture.execution_anchor;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::BibliographyItem(BibliographyItemEvent {
                key: capture.key,
                label_hint: capture.label_hint,
                text,
            }),
            capture.source,
        );
        let execution_is_lossy =
            capture.lossy_prefix || self.diagnostics.len() > capture.diagnostic_mark;
        let projection_is_lossy = event_projection_loss.is_lossy() || nested_projection_loss;
        if execution_is_lossy {
            envelope.meta.producer = EventProducer::Fallback;
            envelope.meta.confidence = SemanticConfidence::Low;
        } else {
            envelope.meta.producer = capture.producer;
            if projection_is_lossy {
                // VM execution is authoritative even when this string-only node loses structure.
                envelope.meta.confidence = SemanticConfidence::Low;
            }
        }
        self.semantic_bibliography.executed_events.push(envelope);
        self.semantic_bibliography
            .executed_event_anchors
            .insert(event_id, execution_anchor);
    }

    pub(super) fn reconcile_executed_bibliography_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_bibliography.scanner_event_ids);
        let scanner_event_anchors =
            mem::take(&mut self.semantic_bibliography.scanner_event_anchors);
        let mut executed = mem::take(&mut self.semantic_bibliography.executed_events);
        let mut executed_event_anchors =
            mem::take(&mut self.semantic_bibliography.executed_event_anchors);
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
            let scanner_execution_anchor = scanner_event_anchors.get(&scanner_event.meta.event_id);
            let exact = executed.iter().position(|candidate| {
                candidate.meta.producer != EventProducer::Fallback
                    && executed_event_anchors.get(&candidate.meta.event_id)
                        == scanner_execution_anchor
                    && bibliography_payloads_match(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
            });
            let compatible = exact.or_else(|| {
                executed.iter().position(|candidate| {
                    candidate.meta.producer != EventProducer::Fallback
                        && executed_event_anchors.get(&candidate.meta.event_id)
                            == scanner_execution_anchor
                        && bibliography_shapes_match(candidate, &scanner_event)
                        && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
                })
            });
            if let Some(index) = compatible {
                let mut executed_event = executed.remove(index);
                executed_event_anchors.remove(&executed_event.meta.event_id);
                let payloads_match = bibliography_payloads_match(&executed_event, &scanner_event);
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
            } else if let Some(index) = executed.iter().position(|candidate| {
                executed_event_anchors.get(&candidate.meta.event_id) == scanner_execution_anchor
                    && bibliography_shapes_match(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
            }) {
                let removed = executed.remove(index);
                executed_event_anchors.remove(&removed.meta.event_id);
                if !self.semantic_source_is_suppressed_in_execution(
                    &scanner_event.meta.source,
                    scanner_execution_anchor,
                ) {
                    reconciled.push(scanner_event);
                }
            } else if !self.semantic_source_is_suppressed_in_execution(
                &scanner_event.meta.source,
                scanner_execution_anchor,
            ) {
                reconciled.push(scanner_event);
            }
        }

        insert_unmatched_bibliography_events(
            &mut reconciled,
            executed,
            &scanner_event_anchors,
            &executed_event_anchors,
        );
        self.render_events.replace_events(reconciled);
    }

    fn capture_bibliography_nested_semantics(&self) -> VmBibliographyNestedSemanticSnapshot {
        VmBibliographyNestedSemanticSnapshot {
            caption: self.semantic_caption_snapshot(),
            environment: self.semantic_environment_snapshot(),
            footnote: self.semantic_footnote_snapshot(),
            front_matter: self.semantic_front_matter_snapshot(),
            graphic: self.semantic_graphic_snapshot(),
            heading: self.semantic_heading_snapshot(),
            list: self.semantic_list_snapshot(),
            table: self.semantic_table_snapshot(),
        }
    }

    fn restore_bibliography_nested_semantics(
        &mut self,
        snapshot: &VmBibliographyNestedSemanticSnapshot,
    ) {
        self.restore_semantic_caption_snapshot(&snapshot.caption);
        self.restore_semantic_environment_snapshot(&snapshot.environment);
        self.restore_semantic_footnote_snapshot(&snapshot.footnote);
        self.restore_semantic_front_matter_snapshot(&snapshot.front_matter);
        self.restore_semantic_graphic_snapshot(&snapshot.graphic);
        self.restore_semantic_heading_snapshot(&snapshot.heading);
        self.restore_semantic_list_snapshot(&snapshot.list);
        self.restore_semantic_table_snapshot(&snapshot.table);
    }
}

fn bibliography_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::BibliographyItem(left), RenderEvent::BibliographyItem(right))
            if left == right
    )
}

fn bibliography_shapes_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::BibliographyItem(left), RenderEvent::BibliographyItem(right))
            if left.key == right.key
    )
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

fn insert_unmatched_bibliography_events(
    events: &mut Vec<RenderEventEnvelope>,
    executed: Vec<RenderEventEnvelope>,
    scanner_event_anchors: &HashMap<EventId, VmExecutionAnchor>,
    executed_event_anchors: &HashMap<EventId, VmExecutionAnchor>,
) {
    for event in executed {
        let Some((path, start_utf8, end_utf8)) = event_anchor(&event) else {
            continue;
        };
        let execution_anchor = executed_event_anchors.get(&event.meta.event_id);
        let insertion = events
            .iter()
            .position(|existing| {
                scanner_event_anchors.get(&existing.meta.event_id) == execution_anchor
                    && event_anchor(existing).is_some_and(
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
            .or_else(|| {
                events.iter().position(|existing| {
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
    match &event.meta.source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8, span.end_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}
