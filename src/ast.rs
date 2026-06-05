//! Vessel AST node types.
//! Produced by the Parser; consumed by KappaInfer and CodeGen.

/// A Kappa range: [lo, hi].  Used throughout inference.
#[derive(Debug, Clone, PartialEq)]
pub struct KappaRange {
    pub lo: f64,
    pub hi: f64,
}

impl KappaRange {
    pub fn new(lo: f64, hi: f64) -> Self { Self { lo, hi } }
    pub fn point(v: f64) -> Self { Self { lo: v, hi: v } }
    pub fn full() -> Self { Self { lo: 0.0, hi: 1.0 } }
    pub fn width(&self) -> f64 { self.hi - self.lo }
    pub fn contains(&self, v: f64) -> bool { v >= self.lo && v <= self.hi }
    pub fn intersects(&self, other: &KappaRange) -> bool {
        self.lo <= other.hi && other.lo <= self.hi
    }
    pub fn intersect(&self, other: &KappaRange) -> Option<KappaRange> {
        let lo = self.lo.max(other.lo);
        let hi = self.hi.min(other.hi);
        if lo <= hi { Some(KappaRange::new(lo, hi)) } else { None }
    }
}

/// Top-level compilation unit.
#[derive(Debug, Clone)]
pub struct Program {
    pub vessels: Vec<VesselDecl>,
    pub couples: Vec<CoupleDecl>,
    pub laws:    Vec<LawDecl>,
    pub fns:     Vec<FnDecl>,
}

/// `vessel <name> { kappa: <expr>, boundary: <expr>, sentient: <bool> }`
#[derive(Debug, Clone)]
pub struct VesselDecl {
    pub name:     String,
    pub kappa:    Expr,
    pub boundary: Expr,
    pub sentient: bool,
    pub line:     usize,
}

/// `couple <a> to <b> { coupling: <expr> }`
#[derive(Debug, Clone)]
pub struct CoupleDecl {
    pub src:      String,
    pub dst:      String,
    pub coupling: Expr,
    pub line:     usize,
}

/// `law <name> { forall <param>: <body> }`
#[derive(Debug, Clone)]
pub struct LawDecl {
    pub name:  String,
    pub param: String,
    pub body:  Expr,
    pub line:  usize,
}

/// `fn <name>(<params>) -> <ret_kappa_hint> { <body> }`
#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name:       String,
    pub params:     Vec<(String, Option<KappaRange>)>,  // (name, kappa hint)
    pub ret_hint:   Option<KappaRange>,
    pub body:       Vec<Stmt>,
    pub line:       usize,
}

/// Statement inside a function body.
#[derive(Debug, Clone)]
pub enum Stmt {
    Let { name: String, value: Expr, line: usize },
    Return { value: Expr, line: usize },
    Expr { value: Expr, line: usize },
}

/// Expression tree — produced by recursive-descent parser.
#[derive(Debug, Clone)]
pub enum Expr {
    FloatLit(f64),
    BoolLit(bool),
    Ident(String),
    BinOp {
        op:    BinOpKind,
        left:  Box<Expr>,
        right: Box<Expr>,
    },
    UnaryOp {
        op:   UnaryOpKind,
        expr: Box<Expr>,
    },
    Call {
        callee: String,
        args:   Vec<Expr>,
    },
    Member {
        object: Box<Expr>,
        field:  String,
    },
    Forall {
        param: String,
        body:  Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Add, Sub, Mul, Div,
    Lt, Gt, LtEq, GtEq,
    And, Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOpKind {
    Neg, Not,
}
