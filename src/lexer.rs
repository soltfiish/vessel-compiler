//! Vessel Lexer.
//! Linear pass: O(n) in source length.
//! Produces a flat Vec<Token> consumed by the Parser.

use crate::error::{Phase, VesselError, VesselResult};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Float(f64),
    Ident(String),
    // Keywords
    Vessel, Boundary, Couple, To, Resign, Rebalance,
    Sentient, Law, Let, Fn, Return, If, Else, Forall,
    True, False, And, Or, Not,
    // Symbols
    LParen, RParen, LBrace, RBrace,
    Colon, Comma, Equals, Arrow, Dot,
    Lt, Gt, LtEq, GtEq, Plus, Minus, Star, Slash,
    // Meta
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col:  usize,
}

pub struct Lexer {
    source: Vec<char>,
    pos:    usize,
    line:   usize,
    col:    usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self { source: source.chars().collect(), pos: 0, line: 1, col: 1 }
    }

    fn peek(&self, offset: usize) -> char {
        self.source.get(self.pos + offset).copied().unwrap_or('\0')
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        if ch == '\n' { self.line += 1; self.col = 1; }
        else          { self.col  += 1; }
        ch
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.source.len() {
            match self.peek(0) {
                ' ' | '\t' | '\r' | '\n' => { self.advance(); }
                '-' if self.peek(1) == '-' => {
                    while self.pos < self.source.len() && self.peek(0) != '\n' {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn read_number(&mut self, sl: usize, sc: usize) -> VesselResult<Token> {
        let mut s = String::new();
        // Optional leading minus already consumed by caller
        while self.pos < self.source.len()
              && (self.peek(0).is_ascii_digit() || self.peek(0) == '.') {
            s.push(self.advance());
        }
        let value: f64 = s.parse().map_err(|_| VesselError::secondary_at(
            Phase::Lexer, sl, sc, format!("Invalid float literal: {s}")
        ))?;
        Ok(Token { kind: TokenKind::Float(value), line: sl, col: sc })
    }

    fn read_word(&mut self, sl: usize, sc: usize) -> Token {
        let mut s = String::new();
        while self.pos < self.source.len()
              && (self.peek(0).is_alphanumeric() || self.peek(0) == '_') {
            s.push(self.advance());
        }
        let kind = match s.as_str() {
            "vessel"    => TokenKind::Vessel,
            "boundary"  => TokenKind::Boundary,
            "couple"    => TokenKind::Couple,
            "to"        => TokenKind::To,
            "resign"    => TokenKind::Resign,
            "rebalance" => TokenKind::Rebalance,
            "sentient"  => TokenKind::Sentient,
            "law"       => TokenKind::Law,
            "let"       => TokenKind::Let,
            "fn"        => TokenKind::Fn,
            "return"    => TokenKind::Return,
            "if"        => TokenKind::If,
            "else"      => TokenKind::Else,
            "forall"    => TokenKind::Forall,
            "true"      => TokenKind::True,
            "false"     => TokenKind::False,
            "and"       => TokenKind::And,
            "or"        => TokenKind::Or,
            "not"       => TokenKind::Not,
            _           => TokenKind::Ident(s),
        };
        Token { kind, line: sl, col: sc }
    }

    pub fn tokenize(&mut self) -> VesselResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.source.len() { break; }
            let sl = self.line; let sc = self.col;
            let ch = self.peek(0);

            let tok = match ch {
                '(' => { self.advance(); Token { kind: TokenKind::LParen,  line: sl, col: sc } }
                ')' => { self.advance(); Token { kind: TokenKind::RParen,  line: sl, col: sc } }
                '{' => { self.advance(); Token { kind: TokenKind::LBrace,  line: sl, col: sc } }
                '}' => { self.advance(); Token { kind: TokenKind::RBrace,  line: sl, col: sc } }
                ':' => { self.advance(); Token { kind: TokenKind::Colon,   line: sl, col: sc } }
                ',' => { self.advance(); Token { kind: TokenKind::Comma,   line: sl, col: sc } }
                '.' => { self.advance(); Token { kind: TokenKind::Dot,     line: sl, col: sc } }
                '+'  => { self.advance(); Token { kind: TokenKind::Plus,   line: sl, col: sc } }
                '*'  => { self.advance(); Token { kind: TokenKind::Star,   line: sl, col: sc } }
                '/'  => { self.advance(); Token { kind: TokenKind::Slash,  line: sl, col: sc } }
                '=' => { self.advance(); Token { kind: TokenKind::Equals,  line: sl, col: sc } }
                '-' if self.peek(1) == '>' => {
                    self.advance(); self.advance();
                    Token { kind: TokenKind::Arrow, line: sl, col: sc }
                }
                '-' if self.peek(1).is_ascii_digit() => {
                    self.advance(); // consume '-'
                    let mut tok = self.read_number(sl, sc)?;
                    if let TokenKind::Float(ref mut v) = tok.kind { *v = -*v; }
                    tok
                }
                '<' if self.peek(1) == '=' => {
                    self.advance(); self.advance();
                    Token { kind: TokenKind::LtEq, line: sl, col: sc }
                }
                '>' if self.peek(1) == '=' => {
                    self.advance(); self.advance();
                    Token { kind: TokenKind::GtEq, line: sl, col: sc }
                }
                '<' => { self.advance(); Token { kind: TokenKind::Lt, line: sl, col: sc } }
                '>' => { self.advance(); Token { kind: TokenKind::Gt, line: sl, col: sc } }
                c if c.is_ascii_digit() => self.read_number(sl, sc)?,
                c if c.is_alphabetic() || c == '_' => self.read_word(sl, sc),
                c => return Err(VesselError::secondary_at(
                    Phase::Lexer, sl, sc, format!("Unexpected character: {c:?}")
                )),
            };
            tokens.push(tok);
        }
        tokens.push(Token { kind: TokenKind::Eof, line: self.line, col: self.col });
        Ok(tokens)
    }
}
