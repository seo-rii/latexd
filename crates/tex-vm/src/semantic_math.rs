use std::{
    collections::{HashSet, VecDeque},
    mem,
};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventProducer, ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpan,
    SourceSpanRole,
};
use tex_tokens::{CatCode, Token, TokenKind};

use crate::{
    Vm,
    command::MathDelimiterCommand,
    input::QueueItem,
    math_source_event,
    snapshot::{
        VmExecutedMathCaptureSnapshot, VmSemanticMathInvocationSnapshot, VmSemanticMathSnapshot,
    },
};

#[derive(Debug)]
pub(super) struct ExecutedMathCapture {
    display: bool,
    command_delimited: bool,
    environment: Option<String>,
    raw_source: String,
    source_path: Utf8PathBuf,
    invocation_start_utf8: u32,
    content_start_utf8: u32,
    semantic_source: SourceProvenance,
    producer: EventProducer,
}

impl Vm<'_> {
    pub(super) fn semantic_math_snapshot(&self) -> VmSemanticMathSnapshot {
        let mut scanner_dollar_event_ids = self
            .scanner_dollar_math_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_dollar_event_ids.sort_unstable();
        let mut scanner_command_event_ids = self
            .scanner_command_math_event_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        scanner_command_event_ids.sort_unstable();
        let mut executed_invocations = self
            .executed_math_invocations
            .iter()
            .map(|(path, start_utf8)| VmSemanticMathInvocationSnapshot {
                path: path.clone(),
                start_utf8: *start_utf8,
            })
            .collect::<Vec<_>>();
        executed_invocations.sort();
        VmSemanticMathSnapshot {
            scanner_dollar_event_ids,
            scanner_command_event_ids,
            executed_invocations,
            executed_events: self.executed_math_events.clone(),
            active_capture: self.executed_math_capture.as_ref().map(|capture| {
                VmExecutedMathCaptureSnapshot {
                    display: capture.display,
                    command_delimited: capture.command_delimited,
                    environment: capture.environment.clone(),
                    raw_source: capture.raw_source.clone(),
                    source_path: capture.source_path.clone(),
                    invocation_start_utf8: capture.invocation_start_utf8,
                    content_start_utf8: capture.content_start_utf8,
                    semantic_source: Some(capture.semantic_source.clone()),
                    producer: Some(capture.producer),
                }
            }),
        }
    }

    pub(super) fn restore_semantic_math_snapshot(&mut self, snapshot: &VmSemanticMathSnapshot) {
        self.scanner_dollar_math_event_ids =
            snapshot.scanner_dollar_event_ids.iter().copied().collect();
        self.scanner_command_math_event_ids =
            snapshot.scanner_command_event_ids.iter().copied().collect();
        self.executed_math_invocations = snapshot
            .executed_invocations
            .iter()
            .map(|invocation| (invocation.path.clone(), invocation.start_utf8))
            .collect();
        self.executed_math_events = snapshot.executed_events.clone();
        self.executed_math_capture =
            snapshot
                .active_capture
                .as_ref()
                .map(|capture| ExecutedMathCapture {
                    display: capture.display,
                    command_delimited: capture.command_delimited,
                    environment: capture.environment.clone(),
                    raw_source: capture.raw_source.clone(),
                    source_path: capture.source_path.clone(),
                    invocation_start_utf8: capture.invocation_start_utf8,
                    content_start_utf8: capture.content_start_utf8,
                    semantic_source: capture.semantic_source.clone().unwrap_or_else(|| {
                        SourceProvenance::file(
                            capture.source_path.clone(),
                            capture.invocation_start_utf8,
                            capture.content_start_utf8,
                        )
                    }),
                    producer: capture.producer.unwrap_or(EventProducer::Primitive),
                });
    }

    pub(super) fn execute_math_shift(
        &mut self,
        ch: char,
        start_utf8: u32,
        end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            self.push_legacy_math_shift(ch);
            return;
        }

        if self.executed_math_capture.is_none() {
            let second_shift = self
                .peek_next_token(queue)
                .filter(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Character {
                            catcode: CatCode::MathShift,
                            ..
                        }
                    )
                })
                .and_then(|_| self.pop_next_token(queue));
            self.push_legacy_math_shift(ch);
            if let Some(second_shift) = &second_shift {
                self.push_legacy_math_shift(ch);
                self.begin_executed_math_capture(
                    true,
                    false,
                    None,
                    start_utf8,
                    second_shift.span.end,
                    second_shift.span.end,
                );
            } else {
                self.begin_executed_math_capture(
                    false, false, None, start_utf8, end_utf8, end_utf8,
                );
            }
            return;
        }

        if self
            .executed_math_capture
            .as_ref()
            .is_some_and(|capture| capture.command_delimited)
        {
            if let Some(capture) = &mut self.executed_math_capture {
                capture.raw_source.push(ch);
            }
            self.output.push(ch);
            self.legacy_output_last_char = Some(ch);
            return;
        }

        let display = self
            .executed_math_capture
            .as_ref()
            .is_some_and(|capture| capture.display);
        if display {
            let second_shift = self
                .peek_next_token(queue)
                .filter(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Character {
                            catcode: CatCode::MathShift,
                            ..
                        }
                    )
                })
                .and_then(|_| self.pop_next_token(queue));
            if second_shift.is_none() {
                if let Some(capture) = &mut self.executed_math_capture {
                    capture.raw_source.push(ch);
                }
                self.push_legacy_math_shift(ch);
                return;
            }
            let invocation_end_utf8 = second_shift
                .as_ref()
                .map_or(end_utf8, |token| token.span.end);
            self.push_legacy_math_shift(ch);
            self.push_legacy_math_shift(ch);
            self.finish_executed_math_capture(start_utf8, invocation_end_utf8);
        } else {
            self.push_legacy_math_shift(ch);
            self.finish_executed_math_capture(start_utf8, end_utf8);
        }
    }

    pub(super) fn execute_command_math_delimiter(
        &mut self,
        delimiter: MathDelimiterCommand,
        start_utf8: u32,
        end_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            self.push_legacy_command_math_shift(delimiter.is_display());
            return;
        }

        if delimiter.is_open() {
            if self.executed_math_capture.is_some() {
                self.capture_executed_math_command(delimiter.source());
                return;
            }
            self.push_legacy_command_math_shift(delimiter.is_display());
            self.begin_executed_math_capture(
                delimiter.is_display(),
                true,
                None,
                start_utf8,
                end_utf8,
                end_utf8,
            );
            return;
        }

        let closes_active_capture = self.executed_math_capture.as_ref().is_some_and(|capture| {
            capture.command_delimited
                && capture.environment.is_none()
                && capture.display == delimiter.is_display()
        });
        if !closes_active_capture {
            self.capture_executed_math_command(delimiter.source());
            return;
        }

        self.push_legacy_command_math_shift(delimiter.is_display());
        self.finish_executed_math_capture(start_utf8, end_utf8);
    }

    pub(super) fn execute_ensuremath(
        &mut self,
        start_utf8: u32,
        end_utf8: u32,
        queue: &mut VecDeque<QueueItem>,
    ) {
        let Some(argument) = self.read_macro_argument(queue) else {
            return;
        };
        if self.legacy_math_output_active || self.executed_math_capture.is_some() {
            for token in argument.into_iter().rev() {
                self.push_token_front(queue, token);
            }
            return;
        }

        let invocation_end_utf8 = self.last_token_end_utf8.max(end_utf8);
        let content_start_utf8 = argument
            .first()
            .map_or(invocation_end_utf8.saturating_sub(1), |token| {
                token.span.start
            });
        let content_end_utf8 = argument
            .last()
            .map_or(content_start_utf8, |token| token.span.end);
        self.push_token_front(
            queue,
            Token::character(
                '$',
                CatCode::MathShift,
                content_end_utf8 as usize,
                invocation_end_utf8 as usize,
            ),
        );
        for token in argument.into_iter().rev() {
            self.push_token_front(queue, token);
        }
        self.push_token_front(
            queue,
            Token::character(
                '$',
                CatCode::MathShift,
                start_utf8 as usize,
                content_start_utf8 as usize,
            ),
        );
    }

    pub(super) fn execute_math_environment_boundary(
        &mut self,
        environment: &str,
        begin: bool,
        start_utf8: u32,
        end_utf8: u32,
        content_start_utf8: u32,
    ) {
        if !self.render_event_capture || !self.execution_in_document {
            return;
        }

        if begin {
            if is_display_math_environment(environment) && self.executed_math_capture.is_none() {
                self.begin_executed_math_capture(
                    true,
                    true,
                    Some(environment.to_string()),
                    start_utf8,
                    end_utf8,
                    content_start_utf8,
                );
                return;
            }
            self.capture_executed_math_environment_boundary(environment, true);
            return;
        }

        let closes_active_capture = self
            .executed_math_capture
            .as_ref()
            .and_then(|capture| capture.environment.as_deref())
            == Some(environment);
        if closes_active_capture {
            self.finish_executed_math_capture(start_utf8, end_utf8);
        } else {
            self.capture_executed_math_environment_boundary(environment, false);
        }
    }

    fn capture_executed_math_environment_boundary(&mut self, environment: &str, begin: bool) {
        let Some(capture) = &mut self.executed_math_capture else {
            return;
        };
        if begin {
            capture.raw_source.push_str(r"\begin{");
        } else {
            capture.raw_source.push_str(r"\end{");
        }
        capture.raw_source.push_str(environment);
        capture.raw_source.push('}');
    }

    fn push_legacy_command_math_shift(&mut self, display: bool) {
        self.push_legacy_math_shift('$');
        if display {
            self.push_legacy_math_shift('$');
        }
    }

    fn capture_executed_math_command(&mut self, command: &str) {
        let Some(capture) = &mut self.executed_math_capture else {
            return;
        };
        capture.raw_source.push('\\');
        capture.raw_source.push_str(command);
    }

    fn begin_executed_math_capture(
        &mut self,
        display: bool,
        command_delimited: bool,
        environment: Option<String>,
        invocation_start_utf8: u32,
        invocation_end_utf8: u32,
        content_start_utf8: u32,
    ) {
        self.mark_executed_inline_content();
        let source_path = self.current_execution_source_path();
        let (semantic_source, producer) =
            self.executed_semantic_source(invocation_start_utf8, invocation_end_utf8);
        let invocation_key = if producer == EventProducer::Macro {
            provenance_primary_key(&semantic_source)
        } else {
            Some((source_path.clone(), invocation_start_utf8))
        };
        if let Some(invocation_key) = invocation_key {
            self.executed_math_invocations.insert(invocation_key);
        }
        self.executed_math_capture = Some(ExecutedMathCapture {
            display,
            command_delimited,
            environment,
            raw_source: String::new(),
            source_path,
            invocation_start_utf8,
            content_start_utf8,
            semantic_source,
            producer,
        });
    }

    fn finish_executed_math_capture(&mut self, content_end_utf8: u32, invocation_end_utf8: u32) {
        let Some(capture) = self.executed_math_capture.take() else {
            return;
        };
        let closing_source_path = self.current_execution_source_path();
        let raw_source = if capture.producer == EventProducer::Macro
            || closing_source_path != capture.source_path
        {
            capture.raw_source.as_str()
        } else {
            self.render_event_sources
                .get(&capture.source_path)
                .and_then(|source| {
                    source.get(capture.content_start_utf8 as usize..content_end_utf8 as usize)
                })
                .unwrap_or(&capture.raw_source)
        };
        let event = if capture.display {
            RenderEvent::DisplayMath(math_source_event(raw_source))
        } else {
            RenderEvent::InlineMath(math_source_event(raw_source))
        };
        let event_id = self.render_events.allocate_event_id();
        let source_path = capture.source_path.clone();
        let invocation_span = if capture.producer == EventProducer::Macro {
            capture.semantic_source.primary.clone()
        } else {
            ProvenanceSpan::File(SourceSpan {
                path: source_path.clone(),
                start_utf8: capture.invocation_start_utf8,
                end_utf8: invocation_end_utf8,
            })
        };
        let mut source = if capture.producer == EventProducer::Macro {
            capture.semantic_source
        } else {
            SourceProvenance::file(
                capture.source_path,
                capture.content_start_utf8,
                content_end_utf8,
            )
        };
        source
            .related
            .retain(|related| related.role != SourceSpanRole::Invocation);
        source = source.with_related(SourceSpanRole::Invocation, invocation_span);
        let mut envelope = RenderEventEnvelope::new(event_id, event, source);
        envelope.meta.producer = capture.producer;
        self.executed_math_events.push(envelope);
    }

    pub(super) fn capture_executed_math_character(&mut self, ch: char) {
        if let Some(capture) = &mut self.executed_math_capture {
            capture.raw_source.push(ch);
        }
    }

    pub(super) fn capture_executed_math_control_sequence(&mut self, name: &str) {
        let Some(capture) = &mut self.executed_math_capture else {
            return;
        };
        capture.raw_source.push('\\');
        capture.raw_source.push_str(name);
        if name.chars().all(|ch| ch.is_ascii_alphabetic()) {
            capture.raw_source.push(' ');
        }
    }

    pub(super) fn executed_math_event_mark(&self) -> usize {
        self.executed_math_events.len()
    }

    pub(super) fn rollback_executed_math_events(&mut self, mark: usize) {
        self.executed_math_events.truncate(mark);
        let mut retained_invocations = self
            .executed_math_events
            .iter()
            .filter_map(math_invocation_key)
            .collect::<HashSet<_>>();
        if let Some(capture) = &self.executed_math_capture {
            let invocation_key = if capture.producer == EventProducer::Macro {
                provenance_primary_key(&capture.semantic_source)
            } else {
                Some((capture.source_path.clone(), capture.invocation_start_utf8))
            };
            retained_invocations.extend(invocation_key);
        }
        self.executed_math_invocations = retained_invocations;
    }

    pub(super) fn reconcile_executed_math_events(&mut self) {
        let mut scanner_math_ids = mem::take(&mut self.scanner_dollar_math_event_ids);
        scanner_math_ids.extend(mem::take(&mut self.scanner_command_math_event_ids));
        let executed_invocations = mem::take(&mut self.executed_math_invocations);
        let mut executed = mem::take(&mut self.executed_math_events);
        if scanner_math_ids.is_empty() {
            self.render_events.append(&mut executed);
            return;
        }

        let mut reconciled = Vec::with_capacity(self.render_events.len() + executed.len());
        for scanner_event in self.render_events.drain(..) {
            if !scanner_math_ids.contains(&scanner_event.meta.event_id) {
                reconciled.push(scanner_event);
                continue;
            }
            let scanner_math = match &scanner_event.event {
                RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math) => math,
                _ => {
                    reconciled.push(scanner_event);
                    continue;
                }
            };
            let matching_event = executed.iter().position(|candidate| {
                matches!(
                    &candidate.event,
                    RenderEvent::InlineMath(math) | RenderEvent::DisplayMath(math)
                        if math.raw_source == scanner_math.raw_source
                ) && candidate.meta.source.primary == scanner_event.meta.source.primary
            });
            let Some(index) = matching_event else {
                let scanner_invocation = math_invocation_key(&scanner_event);
                if scanner_invocation
                    .as_ref()
                    .is_some_and(|key| executed_invocations.contains(key))
                {
                    if let Some(index) = executed
                        .iter()
                        .position(|candidate| math_invocation_key(candidate) == scanner_invocation)
                    {
                        executed.remove(index);
                    }
                    reconciled.push(scanner_event);
                }
                continue;
            };
            let mut executed_event = executed.remove(index);
            executed_event.meta.event_id = scanner_event.meta.event_id;
            executed_event.meta.source =
                reconcile_math_source(executed_event.meta.source, scanner_event.meta.source);
            reconciled.push(executed_event);
        }
        reconciled.append(&mut executed);
        self.render_events.replace_events(reconciled);
    }
}

pub(super) fn is_display_math_environment(environment: &str) -> bool {
    matches!(
        environment,
        "equation"
            | "equation*"
            | "displaymath"
            | "align"
            | "align*"
            | "flalign"
            | "flalign*"
            | "alignat"
            | "alignat*"
            | "gather"
            | "gather*"
            | "multline"
            | "multline*"
            | "eqnarray"
            | "eqnarray*"
    )
}

pub(super) fn starts_with_display_math_environment(source: &str) -> bool {
    let Some(source) = source.strip_prefix(r"\begin{") else {
        return false;
    };
    let Some(environment_end) = source.find('}') else {
        return false;
    };
    is_display_math_environment(&source[..environment_end])
}

fn provenance_primary_key(source: &SourceProvenance) -> Option<(Utf8PathBuf, u32)> {
    match &source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}

fn math_invocation_key(event: &RenderEventEnvelope) -> Option<(Utf8PathBuf, u32)> {
    event.meta.source.related.iter().find_map(|related| {
        if related.role != SourceSpanRole::Invocation {
            return None;
        }
        match &related.span {
            ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8)),
            ProvenanceSpan::Generated(_) => None,
        }
    })
}

fn reconcile_math_source(
    mut executed: SourceProvenance,
    scanner: SourceProvenance,
) -> SourceProvenance {
    if executed.expansion_stack.is_empty() {
        return scanner;
    }
    for frame in &mut executed.expansion_stack {
        if frame.definition_span.is_none()
            && let Some(scanner_frame) = scanner
                .expansion_stack
                .iter()
                .find(|scanner_frame| scanner_frame.command_name == frame.command_name)
        {
            frame.definition_span = scanner_frame.definition_span.clone();
        }
    }
    for related in scanner.related {
        if !executed.related.contains(&related) {
            executed.related.push(related);
        }
    }
    executed
}

impl MathDelimiterCommand {
    fn is_open(self) -> bool {
        matches!(self, Self::InlineOpen | Self::DisplayOpen)
    }

    fn is_display(self) -> bool {
        matches!(self, Self::DisplayOpen | Self::DisplayClose)
    }

    fn source(self) -> &'static str {
        match self {
            Self::InlineOpen => "(",
            Self::InlineClose => ")",
            Self::DisplayOpen => "[",
            Self::DisplayClose => "]",
        }
    }
}
