use crate::{Vm, semantic_inline::ExecutedInlineEventMark};

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
}

#[cfg(test)]
mod tests {
    use crate::{
        semantic_inline::ExecutedInlineEventMark, snapshot::VmExecutedInlineEventMarkSnapshot,
    };

    use super::ExecutedSemanticEventMark;

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
}
