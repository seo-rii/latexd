use std::collections::BTreeMap;

use tex_tokens::Token;

use crate::save_stack::SaveStack;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EqKey {
    Count(u32),
    Dimen(u32),
    Skip(u32),
    Toks(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EqValue {
    Integer(i32),
    Dimension(i32),
    Glue(i32),
    TokenList(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub(crate) fn from_register_values(
        count_values: BTreeMap<u32, i32>,
        dimen_values: BTreeMap<u32, i32>,
        skip_values: BTreeMap<u32, i32>,
        token_values: BTreeMap<u32, Vec<Token>>,
    ) -> Self {
        let mut eqtb = Self::default();
        for (index, value) in count_values {
            eqtb.entries.insert(
                EqKey::Count(index),
                EqEntry {
                    value: EqValue::Integer(value),
                    level: 0,
                },
            );
        }
        for (index, value) in dimen_values {
            eqtb.entries.insert(
                EqKey::Dimen(index),
                EqEntry {
                    value: EqValue::Dimension(value),
                    level: 0,
                },
            );
        }
        for (index, value) in skip_values {
            eqtb.entries.insert(
                EqKey::Skip(index),
                EqEntry {
                    value: EqValue::Glue(value),
                    level: 0,
                },
            );
        }
        for (index, value) in token_values {
            eqtb.entries.insert(
                EqKey::Toks(index),
                EqEntry {
                    value: EqValue::TokenList(value),
                    level: 0,
                },
            );
        }
        eqtb
    }

    pub(crate) fn count(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Count(index))
            .map(|entry| match &entry.value {
                EqValue::Integer(value) => *value,
                EqValue::Dimension(_) | EqValue::Glue(_) | EqValue::TokenList(_) => {
                    unreachable!("count entry must contain an integer")
                }
            })
    }

    pub(crate) fn dimen(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Dimen(index))
            .map(|entry| match &entry.value {
                EqValue::Dimension(value) => *value,
                EqValue::Integer(_) | EqValue::Glue(_) | EqValue::TokenList(_) => {
                    unreachable!("dimen entry must contain a dimension")
                }
            })
    }

    pub(crate) fn skip(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Skip(index))
            .map(|entry| match &entry.value {
                EqValue::Glue(value) => *value,
                EqValue::Integer(_) | EqValue::Dimension(_) | EqValue::TokenList(_) => {
                    unreachable!("skip entry must contain glue")
                }
            })
    }

    pub(crate) fn tokens(&self, index: u32) -> Option<&[Token]> {
        self.entries
            .get(&EqKey::Toks(index))
            .map(|entry| match &entry.value {
                EqValue::TokenList(tokens) => tokens.as_slice(),
                EqValue::Integer(_) | EqValue::Dimension(_) | EqValue::Glue(_) => {
                    unreachable!("toks entry must contain a token list")
                }
            })
    }

    pub(crate) fn contains_count(&self, index: u32) -> bool {
        self.entries.contains_key(&EqKey::Count(index))
    }

    pub(crate) fn contains_dimen(&self, index: u32) -> bool {
        self.entries.contains_key(&EqKey::Dimen(index))
    }

    pub(crate) fn contains_skip(&self, index: u32) -> bool {
        self.entries.contains_key(&EqKey::Skip(index))
    }

    pub(crate) fn contains_tokens(&self, index: u32) -> bool {
        self.entries.contains_key(&EqKey::Toks(index))
    }

    pub(crate) fn ensure_count(&mut self, index: u32) {
        self.entries.entry(EqKey::Count(index)).or_insert(EqEntry {
            value: EqValue::Integer(0),
            level: 0,
        });
    }

    pub(crate) fn ensure_dimen(&mut self, index: u32) {
        self.entries.entry(EqKey::Dimen(index)).or_insert(EqEntry {
            value: EqValue::Dimension(0),
            level: 0,
        });
    }

    pub(crate) fn ensure_skip(&mut self, index: u32) {
        self.entries.entry(EqKey::Skip(index)).or_insert(EqEntry {
            value: EqValue::Glue(0),
            level: 0,
        });
    }

    pub(crate) fn ensure_tokens(&mut self, index: u32) {
        self.entries.entry(EqKey::Toks(index)).or_insert(EqEntry {
            value: EqValue::TokenList(Vec::new()),
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
        self.assign(
            EqKey::Count(index),
            EqValue::Integer(value),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn assign_dimen(
        &mut self,
        index: u32,
        value: i32,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::Dimen(index),
            EqValue::Dimension(value),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn assign_skip(
        &mut self,
        index: u32,
        value: i32,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::Skip(index),
            EqValue::Glue(value),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn assign_tokens(
        &mut self,
        index: u32,
        value: Vec<Token>,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::Toks(index),
            EqValue::TokenList(value),
            scope,
            group_level,
            save_stack,
        );
    }

    fn assign(
        &mut self,
        key: EqKey,
        value: EqValue,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        if scope == AssignmentScope::Global || group_level == 0 {
            save_stack.cancel_restore(key);
            self.entries.insert(key, EqEntry { value, level: 0 });
            return;
        }

        save_stack.save_if_absent(key, self.entries.get(&key).cloned());
        self.entries.insert(
            key,
            EqEntry {
                value,
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
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::Count(index), EqValue::Integer(value)) => Some((*index, *value)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn dimen_values(&self) -> BTreeMap<u32, i32> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::Dimen(index), EqValue::Dimension(value)) => Some((*index, *value)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn skip_values(&self) -> BTreeMap<u32, i32> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::Skip(index), EqValue::Glue(value)) => Some((*index, *value)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn token_values(&self) -> BTreeMap<u32, Vec<Token>> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::Toks(index), EqValue::TokenList(value)) => Some((*index, value.clone())),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{AssignmentScope, Eqtb};
    use crate::save_stack::SaveStack;
    use tex_tokens::{CatCode, Token};

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

    #[test]
    fn dimension_and_glue_assignments_share_group_restore_semantics() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_dimen(0, 10, AssignmentScope::Global, 0, &mut save_stack);
        eqtb.assign_skip(0, 20, AssignmentScope::Global, 0, &mut save_stack);
        save_stack.begin_group();

        eqtb.assign_dimen(0, 30, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.assign_skip(0, 40, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.dimen(0), Some(10));
        assert_eq!(eqtb.skip(0), Some(20));
    }

    #[test]
    fn token_list_assignment_restores_owned_values() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        let outer = vec![Token::character('A', CatCode::Letter, 0, 1)];
        let inner = vec![Token::character('B', CatCode::Letter, 1, 2)];
        eqtb.assign_tokens(
            0,
            outer.clone(),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();

        eqtb.assign_tokens(0, inner, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.tokens(0), Some(outer.as_slice()));
    }
}
