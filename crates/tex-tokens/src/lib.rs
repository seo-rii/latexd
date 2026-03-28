use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ControlSequenceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatCode {
    Escape,
    BeginGroup,
    EndGroup,
    MathShift,
    AlignmentTab,
    EndOfLine,
    Parameter,
    Superscript,
    Subscript,
    Ignored,
    Space,
    Letter,
    Other,
    Active,
    Comment,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenKind {
    ControlSequence { name: ControlSequenceId },
    Character { ch: char, catcode: CatCode },
}

#[derive(Debug, Default, Clone)]
pub struct ControlSequenceInterner {
    ids: HashMap<Box<str>, ControlSequenceId>,
    names: Vec<Box<str>>,
}

impl ControlSequenceInterner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> ControlSequenceId {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }

        let id = ControlSequenceId(self.names.len() as u32);
        let owned: Box<str> = name.into();
        self.ids.insert(owned.clone(), id);
        self.names.push(owned);
        id
    }

    pub fn resolve(&self, id: ControlSequenceId) -> Option<&str> {
        self.names.get(id.0 as usize).map(|name| name.as_ref())
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl Token {
    pub fn control_sequence(name: ControlSequenceId, start: usize, end: usize) -> Self {
        Self {
            kind: TokenKind::ControlSequence { name },
            span: SourceSpan {
                start: start as u32,
                end: end as u32,
            },
        }
    }

    pub fn character(ch: char, catcode: CatCode, start: usize, end: usize) -> Self {
        Self {
            kind: TokenKind::Character { ch, catcode },
            span: SourceSpan {
                start: start as u32,
                end: end as u32,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ControlSequenceInterner;

    #[test]
    fn interns_control_sequence_names_once() {
        let mut interner = ControlSequenceInterner::new();
        let alpha = interner.intern("alpha");
        let alpha_again = interner.intern("alpha");
        let beta = interner.intern("beta");

        assert_eq!(alpha, alpha_again);
        assert_ne!(alpha, beta);
        assert_eq!(interner.resolve(alpha), Some("alpha"));
        assert_eq!(interner.resolve(beta), Some("beta"));
    }
}
