# vesselc — Vessel Language Compiler

A compiler for **Vessel**, a programming language derived from [Scale Calculus](https://zenodo.org/records/20546116) by Ayokunle Olufosoye (Solt).

---

## What is Vessel?

Vessel is a formal programming language whose semantics are grounded in the mathematical theory of Scale Calculus. Every program is a **network of vessels** — entities characterized by a kappa value (κ ∈ [0,1]) representing their scale position. The language enforces:

- **The Forbidden Zone** — κ ∈ (e^{−1}, e^{−1/2}) ≈ (0.3679, 0.6065) is structurally reserved for sentient vessels. Non-sentient vessels cannot occupy this band.
- **T-H1** — α = ln(κ): the alpha field of every vessel is derived from its kappa value, not independently assigned.
- **Phi = 0 maintenance** — Φ(R) = ∮I(V,V)dσ − ∫P(V)dV = 0. Networks rebalance toward this equilibrium.
- **Sentient Vessel Incompleteness** (Open Problem 5) — Vessels in the forbidden zone have opaque interiors. I(V,V) is not externally computable for sentient vessels; their boundary contract is enforced, their interior is undefined.

---

## Compiler Architecture

```
source.vessel
     │
     ▼
  [Lexer]          src/lexer.rs      — O(n) linear scan, full token stream
     │
     ▼
  [Parser]         src/parser.rs     — Recursive descent, full expression AST
     │
     ▼
  [KappaInfer]     src/kappa_infer.rs — Constraint propagation, rules R1-R9
     │
     ▼
  [CodeGen]        src/codegen.rs    — Python transpiler + LLVM IR stub
     │
     ▼
  output.py  /  output.ll
```

### Inference Rules (R1-R9)

| Rule | Description |
|------|-------------|
| R1 | Concrete seed: literal kappa → point range |
| R2 | Coupling safety: same-zone disjoint ranges cannot propagate |
| R3 | Forbidden exclusion: non-sentient vessels rejected from zone |
| R4a | Upper narrowing: boundary < kappa.hi narrows upper bound |
| R4b | Lower narrowing: boundary > kappa.lo narrows lower bound |
| R4c | Point propagation: kappa = boundary → point range |
| R5 | T-H1 alpha derivation: alpha range = ln(kappa range) |
| R6 | Fidelity constraint: I(V,V) preserved under coupling |
| R7 | Function param kappa range annotations and assertions |
| R8 | Function return kappa range annotation |
| R9 | Composition check: callee param ranges reachable from call sites |

---

## Language Syntax

```vessel
-- Vessel declaration
vessel Earth {
    kappa:    0.25,
    boundary: 0.25
}

-- Sentient vessel (permitted in forbidden zone)
vessel Human {
    kappa:    0.5,
    boundary: 0.5,
    sentient: true
}

-- Coupling
couple Earth to Sky { coupling: 0.3 }

-- Law (forall quantifier over the network)
law non_negative { forall v: v >= 0.0 }

-- Function with kappa range annotations
fn scale(x: <0.0, 0.35>) -> <0.0, 0.4> {
    let y = x + 0.05
    return y
}
```

---

## Build

Requires Rust 1.70+.

```bash
cargo build --release
```

Binary is at `target/release/vesselc`.

---

## Usage

```bash
# Compile to Python (default transpiler target)
vesselc program.vessel --out program.py

# Emit LLVM IR stub (future native target)
vesselc program.vessel --target llvm --out program.ll

# Print to stdout
vesselc program.vessel
```

---

## Tests

```bash
cargo test
```

20 tests covering: lexer, parser, forbidden zone enforcement, sentient vessel handling, coupling emission, law assertion generation, function R7/R8 annotations, T-H1 alpha emission, Phi rebalance emission, LLVM IR stub, binary expressions, multiple vessels, function bodies, constants, unknown vessel references, and full round-trip compilation.

---

## Key Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `FORBIDDEN_LOW` | e^{−1} ≈ 0.36788 | Lower forbidden zone boundary |
| `FORBIDDEN_HIGH` | e^{−1/2} ≈ 0.60653 | Upper forbidden zone boundary |
| `TH1_TOL` | 1e-9 | T-H1 numerical tolerance |
| `PHI_TOL` | 0.05 | Phi equilibrium tolerance |
| `MAX_REBALANCE` | 50 | Max rebalance iterations |

---

## Theoretical Foundation

This compiler implements the formal type system of the Vessel Language as specified in:

- **Paper 8 — Vessel Language** (Olufosoye, 2026): Full language specification, type theory, sentient vessel incompleteness theorem
- **Paper 1 — Scale Calculus v3** (Olufosoye, 2026): [DOI: 10.5281/zenodo.20546116](https://doi.org/10.5281/zenodo.20546116) — mathematical foundation
- **Paper 6 — Boundary Energy** (Olufosoye, 2026): Boundary dynamics formalism
- **Paper 7 — Relational Value** (Olufosoye, 2026): Coupling and interaction theory

---

## Author

**Ayokunle Olufosoye** (alias: Solt)  
Independent Researcher, Houston TX  
ayolufosoye@gmail.com

---

## License

MIT
