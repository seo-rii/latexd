use std::{collections::HashSet, mem};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventId, EventProducer, GeneratedBy, ProvenanceSpan, RawFallbackEvent, RenderEvent,
    RenderEventEnvelope, SemanticConfidence, SourceProvenance, TableCellEvent, TableEvent,
    TableRowEvent, TableRulePosition,
};

use crate::Vm;

#[derive(Debug, Default)]
pub(super) struct SemanticTableState {
    scanner_event_ids: HashSet<EventId>,
    open_tables: Vec<ExecutedTableFrame>,
    executed_tables: Vec<ExecutedTable>,
    structured_events: bool,
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
    pub fn enable_structured_table_events(&mut self) {
        self.semantic_table.structured_events = true;
    }

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
            if self.semantic_table.structured_events
                && let RenderEvent::RawFallback(fallback) = &scanner_event.event
                && let Some(table) = table_event_from_fallback(fallback)
            {
                scanner_event.event = RenderEvent::Table(table);
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

fn table_event_from_fallback(event: &RawFallbackEvent) -> Option<TableEvent> {
    let environment = event.environment.as_deref()?;
    if !is_table_environment(environment) {
        return None;
    }
    let visible = event
        .normalized_visible_text
        .as_deref()
        .unwrap_or(&event.source_excerpt);
    let split_nested_table_cell_lines = |text: &str| {
        let mut result = String::with_capacity(text.len());
        let mut wrapper_stack = Vec::new();
        let mut index = 0usize;
        while index < text.len() {
            let ch = text[index..].chars().next().expect("table cell char");
            if ch.is_ascii_alphabetic() {
                let identifier_start = index;
                index += ch.len_utf8();
                while index < text.len()
                    && text[index..]
                        .chars()
                        .next()
                        .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_')
                {
                    index += text[index..]
                        .chars()
                        .next()
                        .expect("table cell identifier char")
                        .len_utf8();
                }
                let identifier = &text[identifier_start..index];
                result.push_str(identifier);
                if text.as_bytes().get(index).copied() == Some(b'(') {
                    result.push('(');
                    wrapper_stack.push(matches!(
                        identifier,
                        "array"
                            | "matrix"
                            | "cases"
                            | "subarray"
                            | "aligned"
                            | "split"
                            | "gathered"
                            | "multlined"
                            | "alignedat"
                            | "substack"
                            | "bordermatrix"
                    ));
                    index += 1;
                }
                continue;
            }
            match ch {
                '(' => wrapper_stack.push(false),
                ')' => {
                    wrapper_stack.pop();
                }
                ';' if wrapper_stack.last().copied().unwrap_or(false) => {
                    result.push('\n');
                    index += ch.len_utf8();
                    while index < text.len()
                        && text[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += text[index..]
                            .chars()
                            .next()
                            .expect("table cell whitespace")
                            .len_utf8();
                    }
                    continue;
                }
                _ => {}
            }
            result.push(ch);
            index += ch.len_utf8();
        }
        result
    };

    let mut serialized_rows = Vec::new();
    let mut row_start = 0usize;
    let mut parenthesis_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in visible.char_indices() {
        match ch {
            '(' => parenthesis_depth += 1,
            ')' => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ';' if parenthesis_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                serialized_rows.push(&visible[row_start..index]);
                row_start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    serialized_rows.push(&visible[row_start..]);

    let mut rows = serialized_rows
        .into_iter()
        .filter_map(|row| {
            let row = row.trim();
            if row.is_empty() {
                return None;
            }
            let mut serialized_cells = Vec::new();
            let mut cell_start = 0usize;
            let mut parenthesis_depth = 0usize;
            let mut bracket_depth = 0usize;
            let mut brace_depth = 0usize;
            for (index, ch) in row.char_indices() {
                match ch {
                    '(' => parenthesis_depth += 1,
                    ')' => parenthesis_depth = parenthesis_depth.saturating_sub(1),
                    '[' => bracket_depth += 1,
                    ']' => bracket_depth = bracket_depth.saturating_sub(1),
                    '{' => brace_depth += 1,
                    '}' => brace_depth = brace_depth.saturating_sub(1),
                    '|' if parenthesis_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && row[..index]
                            .chars()
                            .next_back()
                            .is_none_or(char::is_whitespace)
                        && row[index + ch.len_utf8()..]
                            .chars()
                            .next()
                            .is_none_or(char::is_whitespace) =>
                    {
                        serialized_cells.push(&row[cell_start..index]);
                        cell_start = index + ch.len_utf8();
                    }
                    _ => {}
                }
            }
            serialized_cells.push(&row[cell_start..]);
            let cells = serialized_cells
                .into_iter()
                .map(|text| TableCellEvent {
                    text: split_nested_table_cell_lines(text.trim()),
                    column_span: 1,
                    row_span: None,
                    alignment: None,
                    rule_before_count: 0,
                    rule_after_count: 0,
                    cell_prefix: None,
                    cell_suffix: None,
                })
                .collect::<Vec<_>>();
            Some(TableRowEvent {
                rule_above: false,
                partial_rules_above: Vec::new(),
                cells,
                rule_below: false,
                partial_rules_below: Vec::new(),
            })
        })
        .collect::<Vec<_>>();

    let mut caption = None;
    if environment == "longtable"
        && event.source_excerpt.contains(r"\caption")
        && rows.len() > 1
        && rows.first().is_some_and(|row| row.cells.len() == 1)
    {
        caption = rows
            .first()
            .and_then(|row| row.cells.first())
            .map(|cell| cell.text.clone());
        rows.remove(0);
    }

    for rule in &event.table_rules {
        if rows.is_empty() {
            break;
        }
        match rule.position {
            TableRulePosition::Above => {
                if let Some(row) = rows.get_mut(rule.row_index) {
                    if let Some(span) = rule.column_span {
                        row.partial_rules_above.push(span);
                    } else {
                        row.rule_above = true;
                    }
                } else if let Some(row) = rows.last_mut() {
                    if let Some(span) = rule.column_span {
                        row.partial_rules_below.push(span);
                    } else {
                        row.rule_below = true;
                    }
                }
            }
            TableRulePosition::Below => {
                if let Some(row) = rows.get_mut(rule.row_index) {
                    if let Some(span) = rule.column_span {
                        row.partial_rules_below.push(span);
                    } else {
                        row.rule_below = true;
                    }
                } else if let Some(row) = rows.last_mut() {
                    if let Some(span) = rule.column_span {
                        row.partial_rules_below.push(span);
                    } else {
                        row.rule_below = true;
                    }
                }
            }
        }
    }

    for cell_span in &event.table_cell_spans {
        if let Some(row) = rows.get_mut(cell_span.row_index)
            && let Some(cell) = row.cells.get_mut(cell_span.column_index)
        {
            cell.column_span = cell.column_span.max(cell_span.column_span);
            if let Some(row_span) = cell_span.row_span
                && row_span > 1
            {
                cell.row_span = Some(row_span);
            }
            if let Some(alignment) = cell_span.alignment {
                cell.alignment = Some(alignment);
            }
            cell.rule_before_count = cell.rule_before_count.max(cell_span.rule_before_count);
            cell.rule_after_count = cell.rule_after_count.max(cell_span.rule_after_count);
            if let Some(prefix) = &cell_span.cell_prefix {
                cell.cell_prefix = Some(prefix.clone());
            }
            if let Some(suffix) = &cell_span.cell_suffix {
                cell.cell_suffix = Some(suffix.clone());
            }
        }
    }

    Some(TableEvent {
        environment: environment.to_string(),
        width_spec: event.table_width_spec.clone(),
        columns: event.table_columns.clone(),
        rows,
        caption,
    })
}
