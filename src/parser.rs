//! Vessel recursive-descent parser.
//! Consumes a flat Vec<Token> (from Lexer) and produces a Program AST.
//! Precedence (low to high):
//!   or  ->  and  ->  comparison  ->  add/sub  ->  mul/div  ->  unary  ->  atom

use crate::ast::*;
use crate::error::{Phase, VesselError, VesselResult};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos:    usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ------------------------------------------------------------------ peek/advance

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, kind: TokenKind) -> VesselResult<&Token> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&kind) {
            Ok(self.advance())
        } else {
            Err(VesselError::primary_at(
                Phase::Parser,
                self.peek().line,
                self.peek().col,
                format!("Expected {:?}, got {:?}", kind, self.peek_kind()),
            ))
        }
    }

    fn eat_ident(&mut self) -> VesselResult<String> {
        let t = self.advance();
        match &t.kind {
            TokenKind::Ident(s) => Ok(s.clone()),
            other => Err(VesselError::primary_at(
                Phase::Parser, t.line, t.col,
                format!("Expected identifier, got {:?}", other),
            )),
        }
    }

    fn eat_float(&mut self) -> VesselResult<f64> {
        let t = self.advance();
        match &t.kind {
            TokenKind::Float(v) => Ok(*v),
            other => Err(VesselError::primary_at(
                Phase::Parser, t.line, t.col,
                format!("Expected float literal, got {:?}", other),
            )),
        }
    }

    /// Read any token that can serve as a field/property name inside a vessel/couple block.
    /// Accepts identifiers AND keywords that appear as field names in the language spec.
    fn eat_field_name(&mut self) -> VesselResult<String> {
        let t = self.advance();
        match &t.kind {
            TokenKind::Ident(s)   => Ok(s.clone()),
            // Keywords allowed as field names
            TokenKind::Boundary   => Ok("boundary".into()),
            TokenKind::Couple     => Ok("couple".into()),
            TokenKind::Vessel     => Ok("vessel".into()),
            TokenKind::Sentient   => Ok("sentient".into()),
            TokenKind::Observer   => Ok("observer".into()),
            TokenKind::Resign     => Ok("resign".into()),
            TokenKind::Rebalance  => Ok("rebalance".into()),
            TokenKind::Law        => Ok("law".into()),
            other => Err(VesselError::primary_at(
                Phase::Parser, t.line, t.col,
                format!("Expected field name, got {:?}", other),
            )),
        }
    }

    // ------------------------------------------------------------------ top level

    pub fn parse(&mut self) -> VesselResult<Program> {
        let mut vessels = Vec::new();
        let mut couples  = Vec::new();
        let mut laws     = Vec::new();
        let mut fns      = Vec::new();

        while !self.check(&TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Vessel   => vessels.push(self.parse_vessel()?),
                TokenKind::Couple   => couples.push(self.parse_couple()?),
                TokenKind::Law      => laws.push(self.parse_law()?),
                TokenKind::Fn       => fns.push(self.parse_fn()?),
                _ => {
                    let t = self.advance();
                    return Err(VesselError::primary_at(
                        Phase::Parser, t.line, t.col,
                        format!("Unexpected token at top level: {:?}", t.kind),
                    ));
                }
            }
        }
        Ok(Program { vessels, couples, laws, fns })
    }

    // ------------------------------------------------------------------ vessel

    fn parse_vessel(&mut self) -> VesselResult<VesselDecl> {
        let line = self.peek().line;
        self.eat(TokenKind::Vessel)?;
        let name = self.eat_ident()?;
        self.eat(TokenKind::LBrace)?;

        let mut kappa:    Option<Expr>  = None;
        let mut boundary: Option<Expr>  = None;
        let mut sentient: bool          = false;
        let mut observer: bool          = false;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field = self.eat_field_name()?;
            self.eat(TokenKind::Colon)?;
            match field.as_str() {
                "kappa"    => kappa    = Some(self.parse_expr()?),
                "boundary" => boundary = Some(self.parse_expr()?),
                "sentient" => {
                    // accept `true` / `false` keyword
                    let v = self.advance().kind.clone();
                    sentient = matches!(v, TokenKind::True);
                }
                "observer" => {
                    // accept `true` / `false` keyword
                    let v = self.advance().kind.clone();
                    observer = matches!(v, TokenKind::True);
                }
                _ => { self.parse_expr()?; /* ignore unknown fields */ }
            }
            // optional comma
            if self.check(&TokenKind::Comma) { self.advance(); }
        }
        self.eat(TokenKind::RBrace)?;

        Ok(VesselDecl {
            name,
            kappa:    kappa.unwrap_or(Expr::FloatLit(0.5)),
            boundary: boundary.unwrap_or(Expr::FloatLit(0.5)),
            sentient,
            observer,
            line,
        })
    }

    // ------------------------------------------------------------------ couple

    fn parse_couple(&mut self) -> VesselResult<CoupleDecl> {
        let line = self.peek().line;
        self.eat(TokenKind::Couple)?;
        let src = self.eat_ident()?;
        self.eat(TokenKind::To)?;
        let dst = self.eat_ident()?;
        self.eat(TokenKind::LBrace)?;

        let mut coupling = Expr::FloatLit(0.5);
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field = self.eat_field_name()?;
            self.eat(TokenKind::Colon)?;
            let val = self.parse_expr()?;
            if field == "coupling" { coupling = val; }
            if self.check(&TokenKind::Comma) { self.advance(); }
        }
        self.eat(TokenKind::RBrace)?;
        Ok(CoupleDecl { src, dst, coupling, line })
    }

    // ------------------------------------------------------------------ law

    fn parse_law(&mut self) -> VesselResult<LawDecl> {
        let line = self.peek().line;
        self.eat(TokenKind::Law)?;
        let name = self.eat_ident()?;
        self.eat(TokenKind::LBrace)?;
        self.eat(TokenKind::Forall)?;
        let param = self.eat_ident()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_expr()?;
        if self.check(&TokenKind::Comma) { self.advance(); }
        self.eat(TokenKind::RBrace)?;
        Ok(LawDecl { name, param, body, line })
    }

    // ------------------------------------------------------------------ fn

    fn parse_fn(&mut self) -> VesselResult<FnDecl> {
        let line = self.peek().line;
        self.eat(TokenKind::Fn)?;
        let name = self.eat_ident()?;
        self.eat(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let pname = self.eat_ident()?;
            // optional kappa annotation: p: [lo, hi]
            let hint = if self.check(&TokenKind::Colon) {
                self.advance();
                if self.check(&TokenKind::Lt) {
                    Some(self.parse_kappa_annotation()?)
                } else {
                    None
                }
            } else { None };
            params.push((pname, hint));
            if self.check(&TokenKind::Comma) { self.advance(); }
        }
        self.eat(TokenKind::RParen)?;

        let ret_hint = if self.check(&TokenKind::Arrow) {
            self.advance();
            if self.check(&TokenKind::Lt) {
                Some(self.parse_kappa_annotation()?)
            } else { None }
        } else { None };

        self.eat(TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_stmt()?);
        }
        self.eat(TokenKind::RBrace)?;

        Ok(FnDecl { name, params, ret_hint, body, line })
    }

    /// Parse `<lo, hi>` kappa annotation.
    fn parse_kappa_annotation(&mut self) -> VesselResult<KappaRange> {
        self.eat(TokenKind::Lt)?;
        let lo = self.eat_float()?;
        self.eat(TokenKind::Comma)?;
        let hi = self.eat_float()?;
        self.eat(TokenKind::Gt)?;
        Ok(KappaRange::new(lo, hi))
    }

    // ------------------------------------------------------------------ stmt

    fn parse_stmt(&mut self) -> VesselResult<Stmt> {
        let line = self.peek().line;
        match self.peek_kind().clone() {
            TokenKind::Let => {
                self.advance();
                let name = self.eat_ident()?;
                self.eat(TokenKind::Equals)?;
                let value = self.parse_expr()?;
                Ok(Stmt::Let { name, value, line })
            }
            TokenKind::Return => {
                self.advance();
                let value = self.parse_expr()?;
                Ok(Stmt::Return { value, line })
            }
            _ => {
                let value = self.parse_expr()?;
                Ok(Stmt::Expr { value, line })
            }
        }
    }

    // ------------------------------------------------------------------ expr (Pratt-style precedence)

    pub fn parse_expr(&mut self) -> VesselResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> VesselResult<Expr> {
        let mut left = self.parse_and()?;
        while self.check(&TokenKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp { op: BinOpKind::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> VesselResult<Expr> {
        let mut left = self.parse_comparison()?;
        while self.check(&TokenKind::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp { op: BinOpKind::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> VesselResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt   => BinOpKind::Lt,
                TokenKind::Gt   => BinOpKind::Gt,
                TokenKind::LtEq => BinOpKind::LtEq,
                TokenKind::GtEq => BinOpKind::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> VesselResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus  => BinOpKind::Add,
                TokenKind::Minus => BinOpKind::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> VesselResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star  => BinOpKind::Mul,
                TokenKind::Slash => BinOpKind::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> VesselResult<Expr> {
        match self.peek_kind().clone() {
            TokenKind::Not => {
                self.advance();
                Ok(Expr::UnaryOp { op: UnaryOpKind::Not, expr: Box::new(self.parse_unary()?) })
            }
            TokenKind::Minus => {
                self.advance();
                Ok(Expr::UnaryOp { op: UnaryOpKind::Neg, expr: Box::new(self.parse_unary()?) })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> VesselResult<Expr> {
        let mut base = self.parse_atom()?;
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.eat_ident()?;
                base = Expr::Member { object: Box::new(base), field };
            } else {
                break;
            }
        }
        Ok(base)
    }

    fn parse_atom(&mut self) -> VesselResult<Expr> {
        match self.peek_kind().clone() {
            TokenKind::Float(v) => { self.advance(); Ok(Expr::FloatLit(v)) }
            TokenKind::True     => { self.advance(); Ok(Expr::BoolLit(true)) }
            TokenKind::False    => { self.advance(); Ok(Expr::BoolLit(false)) }
            TokenKind::Ident(name) => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                        args.push(self.parse_expr()?);
                        if self.check(&TokenKind::Comma) { self.advance(); }
                    }
                    self.eat(TokenKind::RParen)?;
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            TokenKind::Forall => {
                self.advance();
                let param = self.eat_ident()?;
                self.eat(TokenKind::Colon)?;
                let body = self.parse_expr()?;
                Ok(Expr::Forall { param, body: Box::new(body) })
            }
            TokenKind::If => {
                self.advance();
                let cond = self.parse_expr()?;
                self.eat(TokenKind::LBrace)?;
                let then = self.parse_expr()?;
                self.eat(TokenKind::RBrace)?;
                self.eat(TokenKind::Else)?;
                self.eat(TokenKind::LBrace)?;
                let else_ = self.parse_expr()?;
                self.eat(TokenKind::RBrace)?;
                Ok(Expr::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_) })
            }
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.eat(TokenKind::RParen)?;
                Ok(e)
            }
            other => {
                let t = self.peek();
                Err(VesselError::primary_at(
                    Phase::Parser, t.line, t.col,
                    format!("Unexpected token in expression: {:?}", other),
                ))
            }
        }
    }
}
