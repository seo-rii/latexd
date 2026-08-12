use std::{
    collections::{BTreeMap, HashMap},
    mem,
};

use tex_lexer::CatCodeTable;
use tex_tokens::{CatCode, Token};

use crate::{command::Meaning, save_stack::SaveStack};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EqKey {
    Count(u32),
    Dimen(u32),
    Skip(u32),
    Toks(u32),
    CatCode(char),
    ControlSequence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EqValue {
    Integer(i32),
    Dimension(i32),
    Glue(i32),
    TokenList(Vec<Token>),
    CatCode(CatCode),
    ControlSequence(Meaning),
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

#[derive(Debug)]
pub(crate) struct Eqtb {
    entries: BTreeMap<EqKey, EqEntry>,
    control_sequences: BTreeMap<String, EqEntry>,
    base_catcodes: CatCodeTable,
    catcodes: CatCodeTable,
}

impl Default for Eqtb {
    fn default() -> Self {
        let base_catcodes = CatCodeTable::plain_tex();
        Self {
            entries: BTreeMap::new(),
            control_sequences: BTreeMap::new(),
            catcodes: base_catcodes.clone(),
            base_catcodes,
        }
    }
}

impl Eqtb {
    pub(crate) fn from_register_values(
        count_values: BTreeMap<u32, i32>,
        dimen_values: BTreeMap<u32, i32>,
        skip_values: BTreeMap<u32, i32>,
        token_values: BTreeMap<u32, Vec<Token>>,
        catcode_values: BTreeMap<char, CatCode>,
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
        for (ch, value) in catcode_values {
            eqtb.entries.insert(
                EqKey::CatCode(ch),
                EqEntry {
                    value: EqValue::CatCode(value),
                    level: 0,
                },
            );
            eqtb.catcodes.set(ch, value);
        }
        eqtb
    }

    pub(crate) fn count(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Count(index))
            .map(|entry| match &entry.value {
                EqValue::Integer(value) => *value,
                EqValue::Dimension(_)
                | EqValue::Glue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("count entry must contain an integer")
                }
            })
    }

    pub(crate) fn dimen(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Dimen(index))
            .map(|entry| match &entry.value {
                EqValue::Dimension(value) => *value,
                EqValue::Integer(_)
                | EqValue::Glue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("dimen entry must contain a dimension")
                }
            })
    }

    pub(crate) fn skip(&self, index: u32) -> Option<i32> {
        self.entries
            .get(&EqKey::Skip(index))
            .map(|entry| match &entry.value {
                EqValue::Glue(value) => *value,
                EqValue::Integer(_)
                | EqValue::Dimension(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("skip entry must contain glue")
                }
            })
    }

    pub(crate) fn tokens(&self, index: u32) -> Option<&[Token]> {
        self.entries
            .get(&EqKey::Toks(index))
            .map(|entry| match &entry.value {
                EqValue::TokenList(tokens) => tokens.as_slice(),
                EqValue::Integer(_)
                | EqValue::Dimension(_)
                | EqValue::Glue(_)
                | EqValue::CatCode(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("toks entry must contain a token list")
                }
            })
    }

    pub(crate) fn catcodes(&self) -> &CatCodeTable {
        &self.catcodes
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

    pub(crate) fn assign_catcode(
        &mut self,
        ch: char,
        value: CatCode,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::CatCode(ch),
            EqValue::CatCode(value),
            scope,
            group_level,
            save_stack,
        );
        self.catcodes.set(ch, value);
    }

    pub(crate) fn assign_control_sequence(
        &mut self,
        name: String,
        meaning: Meaning,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::ControlSequence(name),
            EqValue::ControlSequence(meaning),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn control_sequence(&self, name: &str) -> Option<&Meaning> {
        self.control_sequences
            .get(name)
            .map(control_sequence_meaning)
    }

    pub(crate) fn replace_control_sequence_meaning(
        &mut self,
        name: &str,
        meaning: Meaning,
    ) -> Option<Meaning> {
        let entry = self.control_sequences.get_mut(name)?;
        let EqValue::ControlSequence(current) = &mut entry.value else {
            unreachable!("control-sequence entry must contain a meaning")
        };
        Some(mem::replace(current, meaning))
    }

    pub(crate) fn control_sequence_layers(
        &self,
        save_stack: &SaveStack,
    ) -> Vec<HashMap<String, Meaning>> {
        let mut working = self.control_sequences.clone();
        let mut layers = vec![HashMap::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let layer = &mut layers[group_index + 1];
            for (key, previous) in restores {
                let EqKey::ControlSequence(name) = key else {
                    continue;
                };
                if let Some(entry) = working.get(name) {
                    layer.insert(name.clone(), control_sequence_meaning(entry).clone());
                }
                if let Some(previous) = previous {
                    working.insert(name.clone(), previous.clone());
                } else {
                    working.remove(name);
                }
            }
        }

        layers[0] = working
            .into_iter()
            .map(|(name, entry)| (name, control_sequence_meaning(&entry).clone()))
            .collect();
        layers
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
            save_stack.cancel_restore(&key);
            self.insert_entry(key, EqEntry { value, level: 0 });
            return;
        }

        let previous = self.entry(&key).cloned();
        save_stack.save_if_absent(key.clone(), previous);
        self.insert_entry(
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
            let restored_catcode = match &key {
                EqKey::CatCode(ch) => Some(*ch),
                _ => None,
            };
            if let Some(previous) = previous {
                self.insert_entry(key, previous);
            } else {
                self.remove_entry(&key);
            }
            if let Some(ch) = restored_catcode {
                let key = EqKey::CatCode(ch);
                let value = self
                    .entries
                    .get(&key)
                    .map(|entry| match &entry.value {
                        EqValue::CatCode(value) => *value,
                        _ => unreachable!("catcode entry must contain a catcode"),
                    })
                    .unwrap_or_else(|| self.base_catcodes.catcode(ch));
                self.catcodes.set(ch, value);
            }
        }
    }

    fn entry(&self, key: &EqKey) -> Option<&EqEntry> {
        match key {
            EqKey::ControlSequence(name) => self.control_sequences.get(name),
            _ => self.entries.get(key),
        }
    }

    fn insert_entry(&mut self, key: EqKey, entry: EqEntry) {
        match key {
            EqKey::ControlSequence(name) => {
                self.control_sequences.insert(name, entry);
            }
            key => {
                self.entries.insert(key, entry);
            }
        }
    }

    fn remove_entry(&mut self, key: &EqKey) {
        match key {
            EqKey::ControlSequence(name) => {
                self.control_sequences.remove(name);
            }
            _ => {
                self.entries.remove(key);
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

    pub(crate) fn catcode_values(&self) -> BTreeMap<char, CatCode> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::CatCode(ch), EqValue::CatCode(value)) => Some((*ch, *value)),
                _ => None,
            })
            .collect()
    }
}

fn control_sequence_meaning(entry: &EqEntry) -> &Meaning {
    let EqValue::ControlSequence(meaning) = &entry.value else {
        unreachable!("control-sequence entry must contain a meaning")
    };
    meaning
}

#[cfg(test)]
mod tests {
    use super::{AssignmentScope, Eqtb};
    use crate::{
        command::{Meaning, Primitive},
        save_stack::SaveStack,
    };
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

    #[test]
    fn catcode_assignment_updates_the_mouth_view_and_restores_groups() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        save_stack.begin_group();

        eqtb.assign_catcode(
            '@',
            CatCode::Letter,
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        assert_eq!(eqtb.catcodes().catcode('@'), CatCode::Letter);

        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.catcodes().catcode('@'), CatCode::Other);
    }

    #[test]
    fn control_sequence_assignments_share_restore_and_snapshot_projection() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_control_sequence(
            "state".to_string(),
            Meaning::Primitive(Primitive::Relax),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_control_sequence(
            "state".to_string(),
            Meaning::Primitive(Primitive::Par),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_control_sequence(
            "state".to_string(),
            Meaning::Primitive(Primitive::Def),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );

        assert_eq!(
            eqtb.control_sequence("state"),
            Some(&Meaning::Primitive(Primitive::Def))
        );
        assert_eq!(
            eqtb.control_sequence_layers(&save_stack),
            vec![
                [("state".to_string(), Meaning::Primitive(Primitive::Relax))].into(),
                [("state".to_string(), Meaning::Primitive(Primitive::Par))].into(),
                [("state".to_string(), Meaning::Primitive(Primitive::Def))].into(),
            ]
        );

        eqtb.assign_control_sequence(
            "state".to_string(),
            Meaning::Primitive(Primitive::Let),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );

        assert_eq!(
            eqtb.control_sequence_layers(&save_stack),
            vec![
                [("state".to_string(), Meaning::Primitive(Primitive::Let))].into(),
                Default::default(),
                Default::default(),
            ]
        );
        eqtb.end_group(&mut save_stack);
        eqtb.end_group(&mut save_stack);
        assert_eq!(
            eqtb.control_sequence("state"),
            Some(&Meaning::Primitive(Primitive::Let))
        );
    }

    #[test]
    fn temporary_control_sequence_replacement_preserves_saved_scope_state() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_control_sequence(
            "author-separator".to_string(),
            Meaning::Primitive(Primitive::Relax),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_control_sequence(
            "author-separator".to_string(),
            Meaning::Primitive(Primitive::Par),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );

        let original = eqtb
            .replace_control_sequence_meaning(
                "author-separator",
                Meaning::Primitive(Primitive::Def),
            )
            .expect("visible local meaning");
        assert_eq!(original, Meaning::Primitive(Primitive::Par));
        assert_eq!(
            eqtb.replace_control_sequence_meaning("author-separator", original),
            Some(Meaning::Primitive(Primitive::Def))
        );

        eqtb.end_group(&mut save_stack);
        assert_eq!(
            eqtb.control_sequence("author-separator"),
            Some(&Meaning::Primitive(Primitive::Relax))
        );
    }

    #[test]
    fn control_sequence_restore_chain_matches_layered_scope_model_exhaustively() {
        const ACTION_COUNT: usize = 6;
        const MAX_SEQUENCE_LENGTH: u32 = 6;

        for sequence_length in 0..=MAX_SEQUENCE_LENGTH {
            for encoded_sequence in 0..ACTION_COUNT.pow(sequence_length) {
                let mut eqtb = Eqtb::default();
                let mut save_stack = SaveStack::default();
                let mut expected_layers = vec![std::collections::HashMap::new()];
                let mut actions = encoded_sequence;
                let mut valid = true;

                for step in 0..sequence_length {
                    let action = actions % ACTION_COUNT;
                    actions /= ACTION_COUNT;
                    match action {
                        0 if save_stack.group_level() < 3 => {
                            save_stack.begin_group();
                            expected_layers.push(Default::default());
                        }
                        0 => valid = false,
                        1 if save_stack.group_level() > 0 => {
                            eqtb.end_group(&mut save_stack);
                            expected_layers.pop();
                        }
                        1 => valid = false,
                        2..=5 => {
                            let name = if action % 2 == 0 { "alpha" } else { "beta" };
                            let scope = if action < 4 {
                                AssignmentScope::Local
                            } else {
                                AssignmentScope::Global
                            };
                            let meaning = Meaning::Token(Token::character(
                                char::from(b'A' + step as u8),
                                CatCode::Letter,
                                step as usize,
                                step as usize + 1,
                            ));
                            eqtb.assign_control_sequence(
                                name.to_string(),
                                meaning.clone(),
                                scope,
                                save_stack.group_level(),
                                &mut save_stack,
                            );
                            if scope == AssignmentScope::Global || expected_layers.len() == 1 {
                                for layer in expected_layers.iter_mut().skip(1) {
                                    layer.remove(name);
                                }
                                expected_layers[0].insert(name.to_string(), meaning);
                            } else {
                                expected_layers
                                    .last_mut()
                                    .expect("root scope")
                                    .insert(name.to_string(), meaning);
                            }
                        }
                        _ => unreachable!(),
                    }
                    if !valid {
                        break;
                    }

                    assert_eq!(
                        eqtb.control_sequence_layers(&save_stack),
                        expected_layers,
                        "sequence={encoded_sequence} length={sequence_length} step={step}"
                    );
                    for name in ["alpha", "beta"] {
                        let expected = expected_layers
                            .iter()
                            .rev()
                            .find_map(|layer| layer.get(name));
                        assert_eq!(
                            eqtb.control_sequence(name),
                            expected,
                            "name={name} sequence={encoded_sequence} length={sequence_length} step={step}"
                        );
                    }
                }
            }
        }
    }
}
