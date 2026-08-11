use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
};

use tex_render_model::{EventSequence, RenderEventEnvelope};

use crate::snapshot::VmSemanticSinkSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Used by the next Snapshot v2 integration step.
pub(super) struct SemanticSinkMark {
    event_len: usize,
    next_event_sequence: EventSequence,
    epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticEventBuffer {
    events: Vec<RenderEventEnvelope>,
    next_event_sequence: EventSequence,
    batch_start_event_sequence: EventSequence,
    epoch: u64,
}

impl Default for SemanticEventBuffer {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            next_event_sequence: 1,
            batch_start_event_sequence: 1,
            epoch: 0,
        }
    }
}

impl SemanticEventBuffer {
    pub(super) fn snapshot(&self) -> VmSemanticSinkSnapshot {
        VmSemanticSinkSnapshot {
            events: self.events.clone(),
            next_event_sequence: self.next_event_sequence,
            batch_start_event_sequence: self.batch_start_event_sequence,
            epoch: self.epoch,
        }
    }

    pub(super) fn restore(snapshot: &VmSemanticSinkSnapshot) -> Option<Self> {
        snapshot.is_restorable().then(|| Self {
            events: snapshot.events.clone(),
            next_event_sequence: snapshot.next_event_sequence,
            batch_start_event_sequence: snapshot.batch_start_event_sequence,
            epoch: snapshot.epoch,
        })
    }

    pub(super) fn allocate_event_sequence(&mut self) -> EventSequence {
        let event_sequence = self.next_event_sequence;
        self.next_event_sequence += 1;
        event_sequence
    }

    pub(super) fn next_event_sequence(&self) -> EventSequence {
        self.next_event_sequence
    }

    pub(super) fn batch_start_event_sequence(&self) -> EventSequence {
        self.batch_start_event_sequence
    }

    pub(super) fn set_next_event_sequence(&mut self, next_event_sequence: EventSequence) {
        self.next_event_sequence = next_event_sequence.max(1);
    }

    #[allow(dead_code)]
    pub(super) fn mark(&self) -> SemanticSinkMark {
        SemanticSinkMark {
            event_len: self.events.len(),
            next_event_sequence: self.next_event_sequence,
            epoch: self.epoch,
        }
    }

    #[allow(dead_code)]
    pub(super) fn rollback(&mut self, mark: SemanticSinkMark) -> bool {
        if !self.commit(mark) {
            return false;
        }
        self.events.truncate(mark.event_len);
        self.next_event_sequence = mark.next_event_sequence;
        true
    }

    #[allow(dead_code)]
    pub(super) fn commit(&self, mark: SemanticSinkMark) -> bool {
        mark.epoch == self.epoch
            && mark.event_len <= self.events.len()
            && mark.next_event_sequence <= self.next_event_sequence
    }

    pub(super) fn replace_events(&mut self, events: Vec<RenderEventEnvelope>) {
        self.events = events;
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(super) fn replace_transaction(
        &mut self,
        removed_event_sequences: &BTreeSet<EventSequence>,
        mut replacements: Vec<RenderEventEnvelope>,
    ) -> Option<BTreeMap<EventSequence, EventSequence>> {
        if removed_event_sequences.is_empty() {
            return None;
        }
        let present_event_sequences = self
            .events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<BTreeSet<_>>();
        if !removed_event_sequences.is_subset(&present_event_sequences) {
            return None;
        }
        let emitted_event_sequences = replacements
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let reusable_event_sequences = self
            .events
            .iter()
            .filter(|event| removed_event_sequences.contains(&event.meta.sequence))
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        for (replacement, event_sequence) in replacements.iter_mut().zip(reusable_event_sequences) {
            replacement.meta.sequence = event_sequence;
        }
        let retained_event_sequences = present_event_sequences
            .difference(removed_event_sequences)
            .copied()
            .collect::<BTreeSet<_>>();
        let replacement_event_sequences = replacements
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<BTreeSet<_>>();
        let event_sequence_remap = emitted_event_sequences
            .into_iter()
            .zip(replacements.iter().map(|event| event.meta.sequence))
            .collect::<BTreeMap<_, _>>();
        if event_sequence_remap.len() != replacements.len()
            || replacement_event_sequences.len() != replacements.len()
            || !retained_event_sequences.is_disjoint(&replacement_event_sequences)
        {
            return None;
        }

        let mut replacements = Some(replacements);
        let mut events = Vec::with_capacity(
            self.events
                .len()
                .saturating_sub(removed_event_sequences.len())
                + replacements.as_ref().map_or(0, Vec::len),
        );
        for event in self.events.drain(..) {
            if removed_event_sequences.contains(&event.meta.sequence) {
                if let Some(replacements) = replacements.take() {
                    events.extend(replacements);
                }
            } else {
                events.push(event);
            }
        }
        self.next_event_sequence = events
            .iter()
            .map(|event| event.meta.sequence.saturating_add(1))
            .max()
            .unwrap_or(1)
            .max(self.next_event_sequence);
        self.events = events;
        self.epoch = self.epoch.wrapping_add(1);
        Some(event_sequence_remap)
    }

    pub(super) fn replace_transaction_since(
        &mut self,
        removed_event_sequences: &BTreeSet<EventSequence>,
        mark: SemanticSinkMark,
    ) -> Option<BTreeMap<EventSequence, EventSequence>> {
        if !self.commit(mark) {
            return None;
        }
        let replacements = self.events.split_off(mark.event_len);
        match self.replace_transaction(removed_event_sequences, replacements.clone()) {
            Some(event_sequence_remap) => Some(event_sequence_remap),
            None => {
                self.events.extend(replacements);
                None
            }
        }
    }

    pub(super) fn set_replay_prefix(&mut self, events: Vec<RenderEventEnvelope>) {
        self.next_event_sequence = events
            .iter()
            .map(|event| event.meta.sequence.saturating_add(1))
            .max()
            .unwrap_or(self.next_event_sequence)
            .max(self.next_event_sequence);
        self.batch_start_event_sequence = self.next_event_sequence;
        self.events = events;
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(super) fn take_events(&mut self) -> Vec<RenderEventEnvelope> {
        self.epoch = self.epoch.wrapping_add(1);
        std::mem::take(&mut self.events)
    }

    pub(super) fn finish_batch(&mut self) -> Vec<RenderEventEnvelope> {
        let events = self.take_events();
        self.batch_start_event_sequence = self.next_event_sequence;
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
    use std::collections::BTreeSet;

    use tex_render_model::{
        EventBuildContext, EventOrigin, EventProducer, RenderEvent, RenderEventEnvelope,
        SemanticConfidence, SourceProvenance, TextEvent,
    };

    use super::SemanticEventBuffer;

    fn emit_text(buffer: &mut SemanticEventBuffer, text: &str) -> u64 {
        let event_sequence = buffer.allocate_event_sequence();
        let envelope = RenderEventEnvelope::try_from_origin(
            RenderEvent::Text(TextEvent {
                text: text.to_string(),
            }),
            EventBuildContext::new(
                event_sequence,
                SourceProvenance::file("main.tex", 0, text.len() as u32),
            ),
            EventOrigin::unknown_low(),
        )
        .expect("synthetic sink text must use a valid event origin");
        assert_eq!(envelope.meta.producer, EventProducer::Unknown);
        assert_eq!(envelope.meta.confidence, SemanticConfidence::Low);
        buffer.push(envelope);
        event_sequence
    }

    #[test]
    fn rollback_discards_events_and_reuses_event_sequences() {
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
        assert_eq!(buffer.batch_start_event_sequence(), 2);
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
    fn snapshot_accepts_legacy_event_id_allocator_fields() {
        let mut buffer = SemanticEventBuffer::default();
        assert_eq!(emit_text(&mut buffer, "first"), 1);
        let mut encoded = serde_json::to_value(buffer.snapshot()).expect("encode snapshot");
        let snapshot = encoded.as_object_mut().expect("semantic sink snapshot");
        let next_event_sequence = snapshot
            .remove("next_event_sequence")
            .expect("next event sequence");
        snapshot.insert("next_event_id".to_string(), next_event_sequence);
        let batch_start_event_sequence = snapshot
            .remove("batch_start_event_sequence")
            .expect("batch start event sequence");
        snapshot.insert(
            "batch_start_event_id".to_string(),
            batch_start_event_sequence,
        );

        let legacy_snapshot: crate::snapshot::VmSemanticSinkSnapshot =
            serde_json::from_value(encoded).expect("decode legacy snapshot");
        let restored = SemanticEventBuffer::restore(&legacy_snapshot).expect("restorable snapshot");

        assert_eq!(restored, buffer);
    }

    #[test]
    fn replay_prefix_starts_a_new_event_batch_after_the_prefix() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "prefix");
        let prefix = buffer.finish_batch();

        buffer.set_replay_prefix(prefix);

        assert_eq!(buffer.batch_start_event_sequence(), 2);
        assert_eq!(emit_text(&mut buffer, "body"), 2);
        assert!(SemanticEventBuffer::restore(&buffer.snapshot()).is_some());
    }

    #[test]
    fn source_transaction_replaces_an_event_range_atomically() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "before");
        let old_start = emit_text(&mut buffer, "old start");
        let old_end = emit_text(&mut buffer, "old end");
        emit_text(&mut buffer, "after");
        let replacement_id = buffer.allocate_event_sequence();
        let replacement = RenderEventEnvelope::try_from_origin(
            RenderEvent::Text(TextEvent {
                text: "replacement".to_string(),
            }),
            EventBuildContext::new(replacement_id, SourceProvenance::file("child.tex", 0, 11)),
            EventOrigin::unknown_low(),
        )
        .expect("synthetic sink replacement must use a valid event origin");
        assert_eq!(replacement.meta.producer, EventProducer::Unknown);
        assert_eq!(replacement.meta.confidence, SemanticConfidence::Low);

        let event_sequence_remap = buffer
            .replace_transaction(&BTreeSet::from([old_start, old_end]), vec![replacement])
            .expect("valid source transaction");
        assert_eq!(event_sequence_remap.get(&replacement_id), Some(&old_start));
        assert_eq!(
            buffer
                .iter()
                .filter_map(|event| match &event.event {
                    RenderEvent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec!["before", "replacement", "after"]
        );

        let unchanged = buffer.clone();
        assert!(
            buffer
                .replace_transaction(&BTreeSet::from([99]), Vec::new())
                .is_none()
        );
        assert_eq!(buffer, unchanged);
    }

    #[test]
    fn source_transaction_moves_events_emitted_since_a_mark() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "before");
        let old = emit_text(&mut buffer, "old");
        emit_text(&mut buffer, "after");
        let mark = buffer.mark();
        emit_text(&mut buffer, "replacement");

        assert!(
            buffer
                .replace_transaction_since(&BTreeSet::from([old]), mark)
                .is_some()
        );
        assert_eq!(
            buffer
                .iter()
                .filter_map(|event| match &event.event {
                    RenderEvent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec!["before", "replacement", "after"]
        );
    }

    #[test]
    fn restore_rejects_duplicate_or_out_of_range_event_sequences() {
        let mut buffer = SemanticEventBuffer::default();
        emit_text(&mut buffer, "first");
        let mut snapshot = buffer.snapshot();
        snapshot.events.push(snapshot.events[0].clone());

        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.events.pop();
        snapshot.next_event_sequence = 1;
        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.next_event_sequence = 2;
        snapshot.batch_start_event_sequence = 3;
        assert!(SemanticEventBuffer::restore(&snapshot).is_none());

        snapshot.next_event_sequence = 3;
        snapshot.batch_start_event_sequence = 2;
        assert!(SemanticEventBuffer::restore(&snapshot).is_some());
    }
}
