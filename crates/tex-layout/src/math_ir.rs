use tex_render_model::{
    MathAtomKind, MathLargeOperator, MathNode, MathScriptPlacement, latex_math_symbol,
};

pub(crate) fn parse_display_math_structure(source: &str) -> Option<MathNode> {
    let mut parser = MathParser { source, index: 0 };
    let structure = parser.parse_row(None)?;
    parser.skip_whitespace();
    (parser.index == source.len()).then_some(structure)
}

struct MathParser<'a> {
    source: &'a str,
    index: usize,
}

impl MathParser<'_> {
    fn parse_row(&mut self, closing: Option<char>) -> Option<MathNode> {
        let mut children = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek_char() {
                Some(ch) if Some(ch) == closing => {
                    self.next_char();
                    break;
                }
                Some(_) => {
                    let atom = self.parse_atom()?;
                    children.push(self.parse_postfix(atom)?);
                }
                None if closing.is_some() => return None,
                None => break,
            }
        }
        Some(collapse_row(children))
    }

    fn parse_atom(&mut self) -> Option<MathNode> {
        self.skip_whitespace();
        let ch = self.peek_char()?;
        if ch == '{' {
            self.next_char();
            return self.parse_row(Some('}'));
        }
        if ch == '\\' {
            let command = self.read_command()?;
            if let Some(symbol) = latex_math_symbol(&command) {
                return Some(MathNode::Atom {
                    text: symbol.text.to_string(),
                    atom_kind: symbol.atom_kind,
                });
            }
            return match command.as_str() {
                "frac" | "dfrac" | "tfrac" => Some(MathNode::Fraction {
                    numerator: Box::new(self.parse_required_group()?),
                    denominator: Box::new(self.parse_required_group()?),
                }),
                "sum" => Some(MathNode::LargeOperator {
                    operator: MathLargeOperator::Sum,
                }),
                "prod" => Some(MathNode::LargeOperator {
                    operator: MathLargeOperator::Product,
                }),
                "int" => Some(MathNode::LargeOperator {
                    operator: MathLargeOperator::Integral,
                }),
                "mathrm" | "mathit" | "mathbf" | "mathsf" | "mathtt" | "text" | "operatorname" => {
                    self.parse_required_group()
                }
                "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                    self.parse_atom()
                }
                "alpha" | "beta" | "gamma" | "delta" | "epsilon" | "theta" | "lambda" | "mu"
                | "pi" | "rho" | "sigma" | "phi" | "psi" | "omega" => Some(MathNode::Atom {
                    text: command,
                    atom_kind: MathAtomKind::Identifier,
                }),
                "cdot" => Some(MathNode::Atom {
                    text: "*".to_string(),
                    atom_kind: MathAtomKind::Operator,
                }),
                "times" => Some(MathNode::Atom {
                    text: "x".to_string(),
                    atom_kind: MathAtomKind::Operator,
                }),
                _ => None,
            };
        }

        let atom_kind = if ch.is_ascii_alphabetic() {
            MathAtomKind::Identifier
        } else if ch.is_ascii_digit() {
            MathAtomKind::Number
        } else if matches!(ch, '+' | '-' | '*' | '/') {
            MathAtomKind::Operator
        } else if matches!(ch, '=' | '<' | '>') {
            MathAtomKind::Relation
        } else if matches!(ch, '(' | ')' | '[' | ']' | '|') {
            MathAtomKind::Delimiter
        } else if matches!(ch, ',' | '.' | ';' | ':') {
            MathAtomKind::Punctuation
        } else {
            return None;
        };
        self.next_char();
        let mut text = ch.to_string();
        if atom_kind == MathAtomKind::Number {
            while let Some(next) = self.peek_char()
                && next.is_ascii_digit()
            {
                self.next_char();
                text.push(next);
            }
        }
        Some(MathNode::Atom { text, atom_kind })
    }

    fn parse_postfix(&mut self, base: MathNode) -> Option<MathNode> {
        let mut subscript = None;
        let mut superscript = None;
        let mut placement = None;
        loop {
            self.skip_whitespace();
            match self.peek_char() {
                Some('_') => {
                    if subscript.is_some() {
                        return None;
                    }
                    self.next_char();
                    subscript = Some(Box::new(self.parse_script_argument()?));
                }
                Some('^') => {
                    if superscript.is_some() {
                        return None;
                    }
                    self.next_char();
                    superscript = Some(Box::new(self.parse_script_argument()?));
                }
                Some('\\') => {
                    let checkpoint = self.index;
                    let command = self.read_command()?;
                    match command.as_str() {
                        "limits" | "displaylimits" => placement = Some(MathScriptPlacement::Limits),
                        "nolimits" => placement = Some(MathScriptPlacement::Side),
                        _ => {
                            self.index = checkpoint;
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        if subscript.is_none() && superscript.is_none() {
            return Some(base);
        }
        let placement = placement.unwrap_or_else(|| match &base {
            MathNode::LargeOperator {
                operator: MathLargeOperator::Sum | MathLargeOperator::Product,
            } => MathScriptPlacement::Limits,
            _ => MathScriptPlacement::Side,
        });
        Some(MathNode::Scripts {
            base: Box::new(base),
            subscript,
            superscript,
            placement,
        })
    }

    fn parse_script_argument(&mut self) -> Option<MathNode> {
        self.skip_whitespace();
        if self.peek_char() == Some('{') {
            self.next_char();
            self.parse_row(Some('}'))
        } else {
            self.parse_atom()
        }
    }

    fn parse_required_group(&mut self) -> Option<MathNode> {
        self.skip_whitespace();
        (self.next_char()? == '{').then_some(())?;
        self.parse_row(Some('}'))
    }

    fn read_command(&mut self) -> Option<String> {
        (self.next_char()? == '\\').then_some(())?;
        let first = self.next_char()?;
        if !first.is_ascii_alphabetic() {
            return Some(first.to_string());
        }
        let mut command = first.to_string();
        while let Some(ch) = self.peek_char()
            && ch.is_ascii_alphabetic()
        {
            self.next_char();
            command.push(ch);
        }
        Some(command)
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.next_char();
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.index += ch.len_utf8();
        Some(ch)
    }
}

fn collapse_row(mut children: Vec<MathNode>) -> MathNode {
    if children.len() == 1 {
        children.pop().expect("single math child")
    } else {
        MathNode::Row { children }
    }
}

#[cfg(test)]
mod tests {
    use tex_render_model::{MathAtomKind, MathLargeOperator, MathNode, MathScriptPlacement};

    use super::parse_display_math_structure;

    #[test]
    fn parses_large_operator_scripts_and_fraction() {
        let parsed = parse_display_math_structure(r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}")
            .expect("supported display math");

        assert_eq!(
            parsed,
            MathNode::Row {
                children: vec![
                    MathNode::Scripts {
                        base: Box::new(MathNode::LargeOperator {
                            operator: MathLargeOperator::Sum,
                        }),
                        subscript: Some(Box::new(MathNode::Row {
                            children: vec![
                                MathNode::Atom {
                                    text: "i".to_string(),
                                    atom_kind: MathAtomKind::Identifier,
                                },
                                MathNode::Atom {
                                    text: "=".to_string(),
                                    atom_kind: MathAtomKind::Relation,
                                },
                                MathNode::Atom {
                                    text: "1".to_string(),
                                    atom_kind: MathAtomKind::Number,
                                },
                            ],
                        })),
                        superscript: Some(Box::new(MathNode::Atom {
                            text: "n".to_string(),
                            atom_kind: MathAtomKind::Identifier,
                        })),
                        placement: MathScriptPlacement::Limits,
                    },
                    MathNode::Atom {
                        text: "i".to_string(),
                        atom_kind: MathAtomKind::Identifier,
                    },
                    MathNode::Atom {
                        text: "=".to_string(),
                        atom_kind: MathAtomKind::Relation,
                    },
                    MathNode::Fraction {
                        numerator: Box::new(MathNode::Row {
                            children: vec![
                                MathNode::Atom {
                                    text: "n".to_string(),
                                    atom_kind: MathAtomKind::Identifier,
                                },
                                MathNode::Atom {
                                    text: "(".to_string(),
                                    atom_kind: MathAtomKind::Delimiter,
                                },
                                MathNode::Atom {
                                    text: "n".to_string(),
                                    atom_kind: MathAtomKind::Identifier,
                                },
                                MathNode::Atom {
                                    text: "+".to_string(),
                                    atom_kind: MathAtomKind::Operator,
                                },
                                MathNode::Atom {
                                    text: "1".to_string(),
                                    atom_kind: MathAtomKind::Number,
                                },
                                MathNode::Atom {
                                    text: ")".to_string(),
                                    atom_kind: MathAtomKind::Delimiter,
                                },
                            ],
                        }),
                        denominator: Box::new(MathNode::Atom {
                            text: "2".to_string(),
                            atom_kind: MathAtomKind::Number,
                        }),
                    },
                ],
            }
        );
    }

    #[test]
    fn rejects_unsupported_commands_without_partial_structure() {
        assert!(parse_display_math_structure(r"x + \unknown{y}").is_none());
    }

    #[test]
    fn parses_named_relation_symbols_as_relation_atoms() {
        let parsed = parse_display_math_structure(r"a \le b \ge c \neq d")
            .expect("standard relation commands should be structured");

        assert_eq!(
            parsed,
            MathNode::Row {
                children: vec![
                    MathNode::Atom {
                        text: "a".to_string(),
                        atom_kind: MathAtomKind::Identifier,
                    },
                    MathNode::Atom {
                        text: "≤".to_string(),
                        atom_kind: MathAtomKind::Relation,
                    },
                    MathNode::Atom {
                        text: "b".to_string(),
                        atom_kind: MathAtomKind::Identifier,
                    },
                    MathNode::Atom {
                        text: "≥".to_string(),
                        atom_kind: MathAtomKind::Relation,
                    },
                    MathNode::Atom {
                        text: "c".to_string(),
                        atom_kind: MathAtomKind::Identifier,
                    },
                    MathNode::Atom {
                        text: "≠".to_string(),
                        atom_kind: MathAtomKind::Relation,
                    },
                    MathNode::Atom {
                        text: "d".to_string(),
                        atom_kind: MathAtomKind::Identifier,
                    },
                ],
            }
        );
    }
}
