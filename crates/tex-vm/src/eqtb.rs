use std::collections::BTreeMap;

use crate::save_stack::SaveStack;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EqKey {
    Count(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EqValue {
    Integer(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EqEntry {
    value: EqValue,
    level: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentScope {
    Local,
    Global,
}

#[derive(Debug, Default)]
pub(crate) struct Eqtb {
    entries: BTreeMap<EqKey, EqEntry>,
}

impl Eqtb {
    pub(crate) fn from_count_values(values: BTreeMap<u32, i32>) -> Self {
        Self {
            entries: values
                .into_iter()
                .map(|(index, value)| {
                    (
                        EqKey::Count(index),
                        EqEntry {
                            value: EqValue::Integer(value),
                            level: 0,
                        },
                    )
                })
                .collect(),
        }
    }

    pub(crate) fn count(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Count(index))
            .map(|entry| match entry.value {
                EqValue::Integer(value) => value,
            })
    }

    pub(crate) fn contains_count(&self, index: u32) -> bool {
        self.entries.contains_key(&EqKey::Count(index))
    }

    pub(crate) fn ensure_count(&mut self, index: u32) {
        self.entries.entry(EqKey::Count(index)).or_insert(EqEntry {
            value: EqValue::Integer(0),
            level: 0,
        });
    }

    pub(crate) fn assign_count(
        &mut self,
        index: u32,
        value: i32,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        let key = EqKey::Count(index);
        if scope == AssignmentScope::Global || group_level == 0 {
            save_stack.cancel_restore(key);
            self.entries.insert(
                key,
                EqEntry {
                    value: EqValue::Integer(value),
                    level: 0,
                },
            );
            return;
        }

        save_stack.save_if_absent(key, self.entries.get(&key).copied());
        self.entries.insert(
            key,
            EqEntry {
                value: EqValue::Integer(value),
                level: group_level,
            },
        );
    }

    pub(crate) fn end_group(&mut self, save_stack: &mut SaveStack) {
        let Some(restores) = save_stack.end_group() else {
            return;
        };
        for (key, previous) in restores {
            if let Some(previous) = previous {
                self.entries.insert(key, previous);
            } else {
                self.entries.remove(&key);
            }
        }
    }

    pub(crate) fn count_values(&self) -> BTreeMap<u32, i32> {
        self.entries
            .iter()
            .map(|(key, entry)| {
                let EqKey::Count(index) = key;
                let EqValue::Integer(value) = entry.value;
                (*index, value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{AssignmentScope, Eqtb};
    use crate::save_stack::SaveStack;

    #[test]
    fn local_count_assignment_restores_the_first_value_at_group_end() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_count(0, 1, AssignmentScope::Global, 0, &mut save_stack);
        save_stack.begin_group();

        eqtb.assign_count(0, 2, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.assign_count(0, 3, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.count(0), Some(1));
    }

    #[test]
    fn global_count_assignment_cancels_all_pending_restores() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_count(0, 1, AssignmentScope::Global, 0, &mut save_stack);
        save_stack.begin_group();
        eqtb.assign_count(0, 2, AssignmentScope::Local, 1, &mut save_stack);
        save_stack.begin_group();
        eqtb.assign_count(0, 3, AssignmentScope::Local, 2, &mut save_stack);

        eqtb.assign_count(0, 4, AssignmentScope::Global, 2, &mut save_stack);
        eqtb.end_group(&mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.count(0), Some(4));
    }

    #[test]
    fn local_assignment_after_global_restores_the_global_value() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        save_stack.begin_group();

        eqtb.assign_count(0, 4, AssignmentScope::Global, 1, &mut save_stack);
        eqtb.assign_count(0, 5, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.count(0), Some(4));
    }
}
