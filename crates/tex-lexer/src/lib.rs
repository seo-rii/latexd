use std::collections::HashMap;

use tex_tokens::{CatCode, ControlSequenceInterner, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatCodeTable {
    ascii: [CatCode; 128],
    overrides: HashMap<char, CatCode>,
}

#[derive(Debug)]
pub struct Lexer<'a, 'i> {
    input: &'a str,
    position: usize,
    state: ScannerState,
    catcodes: CatCodeTable,
    interner: &'i mut ControlSequenceInterner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScannerState {
    NewLine,
    MidLine,
    SkipBlanks,
}

#[derive(Debug, Clone, Copy)]
struct NormalizedChar {
    ch: char,
    start: usize,
    end: usize,
}

impl CatCodeTable {
    pub fn plain_tex() -> Self {
        let mut ascii = [CatCode::Other; 128];
        ascii['\\' as usize] = CatCode::Escape;
        ascii['{' as usize] = CatCode::BeginGroup;
        ascii['}' as usize] = CatCode::EndGroup;
        ascii['$' as usize] = CatCode::MathShift;
        ascii['&' as usize] = CatCode::AlignmentTab;
        ascii['\n' as usize] = CatCode::EndOfLine;
        ascii['\t' as usize] = CatCode::Space;
        ascii[' ' as usize] = CatCode::Space;
        ascii['#' as usize] = CatCode::Parameter;
        ascii['^' as usize] = CatCode::Superscript;
        ascii['_' as usize] = CatCode::Subscript;
        ascii['%' as usize] = CatCode::Comment;
        ascii[0x7f] = CatCode::Invalid;
        for byte in b'a'..=b'z' {
            ascii[byte as usize] = CatCode::Letter;
        }
        for byte in b'A'..=b'Z' {
            ascii[byte as usize] = CatCode::Letter;
        }

        Self {
            ascii,
            overrides: HashMap::new(),
        }
    }

    pub fn set(&mut self, ch: char, catcode: CatCode) {
        if ch.is_ascii() {
            self.ascii[ch as usize] = catcode;
            return;
        }
        self.overrides.insert(ch, catcode);
    }

    pub fn catcode(&self, ch: char) -> CatCode {
        if ch.is_ascii() {
            return self.ascii[ch as usize];
        }
        self.overrides.get(&ch).copied().unwrap_or(CatCode::Other)
    }
}

impl<'a, 'i> Lexer<'a, 'i> {
    pub fn new(
        input: &'a str,
        catcodes: CatCodeTable,
        interner: &'i mut ControlSequenceInterner,
    ) -> Self {
        Self {
            input,
            position: 0,
            state: ScannerState::NewLine,
            catcodes,
            interner,
        }
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(next) = self.peek_normalized(self.position) {
            match self.catcodes.catcode(next.ch) {
                CatCode::Escape => tokens.push(self.lex_escape(next)),
                CatCode::EndOfLine => {
                    self.position = next.end;
                    if let Some(token) = self.handle_endline(next.start, next.end) {
                        tokens.push(token);
                    }
                }
                CatCode::Space => {
                    self.position = next.end;
                    if let Some(token) = self.handle_space(next.start, next.end) {
                        tokens.push(token);
                    }
                }
                CatCode::Comment => {
                    self.position = next.end;
                    let mut newline = None;
                    while let Some(ch) = self.peek_normalized(self.position) {
                        self.position = ch.end;
                        if ch.ch == '\n' {
                            newline = Some((next.start, ch.end));
                            break;
                        }
                    }
                    if let Some((start, end)) = newline {
                        if let Some(token) = self.handle_endline(start, end) {
                            tokens.push(token);
                        }
                    }
                }
                CatCode::Ignored => {
                    self.position = next.end;
                }
                catcode => {
                    self.position = next.end;
                    tokens.push(Token::character(next.ch, catcode, next.start, next.end));
                    self.state = ScannerState::MidLine;
                }
            }
        }

        tokens
    }

    fn lex_escape(&mut self, escape: NormalizedChar) -> Token {
        self.position = escape.end;
        let Some(next) = self.peek_normalized(self.position) else {
            let id = self.interner.intern("");
            self.state = ScannerState::SkipBlanks;
            return Token::control_sequence(id, escape.start, escape.end);
        };

        match self.catcodes.catcode(next.ch) {
            CatCode::Letter => {
                let mut end = next.end;
                let mut name = String::new();
                while let Some(letter) = self.peek_normalized(self.position) {
                    if self.catcodes.catcode(letter.ch) != CatCode::Letter {
                        break;
                    }
                    self.position = letter.end;
                    end = letter.end;
                    name.push(letter.ch);
                }
                if name == "makeatletter" {
                    self.catcodes.set('@', CatCode::Letter);
                } else if name == "makeatother" {
                    self.catcodes.set('@', CatCode::Other);
                }
                let id = self.interner.intern(&name);
                self.state = ScannerState::SkipBlanks;
                Token::control_sequence(id, escape.start, end)
            }
            CatCode::EndOfLine => {
                self.position = next.end;
                let id = self.interner.intern("par");
                self.state = ScannerState::NewLine;
                Token::control_sequence(id, escape.start, next.end)
            }
            CatCode::Space => {
                self.position = next.end;
                let id = self.interner.intern(" ");
                self.state = ScannerState::SkipBlanks;
                Token::control_sequence(id, escape.start, next.end)
            }
            _ => {
                self.position = next.end;
                let mut name = String::new();
                name.push(next.ch);
                let id = self.interner.intern(&name);
                self.state = ScannerState::MidLine;
                Token::control_sequence(id, escape.start, next.end)
            }
        }
    }

    fn handle_endline(&mut self, start: usize, end: usize) -> Option<Token> {
        match self.state {
            ScannerState::NewLine => {
                let par = self.interner.intern("par");
                self.state = ScannerState::NewLine;
                Some(Token::control_sequence(par, start, end))
            }
            ScannerState::MidLine => {
                self.state = ScannerState::NewLine;
                Some(Token::character(' ', CatCode::Space, start, end))
            }
            ScannerState::SkipBlanks => {
                self.state = ScannerState::NewLine;
                None
            }
        }
    }

    fn handle_space(&mut self, start: usize, end: usize) -> Option<Token> {
        match self.state {
            ScannerState::MidLine => {
                self.state = ScannerState::SkipBlanks;
                Some(Token::character(' ', CatCode::Space, start, end))
            }
            ScannerState::NewLine | ScannerState::SkipBlanks => None,
        }
    }

    fn peek_normalized(&self, position: usize) -> Option<NormalizedChar> {
        let rest = self.input.get(position..)?;
        if rest.is_empty() {
            return None;
        }

        let mut chars = rest.chars();
        let ch = chars.next()?;
        if ch == '\r' {
            let end = if rest.as_bytes().get(1) == Some(&b'\n') {
                position + 2
            } else {
                position + 1
            };
            return Some(NormalizedChar {
                ch: '\n',
                start: position,
                end,
            });
        }

        Some(NormalizedChar {
            ch,
            start: position,
            end: position + ch.len_utf8(),
        })
    }
}

pub fn lex_plain(input: &str, interner: &mut ControlSequenceInterner) -> Vec<Token> {
    Lexer::new(input, CatCodeTable::plain_tex(), interner).tokenize()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use tex_tokens::{CatCode, ControlSequenceInterner, Token, TokenKind};

    use super::{CatCodeTable, Lexer, lex_plain};

    fn render(tokens: &[Token], interner: &ControlSequenceInterner) -> Vec<String> {
        tokens
            .iter()
            .map(|token| match &token.kind {
                TokenKind::ControlSequence { name } => {
                    format!("cs:{}", interner.resolve(*name).unwrap_or("<missing>"))
                }
                TokenKind::Character { ch, catcode } => format!("char:{catcode:?}:{ch}"),
            })
            .collect()
    }

    #[test]
    fn scans_control_words_and_symbols() {
        let mut interner = ControlSequenceInterner::new();
        let tokens = lex_plain(r"\alpha+\$x", &mut interner);

        assert_eq!(
            render(&tokens, &interner),
            vec!["cs:alpha", "char:Other:+", "cs:$", "char:Letter:x",]
        );
    }

    #[test]
    fn collapses_space_and_comment_newlines() {
        let mut interner = ControlSequenceInterner::new();
        let tokens = lex_plain("a   % note\r\nb", &mut interner);

        assert_eq!(
            render(&tokens, &interner),
            vec!["char:Letter:a", "char:Space: ", "char:Letter:b"]
        );
    }

    #[test]
    fn emits_par_for_blank_lines() {
        let mut interner = ControlSequenceInterner::new();
        let tokens = lex_plain("\n\nx", &mut interner);

        assert_eq!(
            render(&tokens, &interner),
            vec!["cs:par", "cs:par", "char:Letter:x"]
        );
    }

    #[test]
    fn custom_letter_catcode_extends_control_word() {
        let mut interner = ControlSequenceInterner::new();
        let mut catcodes = CatCodeTable::plain_tex();
        catcodes.set('@', CatCode::Letter);

        let tokens = Lexer::new(r"\foo@bar baz", catcodes, &mut interner).tokenize();
        assert_eq!(
            render(&tokens, &interner),
            vec![
                "cs:foo@bar",
                "char:Letter:b",
                "char:Letter:a",
                "char:Letter:z"
            ]
        );
    }

    #[test]
    fn makeatletter_and_makeatother_toggle_at_control_words() {
        let mut interner = ControlSequenceInterner::new();
        let tokens = lex_plain(
            r"\makeatletter\@ifpackageloaded\makeatother\@ifpackageloaded",
            &mut interner,
        );

        assert_eq!(
            render(&tokens, &interner),
            vec![
                "cs:makeatletter",
                "cs:@ifpackageloaded",
                "cs:makeatother",
                "cs:@",
                "char:Letter:i",
                "char:Letter:f",
                "char:Letter:p",
                "char:Letter:a",
                "char:Letter:c",
                "char:Letter:k",
                "char:Letter:a",
                "char:Letter:g",
                "char:Letter:e",
                "char:Letter:l",
                "char:Letter:o",
                "char:Letter:a",
                "char:Letter:d",
                "char:Letter:e",
                "char:Letter:d",
            ]
        );
    }

    #[test]
    fn tracks_utf8_byte_spans() {
        let mut interner = ControlSequenceInterner::new();
        let tokens = lex_plain("é\\alpha z", &mut interner);

        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 2);
        assert_eq!(tokens[1].span.start, 2);
        assert_eq!(tokens[1].span.end, 8);
        assert_eq!(tokens[2].span.start, 9);
        assert_eq!(tokens[2].span.end, 10);
    }

    proptest! {
        #[test]
        fn normalizes_crlf_like_lf(random in "[ -~]{0,64}") {
            let source_lf = random.replace('\r', "R").replace('\n', "N");
            let source_crlf = source_lf.replace('\n', "\r\n");

            let mut interner_lf = ControlSequenceInterner::new();
            let mut interner_crlf = ControlSequenceInterner::new();
            let lf = lex_plain(&source_lf, &mut interner_lf);
            let crlf = lex_plain(&source_crlf, &mut interner_crlf);

            prop_assert_eq!(render(&lf, &interner_lf), render(&crlf, &interner_crlf));
        }

        #[test]
        fn random_ascii_produces_valid_spans(random in "[\\x00-\\x7f]{0,128}") {
            let mut interner = ControlSequenceInterner::new();
            let tokens = lex_plain(&random, &mut interner);

            for token in tokens {
                prop_assert!(token.span.start <= token.span.end);
                prop_assert!((token.span.end as usize) <= random.len());
            }
        }
    }
}
