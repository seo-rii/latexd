use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    EventBuildContext, EventProducer, EventSequence, GraphicAssetFormat, GraphicRefEvent,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpan, SourceSpanRole,
};
use tex_tokens::{CatCode, Token, TokenKind};

use crate::{
    Vm,
    command::{EpsfDimension, GraphicCommand, LegacyGraphicCommand, LegacyGraphicSyntax},
    input::QueueItem,
    merge_graphic_default_options, merge_graphic_options, normalize_latex_text,
    parse_graphic_page_selection, read_braced_source_argument,
    semantic_reconciliation::{call_invocation_primary_anchor, source_locations_overlap},
    semantic_text::event_origin_for_executed_producer,
    skip_ascii_whitespace,
    snapshot::{VmGraphicInvocationRangeSnapshot, VmSemanticGraphicSnapshot},
};

#[derive(Debug, Default)]
pub(super) struct SemanticGraphicState {
    scanner_event_ids: HashSet<EventSequence>,
    executed_events: Vec<RenderEventEnvelope>,
    overridden_invocations: Vec<GraphicInvocationRange>,
    pub(super) scanner_default_options: Vec<ScannerGraphicDefaultOptions>,
    pub(super) graphic_paths: Vec<Utf8PathBuf>,
    pub(super) graphic_extensions: Vec<String>,
    pub(super) default_options: Option<String>,
    pub(super) epsf_pending_options: Option<String>,
}

#[derive(Debug)]
struct GraphicInvocationRange {
    path: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
}

#[derive(Debug, Clone)]
pub(super) struct ScannerGraphicDefaultOptions {
    pub(super) options: String,
    pub(super) path: Utf8PathBuf,
    pub(super) start_utf8: u32,
    pub(super) end_utf8: u32,
    pub(super) first_event_sequence: EventSequence,
    pub(super) execution_anchor: crate::snapshot::VmExecutionAnchor,
}

struct ExecutedGraphicInput {
    include_pdf: bool,
    emit_legacy_placeholder: bool,
    invocation_start_utf8: u32,
    invocation_end_utf8: u32,
    argument_span: Option<(u32, u32)>,
    path: String,
    options: Option<String>,
}

impl Vm<'_> {
    pub(super) fn semantic_graphic_snapshot(&self) -> VmSemanticGraphicSnapshot {
        let mut scanner_event_ids = self
            .semantic_graphic
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        VmSemanticGraphicSnapshot {
            scanner_event_ids,
            executed_events: self.semantic_graphic.executed_events.clone(),
            overridden_invocations: self
                .semantic_graphic
                .overridden_invocations
                .iter()
                .map(|invocation| VmGraphicInvocationRangeSnapshot {
                    path: invocation.path.clone(),
                    start_utf8: invocation.start_utf8,
                    end_utf8: invocation.end_utf8,
                })
                .collect(),
            scanner_default_options: self
                .semantic_graphic
                .scanner_default_options
                .iter()
                .map(
                    |options| crate::snapshot::VmScannerGraphicDefaultOptionsSnapshot {
                        options: options.options.clone(),
                        path: options.path.clone(),
                        start_utf8: options.start_utf8,
                        end_utf8: options.end_utf8,
                        first_event_sequence: options.first_event_sequence,
                        execution_anchor: options.execution_anchor.clone(),
                    },
                )
                .collect(),
        }
    }

    pub(super) fn restore_semantic_graphic_snapshot(
        &mut self,
        snapshot: &VmSemanticGraphicSnapshot,
    ) {
        self.semantic_graphic.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_graphic.executed_events = snapshot.executed_events.clone();
        self.semantic_graphic.overridden_invocations = snapshot
            .overridden_invocations
            .iter()
            .map(|invocation| GraphicInvocationRange {
                path: invocation.path.clone(),
                start_utf8: invocation.start_utf8,
                end_utf8: invocation.end_utf8,
            })
            .collect();
        self.semantic_graphic.scanner_default_options = snapshot
            .scanner_default_options
            .iter()
            .map(|options| ScannerGraphicDefaultOptions {
                options: options.options.clone(),
                path: options.path.clone(),
                start_utf8: options.start_utf8,
                end_utf8: options.end_utf8,
                first_event_sequence: options.first_event_sequence,
                execution_anchor: options.execution_anchor.clone(),
            })
            .collect();
    }

    pub(super) fn prepare_semantic_graphic_capture(&mut self) {
        self.semantic_graphic.scanner_event_ids.clear();
        self.semantic_graphic.executed_events.clear();
        self.semantic_graphic.overridden_invocations.clear();
        self.semantic_graphic.scanner_default_options.clear();
    }

    pub(super) fn mark_scanner_graphic_event(&mut self, event_id: EventSequence) {
        self.semantic_graphic.scanner_event_ids.insert(event_id);
    }

    pub(super) fn record_overridden_graphic_invocation(
        &mut self,
        command_name: &str,
        start_utf8: u32,
        end_utf8: u32,
        expansion_is_empty: bool,
    ) {
        if !self.render_event_capture
            || !self.execution_in_document
            || end_utf8 <= start_utf8
            || (!expansion_is_empty
                && !matches!(
                    command_name,
                    "includegraphics" | "includepdf" | "epsfig" | "psfig" | "epsfbox" | "epsffile"
                ))
        {
            return;
        }
        // An empty replacement consumes its arguments without executing them.
        // Scanner graphics inside that invocation are therefore recovery-only.
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
        self.emit_executed_graphic(ExecutedGraphicInput {
            include_pdf: command.include_pdf,
            emit_legacy_placeholder: !command.include_pdf,
            invocation_start_utf8: source_start_utf8,
            invocation_end_utf8,
            argument_span: Some((path_start_utf8, path_end_utf8)),
            path,
            options,
        });
    }

    pub(super) fn execute_overpic_environment(
        &mut self,
        source_start_utf8: u32,
        source_end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) -> Option<u32> {
        self.skip_optional_spaces(queue);
        let local_options = self.read_optional_bracket_tokens(queue).and_then(|tokens| {
            let options = self.expanded_graphic_text(tokens);
            (!options.trim().is_empty()).then(|| options.trim().to_string())
        });
        let path_tokens = self.read_macro_argument(queue)?;
        let path_start_utf8 = path_tokens
            .first()
            .map_or(source_end_utf8, |token| token.span.start);
        let path_end_utf8 = path_tokens
            .last()
            .map_or(path_start_utf8, |token| token.span.end);
        let invocation_end_utf8 = self.last_token_end_utf8.max(source_end_utf8);
        let path = normalize_latex_text(self.expanded_graphic_text(path_tokens).trim());
        let options = merge_graphic_default_options(
            self.semantic_graphic.default_options.as_deref(),
            local_options,
        );
        self.emit_executed_graphic(ExecutedGraphicInput {
            include_pdf: false,
            emit_legacy_placeholder: false,
            invocation_start_utf8: source_start_utf8,
            invocation_end_utf8,
            argument_span: Some((path_start_utf8, path_end_utf8)),
            path,
            options,
        });
        Some(invocation_end_utf8)
    }

    pub(super) fn execute_legacy_graphic(
        &mut self,
        command: LegacyGraphicCommand,
        source_start_utf8: u32,
        source_end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) {
        let Some(argument_tokens) = self.read_macro_argument(queue) else {
            return;
        };
        let argument_start_utf8 = argument_tokens
            .first()
            .map_or(source_end_utf8, |token| token.span.start);
        let argument_end_utf8 = argument_tokens
            .last()
            .map_or(argument_start_utf8, |token| token.span.end);
        let invocation_end_utf8 = self.last_token_end_utf8.max(source_end_utf8);
        let legacy_path_span = (command.syntax == LegacyGraphicSyntax::KeyValue)
            .then(|| legacy_graphic_path_span(&argument_tokens))
            .flatten();
        let argument = self.expanded_graphic_text(argument_tokens);
        let (path, local_options, argument_span) = match command.syntax {
            LegacyGraphicSyntax::KeyValue => {
                let Some(path) = legacy_graphic_path(&argument) else {
                    return;
                };
                (path, Some(argument.trim().to_string()), legacy_path_span)
            }
            LegacyGraphicSyntax::File => (
                normalize_latex_text(argument.trim()),
                self.semantic_graphic.epsf_pending_options.take(),
                Some((argument_start_utf8, argument_end_utf8)),
            ),
        };
        let options = merge_graphic_default_options(
            self.semantic_graphic.default_options.as_deref(),
            local_options,
        );
        self.emit_executed_graphic(ExecutedGraphicInput {
            include_pdf: false,
            emit_legacy_placeholder: true,
            invocation_start_utf8: source_start_utf8,
            invocation_end_utf8,
            argument_span,
            path,
            options,
        });
    }

    pub(super) fn execute_epsf_dimension(
        &mut self,
        dimension: EpsfDimension,
        queue: &mut VecDeque<QueueItem>,
    ) {
        self.skip_optional_spaces(queue);
        if matches!(
            self.peek_next_token(queue).map(|token| token.kind),
            Some(TokenKind::Character { ch: '=', .. })
        ) {
            self.pop_next_token(queue);
        }
        self.skip_optional_spaces(queue);
        let mut value_tokens = Vec::new();
        while let Some(token) = self.peek_next_token(queue) {
            match token.kind {
                TokenKind::ControlSequence { .. }
                | TokenKind::Character {
                    catcode: CatCode::Space | CatCode::BeginGroup | CatCode::EndGroup,
                    ..
                } => break,
                TokenKind::Character { .. } => {
                    if let Some(token) = self.pop_next_token(queue) {
                        value_tokens.push(token);
                    }
                }
            }
        }
        if value_tokens.is_empty() {
            return;
        }
        let value = self.expanded_graphic_text(value_tokens);
        let key = match dimension {
            EpsfDimension::Width => "width",
            EpsfDimension::Height => "height",
        };
        let option = format!("{key}={}", value.trim());
        self.semantic_graphic.epsf_pending_options = merge_graphic_options(
            self.semantic_graphic.epsf_pending_options.take(),
            Some(&option),
        );
    }

    fn emit_executed_graphic(&mut self, input: ExecutedGraphicInput) {
        let graphic_paths = self.semantic_graphic.graphic_paths.clone();
        let graphic_extensions = self.semantic_graphic.graphic_extensions.clone();
        let source_path = self.current_execution_source_path();
        let resolved_path = self.resolve_graphic_asset_path(
            &source_path,
            &input.path,
            &graphic_paths,
            &graphic_extensions,
        );
        let resolved_asset_path = Utf8Path::new(&resolved_path);
        let asset_format = GraphicAssetFormat::from_path(&resolved_path);
        let asset_hash = self.project_file_hash(resolved_asset_path);
        let asset_dimensions =
            self.project_graphic_asset_dimensions(resolved_asset_path, asset_format);

        self.finish_executed_block_content();
        let (content_start_utf8, content_end_utf8) = input
            .argument_span
            .unwrap_or((input.invocation_start_utf8, input.invocation_end_utf8));
        let (mut source, producer) =
            self.executed_semantic_source(content_start_utf8, content_end_utf8);
        if producer == EventProducer::Primitive {
            source = SourceProvenance::file(
                source_path.clone(),
                input.invocation_start_utf8,
                input.invocation_end_utf8,
            );
            if let Some((argument_start_utf8, argument_end_utf8)) = input.argument_span {
                source = source.with_related(
                    SourceSpanRole::ArgumentContent,
                    ProvenanceSpan::File(SourceSpan {
                        path: source_path,
                        start_utf8: argument_start_utf8,
                        end_utf8: argument_end_utf8,
                    }),
                );
            }
        }

        if input.emit_legacy_placeholder {
            self.output.push_str("[image]");
            self.legacy_output_last_char = Some(']');
        }
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }

        let event = GraphicRefEvent {
            path: resolved_path,
            options: input.options.clone(),
            page_selection: parse_graphic_page_selection(input.options.as_deref()),
            asset_format,
            asset_hash,
            asset_dimensions,
        };
        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::try_from_origin(
            if input.include_pdf {
                RenderEvent::IncludePdf(event)
            } else {
                RenderEvent::GraphicRef(event)
            },
            EventBuildContext::new(event_id, source),
            event_origin_for_executed_producer(producer),
        )
        .expect("executed graphics use an origin valid for ordinary events");
        self.semantic_graphic.executed_events.push(envelope);
    }

    pub(super) fn reconcile_executed_graphic_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_graphic.scanner_event_ids);
        let scanner_default_options = mem::take(&mut self.semantic_graphic.scanner_default_options);
        let mut executed = mem::take(&mut self.semantic_graphic.executed_events);
        let overridden_invocations = mem::take(&mut self.semantic_graphic.overridden_invocations);
        if scanner_ids.is_empty() && executed.is_empty() && overridden_invocations.is_empty() {
            return;
        }

        let scanner_events = self.render_events.take_events();
        let mut reconciled = Vec::with_capacity(scanner_events.len() + executed.len());
        for mut scanner_event in scanner_events {
            if !scanner_ids.contains(&scanner_event.meta.sequence) {
                reconciled.push(scanner_event);
                continue;
            }
            if let RenderEvent::GraphicRef(graphic) | RenderEvent::IncludePdf(graphic) =
                &mut scanner_event.event
                && let Some(options) = graphic.options.as_deref()
            {
                let mut option_parts = graphic_option_parts(options)
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let mut search_start = 0usize;
                for contribution in scanner_default_options.iter().filter(|contribution| {
                    contribution.first_event_sequence <= scanner_event.meta.sequence
                }) {
                    let contribution_parts = graphic_option_parts(&contribution.options);
                    if contribution_parts.is_empty()
                        || contribution_parts.len()
                            > option_parts.len().saturating_sub(search_start)
                    {
                        continue;
                    }
                    let Some(relative_start) = option_parts[search_start..]
                        .windows(contribution_parts.len())
                        .position(|window| {
                            window
                                .iter()
                                .map(String::as_str)
                                .eq(contribution_parts.iter().copied())
                        })
                    else {
                        continue;
                    };
                    let contribution_start = search_start + relative_start;
                    let contribution_end = contribution_start + contribution_parts.len();
                    let source = SourceProvenance::file(
                        contribution.path.clone(),
                        contribution.start_utf8,
                        contribution.end_utf8,
                    );
                    if self.semantic_source_is_suppressed_in_execution(
                        &source,
                        Some(&contribution.execution_anchor),
                    ) {
                        option_parts.drain(contribution_start..contribution_end);
                        search_start = contribution_start;
                    } else {
                        search_start = contribution_end;
                    }
                }
                graphic.options = (!option_parts.is_empty()).then(|| option_parts.join(","));
                graphic.page_selection = parse_graphic_page_selection(graphic.options.as_deref());
            }
            let matching = executed.iter().position(|candidate| {
                graphic_shapes_match(candidate, &scanner_event)
                    && source_locations_overlap(&candidate.meta.source, &scanner_event.meta.source)
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
            let executed_overpic = executed_event.meta.producer == EventProducer::Primitive
                && match &executed_event.meta.source.primary {
                    ProvenanceSpan::File(span) => self
                        .render_event_sources
                        .get(&span.path)
                        .and_then(|source| {
                            source.get(span.start_utf8 as usize..span.end_utf8 as usize)
                        })
                        .and_then(|invocation| invocation.strip_prefix(r"\begin"))
                        .and_then(|invocation| {
                            let environment_start = skip_ascii_whitespace(invocation, 0);
                            read_braced_source_argument(invocation, environment_start)
                        })
                        .is_some_and(|(environment, _, _, _)| {
                            matches!(environment.trim(), "overpic" | "overpic*")
                        }),
                    ProvenanceSpan::Generated(_) => false,
                };
            if executed_overpic
                || graphic_payloads_match(&executed_event, &scanner_event)
                || graphic_options_match(&executed_event, &scanner_event)
                || graphic_executed_options_extend_scanner(&executed_event, &scanner_event)
                || (executed_event.meta.producer == EventProducer::Primitive
                    && match (&executed_event.event, &scanner_event.event) {
                        (RenderEvent::GraphicRef(executed), RenderEvent::GraphicRef(scanner))
                        | (RenderEvent::IncludePdf(executed), RenderEvent::IncludePdf(scanner)) => {
                            let scanner_only_options =
                                match (executed.options.as_deref(), scanner.options.as_deref()) {
                                    (None, Some(scanner)) => Some(scanner),
                                    (Some(executed), Some(scanner)) => scanner
                                        .strip_suffix(executed)
                                        .and_then(|prefix| prefix.strip_suffix(',')),
                                    _ => None,
                                };
                            scanner_only_options.is_some_and(|options| {
                                !options.trim().is_empty()
                                    && options.split(',').map(str::trim).all(|option| {
                                        matches!(
                                            option,
                                            "demo"
                                                | "draft"
                                                | "final"
                                                | "hiderotate"
                                                | "hidescale"
                                                | "hiresbb"
                                                | "setpagesize"
                                        )
                                    })
                            })
                        }
                        _ => false,
                    })
                || (!graphic_paths_match(&executed_event, &scanner_event)
                    && executed_event.meta.producer != EventProducer::Macro)
            {
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                if source.related.is_empty() {
                    source.related = executed_source.related.clone();
                }
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

        // Executed graphics already passed VM control flow. Source suppression is
        // only authoritative for scanner recovery and is coarse across macro calls.
        insert_unmatched_graphic_events(&mut reconciled, executed);
        self.render_events.replace_events(reconciled);
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

fn legacy_graphic_path_span(tokens: &[Token]) -> Option<(u32, u32)> {
    let mut part_start = 0usize;
    let mut depth = 0usize;
    for index in 0..=tokens.len() {
        let delimiter = index == tokens.len()
            || (depth == 0 && matches!(tokens[index].kind, TokenKind::Character { ch: ',', .. }));
        if delimiter {
            let part = &tokens[part_start..index];
            let mut part_depth = 0usize;
            let equals_index = part.iter().position(|token| match token.kind {
                TokenKind::Character {
                    catcode: CatCode::BeginGroup,
                    ..
                } => {
                    part_depth += 1;
                    false
                }
                TokenKind::Character {
                    catcode: CatCode::EndGroup,
                    ..
                } => {
                    part_depth = part_depth.saturating_sub(1);
                    false
                }
                TokenKind::Character { ch: '=', .. } => part_depth == 0,
                _ => false,
            });
            if let Some(equals_index) = equals_index {
                let key = part[..equals_index]
                    .iter()
                    .filter_map(|token| match token.kind {
                        TokenKind::Character { ch, .. } if !ch.is_whitespace() => Some(ch),
                        _ => None,
                    })
                    .collect::<String>();
                if matches!(key.as_str(), "file" | "figure") {
                    let value = &part[equals_index + 1..];
                    let mut visible = value.iter().filter(|token| {
                        !matches!(
                            token.kind,
                            TokenKind::Character {
                                catcode: CatCode::Space | CatCode::BeginGroup | CatCode::EndGroup,
                                ..
                            }
                        )
                    });
                    let first = visible.next()?;
                    let last = visible.next_back().unwrap_or(first);
                    return Some((first.span.start, last.span.end));
                }
            }
            part_start = index.saturating_add(1);
        }
        if let Some(token) = tokens.get(index) {
            match token.kind {
                TokenKind::Character {
                    catcode: CatCode::BeginGroup,
                    ..
                } => depth += 1,
                TokenKind::Character {
                    catcode: CatCode::EndGroup,
                    ..
                } => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    None
}

fn legacy_graphic_path(options: &str) -> Option<String> {
    for part in graphic_option_parts(options) {
        let mut equals_index = None;
        let mut part_depth = 0usize;
        for (index, ch) in part.char_indices() {
            match ch {
                '{' => part_depth += 1,
                '}' => part_depth = part_depth.saturating_sub(1),
                '=' if part_depth == 0 => {
                    equals_index = Some(index);
                    break;
                }
                _ => {}
            }
        }
        if let Some(equals_index) = equals_index {
            let key = part[..equals_index].trim();
            if matches!(key, "file" | "figure") {
                let value = part[equals_index + 1..].trim();
                let value = if let Some((inner, _, _, after_inner)) =
                    read_braced_source_argument(value, 0)
                    && skip_ascii_whitespace(value, after_inner) == value.len()
                {
                    inner
                } else {
                    value
                };
                let path = normalize_latex_text(value);
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn graphic_option_parts(options: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut part_start = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in options
        .char_indices()
        .chain(std::iter::once((options.len(), ',')))
    {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if brace_depth == 0 => {
                let part = options[part_start..index].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                part_start = index + 1;
            }
            _ => {}
        }
    }
    parts
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

fn graphic_executed_options_extend_scanner(
    executed: &RenderEventEnvelope,
    scanner: &RenderEventEnvelope,
) -> bool {
    let (executed, scanner) = match (&executed.event, &scanner.event) {
        (RenderEvent::GraphicRef(executed), RenderEvent::GraphicRef(scanner))
        | (RenderEvent::IncludePdf(executed), RenderEvent::IncludePdf(scanner)) => {
            (executed, scanner)
        }
        _ => return false,
    };
    match (executed.options.as_deref(), scanner.options.as_deref()) {
        (Some(executed), None) => !executed.trim().is_empty(),
        (Some(executed), Some(scanner)) => executed
            .strip_suffix(scanner)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with(',')),
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
