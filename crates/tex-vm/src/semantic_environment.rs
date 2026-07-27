use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    BeginBlockEvent, BlockKind, EndBlockEvent, EventId, EventProducer, ProvenanceSpan, RenderEvent,
    RenderEventEnvelope, SourceProvenance, SourceSpan,
};

use crate::{Vm, input::QueueItem, semantic_list::list_kind_for_environment};

#[derive(Debug, Default)]
pub(super) struct SemanticEnvironmentState {
    scanner_event_ids: HashSet<EventId>,
    executed_events: Vec<RenderEventEnvelope>,
}

impl Vm<'_> {
    pub(super) fn mark_scanner_environment_event(&mut self, event_id: EventId) {
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
        let event_id = self.render_events.allocate_event_id();
        let mut envelope = RenderEventEnvelope::new(event_id, event, source);
        envelope.meta.producer = producer;
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
            if !scanner_ids.contains(&scanner_event.meta.event_id) {
                reconciled.push(scanner_event);
                continue;
            }
            let matching = executed.iter().position(|candidate| {
                environment_payloads_match(candidate, &scanner_event)
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
            } else if !self.semantic_source_is_suppressed(&scanner_event.meta.source) {
                reconciled.push(scanner_event);
            }
        }

        insert_unmatched_environment_events(&mut reconciled, executed);
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
