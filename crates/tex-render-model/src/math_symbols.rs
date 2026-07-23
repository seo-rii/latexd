use crate::MathAtomKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MathSymbol {
    pub text: &'static str,
    pub atom_kind: MathAtomKind,
}

pub fn latex_math_symbol(command: &str) -> Option<MathSymbol> {
    let (text, atom_kind) = match command {
        "le" | "leq" | "leqslant" | "leqq" => ("≤", MathAtomKind::Relation),
        "ge" | "geq" | "geqslant" | "geqq" => ("≥", MathAtomKind::Relation),
        "ne" | "neq" => ("≠", MathAtomKind::Relation),
        _ => return None,
    };
    Some(MathSymbol { text, atom_kind })
}

#[cfg(test)]
mod tests {
    use crate::{MathAtomKind, latex_math_symbol};

    #[test]
    fn resolves_core_relation_aliases() {
        for (command, expected) in [("le", "≤"), ("leq", "≤"), ("ge", "≥"), ("neq", "≠")] {
            let symbol = latex_math_symbol(command).expect("known relation symbol");
            assert_eq!(symbol.text, expected);
            assert_eq!(symbol.atom_kind, MathAtomKind::Relation);
        }
    }
}
