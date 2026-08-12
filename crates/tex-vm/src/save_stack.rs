use std::collections::BTreeMap;

use crate::eqtb::{EqEntry, EqKey};

#[derive(Debug, Default)]
pub(crate) struct SaveStack {
    groups: Vec<SaveGroup>,
}

#[derive(Debug, Default)]
struct SaveGroup {
    restores: BTreeMap<EqKey, Option<EqEntry>>,
    control_sequences_only: bool,
}

impl SaveStack {
    pub(crate) fn begin_group(&mut self) {
        self.groups.push(SaveGroup::default());
    }

    pub(crate) fn begin_legacy_control_sequence_group(&mut self) {
        self.groups.push(SaveGroup {
            control_sequences_only: true,
            ..SaveGroup::default()
        });
    }

    pub(crate) fn save_if_absent(&mut self, key: EqKey, previous: Option<EqEntry>) {
        if let Some(group) = self.groups.last_mut() {
            if group.control_sequences_only && !matches!(key, EqKey::ControlSequence(_)) {
                return;
            }
            group.restores.entry(key).or_insert(previous);
        }
    }

    pub(crate) fn cancel_restore(&mut self, key: &EqKey) {
        for group in &mut self.groups {
            group.restores.remove(key);
        }
    }

    pub(crate) fn group_level(&self) -> usize {
        self.groups.len()
    }

    pub(crate) fn scope_depth(&self) -> usize {
        self.group_level() + 1
    }

    pub(crate) fn restore_groups(
        &self,
    ) -> impl DoubleEndedIterator<Item = &BTreeMap<EqKey, Option<EqEntry>>> + ExactSizeIterator
    {
        self.groups.iter().map(|group| &group.restores)
    }

    pub(crate) fn end_group(&mut self) -> Option<BTreeMap<EqKey, Option<EqEntry>>> {
        self.groups.pop().map(|group| group.restores)
    }
}
