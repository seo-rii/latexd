use std::ops::{Deref, DerefMut};

use tex_render_model::{EventId, RenderEventEnvelope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Used by the next Snapshot v2 integration step.
pub(super) struct SemanticSinkMark {
    event_len: usize,
    next_event_id: EventId,
    epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticEventBuffer {
    events: Vec<RenderEventEnvelope>,
    next_event_id: EventId,
    epoch: u64,
}

impl Default for SemanticEventBuffer {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            next_event_id: 1,
            epoch: 0,
        }
    }
}

impl SemanticEventBuffer {
    pub(super) fn allocate_event_id(&mut self) -> EventId {
        let event_id = self.next_event_id;
        self.next_event_id += 1;
        event_id
    }

    pub(super) fn next_event_id(&self) -> EventId {
        self.next_event_id
    }

    pub(super) fn set_next_event_id(&mut self, next_event_id: EventId) {
        self.next_event_id = next_event_id.max(1);
    }

    #[allow(dead_code)]
    pub(super) fn mark(&self) -> SemanticSinkMark {
        SemanticSinkMark {
            event_len: self.events.len(),
            next_event_id: self.next_event_id,
            epoch: self.epoch,
        }
    }

    #[allow(dead_code)]
    pub(super) fn rollback(&mut self, mark: SemanticSinkMark) -> bool {
        if !self.commit(mark) {
            return false;
        }
        self.events.truncate(mark.event_len);
        self.next_event_id = mark.next_event_id;
        true
    }

    #[allow(dead_code)]
    pub(super) fn commit(&self, mark: SemanticSinkMark) -> bool {
        mark.epoch == self.epoch
            && mark.event_len <= self.events.len()
            && mark.next_event_id <= self.next_event_id
    }

    pub(super) fn replace_events(&mut self, events: Vec<RenderEventEnvelope>) {
        self.events = events;
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(super) fn take_events(&mut self) -> Vec<RenderEventEnvelope> {
        self.epoch = self.epoch.wrapping_add(1);
        std::mem::take(&mut self.events)
    }
}

impl Deref for SemanticEventBuffer {
    type Target = Vec<RenderEventEnvelope>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl DerefMut for SemanticEventBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}

#[cfg(test)]
mod tests {
    use tex_render_model::{RenderEvent, RenderEventEnvelope, SourceProvenance, TextEvent};

    use super::SemanticEventBuffer;

    fn emit_text(buffer: &mut SemanticEventBuffer, text: &str) -> u64 {
        let event_id = buffer.allocate_event_id();
        buffer.push(RenderEventEnvelope::new(
            event_id,
            RenderEvent::Text(TextEvent {
                text: text.to_string(),
            }),
            SourceProvenance::file("main.tex", 0, text.len() as u32),
        ));
        event_id
    }

    #[test]
    fn rollback_discards_events_and_reuses_event_ids() {
        let mut buffer = SemanticEventBuffer::default();
        assert_eq!(emit_text(&mut buffer, "before"), 1);
        let mark = buffer.mark();
        assert_eq!(emit_text(&mut buffer, "discarded"), 2);

        assert!(buffer.rollback(mark));
        assert_eq!(buffer.len(), 1);
        assert_eq!(emit_text(&mut buffer, "replacement"), 2);
    }

    #[test]
    fn nested_marks_can_commit_inner_work_then_rollback_outer_work() {
        let mut buffer = SemanticEventBuffer::default();
        let outer = buffer.mark();
        assert_eq!(emit_text(&mut buffer, "outer"), 1);
        let inner = buffer.mark();
        assert_eq!(emit_text(&mut buffer, "inner"), 2);

        assert!(buffer.commit(inner));
        assert!(buffer.rollback(outer));
        assert!(buffer.is_empty());
        assert_eq!(emit_text(&mut buffer, "replacement"), 1);
    }

    #[test]
    fn replacing_events_invalidates_older_marks() {
        let mut buffer = SemanticEventBuffer::default();
        let mark = buffer.mark();

        buffer.replace_events(Vec::new());

        assert!(!buffer.rollback(mark));
        assert!(!buffer.commit(mark));
    }

    #[test]
    fn taking_and_replacing_events_preserves_the_allocator() {
        let mut buffer = SemanticEventBuffer::default();
        assert_eq!(emit_text(&mut buffer, "first"), 1);

        let events = buffer.take_events();
        assert!(buffer.is_empty());
        buffer.replace_events(events);

        assert_eq!(emit_text(&mut buffer, "second"), 2);
    }
}
