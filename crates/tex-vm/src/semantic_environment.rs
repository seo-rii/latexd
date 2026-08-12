use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    BeginBlockEvent, BlockKind, EndBlockEvent, EventBuildContext, EventProducer, EventSequence,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope,
};
use tex_tokens::TokenKind;

use crate::{
    Vm,
    input::QueueItem,
    semantic_list::list_kind_for_environment,
    semantic_reconciliation::source_locations_overlap,
    semantic_text::event_origin_for_executed_producer,
    snapshot::{
        VmExecutionAnchor, VmIncludedEnvironmentAuthoritySnapshot, VmSemanticEnvironmentSnapshot,
    },
};

#[derive(Debug, Default)]
pub(super) struct SemanticEnvironmentState {
    scanner_event_ids: HashSet<EventSequence>,
    executed_events: Vec<RenderEventEnvelope>,
    included_authorities: Vec<IncludedEnvironmentAuthority>,
}

#[derive(Debug, Clone)]
struct IncludedEnvironmentAuthority {
    environment: String,
    path: Utf8PathBuf,
    start_utf8: u32,
    execution_anchor: VmExecutionAnchor,
}

impl Vm<'_> {
    pub(super) fn semantic_environment_snapshot(&self) -> VmSemanticEnvironmentSnapshot {
        let mut scanner_event_ids = self
            .semantic_environment
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_event_ids.sort_unstable();
        VmSemanticEnvironmentSnapshot {
            scanner_event_ids,
            executed_events: self.semantic_environment.executed_events.clone(),
            included_authorities: self
                .semantic_environment
                .included_authorities
                .iter()
                .map(|authority| VmIncludedEnvironmentAuthoritySnapshot {
                    environment: authority.environment.clone(),
                    path: authority.path.clone(),
                    start_utf8: authority.start_utf8,
                    execution_anchor: authority.execution_anchor.clone(),
                })
                .collect(),
        }
    }

    pub(super) fn restore_semantic_environment_snapshot(
        &mut self,
        snapshot: &VmSemanticEnvironmentSnapshot,
    ) {
        self.semantic_environment.scanner_event_ids =
            snapshot.scanner_event_ids.iter().copied().collect();
        self.semantic_environment.executed_events = snapshot.executed_events.clone();
        self.semantic_environment.included_authorities = snapshot
            .included_authorities
            .iter()
            .map(|authority| IncludedEnvironmentAuthority {
                environment: authority.environment.clone(),
                path: authority.path.clone(),
                start_utf8: authority.start_utf8,
                execution_anchor: authority.execution_anchor.clone(),
            })
            .collect();
    }

    pub(super) fn mark_scanner_environment_event(&mut self, event_id: EventSequence) {
        self.semantic_environment.scanner_event_ids.insert(event_id);
    }

    pub(super) fn read_executed_environment_name(
        &mut self,
        queue: &mut VecDeque<QueueItem>,
    ) -> Option<(String, u32)> {
        let tokens = self.read_macro_argument(queue)?;
        let invocation_end_utf8 = self.last_token_end_utf8;
        Some((
            self.tokens_to_text(tokens).trim().to_string(),
            invocation_end_utf8,
        ))
    }

    pub(super) fn execution_environment_is_hidden(&self, environment: &str) -> bool {
        self.execution_hidden_environments.contains(environment)
    }

    pub(super) fn begin_included_environment_authority(
        &mut self,
        environment: &str,
        start_utf8: u32,
    ) {
        if !self.render_event_capture {
            return;
        }
        self.discard_suppression_containing_current_invocation(start_utf8);
        self.semantic_environment
            .included_authorities
            .push(IncludedEnvironmentAuthority {
                environment: environment.to_string(),
                path: self.current_execution_source_path(),
                start_utf8,
                execution_anchor: self.current_execution_anchor(),
            });
    }

    pub(super) fn end_included_environment_authority(&mut self, environment: &str, end_utf8: u32) {
        let Some(index) = self
            .semantic_environment
            .included_authorities
            .iter()
            .rposition(|authority| authority.environment == environment)
        else {
            return;
        };
        let authority = self.semantic_environment.included_authorities.remove(index);
        self.record_executed_text_authority_range(
            authority.path,
            authority.start_utf8,
            end_utf8,
            authority.execution_anchor,
        );
    }

    pub(super) fn skip_hidden_environment_body(
        &mut self,
        environment: &str,
        queue: &mut VecDeque<QueueItem>,
    ) {
        let mut skipped_start_utf8 = None;
        while let Some(token) = self.pop_next_token(queue) {
            let is_end = match &token.kind {
                TokenKind::ControlSequence { name } => {
                    if self.execute_semantic_expansion_marker(*name) {
                        continue;
                    }
                    self.interner
                        .resolve(*name)
                        .is_some_and(|name| name == "end")
                }
                TokenKind::Character { .. } => false,
            };
            skipped_start_utf8.get_or_insert(token.span.start);
            if !is_end {
                continue;
            }
            let Some((candidate, _)) = self.read_executed_environment_name(queue) else {
                continue;
            };
            if candidate == environment {
                if let Some(start_utf8) = skipped_start_utf8 {
                    self.record_suppressed_source_range(start_utf8, self.last_token_end_utf8);
                }
                break;
            }
        }
    }

    pub(super) fn emit_executed_environment_boundary(
        &mut self,
        environment: &str,
        begin: bool,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }
        let Some(block) = semantic_block_for_environment(environment) else {
            return;
        };

        self.finish_executed_block_content();
        let (source, producer) = self.executed_semantic_source(start_utf8, end_utf8);
        let event = if begin {
            RenderEvent::BeginBlock(BeginBlockEvent { block })
        } else {
            RenderEvent::EndBlock(EndBlockEvent { block })
        };
        let event_id = self.render_events.allocate_event_sequence();
        let envelope = RenderEventEnvelope::try_from_origin(
            event,
            EventBuildContext::new(event_id, source),
            event_origin_for_executed_producer(producer),
        )
        .expect("executed environment boundaries use an origin valid for ordinary events");
        self.semantic_environment.executed_events.push(envelope);
    }

    pub(super) fn executed_environment_covers_source_range(
        &self,
        path: &camino::Utf8Path,
        start_utf8: u32,
        end_utf8: u32,
    ) -> bool {
        self.semantic_environment
            .executed_events
            .iter()
            .any(|event| {
                event.meta.producer == EventProducer::Macro
                    && event_anchor(event).is_some_and(
                        |(event_path, event_start_utf8, event_end_utf8)| {
                            event_path == path
                                && event_start_utf8 < end_utf8
                                && start_utf8 < event_end_utf8
                        },
                    )
            })
    }

    pub(super) fn reconcile_executed_environment_events(&mut self) {
        let scanner_ids = mem::take(&mut self.semantic_environment.scanner_event_ids);
        let mut executed = mem::take(&mut self.semantic_environment.executed_events);
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
            let matching = executed.iter().position(|candidate| {
                environment_payloads_match(candidate, &scanner_event)
                    && source_locations_overlap(&candidate.meta.source, &scanner_event.meta.source)
            });
            if let Some(index) = matching {
                let mut executed_event = executed.remove(index);
                executed_event.meta.sequence = scanner_event.meta.sequence;
                let executed_source = executed_event.meta.source;
                let mut source = scanner_event.meta.source;
                if source.expansion_stack.is_empty() {
                    source.expansion_stack = executed_source.expansion_stack;
                    source.expansion_stack_truncated = executed_source.expansion_stack_truncated;
                }
                executed_event.meta.source = source;
                reconciled.push(executed_event);
            } else if !self.semantic_source_is_suppressed(&scanner_event.meta.source) {
                reconciled.push(scanner_event);
            }
        }

        insert_unmatched_environment_events(&mut reconciled, executed);
        let mut document_class_index = None;
        for event_index in 0..reconciled.len() {
            match &reconciled[event_index].event {
                RenderEvent::DocumentClass(_) if document_class_index.is_none() => {
                    document_class_index = Some(event_index);
                }
                RenderEvent::SetDocumentLayout(layout)
                    if scanner_ids.contains(&reconciled[event_index].meta.sequence)
                        && layout.profile.as_deref() == Some("neurips_2019") =>
                {
                    let Some(document_class_index) = document_class_index else {
                        continue;
                    };
                    let RenderEvent::DocumentClass(document_class) =
                        &mut reconciled[document_class_index].event
                    else {
                        unreachable!("the recorded event is a document class");
                    };
                    document_class.options.retain(|option| {
                        !matches!(
                            option.trim().to_ascii_lowercase().as_str(),
                            "10pt" | "11pt" | "12pt"
                        )
                    });
                    document_class.options.push("10pt".to_string());
                }
                _ => {}
            }
        }
        self.render_events.replace_events(reconciled);
    }
}

fn semantic_block_for_environment(environment: &str) -> Option<BlockKind> {
    if let Some(list_kind) = list_kind_for_environment(environment) {
        return Some(BlockKind::List { list_kind });
    }
    let float_block = match environment {
        "figwindow" | "figure" | "wrapfigure" | "wrapfigure*" | "SCfigure" | "floatingfigure"
        | "marginfigure" | "measuredfigure" => Some(BlockKind::Figure),
        "figure*" | "sidewaysfigure" | "sidewaysfigure*" | "SCfigure*" | "marginfigure*" => {
            Some(BlockKind::FullWidthFigure)
        }
        "tabwindow" | "table" | "wraptable" | "wraptable*" | "SCtable" | "floatingtable"
        | "margintable" => Some(BlockKind::Table),
        "table*" | "sidewaystable" | "sidewaystable*" | "SCtable*" | "margintable*" => {
            Some(BlockKind::FullWidthTable)
        }
        _ => None,
    };
    if float_block.is_some() {
        return float_block;
    }
    match environment {
        "abstract" | "abstract*" | "onecolabstract" => Some(BlockKind::Abstract),
        "thebibliography" => Some(BlockKind::Bibliography),
        "quote" | "quotation" | "verse" | "NoHyper" | "center" | "flushleft" | "flushright"
        | "samepage" | "titlepage" | "framed" | "shaded" | "snugshade" | "leftbar" | "oframed"
        | "tcolorbox" | "mdframed" | "displayquote" | "displayquotation" | "acknowledgements"
        | "acknowledgments" | "acknowledgement" | "acknowledgment" | "keywords" | "keyword"
        | "IEEEkeywords" | "frontmatter" | "widetext" | "strip" | "fullwidth" | "landscape"
        | "landscape*" | "CJK" | "CJK*" | "sloppypar" | "tiny" | "scriptsize" | "footnotesize"
        | "small" | "normalsize" | "large" | "Large" | "LARGE" | "huge" | "Huge" | "spacing"
        | "onehalfspace" | "doublespace" | "singlespace" | "adjustwidth" | "adjustwidth*"
        | "addmargin" | "addmargin*" | "algorithm" | "algorithm*" | "algorithmic"
        | "algorithmic*" | "subequations" | "appendices" | "subappendices" | "multicols"
        | "multicols*" | "paracol" | "paracol*" | "adjustbox" | "threeparttable" | "tablenotes"
        | "theorem" | "proof" | "lemma" | "proposition" | "corollary" | "definition" | "remark"
        | "example" => Some(BlockKind::Environment {
            name: environment.to_string(),
        }),
        _ => None,
    }
}

fn environment_payloads_match(left: &RenderEventEnvelope, right: &RenderEventEnvelope) -> bool {
    match (&left.event, &right.event) {
        (RenderEvent::BeginBlock(left), RenderEvent::BeginBlock(right)) => {
            left.block == right.block
        }
        (RenderEvent::EndBlock(left), RenderEvent::EndBlock(right)) => left.block == right.block,
        _ => false,
    }
}

fn insert_unmatched_environment_events(
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
                                            && existing.meta.sequence > event.meta.sequence))))
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
