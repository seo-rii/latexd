use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    EventId, EventProducer, GraphicAssetFormat, GraphicRefEvent, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{CatCode, Token, TokenKind};

use crate::{
    Vm, command::GraphicCommand, input::QueueItem, merge_graphic_default_options,
    merge_graphic_options, normalize_latex_text, parse_graphic_page_selection,
    read_braced_source_argument, skip_ascii_whitespace,
};

#[derive(Debug, Default)]
pub(super) struct SemanticGraphicState {
    scanner_event_ids: HashSet<EventId>,
    executed_events: Vec<RenderEventEnvelope>,
    overridden_invocations: Vec<GraphicInvocationRange>,
    pub(super) graphic_paths: Vec<Utf8PathBuf>,
    pub(super) graphic_extensions: Vec<String>,
    pub(super) default_options: Option<String>,
}

#[derive(Debug)]
struct GraphicInvocationRange {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
}

impl Vm<'_> {
    pub(super) fn prepare_semantic_graphic_capture(&mut self) {
        self.semantic_graphic.scanner_event_ids.clear();
        self.semantic_graphic.executed_events.clear();
        self.semantic_graphic.overridden_invocations.clear();
    }

    pub(super) fn mark_scanner_graphic_event(&mut self, event_id: EventId) {
        self.semantic_graphic.scanner_event_ids.insert(event_id);
    }

    pub(super) fn record_overridden_graphic_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture
            || !self.execution_in_document
            || !matches!(command_name, "includegraphics" | "includepdf")
            || end_utf8 <= start_utf8
        {
            return;
        }
        self.semantic_graphic
            .overridden_invocations
            .push(GraphicInvocationRange {
                path: self.current_execution_source_path(),
                start_utf8,
                end_utf8,
            });
    }

    pub(super) fn record_graphic_module_options(
        &mut self,
        label: &str,
        path: &Utf8Path,
        options: &[String],
    ) {
        let file_stem = path.file_stem().unwrap_or(path.as_str());
        let selected = if label == "class" {
            options
                .iter()
                .filter(|option| matches!(option.trim(), "draft" | "final"))
                .map(String::as_str)
                .collect::<Vec<_>>()
        } else if label == "package" && matches!(file_stem, "graphicx" | "graphics" | "epsfig") {
            options.iter().map(String::as_str).collect::<Vec<_>>()
        } else {
            return;
        };
        if selected.is_empty() {
            return;
        }
        let options = selected.join(",");
        self.semantic_graphic.default_options =
            merge_graphic_options(self.semantic_graphic.default_options.take(), Some(&options));
    }

    pub(super) fn execute_graphic_path(&mut self, queue: &mut VecDeque<QueueItem>) {
        let Some(tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let text = self.expanded_graphic_text(tokens);
        let mut paths = Vec::new();
        let mut cursor = 0usize;
        while cursor < text.len() {
            cursor = skip_ascii_whitespace(&text, cursor);
            let Some((path, _, _, after)) = read_braced_source_argument(&text, cursor) else {
                break;
            };
            let path = normalize_latex_text(path);
            if let Ok(path) = crate::normalize_relative_path(Utf8Path::new(path.trim())) {
                paths.push(path);
            }
            cursor = after;
        }
        self.semantic_graphic.graphic_paths = paths;
    }

    pub(super) fn execute_declare_graphics_extensions(&mut self, queue: &mut VecDeque<QueueItem>) {
        let Some(tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let extensions = self
            .expanded_graphic_text(tokens)
            .split(',')
            .map(str::trim)
            .map(|extension| extension.trim_start_matches('.'))
            .filter(|extension| !extension.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !extensions.is_empty() {
            self.semantic_graphic.graphic_extensions = extensions;
        }
    }

    pub(super) fn execute_graphic_set_keys(&mut self, queue: &mut VecDeque<QueueItem>) {
        let Some(family_tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let Some(option_tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let family = self.expanded_graphic_text(family_tokens);
        if !family
            .split(',')
            .map(str::trim)
            .any(|family| family == "Gin")
        {
            return;
        }
        let options = self.expanded_graphic_text(option_tokens);
        self.semantic_graphic.default_options = merge_graphic_options(
            self.semantic_graphic.default_options.take(),
            Some(options.trim()),
        );
    }

    pub(super) fn execute_semantic_graphic(
        &mut self,
        command: GraphicCommand,
        source_start_utf8: u32,
        source_end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) {
        self.skip_optional_spaces(queue);
        let starred_clip = if matches!(
            self.peek_next_token(queue).map(|token| token.kind),
            Some(TokenKind::Character {
                ch: '*',
                catcode: CatCode::Other | CatCode::Letter | CatCode::Active,
            })
        ) {
            self.pop_next_token(queue);
            true
        } else {
            false
        };
        let first_options = self.read_optional_bracket_tokens(queue);
        let second_options = self.read_optional_bracket_tokens(queue);
        let Some(path_tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let path_start_utf8 = path_tokens
            .first()
            .map_or(source_end_utf8, |token| token.span.start);
        let path_end_utf8 = path_tokens
            .last()
            .map_or(path_start_utf8, |token| token.span.end);
        let invocation_end_utf8 = self.last_token_end_utf8.max(source_end_utf8);

        let local_options = match (first_options, second_options) {
            (Some(lower_left), Some(upper_right)) => {
                let lower_left = normalize_bbox_option(&self.expanded_graphic_text(lower_left));
                let upper_right = normalize_bbox_option(&self.expanded_graphic_text(upper_right));
                Some(format!("viewport={lower_left} {upper_right}"))
            }
            (Some(options), None) => {
                let options = self.expanded_graphic_text(options);
                (!options.trim().is_empty()).then(|| options.trim().to_string())
            }
            (None, Some(_)) => None,
            (None, None) => None,
        };
        let local_options = merge_graphic_options(local_options, starred_clip.then_some("clip"));
        let options = merge_graphic_default_options(
            self.semantic_graphic.default_options.as_deref(),
            local_options,
        );

        let path = normalize_latex_text(self.expanded_graphic_text(path_tokens).trim());
        let graphic_paths = self.semantic_graphic.graphic_paths.clone();
        let graphic_extensions = self.semantic_graphic.graphic_extensions.clone();
        let source_path = self.current_execution_source_path();
        let resolved_path = self.resolve_graphic_asset_path(
            &source_path,
            &path,
            &graphic_paths,
            &graphic_extensions,
        );
        let resolved_asset_path = Utf8Path::new(&resolved_path);
        let asset_format = GraphicAssetFormat::from_path(&resolved_path);
        let asset_hash = self.project_file_hash(resolved_asset_path);
        let asset_dimensions =
            self.project_graphic_asset_dimensions(resolved_asset_path, asset_format);

        self.finish_executed_block_content();
        let (mut source, producer) = self.executed_semantic_source(path_start_utf8, path_end_utf8);
        if producer == EventProducer::Primitive {
            source =
                SourceProvenance::file(source_path.clone(), source_start_utf8, invocation_end_utf8)
                    .with_related(
                        SourceSpanRole::ArgumentContent,
                        ProvenanceSpan::File(SourceSpan {
                            path: source_path,
                            start_utf8: path_start_utf8,
                            end_utf8: path_end_utf8,
                        }),
                    );
        }

        if !command.include_pdf {
            self.output.push_str("[image]");
            self.legacy_output_last_char = Some(']');
        }
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }

        let event = GraphicRefEvent {
            path: resolved_path,
            options: options.clone(),
            page_selection: parse_graphic_page_selection(options.as_deref()),
            asset_format,
            asset_hash,
            asset_dimensions,
        };
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            if command.include_pdf {
                RenderEvent::IncludePdf(event)
            } else {
                RenderEvent::GraphicRef(event)
            },
            source,
        );
        envelope.meta.producer = producer;
        self.semantic_graphic.executed_events.push(envelope);
    }

    pub(super) fn reconcile_executed_graphic_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_graphic.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_graphic.executed_events);
        let overridden_invocations = mem::take(&mut self.semantic_graphic.overridden_invocations);
        if scanner_ids.is_empty() && executed.is_empty() && overridden_invocations.is_empty() {
            return;
        }

        let scanner_events = mem::take(&mut self.render_events);
        let mut reconciled = Vec::with_capacity(scanner_events.len() + executed.len());
        for scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed.iter().position(|candidate| {
                graphic_shapes_match(candidate, &scanner_event)
                    && provenance_overlaps(&candidate.meta.source, &scanner_event.meta.source)
                    && (graphic_paths_match(candidate, &scanner_event)
                        || candidate.meta.producer != EventProducer::Macro)
            });
            let Some(index) = matching else {
                if !self.semantic_source_is_suppressed(&scanner_event.meta.source)
                    && !provenance_overlaps_invocation(
                        &scanner_event.meta.source,
                        &overridden_invocations,
                    )
                {
                    reconciled.push(scanner_event);
                }
                continue;
            };
            let mut executed_event = executed.remove(index);
            if self.semantic_source_is_suppressed(&scanner_event.meta.source) {
                continue;
            }
            if graphic_payloads_match(&executed_event, &scanner_event)
                || graphic_options_match(&executed_event, &scanner_event)
                || (!graphic_paths_match(&executed_event, &scanner_event)
                    && executed_event.meta.producer != EventProducer::Macro)
            {
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            } else {
                // Scanner wrappers may contribute options that are not visible to the primitive.
                reconciled.push(scanner_event);
            }
        }

        executed.retain(|event| !self.semantic_source_is_suppressed(&event.meta.source));
        insert_unmatched_graphic_events(&mut reconciled, executed);
        self.render_events = reconciled;
    }

    fn expanded_graphic_text(&mut self, tokens: Vec<Token>) -> String {
        self.fully_expand_tokens(tokens)
            .into_iter()
            .map(|token| match token.kind {
                TokenKind::Character { ch, .. } => ch.to_string(),
                TokenKind::ControlSequence { name } => {
                    format!("\\{}", self.interner.resolve(name).unwrap_or(""))
                }
            })
            .collect()
    }
}

fn normalize_bbox_option(value: &str) -> String {
    value
        .replace(',', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn graphic_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::GraphicRef(left), RenderEvent::GraphicRef(right))
        | (RenderEvent::IncludePdf(left), RenderEvent::IncludePdf(right)) => left == right,
        _ => false,
    }
}

fn graphic_options_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::GraphicRef(left), RenderEvent::GraphicRef(right))
        | (RenderEvent::IncludePdf(left), RenderEvent::IncludePdf(right)) => {
            left.options == right.options && left.page_selection == right.page_selection
        }
        _ => false,
    }
}

fn graphic_paths_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::GraphicRef(left), RenderEvent::GraphicRef(right))
        | (RenderEvent::IncludePdf(left), RenderEvent::IncludePdf(right)) => {
            left.path == right.path
        }
        _ => false,
    }
}

fn graphic_shapes_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    matches!(
        (&left.event, &right.event),
        (RenderEvent::GraphicRef(_), RenderEvent::GraphicRef(_))
            | (RenderEvent::IncludePdf(_), RenderEvent::IncludePdf(_))
    )
}

fn insert_unmatched_graphic_events(
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

fn provenance_overlaps(left: &SourceProvenance, right: &SourceProvenance) -> bool {
    provenance_spans(left).any(|left_span| {
        provenance_spans(right).any(|right_span| {
            left_span.path == right_span.path
                && left_span.start_utf8 < right_span.end_utf8
                && right_span.start_utf8 < left_span.end_utf8
        })
    })
}

fn provenance_overlaps_invocation(
    source: &SourceProvenance,
    invocations: &[GraphicInvocationRange],
) -> bool {
    provenance_spans(source).any(|span| {
        invocations.iter().any(|invocation| {
            span.path == invocation.path
                && span.start_utf8 < invocation.end_utf8
                && invocation.start_utf8 < span.end_utf8
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
