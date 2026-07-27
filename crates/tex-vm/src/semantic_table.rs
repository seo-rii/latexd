use std::{collections::HashSet, mem};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventId, EventProducer, GeneratedBy, ProvenanceSpan, RenderEvent, RenderEventEnvelope,
    SemanticConfidence, SourceProvenance,
};

use crate::Vm;

#[derive(Debug, Default)]
pub(super) struct SemanticTableState {
    scanner_event_ids: HashSet<EventId>,
    open_tables: Vec<ExecutedTableFrame>,
    executed_tables: Vec<ExecutedTable>,
}

#[derive(Debug)]
struct ExecutedTableFrame {
    environment: String,
    source: SourceProvenance,
    producer: EventProducer,
}

#[derive(Debug)]
struct ExecutedTable {
    environment: String,
    source: SourceProvenance,
    producer: EventProducer,
}

impl Vm<'_> {
    pub(super) fn prepare_semantic_table_capture(&mut self) {
        self.semantic_table.scanner_event_ids.clear();
        self.semantic_table.open_tables.clear();
        self.semantic_table.executed_tables.clear();
    }

    pub(super) fn mark_scanner_table_event(&mut self, event_id: EventId) {
        self.semantic_table.scanner_event_ids.insert(event_id);
    }

    pub(super) fn begin_executed_table(
        &mut self,
        environment: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture
            || !self.execution_in_document
            || !is_table_environment(environment)
        {
            return;
        }
        let (source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        self.semantic_table.open_tables.push(ExecutedTableFrame {
            environment: environment.to_string(),
            source,
            producer,
        });
    }

    pub(super) fn end_executed_table(&mut self, environment: &str) {
        if !self.render_event_capture || !is_table_environment(environment) {
            return;
        }
        let Some(index) = self
            .semantic_table
            .open_tables
            .iter()
            .rposition(|frame| frame.environment == environment)
        else {
            return;
        };
        let frame = self.semantic_table.open_tables.remove(index);
        self.semantic_table.executed_tables.push(ExecutedTable {
            environment: frame.environment,
            source: frame.source,
            producer: frame.producer,
        });
    }

    pub(super) fn reconcile_executed_table_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_table.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_table.executed_tables);
        self.semantic_table.open_tables.clear();
        if scanner_ids.is_empty() {
            return;
        }

        for scanner_event in &mut self.render_events {
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
                continue;
            }
            let Some(environment) = table_environment(scanner_event) else {
                continue;
            };
            let matching = executed.iter().position(|candidate| {
                candidate.environment == environment
                    && table_start_anchor(&candidate.source)
                        == table_start_anchor(&scanner_event.meta.source)
            });
            let Some(index) = matching else {
                continue;
            };
            let executed_table = executed.remove(index);
            scanner_event.meta.producer = executed_table.producer;
            scanner_event.meta.confidence = SemanticConfidence::High;
            scanner_event.meta.source.generated_by = GeneratedBy::Source;
            if scanner_event.meta.source.expansion_stack.is_empty() {
                scanner_event.meta.source.expansion_stack = executed_table.source.expansion_stack;
                scanner_event.meta.source.expansion_stack_truncated =
                    executed_table.source.expansion_stack_truncated;
            }
        }
    }
}

pub(super) fn is_table_environment(environment: &str) -> bool {
    matches!(
        environment,
        "array"
            | "tabular"
            | "tabular*"
            | "tabularx"
            | "longtable"
            | "longtable*"
            | "tabu"
            | "longtabu"
    )
}

fn table_environment(event: &RenderEventEnvelope) -> Option<&str> {
    match &event.event {
        RenderEvent::RawFallback(fallback) => fallback.environment.as_deref(),
        RenderEvent::Table(table) => Some(table.environment.as_str()),
        _ => None,
    }
    .filter(|environment| is_table_environment(environment))
}

fn table_start_anchor(source: &SourceProvenance) -> Option<(Utf8PathBuf, u32)> {
    match &source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}
