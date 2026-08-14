use std::collections::{BTreeMap, HashMap};

use tex_lexer::CatCodeTable;
use tex_tokens::{CatCode, Token};

use crate::{
    command::Meaning,
    save_stack::{SaveDisposition, SaveStack},
    snapshot::{
        IntegerParameterId, VmCodeTableAssignmentV1, VmCodeTableStateV1,
        VmIntegerParameterAssignmentV1, VmIntegerParameterStateV1,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MuGlueScalarV1(i32);

impl MuGlueScalarV1 {
    pub(crate) const fn from_scaled(value: i32) -> Self {
        Self(value)
    }

    pub(crate) const fn scaled(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MathCodeV1(u16);

impl MathCodeV1 {
    pub(crate) fn try_from_raw(value: i32) -> Option<Self> {
        let value = u16::try_from(value).ok()?;
        (value <= 32_768).then_some(Self(value))
    }

    pub(crate) const fn raw(self) -> i32 {
        self.0 as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DelimiterCodeV1(i32);

impl DelimiterCodeV1 {
    pub(crate) const fn try_from_raw(value: i32) -> Option<Self> {
        if value >= -2_147_483_647 && value <= 16_777_215 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub(crate) const fn raw(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EqKey {
    Count(u32),
    Dimen(u32),
    Skip(u32),
    MuSkip(u32),
    Toks(u32),
    CatCode(char),
    MathCode(u8),
    DelCode(u8),
    IntegerParameter(IntegerParameterId),
    ControlSequence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EqValue {
    Integer(i32),
    Dimension(i32),
    Glue(i32),
    MuGlue(MuGlueScalarV1),
    TokenList(Vec<Token>),
    CatCode(CatCode),
    MathCode(MathCodeV1),
    DelCode(DelimiterCodeV1),
    IntegerParameter(i32),
    ControlSequence(Box<Meaning>),
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
        muskip_values: BTreeMap<u32, i32>,
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
        for (index, value) in muskip_values {
            eqtb.entries.insert(
                EqKey::MuSkip(index),
                EqEntry {
                    value: EqValue::MuGlue(MuGlueScalarV1::from_scaled(value)),
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
                | EqValue::MuGlue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::MathCode(_)
                | EqValue::DelCode(_)
                | EqValue::IntegerParameter(_)
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
                | EqValue::MuGlue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::MathCode(_)
                | EqValue::DelCode(_)
                | EqValue::IntegerParameter(_)
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
                | EqValue::MuGlue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::MathCode(_)
                | EqValue::DelCode(_)
                | EqValue::IntegerParameter(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("skip entry must contain glue")
                }
            })
    }

    pub(crate) fn muskip(&self, index: u32) -> Option<MuGlueScalarV1> {
        self.entries
            .get(&EqKey::MuSkip(index))
            .map(|entry| match &entry.value {
                EqValue::MuGlue(value) => *value,
                EqValue::Integer(_)
                | EqValue::Dimension(_)
                | EqValue::Glue(_)
                | EqValue::TokenList(_)
                | EqValue::CatCode(_)
                | EqValue::MathCode(_)
                | EqValue::DelCode(_)
                | EqValue::IntegerParameter(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("muskip entry must contain math glue")
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
                | EqValue::MuGlue(_)
                | EqValue::CatCode(_)
                | EqValue::MathCode(_)
                | EqValue::DelCode(_)
                | EqValue::IntegerParameter(_)
                | EqValue::ControlSequence(_) => {
                    unreachable!("toks entry must contain a token list")
                }
            })
    }

    pub(crate) fn catcodes(&self) -> &CatCodeTable {
        &self.catcodes
    }

    pub(crate) fn mathcode(&self, character: u8) -> MathCodeV1 {
        self.entries
            .get(&EqKey::MathCode(character))
            .map(|entry| match entry.value {
                EqValue::MathCode(value) => value,
                _ => unreachable!("mathcode entry must contain a mathcode"),
            })
            .unwrap_or_else(|| match character {
                b'0'..=b'9' => MathCodeV1(0x7000 + character as u16),
                b'A'..=b'Z' | b'a'..=b'z' => MathCodeV1(0x7100 + character as u16),
                _ => MathCodeV1(character as u16),
            })
    }

    pub(crate) fn delcode(&self, character: u8) -> DelimiterCodeV1 {
        self.entries
            .get(&EqKey::DelCode(character))
            .map(|entry| match entry.value {
                EqValue::DelCode(value) => value,
                _ => unreachable!("delcode entry must contain a delimiter code"),
            })
            .unwrap_or_else(|| {
                if character == b'.' {
                    DelimiterCodeV1(0)
                } else {
                    DelimiterCodeV1(-1)
                }
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn integer_parameter(&self, parameter: IntegerParameterId) -> i32 {
        self.entries
            .get(&EqKey::IntegerParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::IntegerParameter(value) => value,
                _ => unreachable!("integer-parameter entry must contain an integer parameter"),
            })
            .unwrap_or_else(|| parameter.default_value())
    }

    #[cfg(test)]
    pub(crate) fn integer_parameter_owner(
        &self,
        parameter: IntegerParameterId,
    ) -> Option<(i32, usize)> {
        self.entries
            .get(&EqKey::IntegerParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::IntegerParameter(value) => (value, entry.level),
                _ => unreachable!("integer-parameter entry must contain an integer parameter"),
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

    pub(crate) fn assign_muskip(
        &mut self,
        index: u32,
        value: MuGlueScalarV1,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::MuSkip(index),
            EqValue::MuGlue(value),
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

    pub(crate) fn assign_mathcode(
        &mut self,
        character: u8,
        value: MathCodeV1,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::MathCode(character),
            EqValue::MathCode(value),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn assign_delcode(
        &mut self,
        character: u8,
        value: DelimiterCodeV1,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        self.assign(
            EqKey::DelCode(character),
            EqValue::DelCode(value),
            scope,
            group_level,
            save_stack,
        );
    }

    pub(crate) fn assign_integer_parameter(
        &mut self,
        parameter: IntegerParameterId,
        value: i32,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        let key = EqKey::IntegerParameter(parameter);
        if (scope == AssignmentScope::Global || group_level == 0)
            && value == parameter.default_value()
        {
            save_stack.cancel_restore(&key);
            self.remove_entry(&key);
            return;
        }
        self.assign(
            key,
            EqValue::IntegerParameter(value),
            scope,
            group_level,
            save_stack,
        );
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
            EqValue::ControlSequence(Box::new(meaning)),
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

    pub(crate) fn control_sequence_layers(
        &self,
        save_stack: &SaveStack,
    ) -> Vec<HashMap<String, Meaning>> {
        let mut working = self.control_sequences.clone();
        let mut layers = vec![HashMap::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            let layer = &mut layers[group_index + 1];
            for (key, previous) in restores {
                let EqKey::ControlSequence(name) = key else {
                    continue;
                };
                let current = working
                    .remove(name)
                    .expect("restore record must have a current control-sequence entry");
                assert_eq!(
                    current.level, group_level,
                    "current control-sequence entry must match its restore group level"
                );
                layer.insert(name.clone(), control_sequence_meaning(&current).clone());
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous control-sequence entry must precede its restore group level"
                    );
                    control_sequence_meaning(previous);
                    working.insert(name.clone(), previous.clone());
                }
            }
        }

        layers[0] = working
            .into_iter()
            .map(|(name, entry)| {
                assert_eq!(
                    entry.level, 0,
                    "root control-sequence entry must have level zero"
                );
                (name, control_sequence_meaning(&entry).clone())
            })
            .collect();
        layers
    }

    pub(crate) fn mathcode_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmCodeTableStateV1> {
        self.code_table_snapshot_state(
            save_stack,
            |key| match key {
                EqKey::MathCode(character) => Some(*character),
                _ => None,
            },
            |value| match value {
                EqValue::MathCode(value) => Some(value.raw()),
                _ => None,
            },
        )
    }

    pub(crate) fn delcode_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmCodeTableStateV1> {
        self.code_table_snapshot_state(
            save_stack,
            |key| match key {
                EqKey::DelCode(character) => Some(*character),
                _ => None,
            },
            |value| match value {
                EqValue::DelCode(value) => Some(value.raw()),
                _ => None,
            },
        )
    }

    pub(crate) fn integer_parameter_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmIntegerParameterStateV1> {
        let mut working = self
            .entries
            .iter()
            .filter(|(key, _)| matches!(key, EqKey::IntegerParameter(_)))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut layers = vec![Vec::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            for (key, previous) in restores {
                let EqKey::IntegerParameter(parameter) = key else {
                    continue;
                };
                let current = working
                    .remove(key)
                    .expect("restore record must have a current integer-parameter entry");
                assert_eq!(
                    current.level, group_level,
                    "current integer-parameter entry must match its restore group level"
                );
                let EqValue::IntegerParameter(value) = current.value else {
                    unreachable!("integer-parameter key must contain a matching value");
                };
                layers[group_level].push(VmIntegerParameterAssignmentV1 {
                    parameter: *parameter,
                    value,
                });
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous integer-parameter entry must precede its restore group level"
                    );
                    assert!(matches!(previous.value, EqValue::IntegerParameter(_)));
                    working.insert(key.clone(), previous.clone());
                }
            }
        }

        layers[0] = working
            .into_iter()
            .map(|(key, entry)| {
                assert_eq!(
                    entry.level, 0,
                    "root integer-parameter entry must have level zero"
                );
                let EqKey::IntegerParameter(parameter) = key else {
                    unreachable!("filtered integer-parameter key must contain a parameter");
                };
                let EqValue::IntegerParameter(value) = entry.value else {
                    unreachable!("integer-parameter key must contain a matching value");
                };
                assert_ne!(
                    value,
                    parameter.default_value(),
                    "root integer-parameter defaults must be canonicalized away"
                );
                VmIntegerParameterAssignmentV1 { parameter, value }
            })
            .collect();

        layers
            .iter()
            .any(|layer| !layer.is_empty())
            .then_some(VmIntegerParameterStateV1 { layers })
    }

    fn code_table_snapshot_state(
        &self,
        save_stack: &SaveStack,
        key_character: impl Fn(&EqKey) -> Option<u8> + Copy,
        entry_value: impl Fn(&EqValue) -> Option<i32> + Copy,
    ) -> Option<VmCodeTableStateV1> {
        let mut working = self
            .entries
            .iter()
            .filter(|(key, _)| key_character(key).is_some())
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut layers = vec![Vec::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            for (key, previous) in restores {
                let Some(character) = key_character(key) else {
                    continue;
                };
                let current = working
                    .remove(key)
                    .expect("restore record must have a current code-table entry");
                assert_eq!(
                    current.level, group_level,
                    "current code-table entry must match its restore group level"
                );
                layers[group_level].push(VmCodeTableAssignmentV1 {
                    character,
                    value: entry_value(&current.value)
                        .expect("code-table key must contain a matching value"),
                });
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous code-table entry must precede its restore group level"
                    );
                    entry_value(&previous.value)
                        .expect("code-table restore must contain a matching value");
                    working.insert(key.clone(), previous.clone());
                }
            }
        }

        layers[0] = working
            .into_iter()
            .map(|(key, entry)| {
                assert_eq!(entry.level, 0, "root code-table entry must have level zero");
                VmCodeTableAssignmentV1 {
                    character: key_character(&key)
                        .expect("filtered code-table key must have a character"),
                    value: entry_value(&entry.value)
                        .expect("code-table key must contain a matching value"),
                }
            })
            .collect();

        layers
            .iter()
            .any(|layer| !layer.is_empty())
            .then_some(VmCodeTableStateV1 { layers })
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
        let level = match save_stack.save_if_absent(key.clone(), previous) {
            SaveDisposition::Saved | SaveDisposition::AlreadySaved => group_level,
            SaveDisposition::UntrackedLegacyFrame => 0,
        };
        self.insert_entry(key, EqEntry { value, level });
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

    pub(crate) fn muskip_values(&self) -> BTreeMap<u32, i32> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match (key, &entry.value) {
                (EqKey::MuSkip(index), EqValue::MuGlue(value)) => Some((*index, value.scaled())),
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
    meaning.as_ref()
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{AssignmentScope, DelimiterCodeV1, Eqtb, MathCodeV1, MuGlueScalarV1};
    use crate::{
        command::{Meaning, Primitive},
        save_stack::SaveStack,
        snapshot::IntegerParameterId,
    };
    use tex_tokens::{CatCode, Token};

    #[allow(dead_code)]
    enum RegisterEqValue {
        Integer(i32),
        Dimension(i32),
        Glue(i32),
        MuGlue(MuGlueScalarV1),
        TokenList(Vec<Token>),
        CatCode(CatCode),
        MathCode(MathCodeV1),
        DelCode(DelimiterCodeV1),
    }

    #[test]
    fn control_sequence_values_do_not_inflate_register_entries() {
        assert_eq!(size_of::<super::EqValue>(), size_of::<RegisterEqValue>());
    }

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
    fn muskip_assignments_are_typed_and_restore_independently_from_skip() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_skip(0, 20, AssignmentScope::Global, 0, &mut save_stack);
        eqtb.assign_muskip(
            0,
            MuGlueScalarV1::from_scaled(30),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();

        eqtb.assign_skip(0, 40, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.assign_muskip(
            0,
            MuGlueScalarV1::from_scaled(50),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.skip(0), Some(20));
        assert_eq!(eqtb.muskip(0).map(MuGlueScalarV1::scaled), Some(30));
    }

    #[test]
    fn mathcode_and_delcode_values_enforce_tex82_ranges_and_defaults() {
        assert_eq!(MathCodeV1::try_from_raw(-1), None);
        assert_eq!(MathCodeV1::try_from_raw(0).map(MathCodeV1::raw), Some(0));
        assert_eq!(
            MathCodeV1::try_from_raw(32_768).map(MathCodeV1::raw),
            Some(32_768)
        );
        assert_eq!(MathCodeV1::try_from_raw(32_769), None);

        assert_eq!(DelimiterCodeV1::try_from_raw(i32::MIN), None);
        assert_eq!(
            DelimiterCodeV1::try_from_raw(-2_147_483_647).map(DelimiterCodeV1::raw),
            Some(-2_147_483_647)
        );
        assert_eq!(
            DelimiterCodeV1::try_from_raw(16_777_215).map(DelimiterCodeV1::raw),
            Some(16_777_215)
        );
        assert_eq!(DelimiterCodeV1::try_from_raw(16_777_216), None);

        let eqtb = Eqtb::default();
        assert_eq!(eqtb.mathcode(b'A').raw(), 28_993);
        assert_eq!(eqtb.mathcode(b'a').raw(), 29_025);
        assert_eq!(eqtb.mathcode(b'0').raw(), 28_720);
        assert_eq!(eqtb.mathcode(b'+').raw(), 43);
        assert_eq!(eqtb.delcode(b'A').raw(), -1);
        assert_eq!(eqtb.delcode(b'.').raw(), 0);
    }

    #[test]
    fn tolerance_owner_preserves_sparse_defaults_and_nested_group_state() {
        let tolerance = IntegerParameterId::Tolerance;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();

        assert_eq!(eqtb.integer_parameter(tolerance), 10_000);
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);

        save_stack.begin_group();
        eqtb.assign_integer_parameter(
            tolerance,
            10_000,
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        let same_default = eqtb
            .integer_parameter_snapshot_state(&save_stack)
            .expect("same-default local owner must remain explicit");
        assert!(same_default.layers[0].is_empty());
        assert_eq!(same_default.layers[1][0].parameter, tolerance);
        assert_eq!(same_default.layers[1][0].value, 10_000);

        save_stack.begin_group();
        eqtb.assign_integer_parameter(
            tolerance,
            12_000,
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        let nested = eqtb
            .integer_parameter_snapshot_state(&save_stack)
            .expect("nested parameter state");
        assert_eq!(nested.layers.len(), 3);
        assert!(nested.layers[0].is_empty());
        assert_eq!(nested.layers[1][0].value, 10_000);
        assert_eq!(nested.layers[2][0].value, 12_000);

        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.integer_parameter(tolerance), 10_000);
        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.integer_parameter(tolerance), 10_000);
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);

        eqtb.assign_integer_parameter(
            tolerance,
            12_000,
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(eqtb.integer_parameter(tolerance), 12_000);
        assert_eq!(
            eqtb.integer_parameter_snapshot_state(&save_stack)
                .expect("nondefault root owner")
                .layers[0][0]
                .value,
            12_000
        );
        eqtb.assign_integer_parameter(
            tolerance,
            10_000,
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);
    }

    #[test]
    fn global_default_cancels_all_tolerance_restores() {
        let tolerance = IntegerParameterId::Tolerance;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_integer_parameter(tolerance, 31, AssignmentScope::Global, 0, &mut save_stack);
        save_stack.begin_group();
        eqtb.assign_integer_parameter(tolerance, 41, AssignmentScope::Local, 1, &mut save_stack);
        save_stack.begin_group();
        eqtb.assign_integer_parameter(tolerance, 51, AssignmentScope::Local, 2, &mut save_stack);

        eqtb.assign_integer_parameter(
            tolerance,
            10_000,
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );
        assert_eq!(eqtb.integer_parameter_owner(tolerance), None);
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);

        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.integer_parameter_owner(tolerance), None);
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);
        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.integer_parameter_owner(tolerance), None);
        assert_eq!(eqtb.integer_parameter_snapshot_state(&save_stack), None);
    }

    #[test]
    fn global_nondefault_cancels_all_tolerance_restores() {
        let tolerance = IntegerParameterId::Tolerance;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        save_stack.begin_group();
        eqtb.assign_integer_parameter(
            tolerance,
            10_000,
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_integer_parameter(tolerance, -7, AssignmentScope::Local, 2, &mut save_stack);

        eqtb.assign_integer_parameter(tolerance, 61, AssignmentScope::Global, 2, &mut save_stack);
        for expected_depth in [3, 2, 1] {
            assert_eq!(eqtb.integer_parameter_owner(tolerance), Some((61, 0)));
            let state = eqtb
                .integer_parameter_snapshot_state(&save_stack)
                .expect("global nondefault owner");
            assert_eq!(state.layers.len(), expected_depth);
            assert_eq!(state.layers[0][0].value, 61);
            assert!(state.layers[1..].iter().all(Vec::is_empty));
            if expected_depth > 1 {
                eqtb.end_group(&mut save_stack);
            }
        }
    }

    #[test]
    fn mathcode_and_delcode_assignments_share_nested_group_semantics() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_mathcode(
            b'A',
            MathCodeV1::try_from_raw(100).expect("valid mathcode"),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        eqtb.assign_delcode(
            b'A',
            DelimiterCodeV1::try_from_raw(200).expect("valid delcode"),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_mathcode(
            b'A',
            MathCodeV1::try_from_raw(101).expect("valid mathcode"),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        eqtb.assign_mathcode(
            b'A',
            MathCodeV1::try_from_raw(102).expect("valid mathcode"),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        eqtb.assign_delcode(
            b'A',
            DelimiterCodeV1::try_from_raw(201).expect("valid delcode"),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_mathcode(
            b'A',
            MathCodeV1::try_from_raw(103).expect("valid mathcode"),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        eqtb.assign_delcode(
            b'A',
            DelimiterCodeV1::try_from_raw(202).expect("valid delcode"),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );

        eqtb.assign_mathcode(
            b'A',
            MathCodeV1::try_from_raw(104).expect("valid mathcode"),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );
        eqtb.assign_delcode(
            b'A',
            DelimiterCodeV1::try_from_raw(203).expect("valid delcode"),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );

        let mathcode_state = eqtb
            .mathcode_snapshot_state(&save_stack)
            .expect("explicit mathcode state");
        assert_eq!(mathcode_state.layers.len(), 3);
        assert_eq!(mathcode_state.layers[0][0].character, b'A');
        assert_eq!(mathcode_state.layers[0][0].value, 104);
        assert!(mathcode_state.layers[1].is_empty());
        assert!(mathcode_state.layers[2].is_empty());
        let delcode_state = eqtb
            .delcode_snapshot_state(&save_stack)
            .expect("explicit delcode state");
        assert_eq!(delcode_state.layers.len(), 3);
        assert_eq!(delcode_state.layers[0][0].character, b'A');
        assert_eq!(delcode_state.layers[0][0].value, 203);
        assert!(delcode_state.layers[1].is_empty());
        assert!(delcode_state.layers[2].is_empty());

        eqtb.end_group(&mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.mathcode(b'A').raw(), 104);
        assert_eq!(eqtb.delcode(b'A').raw(), 203);
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
    fn unsupported_legacy_frame_assignment_persists_at_root_level() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_count(0, 1, AssignmentScope::Global, 0, &mut save_stack);
        save_stack.begin_legacy_control_sequence_group();

        eqtb.assign_count(0, 2, AssignmentScope::Local, 1, &mut save_stack);

        assert_eq!(eqtb.entries[&super::EqKey::Count(0)].level, 0);
        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.count(0), Some(2));
        assert_eq!(eqtb.entries[&super::EqKey::Count(0)].level, 0);

        save_stack.begin_group();
        eqtb.assign_count(0, 3, AssignmentScope::Local, 1, &mut save_stack);
        eqtb.end_group(&mut save_stack);

        assert_eq!(eqtb.count(0), Some(2));
        assert_eq!(eqtb.entries[&super::EqKey::Count(0)].level, 0);
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
    fn control_sequence_restore_chain_matches_layered_scope_model_exhaustively() {
        const ACTION_COUNT: usize = 6;
        const MAX_SEQUENCE_LENGTH: u32 = 7;
        let mut observed_repeated_same_value = false;

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
                                char::from(b'A' + action as u8),
                                CatCode::Letter,
                                action,
                                action + 1,
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
                        let expected_values = expected_layers
                            .iter()
                            .filter_map(|layer| layer.get(name))
                            .collect::<Vec<_>>();
                        observed_repeated_same_value |= expected_values
                            .iter()
                            .enumerate()
                            .any(|(index, meaning)| expected_values[..index].contains(meaning));
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

        assert!(
            observed_repeated_same_value,
            "the generated action space must cover equal meanings saved at multiple levels"
        );
    }

    #[test]
    #[should_panic(expected = "restore record must have a current control-sequence entry")]
    fn control_sequence_projection_rejects_a_restore_record_without_current_state() {
        let eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        save_stack.begin_group();
        save_stack.save_if_absent(super::EqKey::ControlSequence("missing".to_string()), None);

        eqtb.control_sequence_layers(&save_stack);
    }

    #[test]
    #[should_panic(expected = "root control-sequence entry must have level zero")]
    fn control_sequence_projection_rejects_a_nonzero_root_entry_level() {
        let mut eqtb = Eqtb::default();
        eqtb.control_sequences.insert(
            "bad-root".to_string(),
            super::EqEntry {
                value: super::EqValue::ControlSequence(Box::new(Meaning::Primitive(
                    Primitive::Relax,
                ))),
                level: 1,
            },
        );

        eqtb.control_sequence_layers(&SaveStack::default());
    }
}
