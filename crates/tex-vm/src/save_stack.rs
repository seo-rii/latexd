use std::collections::BTreeMap;

use crate::eqtb::{EqEntry, EqKey};

#[derive(Debug, Default)]
pub(crate) struct SaveStack {
    groups: Vec<SaveGroup>,
}

#[derive(Debug, Default)]
struct SaveGroup {
    restores: BTreeMap<EqKey, Option<EqEntry>>,
}

impl SaveStack {
    pub(crate) fn begin_group(&mut self) {
        self.groups.push(SaveGroup::default());
    }

    pub(crate) fn save_if_absent(&mut self, key: EqKey, previous: Option<EqEntry>) {
        if let Some(group) = self.groups.last_mut() {
            group.restores.entry(key).or_insert(previous);
        }
    }

    pub(crate) fn cancel_restore(&mut self, key: EqKey) {
        for group in &mut self.groups {
            group.restores.remove(&key);
        }
    }

    pub(crate) fn end_group(&mut self) -> Option<BTreeMap<EqKey, Option<EqEntry>>> {
        self.groups.pop().map(|group| group.restores)
    }
}
