use std::{collections::HashSet, mem};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventId, ListItemEvent, ListKind, ProvenanceSpan, RenderEvent, RenderEventEnvelope,
    SourceProvenance, SourceSpan,
};

use crate::{Vm, snapshot::VmSemanticListSnapshot};

#[derive(Debug, Default)]
pub(super) struct SemanticListState {
    scanner_item_event_ids: HashSet<EventId>,
    executed_items: Vec<RenderEventEnvelope>,
    active_lists: Vec<ListKind>,
}

impl Vm<'_> {
    pub(super) fn semantic_list_snapshot(&self) -> VmSemanticListSnapshot {
        let mut scanner_item_event_ids = self
            .semantic_list
            .scanner_item_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_item_event_ids.sort_unstable();
        VmSemanticListSnapshot {
            scanner_item_event_ids,
            executed_items: self.semantic_list.executed_items.clone(),
            active_lists: self.semantic_list.active_lists.clone(),
        }
    }

    pub(super) fn restore_semantic_list_snapshot(&mut self, snapshot: &VmSemanticListSnapshot) {
        self.semantic_list.scanner_item_event_ids =
            snapshot.scanner_item_event_ids.iter().copied().collect();
        self.semantic_list.executed_items = snapshot.executed_items.clone();
        self.semantic_list.active_lists = snapshot.active_lists.clone();
    }

    pub(super) fn mark_scanner_list_item_event(&mut self, event_id: EventId) {
        self.semantic_list.scanner_item_event_ids.insert(event_id);
    }

    pub(super) fn begin_executed_list(&mut self, environment: &str) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        if let Some(kind) = list_kind_for_environment(environment) {
            self.semantic_list.active_lists.push(kind);
        }
    }

    pub(super) fn end_executed_list(&mut self, environment: &str) {
        let Some(kind) = list_kind_for_environment(environment) else {
            return;
        };
        let Some(position) = self
            .semantic_list
            .active_lists
            .iter()
            .rposition(|active| *active == kind)
        else {
            return;
        };
        self.semantic_list.active_lists.truncate(position);
    }

    pub(super) fn emit_executed_list_item(
        &mut self,
        marker: Option<String>,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture
            || !self.execution_in_document
            || self.semantic_list.active_lists.is_empty()
        {
            return;
        }

        self.finish_executed_block_content();
        let (source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        let event_id = self.render_events.allocate_event_id();
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            RenderEvent::ListItem(ListItemEvent { marker }),
            source,
        );
        envelope.meta.producer = producer;
        self.semantic_list.executed_items.push(envelope);
    }

    pub(super) fn reconcile_executed_list_events(&mut self) {
        self.semantic_list.active_lists.clear();
        let scanner_ids = mem::take(&mut self.semantic_list.scanner_item_event_ids);
        let mut executed = mem::take(&mut self.semantic_list.executed_items);
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
            let matching = executed.iter().position(|candidate| {
                list_item_payloads_match(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
            });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                executed_event.meta.event_id = scanner_event.meta.event_id;
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            }
        }

        insert_unmatched_list_items(&mut reconciled, executed);
        self.render_events.replace_events(reconciled);
    }
}

pub(super) fn list_kind_for_environment(environment: &str) -> Option<ListKind> {
    match environment {
        "itemize" => Some(ListKind::Unordered),
        "enumerate" => Some(ListKind::Ordered),
        "description" => Some(ListKind::Description),
        _ => None,
    }
}

fn list_item_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::ListItem(left), RenderEvent::ListItem(right))
            if left.marker == right.marker
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

fn insert_unmatched_list_items(
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
    match &event.meta.source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8, span.end_utf8)),
        ProvenanceSpan::Generated(_) => {
            event
                .meta
                .source
                .expansion_stack
                .last()
                .and_then(|frame| match &frame.call_span {
                    ProvenanceSpan::File(span) => {
                        Some((span.path.clone(), span.start_utf8, span.end_utf8))
                    }
                    ProvenanceSpan::Generated(_) => None,
                })
        }
    }
}
