//! Scale Calculus constants.
//! These values are not arbitrary — they are forced by log-Poisson uniqueness
//! (Freeburg 2026, arXiv:2604.01632) and T-H1: alpha = ln(kappa).

/// Lower boundary of the forbidden zone: e^{-1} ≈ 0.36787944117
/// Only biological/sentient vessels may occupy kappa in (FORBIDDEN_LOW, FORBIDDEN_HIGH).
pub const FORBIDDEN_LOW: f64  = 0.36787944117144233;  // e^{-1}

/// Upper boundary of the forbidden zone: e^{-1/2} ≈ 0.60653065971
/// Sentient vessels in kappa ∈ (FORBIDDEN_LOW, FORBIDDEN_HIGH) have opaque interiors.
pub const FORBIDDEN_HIGH: f64 = 0.60653065971263342;  // e^{-1/2}

/// Minimum epsilon: kappa is bounded away from 0 and 1 by this amount.
pub const EPS: f64 = 1e-9;

/// T-H1 tolerance: |alpha - ln(kappa)| must be less than this.
pub const TH1_TOL: f64 = 1e-9;

/// Phi equilibrium tolerance for runtime rebalancing.
pub const PHI_TOL: f64 = 0.05;

/// Maximum rebalancing iterations before DRC.TERTIARY.
pub const MAX_REBALANCE: usize = 50;

/// Maximum kappa inference iterations before divergence error.
pub const MAX_INFER_ITER: usize = 200;
