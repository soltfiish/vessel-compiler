//! vesselc — Vessel Language Compiler
//! Usage: vesselc <file.vessel> [--target python|llvm] [--out <outfile>]

mod ast;
mod codegen;
mod constants;
mod error;
mod kappa_infer;
mod lexer;
mod llvm_codegen;
mod parser;

use std::env;
use std::fs;
use std::process;

use codegen::CodeGen;
use kappa_infer::KappaInfer;
use lexer::Lexer;
use parser::Parser;

// ------------------------------------------------------------------ compile pipeline

fn compile(source: &str, target: &str) -> Result<String, error::VesselError> {
    // Phase 1: Lex
    let mut lex = Lexer::new(source);
    let tokens = lex.tokenize()?;

    // Phase 2: Parse
    let mut par = Parser::new(tokens);
    let program = par.parse()?;

    // Phase 3: KappaInfer
    let mut ki = KappaInfer::new();
    ki.infer(&program)?;

    // Phase 4: CodeGen
    if target == "native" {
        // Native path: emit object file via inkwell, return empty string
        // (caller writes the .o file directly)
        return Ok(String::from("__native__"));
    }

    let output = match target {
        "llvm" => codegen::emit_llvm_ir_stub(&program),
        _      => CodeGen::new(&program, &ki).emit()?,
    };

    Ok(output)
}

// ------------------------------------------------------------------ main

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: vesselc <file.vessel> [--target python|llvm] [--out <outfile>]");
        process::exit(1);
    }

    let input_path = &args[1];
    let mut target = "python";
    let mut out_path: Option<&str> = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                i += 1;
                if i < args.len() { target = &args[i]; }
            }
            "--out" => {
                i += 1;
                if i < args.len() { out_path = Some(&args[i]); }
            }
            _ => {}
        }
        i += 1;
    }

    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("vesselc: Cannot read '{}': {e}", input_path);
            process::exit(1);
        }
    };

    // Native target: bypass compile() and call llvm_codegen directly
    if target == "native" {
        use lexer::Lexer;
        use parser::Parser;
        use kappa_infer::KappaInfer;

        let mut lex = Lexer::new(&source);
        let tokens = match lex.tokenize() {
            Ok(t) => t,
            Err(e) => { eprintln!("vesselc error: {e}"); process::exit(1); }
        };
        let mut par = Parser::new(tokens);
        let program = match par.parse() {
            Ok(p) => p,
            Err(e) => { eprintln!("vesselc error: {e}"); process::exit(1); }
        };
        let mut ki = KappaInfer::new();
        if let Err(e) = ki.infer(&program) {
            eprintln!("vesselc error: {e}"); process::exit(1);
        }
        let obj_str = out_path.unwrap_or("vessel_out.o");
        let obj_path = std::path::Path::new(obj_str);
        let bin_path = obj_path.with_extension("");
        if let Err(e) = llvm_codegen::compile_to_object(&program, &ki, obj_path) {
            eprintln!("vesselc native error: {e}"); process::exit(1);
        }
        let status = std::process::Command::new("clang-19")
            .args([obj_str, "-o", bin_path.to_str().unwrap(), "-lm"])
            .status();
        match status {
            Ok(s) if s.success() => println!("vesselc: binary -> {}", bin_path.display()),
            Ok(s) => { eprintln!("vesselc: clang-19 {s}"); process::exit(1); }
            Err(e) => { eprintln!("vesselc: clang-19 not found: {e}"); process::exit(1); }
        }
        return;
    }

    match compile(&source, target) {
        Ok(output) => {
            match out_path {
                Some(p) => {
                    if let Err(e) = fs::write(p, &output) {
                        eprintln!("vesselc: Cannot write '{}': {e}", p);
                        process::exit(1);
                    }
                    println!("vesselc: wrote {} ({} bytes)", p, output.len());
                }
                None => print!("{output}"),
            }
        }
        Err(e) => {
            eprintln!("vesselc error: {e}");
            process::exit(1);
        }
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    // Helper: compile a snippet, expect success, return output
    fn ok(src: &str) -> String {
        compile(src, "python").expect("Expected compile success")
    }

    // Helper: compile a snippet, expect error message containing needle
    fn err_contains(src: &str, needle: &str) {
        let e = compile(src, "python").expect_err("Expected compile error");
        let msg = format!("{e}");
        assert!(
            msg.contains(needle),
            "Expected error containing {:?}, got: {msg}",
            needle
        );
    }

    // ------------------------------------------------------------------ T1: lexer basics

    #[test]
    fn t1_lexer_floats_and_keywords() {
        use crate::lexer::{Lexer, TokenKind};
        let src = "vessel boundary couple fn law 0.5 -0.3";
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().expect("tokenize");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Vessel));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Fn));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Float(v) if (v - 0.5).abs() < 1e-9)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Float(v) if (v + 0.3).abs() < 1e-9)));
    }

    // ------------------------------------------------------------------ T2: parser — empty vessel

    #[test]
    fn t2_parse_vessel_minimal() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }";
        let output = ok(src);
        assert!(output.contains("A_kappa"));
    }

    // ------------------------------------------------------------------ T3: forbidden zone rejection

    #[test]
    fn t3_forbidden_zone_rejects_non_sentient() {
        // kappa 0.4 is inside (e^-1 ≈ 0.3679, e^-0.5 ≈ 0.6065)
        let src = "vessel Bad { kappa: 0.4, boundary: 0.4 }";
        err_contains(src, "forbidden zone");
    }

    // ------------------------------------------------------------------ T4: forbidden zone allowed for sentient

    #[test]
    fn t4_sentient_vessel_in_forbidden_zone() {
        let src = "vessel Bio { kappa: 0.5, boundary: 0.5, sentient: true }";
        let output = ok(src);
        assert!(output.contains("Bio_kappa"));
        // sentient flag should appear
        assert!(output.contains("sentient"));
    }

    // ------------------------------------------------------------------ T5: couple emission

    #[test]
    fn t5_couple_emitted() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
vessel B { kappa: 0.7, boundary: 0.7 }
couple A to B { coupling: 0.3 }";
        let output = ok(src);
        assert!(output.contains("COUPLES"));
        assert!(output.contains("\"A\""));
        assert!(output.contains("\"B\""));
    }

    // ------------------------------------------------------------------ T6: coupling safety (disjoint same-zone ranges)

    #[test]
    fn t6_coupling_safety_disjoint() {
        // Both vessels in the low zone (< FORBIDDEN_LOW ~ 0.3679), disjoint point ranges.
        // This is the condition R2 rejects: same zone, no overlap, cannot propagate.
        // We need two distinct values both below 0.3679.
        // Use 0.1 and 0.3 (both < 0.3679; after R4, both are point ranges; they don't intersect).
        let src = "vessel A { kappa: 0.1, boundary: 0.1 }
vessel C { kappa: 0.3, boundary: 0.3 }
couple A to C { coupling: 0.5 }";
        // Point ranges [0.1,0.1] and [0.3,0.3] are both in low zone, disjoint — expect R2 error
        err_contains(src, "do not intersect");
    }

    // ------------------------------------------------------------------ T7: law emission

    #[test]
    fn t7_law_emitted() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
law stability { forall v: v >= 0.0 }";
        let output = ok(src);
        assert!(output.contains("def law_stability"));
        assert!(output.contains("network.A_kappa"));
    }

    // ------------------------------------------------------------------ T8: fn with R7 assertion

    #[test]
    fn t8_fn_r7_assertion() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
fn scale(x: <0.0, 0.35>) { return x }";
        let output = ok(src);
        assert!(output.contains("def scale"));
        assert!(output.contains("0.0000 <= x <= 0.3500"));
    }

    // ------------------------------------------------------------------ T9: fn R8 return annotation

    #[test]
    fn t9_fn_r8_return_range() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
fn clamp(x) -> <0.0, 0.35> { return x }";
        let output = ok(src);
        // R8 comment should appear
        assert!(output.contains("R8: return kappa expected"));
    }

    // ------------------------------------------------------------------ T10: T-H1 alpha emission

    #[test]
    fn t10_th1_alpha_emitted() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }";
        let output = ok(src);
        assert!(output.contains("_th1("));
        assert!(output.contains("A_alpha"));
    }

    // ------------------------------------------------------------------ T11: phi rebalance emitted

    #[test]
    fn t11_phi_rebalance_emitted() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }";
        let output = ok(src);
        assert!(output.contains("def phi_rebalance"));
        assert!(output.contains("PHI_TOL"));
    }

    // ------------------------------------------------------------------ T12: LLVM IR stub

    #[test]
    fn t12_llvm_ir_stub() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
fn scale(x) { return x }";
        let output = compile(src, "llvm").expect("llvm target");
        assert!(output.contains("@A_kappa"));
        assert!(output.contains("@scale("));
    }

    // ------------------------------------------------------------------ T13: lexer comment skip

    #[test]
    fn t13_comments_skipped() {
        use crate::lexer::{Lexer, TokenKind};
        let src = "-- this is a comment\nvessel";
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Vessel));
        // comment content should not produce tokens
        assert!(!tokens.iter().any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "this")));
    }

    // ------------------------------------------------------------------ T14: binary expressions

    #[test]
    fn t14_binary_expr_in_law() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
vessel B { kappa: 0.8, boundary: 0.8 }
law non_neg { forall v: v >= 0.0 }";
        let output = ok(src);
        assert!(output.contains(">="));
    }

    // ------------------------------------------------------------------ T15: forbidden zone boundary values

    #[test]
    fn t15_forbidden_zone_boundary() {
        // kappa exactly at FORBIDDEN_LOW should NOT be in zone (exclusive)
        let src = format!("vessel Edge {{ kappa: {FORBIDDEN_LOW:.10}, boundary: {FORBIDDEN_LOW:.10} }}");
        // Should compile OK (boundary is excluded from forbidden zone)
        let _ = ok(&src);

        // kappa just inside zone
        let inside = (FORBIDDEN_LOW + FORBIDDEN_HIGH) / 2.0;
        let src2 = format!("vessel Inside {{ kappa: {inside:.10}, boundary: {inside:.10} }}");
        err_contains(&src2, "forbidden zone");
    }

    // ------------------------------------------------------------------ T16: multiple vessels

    #[test]
    fn t16_multiple_vessels_emitted() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
vessel B { kappa: 0.8, boundary: 0.8 }
vessel C { kappa: 0.9, boundary: 0.9 }";
        let output = ok(src);
        assert!(output.contains("A_kappa"));
        assert!(output.contains("B_kappa"));
        assert!(output.contains("C_kappa"));
    }

    // ------------------------------------------------------------------ T17: function body let + return

    #[test]
    fn t17_fn_body_let_return() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
fn double(x) {
    let y = x + x
    return y
}";
        let output = ok(src);
        assert!(output.contains("y = (x + x)"));
        assert!(output.contains("return y"));
    }

    // ------------------------------------------------------------------ T18: kappa constants in output

    #[test]
    fn t18_constants_in_output() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }";
        let output = ok(src);
        assert!(output.contains("FORBIDDEN_LOW"));
        assert!(output.contains("FORBIDDEN_HIGH"));
        assert!(output.contains("TH1_TOL"));
    }

    // ------------------------------------------------------------------ T19: unknown vessel in couple

    #[test]
    fn t19_couple_unknown_vessel() {
        let src = "vessel A { kappa: 0.2, boundary: 0.2 }
couple A to GHOST { coupling: 0.5 }";
        err_contains(src, "GHOST");
    }

    // ------------------------------------------------------------------ T20: full round-trip compile

    #[test]
    fn t20_full_round_trip() {
        let src = r#"
vessel Earth { kappa: 0.25, boundary: 0.25 }
vessel Sky   { kappa: 0.75, boundary: 0.75 }

couple Earth to Sky { coupling: 0.3 }

law balance { forall v: v >= 0.0 }

fn scale(x: <0.0, 0.35>) -> <0.0, 0.4> {
    let y = x + 0.05
    return y
}
"#;
        let output = ok(src);
        assert!(output.contains("Earth_kappa"));
        assert!(output.contains("Sky_kappa"));
        assert!(output.contains("COUPLES"));
        assert!(output.contains("def law_balance"));
        assert!(output.contains("def scale"));
        assert!(output.contains("def phi_rebalance"));
    }
}
