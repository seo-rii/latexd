use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventProducer, EventSequence, GeneratedBy, ProvenanceSpan, RawFallbackEvent, RelatedSourceSpan,
    RenderEvent, RenderEventEnvelope, SemanticConfidence, SourceProvenance, SourceSpanRole,
    TableCellEvent, TableColumnAlignment, TableColumnSpec, TableEvent, TableRowEvent,
    TableRulePosition,
};

use crate::{
    Vm,
    input::QueueItem,
    snapshot::{VmExecutedTableFrameSnapshot, VmExecutedTableSnapshot, VmSemanticTableSnapshot},
};

#[derive(Debug, Default)]
pub(super) struct SemanticTableState {
    scanner_event_ids: HashSet<EventSequence>,
    open_tables: Vec<ExecutedTableFrame>,
    executed_tables: Vec<ExecutedTable>,
    structured_events: bool,
}

#[derive(Debug)]
struct ExecutedTableFrame {
    environment: String,
    source: SourceProvenance,
    producer: EventProducer,
    width_spec: Option<String>,
    columns: Vec<TableColumnSpec>,
    rows: Vec<TableRowEvent>,
    current_cells: Vec<TableCellEvent>,
    current_text: String,
    current_source: Option<SourceProvenance>,
    row_source: Option<SourceProvenance>,
    row_started: bool,
}

#[derive(Debug)]
struct ExecutedTable {
    environment: String,
    source: SourceProvenance,
    native_event: Option<RenderEventEnvelope>,
}

impl ExecutedTableFrame {
    fn finish_cell(&mut self, boundary_source: Option<SourceProvenance>) {
        let source = self.current_source.take().or(boundary_source);
        if let Some(source) = &source {
            merge_source_range(&mut self.row_source, source.clone());
        }
        self.current_cells.push(TableCellEvent {
            text: self.current_text.trim().to_string(),
            source,
            column_span: 1,
            row_span: None,
            alignment: None,
            rule_before_count: 0,
            rule_after_count: 0,
            cell_prefix: None,
            cell_suffix: None,
        });
        self.current_text.clear();
        self.row_started = true;
    }

    fn finish_row(&mut self, force: bool, boundary_source: Option<SourceProvenance>) {
        if !force && !self.row_started && self.current_text.trim().is_empty() {
            return;
        }
        self.finish_cell(boundary_source.clone());
        self.rows.push(TableRowEvent {
            rule_above: false,
            partial_rules_above: Vec::new(),
            cells: mem::take(&mut self.current_cells),
            source: self.row_source.take().or(boundary_source),
            rule_below: false,
            partial_rules_below: Vec::new(),
        });
        self.row_started = false;
    }
}

impl Vm<'_> {
    pub(super) fn semantic_table_snapshot(&self) -> VmSemanticTableSnapshot {
        let mut scanner_event_ids = self
            .semantic_table
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        VmSemanticTableSnapshot {
            scanner_event_ids,
            open_tables: self
                .semantic_table
                .open_tables
                .iter()
                .map(|frame| VmExecutedTableFrameSnapshot {
                    environment: frame.environment.clone(),
                    source: frame.source.clone(),
                    producer: frame.producer,
                    width_spec: frame.width_spec.clone(),
                    columns: frame.columns.clone(),
                    rows: frame.rows.clone(),
                    current_cells: frame.current_cells.clone(),
                    current_text: frame.current_text.clone(),
                    current_source: frame.current_source.clone(),
                    row_source: frame.row_source.clone(),
                    row_started: frame.row_started,
                })
                .collect(),
            executed_tables: self
                .semantic_table
                .executed_tables
                .iter()
                .map(|table| VmExecutedTableSnapshot {
                    environment: table.environment.clone(),
                    source: table.source.clone(),
                    native_event: table.native_event.clone(),
                })
                .collect(),
            structured_events: self.semantic_table.structured_events,
        }
    }

    pub(super) fn restore_semantic_table_snapshot(&mut self, snapshot: &VmSemanticTableSnapshot) {
        self.semantic_table.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_table.open_tables = snapshot
            .open_tables
            .iter()
            .map(|frame| ExecutedTableFrame {
                environment: frame.environment.clone(),
                source: frame.source.clone(),
                producer: frame.producer,
                width_spec: frame.width_spec.clone(),
                columns: frame.columns.clone(),
                rows: frame.rows.clone(),
                current_cells: frame.current_cells.clone(),
                current_text: frame.current_text.clone(),
                current_source: frame.current_source.clone(),
                row_source: frame.row_source.clone(),
                row_started: frame.row_started,
            })
            .collect();
        self.semantic_table.executed_tables = snapshot
            .executed_tables
            .iter()
            .map(|table| ExecutedTable {
                environment: table.environment.clone(),
                source: table.source.clone(),
                native_event: table.native_event.clone(),
            })
            .collect();
        self.semantic_table.structured_events = snapshot.structured_events;
    }

    pub fn enable_structured_table_events(&mut self) {
        self.semantic_table.structured_events = true;
    }

    pub(super) fn prepare_semantic_table_capture(&mut self) {
        self.semantic_table.scanner_event_ids.clear();
        self.semantic_table.open_tables.clear();
        self.semantic_table.executed_tables.clear();
    }

    pub(super) fn mark_scanner_table_event(&mut self, event_id: EventSequence) {
        self.semantic_table.scanner_event_ids.insert(event_id);
    }

    pub(super) fn begin_executed_table(
        &mut self,
        environment: &str,
        start_utf8: u32,
        end_utf8: u32,
        width_spec: Option<String>,
        column_spec: Option<String>,
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
            width_spec,
            columns: column_spec
                .as_deref()
                .map(simple_table_columns)
                .unwrap_or_default(),
            rows: Vec::new(),
            current_cells: Vec::new(),
            current_text: String::new(),
            current_source: None,
            row_source: None,
            row_started: false,
        });
    }

    pub(super) fn capture_executed_table_character(
        &mut self,
        ch: char,
        start_utf8: u32,
        end_utf8: u32,
    ) -> bool {
        if !self.semantic_table.structured_events || self.semantic_table.open_tables.is_empty() {
            return false;
        }
        let source = self.executed_table_token_source(start_utf8, end_utf8);
        let table = self
            .semantic_table
            .open_tables
            .last_mut()
            .expect("table frame exists");
        table.current_text.push(ch);
        merge_source_range(&mut table.current_source, source);
        table.row_started = true;
        true
    }

    pub(super) fn capture_executed_table_space(&mut self) -> bool {
        if !self.semantic_table.structured_events {
            return false;
        }
        let Some(table) = self.semantic_table.open_tables.last_mut() else {
            return false;
        };
        if !table.current_text.is_empty() && !table.current_text.ends_with(char::is_whitespace) {
            table.current_text.push(' ');
        }
        true
    }

    pub(super) fn remove_last_executed_table_space(&mut self) -> bool {
        if !self.semantic_table.structured_events {
            return false;
        }
        let Some(table) = self.semantic_table.open_tables.last_mut() else {
            return false;
        };
        while table.current_text.ends_with(char::is_whitespace) {
            table.current_text.pop();
        }
        true
    }

    pub(super) fn capture_executed_table_alignment_tab(
        &mut self,
        start_utf8: u32,
        end_utf8: u32,
    ) -> bool {
        if !self.semantic_table.structured_events || self.semantic_table.open_tables.is_empty() {
            return false;
        }
        let source = self.executed_table_token_source(start_utf8, end_utf8);
        let table = self
            .semantic_table
            .open_tables
            .last_mut()
            .expect("table frame exists");
        table.finish_cell(Some(source));
        true
    }

    pub(super) fn capture_executed_table_control_sequence(
        &mut self,
        control_sequence: &str,
        start_utf8: u32,
        end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) -> bool {
        if self.semantic_table.open_tables.is_empty()
            || !matches!(control_sequence, "\\" | "tabularnewline" | "cr" | "crcr")
        {
            return false;
        }
        if !self.semantic_table.structured_events {
            return control_sequence == "\\";
        }
        if control_sequence == "\\" {
            let _ = self.read_optional_bracket_tokens(queue);
        }
        let source = self.executed_table_token_source(start_utf8, end_utf8);
        self.semantic_table
            .open_tables
            .last_mut()
            .expect("table frame exists")
            .finish_row(true, Some(source));
        true
    }

    pub(super) fn end_executed_table(&mut self, environment: &str, start_utf8: u32, end_utf8: u32) {
        if !self.render_event_capture || !is_table_environment(environment) {
            return;
        }
        let end_source = self
            .semantic_table
            .structured_events
            .then(|| self.executed_table_token_source(start_utf8, end_utf8));
        let Some(index) = self
            .semantic_table
            .open_tables
            .iter()
            .rposition(|frame| frame.environment == environment)
        else {
            return;
        };
        let mut frame = self.semantic_table.open_tables.remove(index);
        frame.finish_row(false, end_source);
        let native_event = if self.semantic_table.structured_events && !frame.rows.is_empty() {
            let event_id = self.render_events.allocate_event_sequence();
            let mut envelope = RenderEventEnvelope::new(
                event_id,
                RenderEvent::Table(TableEvent {
                    environment: frame.environment.clone(),
                    width_spec: frame.width_spec,
                    columns: frame.columns,
                    rows: frame.rows,
                    caption: None,
                }),
                frame.source.clone(),
            );
            envelope.meta.producer = frame.producer;
            Some(envelope)
        } else {
            None
        };
        self.semantic_table.executed_tables.push(ExecutedTable {
            environment: frame.environment,
            source: frame.source,
            native_event,
        });
    }

    fn executed_table_token_source(&self, start_utf8: u32, end_utf8: u32) -> SourceProvenance {
        let (execution_source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        if producer != EventProducer::Macro || start_utf8 >= end_utf8 {
            return execution_source;
        }
        let mut source =
            SourceProvenance::file(self.current_execution_source_path(), start_utf8, end_utf8);
        if source.primary != execution_source.primary {
            source =
                source.with_related(SourceSpanRole::Invocation, execution_source.primary.clone());
        }
        source.expansion_stack = execution_source.expansion_stack;
        source.expansion_stack_truncated = execution_source.expansion_stack_truncated;
        source.generated_by = execution_source.generated_by;
        source
    }

    pub(super) fn reconcile_executed_table_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_table.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_table.executed_tables);
        self.semantic_table.open_tables.clear();

        for scanner_event in self.render_events.iter_mut() {
            if !scanner_ids.contains(&scanner_event.meta.sequence) {
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
            if self.semantic_table.structured_events
                && let RenderEvent::RawFallback(fallback) = &scanner_event.event
                && let Some(table) = table_event_from_fallback(fallback)
            {
                scanner_event.event = RenderEvent::Table(table);
                scanner_event.meta.producer = EventProducer::ScannerRecovery;
                scanner_event.meta.confidence = SemanticConfidence::Medium;
                scanner_event.meta.source.generated_by = GeneratedBy::Source;
                if scanner_event.meta.source.expansion_stack.is_empty() {
                    scanner_event.meta.source.expansion_stack =
                        executed_table.source.expansion_stack;
                    scanner_event.meta.source.expansion_stack_truncated =
                        executed_table.source.expansion_stack_truncated;
                }
            }
        }

        for executed_table in executed {
            let Some(native_event) = executed_table.native_event else {
                continue;
            };
            let Some((path, start_utf8)) = table_start_anchor(&native_event.meta.source) else {
                continue;
            };
            let insertion = self
                .render_events
                .iter()
                .position(|event| {
                    table_start_anchor(&event.meta.source).is_some_and(
                        |(event_path, event_start_utf8)| {
                            event_path == path
                                && (event_start_utf8 > start_utf8
                                    || (event_start_utf8 == start_utf8
                                        && event.meta.sequence > native_event.meta.sequence))
                        },
                    )
                })
                .unwrap_or(self.render_events.len());
            self.render_events.insert(insertion, native_event);
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

fn merge_source_range(target: &mut Option<SourceProvenance>, source: SourceProvenance) {
    let Some(existing) = target else {
        *target = Some(source);
        return;
    };
    if existing.expansion_stack == source.expansion_stack
        && let (ProvenanceSpan::File(existing_span), ProvenanceSpan::File(source_span)) =
            (&mut existing.primary, &source.primary)
        && existing_span.path == source_span.path
    {
        existing_span.start_utf8 = existing_span.start_utf8.min(source_span.start_utf8);
        existing_span.end_utf8 = existing_span.end_utf8.max(source_span.end_utf8);
        return;
    }
    if existing.primary != source.primary
        && !existing
            .related
            .iter()
            .any(|related| related.span == source.primary)
    {
        existing.related.push(RelatedSourceSpan {
            role: SourceSpanRole::ArgumentContent,
            span: source.primary,
        });
    }
}

fn simple_table_columns(spec: &str) -> Vec<TableColumnSpec> {
    let mut columns = Vec::<TableColumnSpec>::new();
    let mut pending_rules = 0u8;
    for ch in spec.chars() {
        if ch == '|' {
            pending_rules = pending_rules.saturating_add(1);
            continue;
        }
        let alignment = match ch {
            'l' => TableColumnAlignment::Left,
            'c' => TableColumnAlignment::Center,
            'r' => TableColumnAlignment::Right,
            'p' | 'm' | 'b' => TableColumnAlignment::Paragraph,
            _ => continue,
        };
        if pending_rules > 0
            && let Some(previous) = columns.last_mut()
        {
            previous.rule_after = true;
            previous.rule_after_count = pending_rules;
        }
        columns.push(TableColumnSpec {
            alignment,
            rule_before: pending_rules > 0,
            rule_before_count: pending_rules,
            rule_after: false,
            rule_after_count: 0,
            separator_after: None,
            width_pt_milli: None,
            cell_prefix: None,
            cell_suffix: None,
        });
        pending_rules = 0;
    }
    if pending_rules > 0
        && let Some(last) = columns.last_mut()
    {
        last.rule_after = true;
        last.rule_after_count = pending_rules;
    }
    columns
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
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(|text| TableCellEvent {
                    text: split_nested_table_cell_lines(text),
                    source: None,
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
                source: None,
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
