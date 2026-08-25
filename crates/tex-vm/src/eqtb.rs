use std::collections::{BTreeMap, HashMap};

use tex_lexer::CatCodeTable;
use tex_tokens::{CatCode, Token};

use crate::{
    command::Meaning,
    dimension_parameter::{
        DimensionParameterId, RawDimensionSp, VmDimensionParameterAssignmentV1,
        VmDimensionParameterStateV1,
    },
    magnification::{
        MagnificationPreparationIssue, PreparedMagnification, RequestedMagnification,
        VmMagnificationStateV1,
    },
    save_stack::{SaveDisposition, SaveStack},
    snapshot::{
        IntegerParameterId, LayoutIntegerParameterId, VmCodeTableAssignmentV1, VmCodeTableStateV1,
        VmIntegerParameterAssignmentV1, VmIntegerParameterStateV1,
        VmLayoutIntegerParameterAssignmentV1, VmLayoutIntegerParameterStateV1,
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
    LayoutIntegerParameter(LayoutIntegerParameterId),
    DimensionParameter(DimensionParameterId),
    ControlSequence(String),
    MagnificationRequested,
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
    DimensionParameter(RawDimensionSp),
    ControlSequence(Box<Meaning>),
    MagnificationRequested(RequestedMagnification),
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
    prepared_magnification: Option<PreparedMagnification>,
}

impl Default for Eqtb {
    fn default() -> Self {
        let base_catcodes = CatCodeTable::plain_tex();
        Self {
            entries: BTreeMap::new(),
            control_sequences: BTreeMap::new(),
            catcodes: base_catcodes.clone(),
            base_catcodes,
            prepared_magnification: None,
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
                | EqValue::DimensionParameter(_)
                | EqValue::MagnificationRequested(_)
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
                | EqValue::DimensionParameter(_)
                | EqValue::MagnificationRequested(_)
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
                | EqValue::DimensionParameter(_)
                | EqValue::MagnificationRequested(_)
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
                | EqValue::DimensionParameter(_)
                | EqValue::MagnificationRequested(_)
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
                | EqValue::DimensionParameter(_)
                | EqValue::MagnificationRequested(_)
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

    pub(crate) fn integer_parameter(&self, parameter: IntegerParameterId) -> i32 {
        self.entries
            .get(&EqKey::IntegerParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::IntegerParameter(value) => value,
                _ => unreachable!("integer-parameter entry must contain an integer parameter"),
            })
            .unwrap_or_else(|| parameter.default_value())
    }

    pub(crate) fn layout_integer_parameter(&self, parameter: LayoutIntegerParameterId) -> i32 {
        self.entries
            .get(&EqKey::LayoutIntegerParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::IntegerParameter(value) => value,
                _ => {
                    unreachable!("layout-integer-parameter entry must contain an integer parameter")
                }
            })
            .unwrap_or_else(|| parameter.default_value())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn dimension_parameter(&self, parameter: DimensionParameterId) -> RawDimensionSp {
        self.entries
            .get(&EqKey::DimensionParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::DimensionParameter(value) => value,
                _ => unreachable!("dimension-parameter entry must contain a dimension parameter"),
            })
            .unwrap_or_else(|| parameter.default_value())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn requested_magnification(&self) -> RequestedMagnification {
        self.entries
            .get(&EqKey::MagnificationRequested)
            .map(|entry| match entry.value {
                EqValue::MagnificationRequested(value) => value,
                _ => unreachable!("magnification key must contain requested magnification"),
            })
            .unwrap_or_default()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn prepared_magnification(&self) -> Option<PreparedMagnification> {
        self.prepared_magnification
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

    #[cfg(test)]
    pub(crate) fn layout_integer_parameter_owner(
        &self,
        parameter: LayoutIntegerParameterId,
    ) -> Option<(i32, usize)> {
        self.entries
            .get(&EqKey::LayoutIntegerParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::IntegerParameter(value) => (value, entry.level),
                _ => {
                    unreachable!("layout-integer-parameter entry must contain an integer parameter")
                }
            })
    }

    #[cfg(test)]
    pub(crate) fn dimension_parameter_owner(
        &self,
        parameter: DimensionParameterId,
    ) -> Option<(RawDimensionSp, usize)> {
        self.entries
            .get(&EqKey::DimensionParameter(parameter))
            .map(|entry| match entry.value {
                EqValue::DimensionParameter(value) => (value, entry.level),
                _ => unreachable!("dimension-parameter entry must contain a dimension parameter"),
            })
    }

    #[cfg(test)]
    pub(crate) fn requested_magnification_owner(&self) -> Option<(RequestedMagnification, usize)> {
        self.entries
            .get(&EqKey::MagnificationRequested)
            .map(|entry| match entry.value {
                EqValue::MagnificationRequested(value) => (value, entry.level),
                _ => unreachable!("magnification key must contain requested magnification"),
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

    pub(crate) fn assign_layout_integer_parameter(
        &mut self,
        parameter: LayoutIntegerParameterId,
        value: i32,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        let key = EqKey::LayoutIntegerParameter(parameter);
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn assign_dimension_parameter(
        &mut self,
        parameter: DimensionParameterId,
        value: RawDimensionSp,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        let key = EqKey::DimensionParameter(parameter);
        if (scope == AssignmentScope::Global || group_level == 0)
            && value == parameter.default_value()
        {
            save_stack.cancel_restore(&key);
            self.remove_entry(&key);
            return;
        }
        self.assign(
            key,
            EqValue::DimensionParameter(value),
            scope,
            group_level,
            save_stack,
        );
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn assign_requested_magnification(
        &mut self,
        value: RequestedMagnification,
        scope: AssignmentScope,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) {
        let key = EqKey::MagnificationRequested;
        if (scope == AssignmentScope::Global || group_level == 0)
            && value == RequestedMagnification::DEFAULT
        {
            save_stack.cancel_restore(&key);
            self.remove_entry(&key);
            return;
        }
        self.assign(
            key,
            EqValue::MagnificationRequested(value),
            scope,
            group_level,
            save_stack,
        );
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn prepare_magnification(
        &mut self,
        group_level: usize,
        save_stack: &mut SaveStack,
    ) -> Result<PreparedMagnification, MagnificationPreparationIssue> {
        let requested = self.requested_magnification();
        if let Some(prepared) = self.prepared_magnification {
            if i32::from(prepared.get()) == requested.get() {
                return Ok(prepared);
            }
            self.assign_requested_magnification(
                RequestedMagnification::new(i32::from(prepared.get())),
                AssignmentScope::Global,
                group_level,
                save_stack,
            );
            return Err(MagnificationPreparationIssue::Incompatible {
                requested,
                prepared,
            });
        }

        if let Some(prepared) = PreparedMagnification::from_requested(requested) {
            self.prepared_magnification = Some(prepared);
            return Ok(prepared);
        }

        let prepared = PreparedMagnification::from_requested(RequestedMagnification::DEFAULT)
            .expect("default magnification must be legal");
        self.assign_requested_magnification(
            RequestedMagnification::DEFAULT,
            AssignmentScope::Global,
            group_level,
            save_stack,
        );
        self.prepared_magnification = Some(prepared);
        Err(MagnificationPreparationIssue::Illegal { requested })
    }

    pub(crate) fn restore_prepared_magnification(
        &mut self,
        prepared: Option<PreparedMagnification>,
    ) {
        self.prepared_magnification = prepared;
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

    pub(crate) fn layout_integer_parameter_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmLayoutIntegerParameterStateV1> {
        let mut working = self
            .entries
            .iter()
            .filter(|(key, _)| matches!(key, EqKey::LayoutIntegerParameter(_)))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut layers = vec![Vec::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            for (key, previous) in restores {
                let EqKey::LayoutIntegerParameter(parameter) = key else {
                    continue;
                };
                let current = working
                    .remove(key)
                    .expect("restore record must have a current layout-integer-parameter entry");
                assert_eq!(
                    current.level, group_level,
                    "current layout-integer-parameter entry must match its restore group level"
                );
                let EqValue::IntegerParameter(value) = current.value else {
                    unreachable!("layout-integer-parameter key must contain a matching value");
                };
                layers[group_level].push(VmLayoutIntegerParameterAssignmentV1 {
                    parameter: *parameter,
                    value,
                });
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous layout-integer-parameter entry must precede its restore group level"
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
                    "root layout-integer-parameter entry must have level zero"
                );
                let EqKey::LayoutIntegerParameter(parameter) = key else {
                    unreachable!("filtered layout-integer-parameter key must contain a parameter");
                };
                let EqValue::IntegerParameter(value) = entry.value else {
                    unreachable!("layout-integer-parameter key must contain a matching value");
                };
                assert_ne!(
                    value,
                    parameter.default_value(),
                    "root layout-integer-parameter defaults must be canonicalized away"
                );
                VmLayoutIntegerParameterAssignmentV1 { parameter, value }
            })
            .collect();

        layers
            .iter()
            .any(|layer| !layer.is_empty())
            .then_some(VmLayoutIntegerParameterStateV1 { layers })
    }

    pub(crate) fn dimension_parameter_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmDimensionParameterStateV1> {
        let mut working = self
            .entries
            .iter()
            .filter(|(key, _)| matches!(key, EqKey::DimensionParameter(_)))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut layers = vec![Vec::new(); save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            for (key, previous) in restores {
                let EqKey::DimensionParameter(parameter) = key else {
                    continue;
                };
                let current = working
                    .remove(key)
                    .expect("restore record must have a current dimension-parameter entry");
                assert_eq!(
                    current.level, group_level,
                    "current dimension-parameter entry must match its restore group level"
                );
                let EqValue::DimensionParameter(value) = current.value else {
                    unreachable!("dimension-parameter key must contain a matching value");
                };
                layers[group_level].push(VmDimensionParameterAssignmentV1 {
                    parameter: *parameter,
                    value,
                });
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous dimension-parameter entry must precede its restore group level"
                    );
                    assert!(matches!(previous.value, EqValue::DimensionParameter(_)));
                    working.insert(key.clone(), previous.clone());
                }
            }
        }

        layers[0] = working
            .into_iter()
            .map(|(key, entry)| {
                assert_eq!(
                    entry.level, 0,
                    "root dimension-parameter entry must have level zero"
                );
                let EqKey::DimensionParameter(parameter) = key else {
                    unreachable!("filtered dimension-parameter key must contain a parameter");
                };
                let EqValue::DimensionParameter(value) = entry.value else {
                    unreachable!("dimension-parameter key must contain a matching value");
                };
                assert_ne!(
                    value,
                    parameter.default_value(),
                    "root dimension-parameter defaults must be canonicalized away"
                );
                VmDimensionParameterAssignmentV1 { parameter, value }
            })
            .collect();

        layers
            .iter()
            .any(|layer| !layer.is_empty())
            .then_some(VmDimensionParameterStateV1 { layers })
    }

    pub(crate) fn magnification_snapshot_state(
        &self,
        save_stack: &SaveStack,
    ) -> Option<VmMagnificationStateV1> {
        let key = EqKey::MagnificationRequested;
        let mut working = self.entries.get(&key).cloned();
        let mut requested_layers = vec![None; save_stack.scope_depth()];

        for (group_index, restores) in save_stack.restore_groups().enumerate().rev() {
            let group_level = group_index + 1;
            for (restore_key, previous) in restores {
                if restore_key != &key {
                    continue;
                }
                let current = working
                    .take()
                    .expect("restore record must have a current magnification entry");
                assert_eq!(
                    current.level, group_level,
                    "current magnification entry must match its restore group level"
                );
                let EqValue::MagnificationRequested(value) = current.value else {
                    unreachable!("magnification key must contain a matching value");
                };
                requested_layers[group_level] = Some(value.get());
                if let Some(previous) = previous {
                    assert!(
                        previous.level < group_level,
                        "previous magnification entry must precede its restore group level"
                    );
                    assert!(matches!(previous.value, EqValue::MagnificationRequested(_)));
                    working = Some(previous.clone());
                }
            }
        }

        requested_layers[0] = working.map(|entry| {
            assert_eq!(
                entry.level, 0,
                "root magnification entry must have level zero"
            );
            let EqValue::MagnificationRequested(value) = entry.value else {
                unreachable!("magnification key must contain a matching value");
            };
            assert_ne!(
                value,
                RequestedMagnification::DEFAULT,
                "root default magnification must be canonicalized away"
            );
            value.get()
        });
        let prepared_effective = self
            .prepared_magnification
            .map(|prepared| i32::from(prepared.get()));

        (requested_layers.iter().any(Option::is_some) || prepared_effective.is_some()).then_some(
            VmMagnificationStateV1 {
                requested_layers,
                prepared_effective,
            },
        )
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
        dimension_parameter::{
            DimensionParameterId, RawDimensionSp, VmDimensionParameterAssignmentV1,
            VmDimensionParameterStateV1,
        },
        magnification::{MagnificationPreparationIssue, RequestedMagnification},
        save_stack::SaveStack,
        snapshot::{IntegerParameterId, LayoutIntegerParameterId},
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

    #[derive(Debug, Clone)]
    struct LayoutParameterReference {
        default: i32,
        root: Option<i32>,
        locals: Vec<Option<i32>>,
    }

    impl LayoutParameterReference {
        fn new(default: i32) -> Self {
            Self {
                default,
                root: None,
                locals: Vec::new(),
            }
        }

        fn depth(&self) -> usize {
            self.locals.len()
        }

        fn read(&self) -> i32 {
            self.locals
                .iter()
                .rev()
                .find_map(|value| *value)
                .or(self.root)
                .unwrap_or(self.default)
        }

        fn begin_group(&mut self) {
            self.locals.push(None);
        }

        fn end_group(&mut self) {
            self.locals.pop().expect("reference group");
        }

        fn assign(&mut self, value: i32, scope: AssignmentScope) {
            if scope == AssignmentScope::Global || self.locals.is_empty() {
                self.locals.fill(None);
                self.root = (value != self.default).then_some(value);
            } else {
                *self.locals.last_mut().expect("current reference group") = Some(value);
            }
        }

        fn projected_state(
            &self,
            parameter: LayoutIntegerParameterId,
        ) -> Option<crate::snapshot::VmLayoutIntegerParameterStateV1> {
            let mut layers = vec![Vec::new(); self.depth() + 1];
            if let Some(value) = self.root {
                layers[0].push(crate::snapshot::VmLayoutIntegerParameterAssignmentV1 {
                    parameter,
                    value,
                });
            }
            for (index, value) in self.locals.iter().enumerate() {
                if let Some(value) = value {
                    layers[index + 1].push(crate::snapshot::VmLayoutIntegerParameterAssignmentV1 {
                        parameter,
                        value: *value,
                    });
                }
            }
            layers
                .iter()
                .any(|layer| !layer.is_empty())
                .then_some(crate::snapshot::VmLayoutIntegerParameterStateV1 { layers })
        }
    }

    #[test]
    fn control_sequence_values_do_not_inflate_register_entries() {
        assert_eq!(size_of::<super::EqValue>(), size_of::<RegisterEqValue>());
    }

    #[test]
    fn dimension_parameter_owner_preserves_sparse_defaults_and_nested_groups() {
        let hangindent = DimensionParameterId::HangIndent;
        let mut eqtb = Eqtb::default();
        let untouched = Eqtb::default();
        let mut save_stack = SaveStack::default();

        assert_eq!(eqtb.dimension_parameter(hangindent), RawDimensionSp::new(0));
        assert_eq!(eqtb.dimension_parameter_owner(hangindent), None);

        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(0),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.dimension_parameter_owner(hangindent),
            Some((RawDimensionSp::new(0), 1))
        );
        assert_eq!(untouched.dimension_parameter_owner(hangindent), None);

        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(i32::MIN),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.dimension_parameter_owner(hangindent),
            Some((RawDimensionSp::new(i32::MIN), 2))
        );

        eqtb.end_group(&mut save_stack);
        assert_eq!(
            eqtb.dimension_parameter_owner(hangindent),
            Some((RawDimensionSp::new(0), 1))
        );
        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.dimension_parameter_owner(hangindent), None);
        assert_eq!(eqtb.dimension_parameter(hangindent), RawDimensionSp::new(0));

        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(i32::MAX),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.dimension_parameter_owner(hangindent),
            Some((RawDimensionSp::new(i32::MAX), 0))
        );
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(0),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(eqtb.dimension_parameter_owner(hangindent), None);
    }

    #[test]
    fn global_dimension_parameter_assignments_cancel_all_pending_restores() {
        let hangindent = DimensionParameterId::HangIndent;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();

        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(41),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(-51),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(61),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );
        for expected_depth in [2, 1, 0] {
            assert_eq!(save_stack.group_level(), expected_depth);
            assert_eq!(
                eqtb.dimension_parameter_owner(hangindent),
                Some((RawDimensionSp::new(61), 0))
            );
            if expected_depth > 0 {
                eqtb.end_group(&mut save_stack);
            }
        }

        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(71),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(81),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(0),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );
        for expected_depth in [2, 1, 0] {
            assert_eq!(save_stack.group_level(), expected_depth);
            assert_eq!(eqtb.dimension_parameter_owner(hangindent), None);
            if expected_depth > 0 {
                eqtb.end_group(&mut save_stack);
            }
        }
    }

    #[test]
    fn dimension_parameter_owner_matches_an_independent_bounded_state_model() {
        #[derive(Clone, Copy, Debug)]
        enum Operation {
            BeginGroup,
            EndGroup,
            AssignLocal(i32),
            AssignGlobal(i32),
        }

        let operations = [
            Operation::BeginGroup,
            Operation::EndGroup,
            Operation::AssignLocal(0),
            Operation::AssignLocal(1),
            Operation::AssignLocal(-1),
            Operation::AssignGlobal(0),
            Operation::AssignGlobal(1),
            Operation::AssignGlobal(-1),
        ];
        let hangindent = DimensionParameterId::HangIndent;
        let default = RawDimensionSp::new(0);
        let mut traces = vec![(Vec::new(), 0_usize)];

        for trace_length in 0..=5 {
            for (trace, _) in &traces {
                let mut eqtb = Eqtb::default();
                let mut save_stack = SaveStack::default();
                let mut model_owner = None;
                let mut model_restores = Vec::<Option<Option<(RawDimensionSp, usize)>>>::new();

                for operation in trace {
                    match *operation {
                        Operation::BeginGroup => {
                            save_stack.begin_group();
                            model_restores.push(None);
                        }
                        Operation::EndGroup => {
                            eqtb.end_group(&mut save_stack);
                            if let Some(previous) = model_restores
                                .pop()
                                .expect("generated trace must have an open model group")
                            {
                                model_owner = previous;
                            }
                        }
                        Operation::AssignLocal(raw) => {
                            let value = RawDimensionSp::new(raw);
                            let group_level = model_restores.len();
                            eqtb.assign_dimension_parameter(
                                hangindent,
                                value,
                                AssignmentScope::Local,
                                group_level,
                                &mut save_stack,
                            );
                            if group_level == 0 {
                                model_owner = (value != default).then_some((value, 0));
                            } else {
                                let restore = model_restores
                                    .last_mut()
                                    .expect("positive model depth must have a group");
                                if restore.is_none() {
                                    *restore = Some(model_owner);
                                }
                                model_owner = Some((value, group_level));
                            }
                        }
                        Operation::AssignGlobal(raw) => {
                            let value = RawDimensionSp::new(raw);
                            eqtb.assign_dimension_parameter(
                                hangindent,
                                value,
                                AssignmentScope::Global,
                                model_restores.len(),
                                &mut save_stack,
                            );
                            model_restores.fill(None);
                            model_owner = (value != default).then_some((value, 0));
                        }
                    }

                    assert_eq!(
                        save_stack.group_level(),
                        model_restores.len(),
                        "group depth diverged after trace {trace:?}"
                    );
                    assert_eq!(
                        eqtb.dimension_parameter(hangindent),
                        model_owner.map_or(default, |(value, _)| value),
                        "effective value diverged after trace {trace:?}"
                    );
                    assert_eq!(
                        eqtb.dimension_parameter_owner(hangindent),
                        model_owner,
                        "materialized owner diverged after trace {trace:?}"
                    );
                }

                while let Some(previous) = model_restores.pop() {
                    eqtb.end_group(&mut save_stack);
                    if let Some(previous) = previous {
                        model_owner = previous;
                    }
                    assert_eq!(
                        eqtb.dimension_parameter(hangindent),
                        model_owner.map_or(default, |(value, _)| value),
                        "drained effective value diverged after trace {trace:?}"
                    );
                    assert_eq!(
                        eqtb.dimension_parameter_owner(hangindent),
                        model_owner,
                        "drained materialized owner diverged after trace {trace:?}"
                    );
                }
            }

            if trace_length == 5 {
                break;
            }
            let mut next_traces = Vec::new();
            for (trace, depth) in &traces {
                for operation in operations {
                    let next_depth = match operation {
                        Operation::BeginGroup if *depth < 2 => depth + 1,
                        Operation::EndGroup if *depth > 0 => depth - 1,
                        Operation::AssignLocal(_) | Operation::AssignGlobal(_) => *depth,
                        Operation::BeginGroup | Operation::EndGroup => continue,
                    };
                    let mut next_trace = trace.clone();
                    next_trace.push(operation);
                    next_traces.push((next_trace, next_depth));
                }
            }
            traces = next_traces;
        }
    }

    #[test]
    fn dimension_parameter_snapshot_state_preserves_canonical_owner_layers() {
        let hangindent = DimensionParameterId::HangIndent;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();

        assert_eq!(eqtb.dimension_parameter_snapshot_state(&save_stack), None);
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(i32::MAX),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(0),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(i32::MIN),
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );

        assert_eq!(
            eqtb.dimension_parameter_snapshot_state(&save_stack),
            Some(VmDimensionParameterStateV1 {
                layers: vec![
                    vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(i32::MAX),
                    }],
                    vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(0),
                    }],
                    vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(i32::MIN),
                    }],
                ],
            })
        );

        eqtb.end_group(&mut save_stack);
        assert_eq!(
            eqtb.dimension_parameter_snapshot_state(&save_stack),
            Some(VmDimensionParameterStateV1 {
                layers: vec![
                    vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(i32::MAX),
                    }],
                    vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(0),
                    }],
                ],
            })
        );

        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(0),
            AssignmentScope::Global,
            1,
            &mut save_stack,
        );
        assert_eq!(eqtb.dimension_parameter_snapshot_state(&save_stack), None);
    }

    #[test]
    fn global_nondefault_dimension_assignment_cancels_nested_local_owner_layers() {
        let hangindent = DimensionParameterId::HangIndent;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();

        save_stack.begin_group();
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(41),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        save_stack.begin_group();
        for value in [51, 52] {
            eqtb.assign_dimension_parameter(
                hangindent,
                RawDimensionSp::new(value),
                AssignmentScope::Local,
                2,
                &mut save_stack,
            );
        }
        eqtb.assign_dimension_parameter(
            hangindent,
            RawDimensionSp::new(61),
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );

        for expected_layers in [3, 2, 1] {
            assert_eq!(
                eqtb.dimension_parameter_snapshot_state(&save_stack),
                Some(VmDimensionParameterStateV1 {
                    layers: std::iter::once(vec![VmDimensionParameterAssignmentV1 {
                        parameter: hangindent,
                        value: RawDimensionSp::new(61),
                    }])
                    .chain(std::iter::repeat_n(Vec::new(), expected_layers - 1))
                    .collect(),
                })
            );
            assert_eq!(
                eqtb.dimension_parameter_owner(hangindent),
                Some((RawDimensionSp::new(61), 0))
            );
            if expected_layers > 1 {
                eqtb.end_group(&mut save_stack);
            }
        }
    }

    #[test]
    fn layout_integer_parameters_preserve_virtual_defaults_and_nested_owners() {
        let parameters = [
            (LayoutIntegerParameterId::AdjDemerits, 0),
            (LayoutIntegerParameterId::BinOpPenalty, 0),
            (LayoutIntegerParameterId::BrokenPenalty, 0),
            (LayoutIntegerParameterId::ClubPenalty, 0),
            (LayoutIntegerParameterId::DisplayWidowPenalty, 0),
            (LayoutIntegerParameterId::DoubleHyphenDemerits, 0),
            (LayoutIntegerParameterId::ExHyphenPenalty, 0),
            (LayoutIntegerParameterId::FinalHyphenDemerits, 0),
            (LayoutIntegerParameterId::HangAfter, 1),
            (LayoutIntegerParameterId::HyphenPenalty, 0),
            (LayoutIntegerParameterId::InterlinePenalty, 0),
            (LayoutIntegerParameterId::LinePenalty, 0),
            (LayoutIntegerParameterId::Looseness, 0),
            (LayoutIntegerParameterId::PostDisplayPenalty, 0),
            (LayoutIntegerParameterId::PreDisplayPenalty, 0),
            (LayoutIntegerParameterId::PreTolerance, 0),
            (LayoutIntegerParameterId::RelPenalty, 0),
            (LayoutIntegerParameterId::WidowPenalty, 0),
        ];
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();

        for (parameter, default) in parameters {
            assert_eq!(eqtb.layout_integer_parameter(parameter), default);
            assert_eq!(eqtb.layout_integer_parameter_owner(parameter), None);
        }
        assert_eq!(
            eqtb.layout_integer_parameter_snapshot_state(&save_stack),
            None
        );

        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::AdjDemerits,
            9,
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::AdjDemerits,
            0,
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.layout_integer_parameter_owner(LayoutIntegerParameterId::AdjDemerits),
            None
        );

        save_stack.begin_group();
        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::HangAfter,
            1,
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.layout_integer_parameter_owner(LayoutIntegerParameterId::HangAfter),
            Some((1, 1))
        );

        save_stack.begin_group();
        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::PreTolerance,
            7,
            AssignmentScope::Local,
            2,
            &mut save_stack,
        );
        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::HangAfter,
            2,
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );
        eqtb.assign_layout_integer_parameter(
            LayoutIntegerParameterId::PreTolerance,
            0,
            AssignmentScope::Global,
            2,
            &mut save_stack,
        );

        assert_eq!(
            eqtb.layout_integer_parameter_owner(LayoutIntegerParameterId::HangAfter),
            Some((2, 0))
        );
        assert_eq!(
            eqtb.layout_integer_parameter_owner(LayoutIntegerParameterId::PreTolerance),
            None
        );
        let state = eqtb
            .layout_integer_parameter_snapshot_state(&save_stack)
            .expect("global nondefault owner");
        assert_eq!(state.layers.len(), 3);
        assert_eq!(state.layers[0].len(), 1);
        assert_eq!(
            state.layers[0][0].parameter,
            LayoutIntegerParameterId::HangAfter
        );
        assert_eq!(state.layers[0][0].value, 2);
        assert!(state.layers[1].is_empty());
        assert!(state.layers[2].is_empty());

        eqtb.end_group(&mut save_stack);
        eqtb.end_group(&mut save_stack);
        assert_eq!(
            eqtb.layout_integer_parameter_owner(LayoutIntegerParameterId::HangAfter),
            Some((2, 0))
        );
        assert_eq!(
            eqtb.layout_integer_parameter_snapshot_state(&save_stack)
                .expect("global owner")
                .layers
                .len(),
            1
        );
    }

    #[test]
    fn layout_integer_parameter_owner_matches_reference_model_depth_zero_through_eight() {
        let parameters = [
            (LayoutIntegerParameterId::AdjDemerits, 0),
            (LayoutIntegerParameterId::BinOpPenalty, 0),
            (LayoutIntegerParameterId::BrokenPenalty, 0),
            (LayoutIntegerParameterId::ClubPenalty, 0),
            (LayoutIntegerParameterId::DisplayWidowPenalty, 0),
            (LayoutIntegerParameterId::DoubleHyphenDemerits, 0),
            (LayoutIntegerParameterId::ExHyphenPenalty, 0),
            (LayoutIntegerParameterId::FinalHyphenDemerits, 0),
            (LayoutIntegerParameterId::HangAfter, 1),
            (LayoutIntegerParameterId::HyphenPenalty, 0),
            (LayoutIntegerParameterId::InterlinePenalty, 0),
            (LayoutIntegerParameterId::LinePenalty, 0),
            (LayoutIntegerParameterId::Looseness, 0),
            (LayoutIntegerParameterId::PostDisplayPenalty, 0),
            (LayoutIntegerParameterId::PreDisplayPenalty, 0),
            (LayoutIntegerParameterId::PreTolerance, 0),
            (LayoutIntegerParameterId::RelPenalty, 0),
            (LayoutIntegerParameterId::WidowPenalty, 0),
        ];

        for (parameter, default) in parameters {
            for maximum_depth in 0..=8 {
                let mut eqtb = Eqtb::default();
                let mut save_stack = SaveStack::default();
                let mut reference = LayoutParameterReference::new(default);
                let assert_matches =
                    |eqtb: &Eqtb, save_stack: &SaveStack, reference: &LayoutParameterReference| {
                        assert_eq!(
                            eqtb.layout_integer_parameter(parameter),
                            reference.read(),
                            "value mismatch for {parameter:?} at depth {}",
                            reference.depth()
                        );
                        assert_eq!(
                            eqtb.layout_integer_parameter_snapshot_state(save_stack),
                            reference.projected_state(parameter),
                            "projection mismatch for {parameter:?} at depth {}",
                            reference.depth()
                        );
                    };

                for (value, scope) in [
                    (i32::MIN, AssignmentScope::Global),
                    (default, AssignmentScope::Local),
                    (i32::MAX, AssignmentScope::Global),
                    (-17, AssignmentScope::Local),
                ] {
                    eqtb.assign_layout_integer_parameter(
                        parameter,
                        value,
                        scope,
                        0,
                        &mut save_stack,
                    );
                    reference.assign(value, scope);
                    assert_matches(&eqtb, &save_stack, &reference);
                }

                for level in 1..=maximum_depth {
                    save_stack.begin_group();
                    reference.begin_group();
                    assert_matches(&eqtb, &save_stack, &reference);
                    let operations: &[(i32, AssignmentScope)] = match level {
                        1 => &[(default, AssignmentScope::Local)],
                        2 => &[(reference.read(), AssignmentScope::Local)],
                        3 => &[
                            (default, AssignmentScope::Local),
                            (default.wrapping_add(1), AssignmentScope::Local),
                            (-31, AssignmentScope::Local),
                        ],
                        4 => &[],
                        5 => &[
                            (i32::MAX, AssignmentScope::Global),
                            (default, AssignmentScope::Local),
                        ],
                        6 => &[(i32::MIN, AssignmentScope::Local)],
                        7 => &[(default, AssignmentScope::Global)],
                        8 => &[(-17, AssignmentScope::Local)],
                        _ => unreachable!(),
                    };
                    for &(value, scope) in operations {
                        eqtb.assign_layout_integer_parameter(
                            parameter,
                            value,
                            scope,
                            level,
                            &mut save_stack,
                        );
                        reference.assign(value, scope);
                        assert_matches(&eqtb, &save_stack, &reference);
                    }
                }

                let projected = eqtb.layout_integer_parameter_snapshot_state(&save_stack);
                let mut restored = Eqtb::default();
                let mut restored_stack = SaveStack::default();
                if let Some(state) = &projected {
                    for (level, layer) in state.layers.iter().enumerate() {
                        if level > 0 {
                            restored_stack.begin_group();
                        }
                        let scope = if level == 0 {
                            AssignmentScope::Global
                        } else {
                            AssignmentScope::Local
                        };
                        for assignment in layer {
                            restored.assign_layout_integer_parameter(
                                assignment.parameter,
                                assignment.value,
                                scope,
                                level,
                                &mut restored_stack,
                            );
                        }
                    }
                } else {
                    for _ in 0..maximum_depth {
                        restored_stack.begin_group();
                    }
                }
                assert_matches(&restored, &restored_stack, &reference);
                assert_eq!(
                    restored.layout_integer_parameter_snapshot_state(&restored_stack),
                    projected
                );

                while reference.depth() > 0 {
                    eqtb.end_group(&mut save_stack);
                    restored.end_group(&mut restored_stack);
                    reference.end_group();
                    assert_matches(&eqtb, &save_stack, &reference);
                    assert_matches(&restored, &restored_stack, &reference);
                }
            }
        }
    }

    #[test]
    fn layout_integer_parameter_global_assignment_cancels_every_reference_restore() {
        let parameter = LayoutIntegerParameterId::HangAfter;
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        let mut reference = LayoutParameterReference::new(1);

        for (level, value) in [(1, 2), (2, 1), (3, -7), (4, i32::MIN)] {
            save_stack.begin_group();
            reference.begin_group();
            eqtb.assign_layout_integer_parameter(
                parameter,
                value,
                AssignmentScope::Local,
                level,
                &mut save_stack,
            );
            reference.assign(value, AssignmentScope::Local);
        }
        eqtb.assign_layout_integer_parameter(
            parameter,
            i32::MAX,
            AssignmentScope::Global,
            4,
            &mut save_stack,
        );
        reference.assign(i32::MAX, AssignmentScope::Global);
        assert_eq!(
            eqtb.layout_integer_parameter_snapshot_state(&save_stack),
            reference.projected_state(parameter)
        );

        while reference.depth() > 0 {
            eqtb.end_group(&mut save_stack);
            reference.end_group();
            assert_eq!(eqtb.layout_integer_parameter(parameter), reference.read());
            assert_eq!(
                eqtb.layout_integer_parameter_snapshot_state(&save_stack),
                reference.projected_state(parameter)
            );
        }
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

    #[test]
    fn requested_magnification_groups_without_mutating_the_prepared_latch() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        assert_eq!(eqtb.requested_magnification().get(), 1000);
        assert_eq!(eqtb.prepared_magnification(), None);

        save_stack.begin_group();
        eqtb.assign_requested_magnification(
            RequestedMagnification::new(i32::MIN),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        assert_eq!(eqtb.requested_magnification().get(), i32::MIN);
        assert_eq!(
            eqtb.requested_magnification_owner(),
            Some((RequestedMagnification::new(i32::MIN), 1))
        );
        assert_eq!(eqtb.prepared_magnification(), None);

        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.requested_magnification().get(), 1000);
        assert_eq!(eqtb.requested_magnification_owner(), None);
        assert_eq!(eqtb.prepared_magnification(), None);
    }

    #[test]
    fn preparing_magnification_freezes_the_native_range_and_global_corrections() {
        let mut eqtb = Eqtb::default();
        let mut save_stack = SaveStack::default();
        eqtb.assign_requested_magnification(
            RequestedMagnification::new(32_768),
            AssignmentScope::Global,
            0,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.prepare_magnification(0, &mut save_stack)
                .expect("32768 is the maximum legal first preparation")
                .get(),
            32_768
        );

        save_stack.begin_group();
        eqtb.assign_requested_magnification(
            RequestedMagnification::new(1_000),
            AssignmentScope::Local,
            1,
            &mut save_stack,
        );
        assert_eq!(
            eqtb.prepare_magnification(1, &mut save_stack),
            Err(MagnificationPreparationIssue::Incompatible {
                requested: RequestedMagnification::new(1_000),
                prepared: eqtb.prepared_magnification().expect("prepared latch"),
            })
        );
        assert_eq!(eqtb.requested_magnification().get(), 32_768);
        assert_eq!(eqtb.requested_magnification_owner().unwrap().1, 0);
        eqtb.end_group(&mut save_stack);
        assert_eq!(eqtb.requested_magnification().get(), 32_768);

        let mut invalid = Eqtb::default();
        let mut invalid_save_stack = SaveStack::default();
        invalid_save_stack.begin_group();
        invalid.assign_requested_magnification(
            RequestedMagnification::new(32_769),
            AssignmentScope::Local,
            1,
            &mut invalid_save_stack,
        );
        assert_eq!(
            invalid.prepare_magnification(1, &mut invalid_save_stack),
            Err(MagnificationPreparationIssue::Illegal {
                requested: RequestedMagnification::new(32_769),
            })
        );
        assert_eq!(invalid.prepared_magnification().unwrap().get(), 1_000);
        assert_eq!(invalid.requested_magnification().get(), 1_000);
        assert_eq!(invalid.requested_magnification_owner(), None);
        invalid.end_group(&mut invalid_save_stack);
        assert_eq!(invalid.requested_magnification().get(), 1_000);
    }
}
