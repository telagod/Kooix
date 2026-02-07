use crate::error::{Diagnostic, Span};
use crate::token::{Token, TokenKind};

pub fn lex(input: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(input).lex()
}

struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();

        while !self.is_eof() {
            self.skip_ws_and_comments();
            if self.is_eof() {
                break;
            }

            let start = self.pos;
            let Some(current) = self.peek() else {
                break;
            };

            let token = if is_ident_start(current) {
                self.lex_ident_or_keyword(start)
            } else if current.is_ascii_digit() {
                self.lex_number(start)
            } else {
                match current {
                    b'(' => self.single_char_token(TokenKind::LParen, start),
                    b')' => self.single_char_token(TokenKind::RParen, start),
                    b'{' => self.single_char_token(TokenKind::LBrace, start),
                    b'}' => self.single_char_token(TokenKind::RBrace, start),
                    b'[' => self.single_char_token(TokenKind::LBracket, start),
                    b']' => self.single_char_token(TokenKind::RBracket, start),
                    b'<' => {
                        if self.peek_n(1) == Some(b'=') {
                            self.pos += 2;
                            Token::new(TokenKind::Lte, Span::new(start, self.pos))
                        } else {
                            self.single_char_token(TokenKind::LAngle, start)
                        }
                    }
                    b'>' => {
                        if self.peek_n(1) == Some(b'=') {
                            self.pos += 2;
                            Token::new(TokenKind::Gte, Span::new(start, self.pos))
                        } else {
                            self.single_char_token(TokenKind::RAngle, start)
                        }
                    }
                    b',' => self.single_char_token(TokenKind::Comma, start),
                    b'+' => self.single_char_token(TokenKind::Plus, start),
                    b'.' => self.single_char_token(TokenKind::Dot, start),
                    b':' => self.single_char_token(TokenKind::Colon, start),
                    b';' => self.single_char_token(TokenKind::Semicolon, start),
                    b'!' => {
                        if self.peek_n(1) == Some(b'=') {
                            self.pos += 2;
                            Token::new(TokenKind::NotEq, Span::new(start, self.pos))
                        } else {
                            self.single_char_token(TokenKind::Bang, start)
                        }
                    }
                    b'=' => {
                        if self.peek_n(1) == Some(b'=') {
                            self.pos += 2;
                            Token::new(TokenKind::EqEq, Span::new(start, self.pos))
                        } else {
                            self.single_char_token(TokenKind::Eq, start)
                        }
                    }
                    b'-' => {
                        if self.peek_n(1) == Some(b'>') {
                            self.pos += 2;
                            Token::new(TokenKind::Arrow, Span::new(start, self.pos))
                        } else {
                            return Err(Diagnostic::error(
                                "unexpected '-', expected '->'",
                                Span::new(start, start + 1),
                            ));
                        }
                    }
                    b'"' => self.lex_string(start)?,
                    _ => {
                        return Err(Diagnostic::error(
                            format!("unexpected character '{}'", current as char),
                            Span::new(start, start + 1),
                        ))
                    }
                }
            };
            tokens.push(token);
        }

        tokens.push(Token::new(TokenKind::Eof, Span::new(self.pos, self.pos)));

        Ok(tokens)
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while let Some(byte) = self.peek() {
                if byte.is_ascii_whitespace() {
                    self.pos += 1;
                } else {
                    break;
                }
            }

            if self.peek() == Some(b'/') && self.peek_n(1) == Some(b'/') {
                while let Some(byte) = self.peek() {
                    self.pos += 1;
                    if byte == b'\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }

    fn lex_ident_or_keyword(&mut self, start: usize) -> Token {
        while let Some(byte) = self.peek() {
            if is_ident_continue(byte) {
                self.pos += 1;
            } else {
                break;
            }
        }

        let value = &self.input[start..self.pos];
        let kind = match value {
            "cap" => TokenKind::KwCap,
            "fn" => TokenKind::KwFn,
            "workflow" => TokenKind::KwWorkflow,
            "agent" => TokenKind::KwAgent,
            "record" => TokenKind::KwRecord,
            "steps" => TokenKind::KwSteps,
            "on_fail" => TokenKind::KwOnFail,
            "output" => TokenKind::KwOutput,
            "state" => TokenKind::KwState,
            "policy" => TokenKind::KwPolicy,
            "loop" => TokenKind::KwLoop,
            "allow_tools" => TokenKind::KwAllowTools,
            "deny_tools" => TokenKind::KwDenyTools,
            "max_iterations" => TokenKind::KwMaxIterations,
            "human_in_loop" => TokenKind::KwHumanInLoop,
            "stop" => TokenKind::KwStop,
            "when" => TokenKind::KwWhen,
            "any" => TokenKind::KwAny,
            "intent" => TokenKind::KwIntent,
            "ensures" => TokenKind::KwEnsures,
            "failure" => TokenKind::KwFailure,
            "evidence" => TokenKind::KwEvidence,
            "trace" => TokenKind::KwTrace,
            "metrics" => TokenKind::KwMetrics,
            "in" => TokenKind::KwIn,
            "requires" => TokenKind::KwRequires,
            "where" => TokenKind::KwWhere,
            _ => TokenKind::Ident(value.to_string()),
        };
        Token::new(kind, Span::new(start, self.pos))
    }

    fn lex_number(&mut self, start: usize) -> Token {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let value = self.input[start..self.pos].to_string();
        Token::new(TokenKind::Number(value), Span::new(start, self.pos))
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
        self.pos += 1;
        let content_start = self.pos;
        let mut escaped = false;

        while let Some(byte) = self.peek() {
            self.pos += 1;
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == b'"' {
                let raw = &self.input[content_start..self.pos - 1];
                let value = raw
                    .replace("\\\"", "\"")
                    .replace("\\n", "\n")
                    .replace("\\t", "\t");
                return Ok(Token::new(
                    TokenKind::StringLiteral(value),
                    Span::new(start, self.pos),
                ));
            }
        }

        Err(Diagnostic::error(
            "unterminated string literal",
            Span::new(start, self.pos),
        ))
    }

    fn single_char_token(&mut self, kind: TokenKind, start: usize) -> Token {
        self.pos += 1;
        Token::new(kind, Span::new(start, self.pos))
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_n(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
