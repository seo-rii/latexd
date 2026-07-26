use std::{collections::VecDeque, mem};

use camino::Utf8PathBuf;
use tex_render_model::{
    EventProducer, ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpan,
    SourceSpanRole,
};
use tex_tokens::{CatCode, TokenKind};

use crate::{Vm, input::QueueItem, math_source_event};

#[derive(Debug)]
pub(super) struct ExecutedMathCapture {
    display: bool,
    raw_source: String,
    source_path: Utf8PathBuf,
    invocation_start_utf8: u32,
    content_start_utf8: u32,
}

impl Vm<'_> {
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
            self.mark_executed_inline_content();
            let source_path = self
                .source_stack
                .last()
                .map(|frame| frame.path.clone())
                .or_else(|| self.entry_source_path.clone())
                .unwrap_or_else(|| Utf8PathBuf::from("texput.tex"));
            self.executed_math_invocations
                .insert((source_path.clone(), start_utf8));
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
                self.executed_math_capture = Some(ExecutedMathCapture {
                    display: true,
                    raw_source: String::new(),
                    source_path,
                    invocation_start_utf8: start_utf8,
                    content_start_utf8: second_shift.span.end,
                });
            } else {
                self.executed_math_capture = Some(ExecutedMathCapture {
                    display: false,
                    raw_source: String::new(),
                    source_path,
                    invocation_start_utf8: start_utf8,
                    content_start_utf8: end_utf8,
                });
            }
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

    fn finish_executed_math_capture(&mut self, content_end_utf8: u32, invocation_end_utf8: u32) {
        let Some(capture) = self.executed_math_capture.take() else {
            return;
        };
        let raw_source = self
            .render_event_sources
            .get(&capture.source_path)
            .and_then(|source| {
                source.get(capture.content_start_utf8 as usize..content_end_utf8 as usize)
            })
            .unwrap_or(&capture.raw_source);
        let event = if capture.display {
            RenderEvent::DisplayMath(math_source_event(raw_source))
        } else {
            RenderEvent::InlineMath(math_source_event(raw_source))
        };
        let event_id = self.next_render_event_id;
        self.next_render_event_id += 1;
        let source_path = capture.source_path.clone();
        let mut envelope = RenderEventEnvelope::new(
            event_id,
            event,
            SourceProvenance::file(
                capture.source_path,
                capture.content_start_utf8,
                content_end_utf8,
            )
            .with_related(
                SourceSpanRole::Invocation,
                ProvenanceSpan::File(SourceSpan {
                    path: source_path,
                    start_utf8: capture.invocation_start_utf8,
                    end_utf8: invocation_end_utf8,
                }),
            ),
        );
        envelope.meta.producer = EventProducer::Primitive;
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
    }

    pub(super) fn reconcile_executed_math_events(&mut self) {
        let scanner_dollar_ids = mem::take(&mut self.scanner_dollar_math_event_ids);
        let executed_invocations = mem::take(&mut self.executed_math_invocations);
        let mut executed = mem::take(&mut self.executed_math_events);
        if scanner_dollar_ids.is_empty() {
            self.render_events.append(&mut executed);
            return;
        }

        let invocation_key = |event: &RenderEventEnvelope| {
            event.meta.source.related.iter().find_map(|related| {
                if related.role != SourceSpanRole::Invocation {
                    return None;
                }
                match &related.span {
                    ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8)),
                    ProvenanceSpan::Generated(_) => None,
                }
            })
        };
        let mut reconciled = Vec::with_capacity(self.render_events.len() + executed.len());
        for scanner_event in self.render_events.drain(..) {
            if !scanner_dollar_ids.contains(&scanner_event.meta.event_id) {
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
                let scanner_invocation = invocation_key(&scanner_event);
                if scanner_invocation
                    .as_ref()
                    .is_some_and(|key| executed_invocations.contains(key))
                {
                    if let Some(index) = executed
                        .iter()
                        .position(|candidate| invocation_key(candidate) == scanner_invocation)
                    {
                        executed.remove(index);
                    }
                    reconciled.push(scanner_event);
                }
                continue;
            };
            let mut executed_event = executed.remove(index);
            executed_event.meta.event_id = scanner_event.meta.event_id;
            executed_event.meta.source = scanner_event.meta.source;
            reconciled.push(executed_event);
        }
        reconciled.append(&mut executed);
        self.render_events = reconciled;
    }
}
