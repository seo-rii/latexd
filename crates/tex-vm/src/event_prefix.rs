//! Immutable authoritative event storage with cheap checkpoint prefix views.
//!
//! Mutation preserves the historical Vec semantics through copy-on-write. This
//! is not a persistent VM state tree: arbitrary semantic reconciliation still
//! materializes a mutable prefix. Durable formats retain their ordinary arrays.
use std::{
    ops::{Deref, DerefMut, RangeBounds},
    sync::Arc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tex_render_model::RenderEventEnvelope;

#[derive(Debug, Clone, Default)]
pub struct SharedRenderEventPrefix {
    events: Arc<Vec<RenderEventEnvelope>>,
    end: usize,
}

impl SharedRenderEventPrefix {
    /// A view into the same immutable segment, without cloning event payloads.
    pub fn prefix(&self, end: usize) -> Option<Self> {
        (end <= self.end).then(|| Self {
            events: self.events.clone(),
            end,
        })
    }

    pub fn as_slice(&self) -> &[RenderEventEnvelope] {
        &self.events[..self.end]
    }

    /// Explicit compatibility boundary for code that rewrites semantic events.
    fn make_mut(&mut self) -> &mut Vec<RenderEventEnvelope> {
        if self.end != self.events.len() {
            self.events = Arc::new(self.as_slice().to_vec());
        }
        Arc::make_mut(&mut self.events)
    }

    /// The guard synchronizes the visible cursor even if mutation unwinds.
    pub fn to_mut(&mut self) -> RenderEventPrefixMut<'_> {
        self.make_mut();
        RenderEventPrefixMut {
            events: Arc::get_mut(&mut self.events).expect("unique mutable events"),
            end: &mut self.end,
        }
    }

    pub fn into_vec(self) -> Vec<RenderEventEnvelope> {
        let mut events =
            Arc::try_unwrap(self.events).unwrap_or_else(|events| events[..self.end].to_vec());
        events.truncate(self.end);
        events
    }

    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.events, &other.events)
    }

    /// Payload slots retained by this segment, including data after the cursor.
    /// Count shared segments once when measuring a collection of checkpoints.
    pub fn retained_event_count(&self) -> usize {
        self.events.len()
    }

    pub fn push(&mut self, event: RenderEventEnvelope) {
        self.to_mut().push(event);
    }

    pub fn pop(&mut self) -> Option<RenderEventEnvelope> {
        self.to_mut().pop()
    }

    pub fn truncate(&mut self, end: usize) {
        self.end = self.end.min(end);
    }

    pub fn split_off(&mut self, at: usize) -> Vec<RenderEventEnvelope> {
        self.to_mut().split_off(at)
    }

    pub fn drain<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> std::vec::Drain<'_, RenderEventEnvelope> {
        self.make_mut();
        let drain = Arc::get_mut(&mut self.events)
            .expect("unique mutable events")
            .drain(range);
        self.end -= drain.len();
        drain
    }

    pub fn append(&mut self, events: &mut Vec<RenderEventEnvelope>) {
        self.to_mut().append(events);
    }

    pub fn extend(&mut self, events: impl IntoIterator<Item = RenderEventEnvelope>) {
        self.to_mut().extend(events);
    }

    pub fn retain(&mut self, predicate: impl FnMut(&RenderEventEnvelope) -> bool) {
        self.to_mut().retain(predicate);
    }

    pub fn insert(&mut self, index: usize, event: RenderEventEnvelope) {
        self.to_mut().insert(index, event);
    }
}

impl From<Vec<RenderEventEnvelope>> for SharedRenderEventPrefix {
    fn from(events: Vec<RenderEventEnvelope>) -> Self {
        let end = events.len();
        Self {
            events: Arc::new(events),
            end,
        }
    }
}

impl Deref for SharedRenderEventPrefix {
    type Target = [RenderEventEnvelope];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for SharedRenderEventPrefix {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.make_mut().as_mut_slice()
    }
}

impl PartialEq for SharedRenderEventPrefix {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for SharedRenderEventPrefix {}

impl PartialEq<Vec<RenderEventEnvelope>> for SharedRenderEventPrefix {
    fn eq(&self, other: &Vec<RenderEventEnvelope>) -> bool {
        self.as_slice() == other
    }
}

impl Serialize for SharedRenderEventPrefix {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SharedRenderEventPrefix {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<RenderEventEnvelope>::deserialize(deserializer).map(Self::from)
    }
}

/// Scoped Vec compatibility for semantic reconciliation and batch assembly.
pub struct RenderEventPrefixMut<'a> {
    events: &'a mut Vec<RenderEventEnvelope>,
    end: &'a mut usize,
}

impl Deref for RenderEventPrefixMut<'_> {
    type Target = Vec<RenderEventEnvelope>;
    fn deref(&self) -> &Self::Target {
        self.events
    }
}

impl DerefMut for RenderEventPrefixMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.events
    }
}

impl Drop for RenderEventPrefixMut<'_> {
    fn drop(&mut self) {
        *self.end = self.events.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tex_render_model::{
        EventBuildContext, EventOrigin, RenderEvent, SourceProvenance, TextEvent,
    };

    fn events(count: usize) -> Vec<RenderEventEnvelope> {
        (0..count)
            .map(|index| {
                RenderEventEnvelope::try_from_origin(
                    RenderEvent::Text(TextEvent {
                        text: format!("event {index}"),
                    }),
                    EventBuildContext::new(
                        index as u64 + 1,
                        SourceProvenance::file("main.tex", index as u32, index as u32 + 1),
                    ),
                    EventOrigin::unknown_low(),
                )
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn one_hundred_checkpoint_prefixes_retain_one_event_segment() {
        let log = SharedRenderEventPrefix::from(events(10_000));
        let checkpoints = (1..=100)
            .map(|page| log.prefix(page * 100).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            checkpoints.iter().map(|prefix| prefix.len()).sum::<usize>(),
            505_000
        );
        assert!(
            checkpoints
                .iter()
                .all(|prefix| prefix.shares_storage_with(&log))
        );
        assert_eq!(log.retained_event_count(), 10_000);
        // Dropping the owner leaves every checkpoint independently usable.
        drop(log);
        assert_eq!(checkpoints[0].last().unwrap().meta.sequence, 100);
        assert_eq!(checkpoints[99].last().unwrap().meta.sequence, 10_000);
    }

    #[test]
    fn prefix_wire_encoding_is_exactly_the_legacy_array() {
        let original = events(3);
        let log = SharedRenderEventPrefix::from(original.clone());
        let prefix = log.prefix(2).unwrap();
        let wire = serde_json::to_vec(&prefix).unwrap();
        assert_eq!(wire, serde_json::to_vec(&original[..2]).unwrap());
        let restored: SharedRenderEventPrefix = serde_json::from_slice(&wire).unwrap();
        assert_eq!(restored.as_slice(), &original[..2]);
        assert!(serde_json::from_str::<SharedRenderEventPrefix>(r#"{"events":[]}"#).is_err());
        assert!(log.prefix(4).is_none());
    }

    #[test]
    fn mutable_compatibility_detaches_and_keeps_the_cursor_consistent() {
        let log = SharedRenderEventPrefix::from(events(4));
        let mut prefix = log.prefix(2).unwrap();
        let saved = prefix.clone();
        prefix.to_mut().push(log[2].clone());
        assert_eq!(prefix.len(), 3);
        prefix[0].meta.sequence = 10;
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].meta.sequence, 1);
        assert!(saved.shares_storage_with(&log));
        assert!(!prefix.shares_storage_with(&log));
        assert_eq!(prefix.drain(1..).count(), 2);
        assert_eq!(prefix.len(), 1);
        prefix.append(&mut events(2));
        assert_eq!(prefix.len(), 3);
        assert_eq!(prefix.split_off(1).len(), 2);
        prefix.truncate(0);
        assert!(prefix.into_vec().is_empty());
        assert_eq!(saved.into_vec(), log[..2]);
    }

    #[test]
    fn mutation_guard_updates_cursor_during_unwind() {
        let mut prefix = SharedRenderEventPrefix::from(events(2));
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut events = prefix.to_mut();
            events.clear();
            panic!("simulated reconciliation failure");
        }));
        assert!(failed.is_err());
        assert!(prefix.is_empty());
    }
}
