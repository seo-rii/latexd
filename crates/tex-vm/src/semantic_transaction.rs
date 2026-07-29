use tex_render_model::RenderEventEnvelope;

use crate::{Vm, semantic_inline::ExecutedInlineEventMark, semantic_text::ExecutedTextFlowMark};

#[derive(Debug, Clone, Copy)]
pub(super) struct ExecutedSemanticEventMark {
    text_event_mark: usize,
    inline_event_mark: ExecutedInlineEventMark,
    math_event_mark: usize,
}

impl ExecutedSemanticEventMark {
    pub(super) fn from_parts(
        text_event_mark: usize,
        inline_event_mark: ExecutedInlineEventMark,
        math_event_mark: usize,
    ) -> Self {
        Self {
            text_event_mark,
            inline_event_mark,
            math_event_mark,
        }
    }

    pub(super) fn text_event_mark(self) -> usize {
        self.text_event_mark
    }

    pub(super) fn inline_event_mark(self) -> ExecutedInlineEventMark {
        self.inline_event_mark
    }

    pub(super) fn math_event_mark(self) -> usize {
        self.math_event_mark
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ExecutedSemanticFlowMark {
    text_flow_mark: ExecutedTextFlowMark,
    inline_event_mark: ExecutedInlineEventMark,
    math_event_mark: usize,
}

impl ExecutedSemanticFlowMark {
    pub(super) fn from_parts(
        text_flow_mark: ExecutedTextFlowMark,
        inline_event_mark: ExecutedInlineEventMark,
        math_event_mark: usize,
    ) -> Self {
        Self {
            text_flow_mark,
            inline_event_mark,
            math_event_mark,
        }
    }

    pub(super) fn text_flow_mark(self) -> ExecutedTextFlowMark {
        self.text_flow_mark
    }

    pub(super) fn inline_event_mark(self) -> ExecutedInlineEventMark {
        self.inline_event_mark
    }

    pub(super) fn math_event_mark(self) -> usize {
        self.math_event_mark
    }
}

impl Vm<'_> {
    pub(super) fn mark_executed_semantic_events(&mut self) -> ExecutedSemanticEventMark {
        ExecutedSemanticEventMark::from_parts(
            self.executed_text_event_mark(),
            self.executed_inline_event_mark(),
            self.executed_math_event_mark(),
        )
    }

    pub(super) fn rollback_executed_semantic_events(&mut self, mark: ExecutedSemanticEventMark) {
        self.rollback_executed_text_events(mark.text_event_mark());
        self.rollback_executed_inline_events(mark.inline_event_mark());
        self.rollback_executed_math_events(mark.math_event_mark());
    }

    pub(super) fn mark_executed_semantic_flow(&mut self) -> ExecutedSemanticFlowMark {
        ExecutedSemanticFlowMark::from_parts(
            self.executed_text_flow_mark(),
            self.executed_inline_event_mark(),
            self.executed_math_event_mark(),
        )
    }

    pub(super) fn take_executed_semantic_events_since(
        &mut self,
        mark: ExecutedSemanticFlowMark,
    ) -> Vec<RenderEventEnvelope> {
        let mut events = self.take_executed_text_events_since(mark.text_flow_mark());
        events.extend(self.take_executed_inline_events_since(mark.inline_event_mark()));
        events.extend(self.take_executed_math_events_since(mark.math_event_mark()));
        events.sort_by_key(|event| event.meta.event_id);
        events
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        semantic_inline::ExecutedInlineEventMark,
        semantic_text::ExecutedTextFlowMark,
        snapshot::{VmExecutedInlineEventMarkSnapshot, VmExecutedTextFlowMarkSnapshot},
    };

    use super::{ExecutedSemanticEventMark, ExecutedSemanticFlowMark};

    #[test]
    fn semantic_event_mark_preserves_each_nested_buffer_cursor() {
        let inline = ExecutedInlineEventMark::restore(&VmExecutedInlineEventMarkSnapshot {
            citations: 2,
            references: 3,
            links: 5,
            labels: 7,
            caption_placeholders: 11,
        });
        let mark = ExecutedSemanticEventMark::from_parts(13, inline, 17);

        assert_eq!(mark.text_event_mark(), 13);
        assert_eq!(mark.inline_event_mark().snapshot(), inline.snapshot());
        assert_eq!(mark.math_event_mark(), 17);
    }

    #[test]
    fn semantic_flow_mark_preserves_text_state_and_nested_buffer_cursors() {
        let text_flow = ExecutedTextFlowMark::restore(&VmExecutedTextFlowMarkSnapshot {
            event_mark: 19,
            paragraph_has_content: true,
            space_run_active: false,
        });
        let inline = ExecutedInlineEventMark::restore(&VmExecutedInlineEventMarkSnapshot {
            citations: 23,
            references: 29,
            links: 31,
            labels: 37,
            caption_placeholders: 41,
        });
        let mark = ExecutedSemanticFlowMark::from_parts(text_flow, inline, 43);

        assert_eq!(mark.text_flow_mark().snapshot(), text_flow.snapshot());
        assert_eq!(mark.inline_event_mark().snapshot(), inline.snapshot());
        assert_eq!(mark.math_event_mark(), 43);
    }
}
