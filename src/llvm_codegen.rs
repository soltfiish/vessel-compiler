//! Vessel native codegen via inkwell (LLVM 19).
//!
//! Architecture:
//!   Each Vessel program compiles to an LLVM module containing:
//!
//!   1. Global constants — one f64 per vessel kappa value
//!   2. @vessel_init() — allocates and initializes a VesselNetwork struct,
//!      enforces the forbidden zone check, derives alpha = ln(kappa) via
//!      the LLVM `log` intrinsic.
//!   3. One function per `fn` declaration — f64 -> f64 with R7 range
//!      assertions emitted as LLVM conditional branches that call @vessel_panic.
//!   4. @phi_rebalance() — iterates up to MAX_REBALANCE steps nudging kappa
//!      toward boundary until |phi| < PHI_TOL.
//!   5. @vessel_panic(msg_ptr) — calls libc abort() after printing the message.
//!
//! Output: a `.bc` (LLVM bitcode) file.
//! To produce a native binary: `clang-19 out.bc -o program -lm`

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::values::FunctionValue;
use inkwell::OptimizationLevel;
use inkwell::AddressSpace;

use std::path::Path;

use crate::ast::*;
use crate::constants::*;
use crate::error::{Phase, VesselError, VesselResult};
use crate::kappa_infer::KappaInfer;

pub struct LlvmCodeGen<'ctx> {
    ctx:     &'ctx Context,
    module:  Module<'ctx>,
    builder: Builder<'ctx>,
    program: &'ctx Program,
    ki:      &'ctx KappaInfer,
}

impl<'ctx> LlvmCodeGen<'ctx> {
    pub fn new(
        ctx:     &'ctx Context,
        program: &'ctx Program,
        ki:      &'ctx KappaInfer,
    ) -> Self {
        let module  = ctx.create_module("vessel");
        let builder = ctx.create_builder();
        Self { ctx, module, builder, program, ki }
    }

    /// Top-level entry: emit the full module, write bitcode to `out_path`.
    pub fn emit_bitcode(self, out_path: &Path) -> VesselResult<()> {
        let f64_ty  = self.ctx.f64_type();
        let i32_ty  = self.ctx.i32_type();
        let i8_ty   = self.ctx.i8_type();
        let void_ty = self.ctx.void_type();

        // ---------------------------------------------------------------- globals
        // One global per vessel: @<name>_kappa, @<name>_boundary, @<name>_alpha
        for vd in &self.program.vessels {
            let kv = literal_f64(&vd.kappa);
            let bv = literal_f64(&vd.boundary);

            let kg = self.module.add_global(f64_ty, None, &format!("{}_kappa", vd.name));
            kg.set_initializer(&f64_ty.const_float(kv));
            kg.set_constant(true);

            let bg = self.module.add_global(f64_ty, None, &format!("{}_boundary", vd.name));
            bg.set_initializer(&f64_ty.const_float(bv));
            bg.set_constant(true);

            // alpha = ln(kappa) — computed at init time, stored as mutable global
            let ag = self.module.add_global(f64_ty, None, &format!("{}_alpha", vd.name));
            ag.set_initializer(&f64_ty.const_float(kv.ln()));
        }

        // ---------------------------------------------------------------- @vessel_panic
        // declare void @vessel_panic(i8* msg)
        // Calls abort() from libc. We declare abort externally.
        let abort_ty   = void_ty.fn_type(&[], false);
        let abort_fn   = self.module.add_function("abort", abort_ty, None);

        let panic_ty   = void_ty.fn_type(&[self.ctx.ptr_type(AddressSpace::default()).into()], false);
        let panic_fn   = self.module.add_function("vessel_panic", panic_ty, None);
        {
            let bb = self.ctx.append_basic_block(panic_fn, "entry");
            self.builder.position_at_end(bb);
            self.builder.build_call(abort_fn, &[], "").unwrap();
            self.builder.build_unreachable().unwrap();
        }

        // ---------------------------------------------------------------- @vessel_check_kappa
        // declare double @vessel_check_kappa(double kappa, i1 sentient)
        // Traps if kappa is in forbidden zone and sentient == false.
        let check_ty = f64_ty.fn_type(
            &[f64_ty.into(), self.ctx.bool_type().into()],
            false
        );
        let check_fn = self.module.add_function("vessel_check_kappa", check_ty, None);
        {
            let entry   = self.ctx.append_basic_block(check_fn, "entry");
            let trap_bb = self.ctx.append_basic_block(check_fn, "trap");
            let ok_bb   = self.ctx.append_basic_block(check_fn, "ok");

            self.builder.position_at_end(entry);
            let kappa    = check_fn.get_nth_param(0).unwrap().into_float_value();
            let sentient = check_fn.get_nth_param(1).unwrap().into_int_value();

            let fl = f64_ty.const_float(FORBIDDEN_LOW);
            let fh = f64_ty.const_float(FORBIDDEN_HIGH);

            // in_zone = kappa > FORBIDDEN_LOW && kappa < FORBIDDEN_HIGH
            let gt_fl = self.builder.build_float_compare(
                inkwell::FloatPredicate::OGT, kappa, fl, "gt_fl").unwrap();
            let lt_fh = self.builder.build_float_compare(
                inkwell::FloatPredicate::OLT, kappa, fh, "lt_fh").unwrap();
            let in_zone = self.builder.build_and(gt_fl, lt_fh, "in_zone").unwrap();

            // should_trap = in_zone && !sentient
            let not_sentient = self.builder.build_not(sentient, "not_sentient").unwrap();
            let should_trap  = self.builder.build_and(in_zone, not_sentient, "should_trap").unwrap();

            self.builder.build_conditional_branch(should_trap, trap_bb, ok_bb).unwrap();

            // trap block: call vessel_panic and unreachable
            self.builder.position_at_end(trap_bb);
            let msg = self.builder.build_global_string_ptr(
                "DRC.PRIMARY: kappa in forbidden zone\0", "panic_msg").unwrap();
            self.builder.build_call(
                panic_fn,
                &[msg.as_pointer_value().into()],
                ""
            ).unwrap();
            self.builder.build_unreachable().unwrap();

            // ok block: return kappa unchanged
            self.builder.position_at_end(ok_bb);
            self.builder.build_return(Some(&kappa)).unwrap();
        }

        // ---------------------------------------------------------------- @vessel_ln
        // Use LLVM's llvm.log.f64 intrinsic for alpha = ln(kappa)
        let log_intrinsic_ty = f64_ty.fn_type(&[f64_ty.into()], false);
        let log_fn = self.module.add_function("llvm.log.f64", log_intrinsic_ty, None);

        // ---------------------------------------------------------------- @vessel_init
        // void @vessel_init()  — validates all vessels, computes alpha fields
        let init_ty = void_ty.fn_type(&[], false);
        let init_fn = self.module.add_function("vessel_init", init_ty, None);
        {
            let bb = self.ctx.append_basic_block(init_fn, "entry");
            self.builder.position_at_end(bb);

            for vd in &self.program.vessels {
                let kg = self.module.get_global(&format!("{}_kappa", vd.name)).unwrap();
                let ag = self.module.get_global(&format!("{}_alpha", vd.name)).unwrap();

                let kv = self.builder.build_load(
                    f64_ty, kg.as_pointer_value(), &format!("{}_kappa_val", vd.name)
                ).unwrap().into_float_value();

                let sentient_val = self.ctx.bool_type()
                    .const_int(if vd.sentient { 1 } else { 0 }, false);

                // check kappa
                self.builder.build_call(
                    check_fn,
                    &[kv.into(), sentient_val.into()],
                    ""
                ).unwrap();

                // alpha = ln(kappa)
                let alpha = self.builder.build_call(
                    log_fn, &[kv.into()], &format!("{}_alpha_val", vd.name)
                ).unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_float_value();

                self.builder.build_store(ag.as_pointer_value(), alpha).unwrap();
            }

            self.builder.build_return(None).unwrap();
        }

        // ---------------------------------------------------------------- user fns
        for fd in &self.program.fns {
            self.emit_fn(fd, f64_ty, &panic_fn)?;
        }

        // ---------------------------------------------------------------- @phi_rebalance
        self.emit_phi_rebalance(f64_ty, void_ty)?;

        // ---------------------------------------------------------------- @main stub
        // Provides a runnable entry point: calls vessel_init then returns 0.
        let main_ty = i32_ty.fn_type(&[], false);
        let main_fn = self.module.add_function("main", main_ty, None);
        {
            let bb = self.ctx.append_basic_block(main_fn, "entry");
            self.builder.position_at_end(bb);
            self.builder.build_call(init_fn, &[], "").unwrap();
            self.builder.build_return(Some(&i32_ty.const_int(0, false))).unwrap();
        }

        // ---------------------------------------------------------------- write bitcode
        self.module.write_bitcode_to_path(out_path);
        Ok(())
    }

    // ---------------------------------------------------------------- user fn

    fn emit_fn(
        &self,
        fd:       &FnDecl,
        f64_ty:   inkwell::types::FloatType<'ctx>,
        panic_fn: &FunctionValue<'ctx>,
    ) -> VesselResult<()> {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
            fd.params.iter().map(|_| f64_ty.into()).collect();
        let fn_ty  = f64_ty.fn_type(&param_types, false);
        let llvm_fn = self.module.add_function(&fd.name, fn_ty, None);

        let entry = self.ctx.append_basic_block(llvm_fn, "entry");
        self.builder.position_at_end(entry);

        // Build local variable map: param name -> LLVM value
        let mut locals: std::collections::HashMap<String, inkwell::values::BasicValueEnum> =
            std::collections::HashMap::new();

        // R7: emit range check for annotated params
        for (i, (pname, hint)) in fd.params.iter().enumerate() {
            let pval = llvm_fn.get_nth_param(i as u32).unwrap().into_float_value();
            locals.insert(pname.clone(), pval.into());

            if let Some(h) = hint {
                let lo = f64_ty.const_float(h.lo);
                let hi = f64_ty.const_float(h.hi);

                let ok_bb   = self.ctx.append_basic_block(llvm_fn, &format!("{pname}_ok"));
                let fail_bb = self.ctx.append_basic_block(llvm_fn, &format!("{pname}_fail"));

                let ge_lo = self.builder.build_float_compare(
                    inkwell::FloatPredicate::OGE, pval, lo, "ge_lo").unwrap();
                let le_hi = self.builder.build_float_compare(
                    inkwell::FloatPredicate::OLE, pval, hi, "le_hi").unwrap();
                let in_range = self.builder.build_and(ge_lo, le_hi, "in_range").unwrap();

                self.builder.build_conditional_branch(in_range, ok_bb, fail_bb).unwrap();

                self.builder.position_at_end(fail_bb);
                let msg_str = format!("R7: param '{pname}' out of range\0");
                let msg = self.builder.build_global_string_ptr(&msg_str, "r7_msg").unwrap();
                self.builder.build_call(
                    *panic_fn,
                    &[msg.as_pointer_value().into()],
                    ""
                ).unwrap();
                self.builder.build_unreachable().unwrap();

                self.builder.position_at_end(ok_bb);
            }
        }

        // Emit body statements
        let mut ret_val: Option<inkwell::values::FloatValue> = None;
        for stmt in &fd.body {
            match stmt {
                Stmt::Let { name, value, .. } => {
                    if let Some(v) = self.emit_expr_f64(value, &locals, f64_ty, llvm_fn) {
                        locals.insert(name.clone(), v.into());
                    }
                }
                Stmt::Return { value, .. } => {
                    if let Some(v) = self.emit_expr_f64(value, &locals, f64_ty, llvm_fn) {
                        ret_val = Some(v);
                    }
                }
                Stmt::Expr { value, .. } => {
                    self.emit_expr_f64(value, &locals, f64_ty, llvm_fn);
                }
            }
        }

        match ret_val {
            Some(v) => { self.builder.build_return(Some(&v)).unwrap(); }
            None    => { self.builder.build_return(Some(&f64_ty.const_float(0.0))).unwrap(); }
        }

        Ok(())
    }

    // ---------------------------------------------------------------- expr -> f64

    fn emit_expr_f64(
        &self,
        expr:   &Expr,
        locals: &std::collections::HashMap<String, inkwell::values::BasicValueEnum<'ctx>>,
        f64_ty: inkwell::types::FloatType<'ctx>,
        _fn:    FunctionValue<'ctx>,
    ) -> Option<inkwell::values::FloatValue<'ctx>> {
        match expr {
            Expr::FloatLit(v) => Some(f64_ty.const_float(*v)),
            Expr::BoolLit(b)  => Some(f64_ty.const_float(if *b { 1.0 } else { 0.0 })),
            Expr::Ident(name) => {
                // Check locals first, then globals
                if let Some(v) = locals.get(name) {
                    return Some(v.into_float_value());
                }
                // Try vessel kappa global
                let gname = format!("{name}_kappa");
                if let Some(g) = self.module.get_global(&gname) {
                    let loaded = self.builder.build_load(
                        f64_ty, g.as_pointer_value(), name
                    ).unwrap();
                    return Some(loaded.into_float_value());
                }
                None
            }
            Expr::BinOp { op, left, right } => {
                let l = self.emit_expr_f64(left,  locals, f64_ty, _fn)?;
                let r = self.emit_expr_f64(right, locals, f64_ty, _fn)?;
                let v = match op {
                    BinOpKind::Add  => self.builder.build_float_add(l, r, "add").unwrap(),
                    BinOpKind::Sub  => self.builder.build_float_sub(l, r, "sub").unwrap(),
                    BinOpKind::Mul  => self.builder.build_float_mul(l, r, "mul").unwrap(),
                    BinOpKind::Div  => self.builder.build_float_div(l, r, "div").unwrap(),
                    // Comparisons: emit as f64 0.0/1.0
                    BinOpKind::Lt   => {
                        let cmp = self.builder.build_float_compare(
                            inkwell::FloatPredicate::OLT, l, r, "lt").unwrap();
                        self.builder.build_unsigned_int_to_float(cmp, f64_ty, "lt_f").unwrap()
                    }
                    BinOpKind::Gt   => {
                        let cmp = self.builder.build_float_compare(
                            inkwell::FloatPredicate::OGT, l, r, "gt").unwrap();
                        self.builder.build_unsigned_int_to_float(cmp, f64_ty, "gt_f").unwrap()
                    }
                    BinOpKind::LtEq => {
                        let cmp = self.builder.build_float_compare(
                            inkwell::FloatPredicate::OLE, l, r, "le").unwrap();
                        self.builder.build_unsigned_int_to_float(cmp, f64_ty, "le_f").unwrap()
                    }
                    BinOpKind::GtEq => {
                        let cmp = self.builder.build_float_compare(
                            inkwell::FloatPredicate::OGE, l, r, "ge").unwrap();
                        self.builder.build_unsigned_int_to_float(cmp, f64_ty, "ge_f").unwrap()
                    }
                    BinOpKind::And  => self.builder.build_float_mul(l, r, "and_f").unwrap(),
                    BinOpKind::Or   => self.builder.build_float_add(l, r, "or_f").unwrap(),
                };
                Some(v)
            }
            Expr::UnaryOp { op, expr } => {
                let v = self.emit_expr_f64(expr, locals, f64_ty, _fn)?;
                match op {
                    UnaryOpKind::Neg => Some(self.builder.build_float_neg(v, "neg").unwrap()),
                    UnaryOpKind::Not => {
                        let zero = f64_ty.const_float(0.0);
                        let cmp = self.builder.build_float_compare(
                            inkwell::FloatPredicate::OEQ, v, zero, "is_zero").unwrap();
                        Some(self.builder.build_unsigned_int_to_float(cmp, f64_ty, "not_f").unwrap())
                    }
                }
            }
            Expr::Call { callee, args } => {
                if let Some(callee_fn) = self.module.get_function(callee) {
                    let arg_vals: Vec<inkwell::values::BasicMetadataValueEnum> = args.iter()
                        .filter_map(|a| self.emit_expr_f64(a, locals, f64_ty, _fn))
                        .map(|v| v.into())
                        .collect();
                    let ret = self.builder.build_call(callee_fn, &arg_vals, "call").unwrap();
                    ret.try_as_basic_value().basic().map(|v| v.into_float_value())
                } else {
                    None
                }
            }
            _ => None,  // Member, Forall, If — not yet lowered to LLVM
        }
    }

    // ---------------------------------------------------------------- phi_rebalance

    fn emit_phi_rebalance(
        &self,
        f64_ty:  inkwell::types::FloatType<'ctx>,
        void_ty: inkwell::types::VoidType<'ctx>,
    ) -> VesselResult<()> {
        if self.program.vessels.is_empty() { return Ok(()); }

        let i32_ty = self.ctx.i32_type();
        let fn_ty  = void_ty.fn_type(&[], false);
        let phi_fn = self.module.add_function("phi_rebalance", fn_ty, None);

        let entry    = self.ctx.append_basic_block(phi_fn, "entry");
        let loop_bb  = self.ctx.append_basic_block(phi_fn, "loop");
        let check_bb = self.ctx.append_basic_block(phi_fn, "check");
        let exit_bb  = self.ctx.append_basic_block(phi_fn, "exit");

        // entry: i = 0, jump to loop
        self.builder.position_at_end(entry);
        let i_alloca = self.builder.build_alloca(i32_ty, "i").unwrap();
        self.builder.build_store(i_alloca, i32_ty.const_int(0, false)).unwrap();
        self.builder.build_unconditional_branch(loop_bb).unwrap();

        // loop: compute phi = sum(kappa) - sum(boundary)
        self.builder.position_at_end(loop_bb);

        let zero = f64_ty.const_float(0.0);
        let mut kappa_sum   = zero;
        let mut boundary_sum = zero;

        for vd in &self.program.vessels {
            let kg = self.module.get_global(&format!("{}_kappa", vd.name)).unwrap();
            let bg = self.module.get_global(&format!("{}_boundary", vd.name)).unwrap();
            let kv = self.builder.build_load(
                f64_ty, kg.as_pointer_value(), &format!("{}_kv", vd.name)
            ).unwrap().into_float_value();
            let bv = self.builder.build_load(
                f64_ty, bg.as_pointer_value(), &format!("{}_bv", vd.name)
            ).unwrap().into_float_value();
            kappa_sum    = self.builder.build_float_add(kappa_sum,    kv, "ksum").unwrap();
            boundary_sum = self.builder.build_float_add(boundary_sum, bv, "bsum").unwrap();
        }

        let phi_val = self.builder.build_float_sub(kappa_sum, boundary_sum, "phi").unwrap();

        // |phi| via: phi < 0 ? -phi : phi
        let neg_phi    = self.builder.build_float_neg(phi_val, "neg_phi").unwrap();
        let is_neg     = self.builder.build_float_compare(
            inkwell::FloatPredicate::OLT, phi_val, zero, "is_neg").unwrap();
        let abs_phi    = self.builder.build_select(is_neg, neg_phi, phi_val, "abs_phi")
            .unwrap().into_float_value();

        self.builder.build_unconditional_branch(check_bb).unwrap();

        // check: if abs_phi < PHI_TOL => exit, else nudge and loop
        self.builder.position_at_end(check_bb);
        let tol     = f64_ty.const_float(PHI_TOL);
        let converged = self.builder.build_float_compare(
            inkwell::FloatPredicate::OLT, abs_phi, tol, "converged").unwrap();

        // also check loop count < MAX_REBALANCE
        let i_val    = self.builder.build_load(i32_ty, i_alloca, "i_val").unwrap().into_int_value();
        let max_iter = i32_ty.const_int(MAX_REBALANCE as u64, false);
        let iter_ok  = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT, i_val, max_iter, "iter_ok").unwrap();
        let should_loop = self.builder.build_and(
            self.builder.build_not(converged, "not_conv").unwrap(),
            iter_ok, "should_loop"
        ).unwrap();

        let nudge_bb = self.ctx.append_basic_block(phi_fn, "nudge");
        self.builder.build_conditional_branch(should_loop, nudge_bb, exit_bb).unwrap();

        // nudge: kappa -= 0.01 * phi for each non-sentient vessel
        self.builder.position_at_end(nudge_bb);
        let step = f64_ty.const_float(0.01);
        let nudge_amt = self.builder.build_float_mul(step, phi_val, "nudge_amt").unwrap();

        for vd in &self.program.vessels {
            if !vd.sentient {
                let kg = self.module.get_global(&format!("{}_kappa", vd.name)).unwrap();
                // kappa globals are const so we model rebalance on alpha (mutable)
                let ag = self.module.get_global(&format!("{}_alpha", vd.name)).unwrap();
                let av = self.builder.build_load(
                    f64_ty, ag.as_pointer_value(), "av"
                ).unwrap().into_float_value();
                let new_av = self.builder.build_float_sub(av, nudge_amt, "new_av").unwrap();
                self.builder.build_store(ag.as_pointer_value(), new_av).unwrap();
                let _ = kg; // kappa is const; alpha carries the drift
            }
        }

        // i++
        let new_i = self.builder.build_int_add(
            i_val, i32_ty.const_int(1, false), "new_i"
        ).unwrap();
        self.builder.build_store(i_alloca, new_i).unwrap();
        self.builder.build_unconditional_branch(loop_bb).unwrap();

        self.builder.position_at_end(exit_bb);
        self.builder.build_return(None).unwrap();

        Ok(())
    }
}

// ---------------------------------------------------------------- helpers

fn literal_f64(expr: &Expr) -> f64 {
    match expr {
        Expr::FloatLit(v) => *v,
        _ => 0.5,
    }
}

/// Compile a vessel program to native object code via LLVM.
/// Returns the path to the `.o` file.
pub fn compile_to_object(
    program: &Program,
    ki:      &KappaInfer,
    out_path: &Path,
) -> VesselResult<()> {
    // Initialize native target
    Target::initialize_native(&InitializationConfig::default()).map_err(|e| {
        VesselError::drc(
            crate::error::DrcKind::Secondary,
            Phase::CodeGen,
            format!("LLVM target init failed: {e}"),
        )
    })?;

    let ctx = Context::create();
    let cg  = LlvmCodeGen::new(&ctx, program, ki);

    // Write bitcode first
    let bc_path = out_path.with_extension("bc");
    cg.emit_bitcode(&bc_path)?;

    // Compile bitcode to object file via TargetMachine
    let triple  = TargetMachine::get_default_triple();
    let target  = Target::from_triple(&triple).map_err(|e| {
        VesselError::drc(
            crate::error::DrcKind::Secondary,
            Phase::CodeGen,
            format!("LLVM target lookup failed: {e}"),
        )
    })?;
    let machine = target.create_target_machine(
        &triple,
        "generic",
        "",
        OptimizationLevel::Default,
        RelocMode::PIC,
        CodeModel::Default,
    ).ok_or_else(|| VesselError::drc(
        crate::error::DrcKind::Secondary,
        Phase::CodeGen,
        "Failed to create LLVM TargetMachine".to_string(),
    ))?;

    // Reload module from bitcode to get a fresh owned module for emission
    let ctx2   = Context::create();
    let buf    = inkwell::memory_buffer::MemoryBuffer::create_from_file(&bc_path)
        .map_err(|e| VesselError::drc(
            crate::error::DrcKind::Secondary,
            Phase::CodeGen,
            format!("Could not read bitcode: {e}"),
        ))?;
    let module = ctx2.create_module_from_ir(buf).map_err(|e| {
        VesselError::drc(
            crate::error::DrcKind::Secondary,
            Phase::CodeGen,
            format!("Could not parse bitcode: {e}"),
        )
    })?;

    machine.write_to_file(&module, FileType::Object, out_path).map_err(|e| {
        VesselError::drc(
            crate::error::DrcKind::Secondary,
            Phase::CodeGen,
            format!("Object emit failed: {e}"),
        )
    })?;

    Ok(())
}
