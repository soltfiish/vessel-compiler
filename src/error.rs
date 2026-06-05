//! DRC error types and compiler diagnostics.
//! Maps directly onto the three DRC error classes in Scale Calculus.

use std::fmt;

/// The three DRC error kinds from Scale Calculus.
#[derive(Debug, Clone, PartialEq)]
pub enum DrcKind {
    /// PRIMARY: recoverable drift. Kappa moved but rebalance is possible.
    Primary,
    /// SECONDARY: fatal. Forbidden zone violation or T-H1 breach.
    Secondary,
    /// TERTIARY: supervised. Phi collapse requires external vessel intervention.
    Tertiary,
}

impl fmt::Display for DrcKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrcKind::Primary   => write!(f, "DRC.PRIMARY"),
            DrcKind::Secondary => write!(f, "DRC.SECONDARY"),
            DrcKind::Tertiary  => write!(f, "DRC.TERTIARY"),
        }
    }
}

/// A compiler or runtime diagnostic.
#[derive(Debug, Clone)]
pub struct VesselError {
    pub kind:    DrcKind,
    pub phase:   Phase,
    pub message: String,
    pub line:    Option<usize>,
    pub col:     Option<usize>,
}

/// Which compiler phase produced this error.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Lexer,
    Parser,
    GraphBuilder,
    KappaInfer,
    CodeGen,
    Runtime,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Phase::Lexer        => write!(f, "Lexer"),
            Phase::Parser       => write!(f, "Parser"),
            Phase::GraphBuilder => write!(f, "GraphBuilder"),
            Phase::KappaInfer   => write!(f, "KappaInfer"),
            Phase::CodeGen      => write!(f, "CodeGen"),
            Phase::Runtime      => write!(f, "Runtime"),
        }
    }
}

impl fmt::Display for VesselError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let loc = match (self.line, self.col) {
            (Some(l), Some(c)) => format!(" at {l}:{c}"),
            (Some(l), None)    => format!(" at line {l}"),
            _                  => String::new(),
        };
        write!(f, "[{}][{}{}] {}", self.kind, self.phase, loc, self.message)
    }
}

impl VesselError {
    pub fn secondary(phase: Phase, msg: impl Into<String>) -> Self {
        Self { kind: DrcKind::Secondary, phase, message: msg.into(),
               line: None, col: None }
    }
    pub fn secondary_at(phase: Phase, line: usize, col: usize,
                        msg: impl Into<String>) -> Self {
        Self { kind: DrcKind::Secondary, phase, message: msg.into(),
               line: Some(line), col: Some(col) }
    }
    pub fn tertiary(phase: Phase, msg: impl Into<String>) -> Self {
        Self { kind: DrcKind::Tertiary, phase, message: msg.into(),
               line: None, col: None }
    }
    pub fn primary_at(phase: Phase, line: usize, col: usize,
                      msg: impl Into<String>) -> Self {
        Self { kind: DrcKind::Primary, phase, message: msg.into(),
               line: Some(line), col: Some(col) }
    }
    pub fn drc(kind: DrcKind, phase: Phase, msg: impl Into<String>) -> Self {
        Self { kind, phase, message: msg.into(), line: None, col: None }
    }
}

pub type VesselResult<T> = Result<T, VesselError>;
