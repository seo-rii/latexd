use std::ops::{Deref, DerefMut};

use tex_render_model::{EventId, RenderEventEnvelope};

use crate::snapshot::VmSemanticSinkSnapshot;

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
    batch_start_event_id: EventId,
    epoch: u64,
}

impl Default for SemanticEventBuffer {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            next_event_id: 1,
            batch_start_event_id: 1,
            epoch: 0,
        }
    }
}

impl SemanticEventBuffer {
    pub(super) fn snapshot(&self) -> VmSemanticSinkSnapshot {
        VmSemanticSinkSnapshot {
            events: self.events.clone(),
            next_event_id: self.next_event_id,
            batch_start_event_id: self.batch_start_event_id,
            epoch: self.epoch,
        }
    }

    pub(super) fn restore(snapshot: &VmSemanticSinkSnapshot) -> Option<Self> {
        snapshot.is_restorable().then(|| Self {
            events: snapshot.events.clone(),
            next_event_id: snapshot.next_event_id,
            batch_start_event_id: snapshot.batch_start_event_id,
            epoch: snapshot.epoch,
        })
    }

    pub(super) fn allocate_event_id(&mut self) -> EventId {
        let event_id = self.next_event_id;
        self.next_event_id += 1;
        event_id
    }

    pub(super) fn next_event_id(&self) -> EventId {
        self.next_event_id
    }

    pub(super) fn batch_start_event_id(&self) -> EventId {
        self.batch_start_event_id
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

    pub(super) fn set_replay_prefix(&mut self, events: Vec<RenderEventEnvelope>) {
        self.next_event_id = events
            .iter()
            .map(|event| event.meta.event_id.saturating_add(1))
            .max()
            .unwrap_or(self.next_event_id)
            .max(self.next_event_id);
        self.batch_start_event_id = self.next_event_id;
        self.events = events;
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(super) fn take_events(&mut self) -> Vec<RenderEventEnvelope> {
        self.epoch = self.epoch.wrapping_add(1);
        std::mem::take(&mut self.events)
    }

    pub(super) fn finish_batch(&mut self) -> Vec<RenderEventEnvelope> {
        let events = self.take_events();
        self.batch_start_event_id = self.next_event_id;
        events
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
    fn finishing_and_replacing_events_preserves_the_allocator() {
        let mut buffer = SemanticEventBuffer::default();
        assert_eq!(emit_text(&mut buffer, "first"), 1);

        let events = buffer.finish_batch();
        assert!(buffer.is_empty());
        assert_eq!(buffer.batch_start_event_id(), 2);
        buffer.replace_events(events);

        assert_eq!(emit_text(&mut buffer, "second"), 2);
    }

    #[test]
    fn snapshot_roundtrip_preserves_events_allocator_and_epoch() {
        let mut buffer = SemanticEventBuffer::default();
        assert_eq!(emit_text(&mut buffer, "first"), 1);
        let events = buffer.to_vec();
        buffer.replace_events(events);
        let snapshot = buffer.snapshot();

        let restored = SemanticEventBuffer::restore(&snapshot).expect("restorable snapshot");

        assert_eq!(restored, buffer);
    }

    #[test]
    fn replay_prefix_starts_a_new_event_batch_after_the_prefix() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "prefix");
        let prefix = buffer.finish_batch();

        buffer.set_replay_prefix(prefix);

        assert_eq!(buffer.batch_start_event_id(), 2);
        assert_eq!(emit_text(&mut buffer, "body"), 2);
        assert!(SemanticEventBuffer::restore(&buffer.snapshot()).is_some());
    }

    #[test]
    fn restore_rejects_duplicate_or_out_of_range_event_ids() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "first");
        let mut snapshot = buffer.snapshot();
        snapshot.events.push(snapshot.events[0].clone());

        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.events.pop();
        snapshot.next_event_id = 1;
        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.next_event_id = 2;
        snapshot.batch_start_event_id = 3;
        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.next_event_id = 3;
        snapshot.batch_start_event_id = 2;
        assert!(SemanticEventBuffer::restore(&snapshot).is_some());
    }
}
