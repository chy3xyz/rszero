//! Lexer for go-zero `.api` files.

use crate::error::RszeroResult;

/// Token kind enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// End of file.
    Eof,
    /// Newline.
    Newline,
    /// Identifier or keyword.
    Ident(String),
    /// String literal.
    String(String),
    /// Number literal.
    Number(String),
    /// Left brace `{`.
    LBrace,
    /// Right brace `}`.
    RBrace,
    /// Left paren `(`.
    LParen,
    /// Right paren `)`.
    RParen,
    /// Left bracket `[`.
    LBracket,
    /// Right bracket `]`.
    RBracket,
    /// Colon `:`.
    Colon,
    /// Semicolon `;`.
    Semi,
    /// Comma `,`.
    Comma,
    /// Equals `=`.
    Eq,
    /// Star `*`.
    Star,
    /// At `@`.
    At,
    /// Forward slash `/`.
    Slash,
    /// Backtick string `` `...` ``.
    Backtick(String),
    /// Unknown character.
    Unknown(char),
}

/// Token with position info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
    pub column: usize,
}

/// Lexer for `.api` files.
pub struct Lexer<'a> {
    #[allow(dead_code)]
    source: &'a str,
    chars: std::str::Chars<'a>,
    current: Option<char>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer.
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.chars();
        let current = chars.next();
        Self {
            source,
            chars,
            current,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire source.
    pub fn tokenize(&mut self) -> RszeroResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = matches!(tok.kind, TokenKind::Eof);
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let line = self.line;
        let column = self.column;

        match self.current {
            None => Token { kind: TokenKind::Eof, line, column },
            Some('\n') => {
                self.advance();
                Token { kind: TokenKind::Newline, line, column }
            }
            Some('{') => { self.advance(); Token { kind: TokenKind::LBrace, line, column } }
            Some('}') => { self.advance(); Token { kind: TokenKind::RBrace, line, column } }
            Some('(') => { self.advance(); Token { kind: TokenKind::LParen, line, column } }
            Some(')') => { self.advance(); Token { kind: TokenKind::RParen, line, column } }
            Some('[') => { self.advance(); Token { kind: TokenKind::LBracket, line, column } }
            Some(']') => { self.advance(); Token { kind: TokenKind::RBracket, line, column } }
            Some(':') => { self.advance(); Token { kind: TokenKind::Colon, line, column } }
            Some(';') => { self.advance(); Token { kind: TokenKind::Semi, line, column } }
            Some(',') => { self.advance(); Token { kind: TokenKind::Comma, line, column } }
            Some('=') => { self.advance(); Token { kind: TokenKind::Eq, line, column } }
            Some('*') => { self.advance(); Token { kind: TokenKind::Star, line, column } }
            Some('@') => { self.advance(); Token { kind: TokenKind::At, line, column } }
            Some('/') => {
                self.advance();
                if self.current == Some('/') {
                    self.skip_line_comment();
                    self.next_token()
                } else {
                    Token { kind: TokenKind::Slash, line, column }
                }
            }
            Some('"') => self.read_string(),
            Some('`') => self.read_backtick(),
            Some(c) if c.is_ascii_digit() => self.read_number(),
            Some(c) if c.is_alphabetic() || c == '_' => self.read_ident(),
            Some(c) => { self.advance(); Token { kind: TokenKind::Unknown(c), line, column } }
        }
    }

    fn advance(&mut self) {
        if let Some('\n') = self.current {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.current = self.chars.next();
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.current {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn read_string(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        self.advance(); // consume opening "
        let mut value = String::new();
        while let Some(c) = self.current {
            if c == '"' {
                self.advance();
                break;
            }
            value.push(c);
            self.advance();
        }
        Token { kind: TokenKind::String(value), line, column }
    }

    fn read_backtick(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        self.advance(); // consume opening `
        let mut value = String::new();
        while let Some(c) = self.current {
            if c == '`' {
                self.advance();
                break;
            }
            value.push(c);
            self.advance();
        }
        Token { kind: TokenKind::Backtick(value), line, column }
    }

    fn read_number(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        let mut value = String::new();
        while let Some(c) = self.current {
            if c.is_ascii_digit() {
                value.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Token { kind: TokenKind::Number(value), line, column }
    }

    fn read_ident(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        let mut value = String::new();
        while let Some(c) = self.current {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                value.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Token { kind: TokenKind::Ident(value), line, column }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new(r#"syntax = "v1""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("syntax".into()));
        assert_eq!(tokens[1].kind, TokenKind::Eq);
        assert_eq!(tokens[2].kind, TokenKind::String("v1".into()));
    }

    #[test]
    fn test_lexer_braces_and_comments() {
        let mut lexer = Lexer::new("{ } // comment\n@handler");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LBrace);
        assert_eq!(tokens[1].kind, TokenKind::RBrace);
        assert_eq!(tokens[2].kind, TokenKind::Newline);
        assert_eq!(tokens[3].kind, TokenKind::At);
        assert_eq!(tokens[4].kind, TokenKind::Ident("handler".into()));
    }

    #[test]
    fn test_lexer_backtick() {
        let mut lexer = Lexer::new("`json:\"id\"`");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Backtick("json:\"id\"".into()));
    }
}
