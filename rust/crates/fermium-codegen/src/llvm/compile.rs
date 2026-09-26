//! IR → LLVM IR. Semantics mirror the tree-walker (eval.rs) exactly: the same IEEE helpers (fdiv, powc, fpow),
//! the same loop counts, the same run-time errors on the same lines, the same math functions (called through
//! shims, so LLVM can't constant-fold them with a different libm).
//!
//! Values: numbers are f64, booleans i1, texts i64 ids, lists pointers to `rt::FmList`, vectors / matrices /
//! complex numbers `[n x double]`. A construct the back end can't compile makes `compile` fail with a reason, and
//! the CLI runs the tree-walker instead (`llvm::supports`).
//!
//! Run-time errors: the callback stores the message and sets the context's flag; the compiled code checks the
//! flag after every call that can fail and returns up to fm_main. The program line of an error is the global
//! `fm.line`, updated like the tree-walker's `self.line` (every statement and expression with a line).
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{Linkage, Module as LModule};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, FloatValue, FunctionValue, GlobalValue,
                      IntValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use fermium_ir::{BinOp, CmpOp, Expr, ExprKind, Module, PrintItem, Stmt, StmtKind, Storage, SymId, Ty};

use super::rt::{self, BuiltinSite, Kind};


/// Bytes of stack a program may use before "called itself too many times" (it runs on a thread with a 512 MB
/// stack, like v1: codegen_llvm.STACK_LIMIT).
pub const STACK_LIMIT: u64 = 400 << 20;

pub type R<T> = Result<T, String>;

pub fn kind_of(ty: &Ty) -> R<Kind> {
    Ok(match ty {
        Ty::Num(_) => Kind::F,
        Ty::Bool => Kind::B,
        Ty::Str => Kind::S,
        Ty::List(_) => Kind::L,
        Ty::TextList => Kind::TL,
        Ty::Vec { n, .. } => Kind::V(*n),
        Ty::Mat { r, c, .. } => Kind::V(r * c),
        Ty::Complex(_) => Kind::V(2),
        Ty::Void => Kind::Void,
        Ty::Sol(_) => Kind::H,
        other => return Err(format!("values of type {} aren't compiled yet", other.kind())),
    })
}

/// What compile hands to the run-time context (texts made at compile time, built-in call sites, formats).
pub type Tables = crate::native::blob::GenTables;

#[derive(Clone, Copy)]
pub struct Val<'c> {
    pub k: Kind,
    pub v: Option<BasicValueEnum<'c>>,
}

/// The one-argument math built-ins called through shims (the same Rust functions as eval::math1).
const SHIM1: &[&str] = &["sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
                         "exp", "ln", "log", "log10", "log2", "expm1", "log1p", "cot", "sec", "csc", "erf", "erfc",
                         "gamma", "lgamma"];

pub struct Gen<'c, 'm> {
    cx: &'c Context,
    pub lm: LModule<'c>,
    b: Builder<'c>,
    m: &'m Module,
    pub tables: Tables,
    ctx_ptr: PointerValue<'c>,
    line_g: GlobalValue<'c>,
    stackbase_g: GlobalValue<'c>,
    externs: HashMap<&'static str, FunctionValue<'c>>,
    /// declared callbacks and the addresses they are mapped to in the JIT
    pub mappings: Vec<(&'static str, usize)>,
    globals: HashMap<SymId, (PointerValue<'c>, Kind)>,
    funcs: Vec<FunctionValue<'c>>,
    // the function being compiled
    fnv: Option<FunctionValue<'c>>,
    entry_b: Option<Builder<'c>>,
    locals: HashMap<SymId, (PointerValue<'c>, Kind)>,
    err_bb: Option<BasicBlock<'c>>,
    loops: Vec<(BasicBlock<'c>, BasicBlock<'c>)>,
    ret_kind: Kind,
    known_line: Option<u32>,
    /// variables that live somewhere else in the function being compiled (a parallel for's body function)
    overrides: HashMap<SymId, (PointerValue<'c>, Kind)>,
    par_count: usize,
    /// compiling for an executable (`fermium build`): the context is the global fm_ctx (set by the run time),
    /// not an address known now
    aot: Option<GlobalValue<'c>>,
}

macro_rules! bl {
    ($e:expr) => {
        $e.map_err(|e| format!("LLVM builder: {e:?}"))?
    };
}

impl<'c, 'm> Gen<'c, 'm> {
    /// For an executable: the context pointer is read from the global `fm_ctx` at each function's start.
    pub fn new_aot(cx: &'c Context, m: &'m Module) -> Gen<'c, 'm> {
        let mut g = Gen::new(cx, m, 0);
        let gv = g.lm.add_global(g.ptrt(), None, "fm_ctx");
        gv.set_linkage(Linkage::External);
        g.aot = Some(gv);
        g
    }

    pub fn new(cx: &'c Context, m: &'m Module, ctx_addr: usize) -> Gen<'c, 'm> {
        let lm = cx.create_module("fermium");
        let i64t = cx.i64_type();
        let ptr = cx.ptr_type(AddressSpace::default());
        let ctx_ptr = i64t.const_int(ctx_addr as u64, false).const_to_pointer(ptr);
        let line_g = lm.add_global(cx.i32_type(), None, "fm.line");
        line_g.set_initializer(&cx.i32_type().const_zero());
        line_g.set_linkage(Linkage::Internal);
        let stackbase_g = lm.add_global(i64t, None, "fm.stackbase");
        stackbase_g.set_initializer(&i64t.const_zero());
        stackbase_g.set_linkage(Linkage::Internal);
        let tables = Tables { texts: m.tables.texts.iter().map(|s| Rc::from(s.as_str())).collect(), ..Default::default() };
        let mut g = Gen { cx, lm, b: cx.create_builder(), m, tables, ctx_ptr, line_g, stackbase_g, externs: HashMap::new(),
                          mappings: vec![], globals: HashMap::new(), funcs: vec![], fnv: None, entry_b: None,
                          locals: HashMap::new(), err_bb: None, loops: vec![], ret_kind: Kind::Void, known_line: None,
                          overrides: HashMap::new(), par_count: 0, aot: None };
        g.declare_runtime();
        g
    }

    // ------------------------------------------------------------ declarations
    fn declare(&mut self, name: &'static str, ret: Option<BasicTypeEnum<'c>>, args: &[BasicTypeEnum<'c>], addr: usize,
               pure: bool) {
        let a: Vec<BasicMetadataTypeEnum> = args.iter().map(|t| (*t).into()).collect();
        let ft = match ret {
            Some(r) => r.fn_type(&a, false),
            None => self.cx.void_type().fn_type(&a, false),
        };
        let f = self.lm.add_function(name, ft, Some(Linkage::External));
        if pure {
            // memory(none): the shim reads and writes no memory the compiled code can see
            let mem = Attribute::get_named_enum_kind_id("memory");
            f.add_attribute(AttributeLoc::Function, self.cx.create_enum_attribute(mem, 0));
            for a in ["nounwind", "willreturn", "nosync"] {
                f.add_attribute(AttributeLoc::Function,
                                self.cx.create_enum_attribute(Attribute::get_named_enum_kind_id(a), 0));
            }
        } else {
            f.add_attribute(AttributeLoc::Function,
                            self.cx.create_enum_attribute(Attribute::get_named_enum_kind_id("nounwind"), 0));
        }
        self.externs.insert(name, f);
        self.mappings.push((name, addr));
    }

    fn declare_runtime(&mut self) {
        let f = self.cx.f64_type().as_basic_type_enum();
        let i = self.cx.i64_type().as_basic_type_enum();
        let i32t = self.cx.i32_type().as_basic_type_enum();
        let p = self.cx.ptr_type(AddressSpace::default()).as_basic_type_enum();
        self.declare("fm_error", None, &[p, i, f, f, i32t], rt::fm_error as *const () as usize, false);
        self.declare("fm_print_num", None, &[p, i, f], rt::fm_print_num as *const () as usize, false);
        self.declare("fm_print_bool", None, &[p, i32t], rt::fm_print_bool as *const () as usize, false);
        self.declare("fm_print_text", None, &[p, i], rt::fm_print_text as *const () as usize, false);
        self.declare("fm_print_list", None, &[p, i, p], rt::fm_print_list as *const () as usize, false);
        self.declare("fm_print_tlist", None, &[p, p], rt::fm_print_tlist as *const () as usize, false);
        self.declare("fm_print_vec", None, &[p, i, p, i], rt::fm_print_vec as *const () as usize, false);
        self.declare("fm_print_mvec", None, &[p, i, p, i], rt::fm_print_mvec as *const () as usize, false);
        self.declare("fm_print_mat", None, &[p, i, p, i, i], rt::fm_print_mat as *const () as usize, false);
        self.declare("fm_print_complex", None, &[p, i, f, f], rt::fm_print_complex as *const () as usize, false);
        self.declare("fm_print_end", None, &[p], rt::fm_print_end as *const () as usize, false);
        self.declare("fm_list_new", Some(p), &[p, i], rt::fm_list_new as *const () as usize, false);
        self.declare("fm_list_push", None, &[p, f], rt::fm_list_push as *const () as usize, false);
        self.declare("fm_list_extend", None, &[p, p], rt::fm_list_extend as *const () as usize, false);
        self.declare("fm_list_clear", None, &[p], rt::fm_list_clear as *const () as usize, false);
        self.declare("fm_list_copy", Some(p), &[p, p], rt::fm_list_copy as *const () as usize, false);
        self.declare("fm_list_binll", Some(p), &[p, i32t, p, p, i32t], rt::fm_list_binll as *const () as usize, false);
        self.declare("fm_list_binls", Some(p), &[p, i32t, p, f, i32t], rt::fm_list_binls as *const () as usize, false);
        self.declare("fm_list_powc", Some(p), &[p, p, f], rt::fm_list_powc as *const () as usize, false);
        self.declare("fm_list_neg", Some(p), &[p, p], rt::fm_list_neg as *const () as usize, false);
        self.declare("fm_tlist_push", None, &[p, i], rt::fm_tlist_push as *const () as usize, false);
        self.declare("fm_tlist_len", Some(i), &[p], rt::fm_tlist_len as *const () as usize, false);
        self.declare("fm_tlist_at", Some(i), &[p, i], rt::fm_tlist_at as *const () as usize, false);
        self.declare("fm_tlist_copy", Some(p), &[p, p], rt::fm_tlist_copy as *const () as usize, false);
        self.declare("fm_par_run", None, &[p, p, p, i, f, f, p, i], rt::fm_par_run as *const () as usize, false);
        self.declare("fm_quad", Some(f), &[p, p, p, f, f, f, f, i, i32t], rt::fm_quad as *const () as usize, false);
        self.declare("fm_root", Some(f), &[p, p, p, p, p, f, f, i, i32t], rt::fm_root as *const () as usize, false);
        {
            use crate::llvm::solve_rt as s;
            self.declare("fm_ode", Some(i), &[p, i, p, p, p, p, p, i, f, f, f, i32t], s::fm_ode as *const () as usize, false);
            self.declare("fm_eigen", Some(i), &[p, i, p, p, f, f, i32t], s::fm_eigen as *const () as usize, false);
            self.declare("fm_pde", Some(i), &[p, i, p, p, f, f, f, f, f, i32t], s::fm_pde as *const () as usize, false);
            self.declare("fm_sol_eval", Some(f), &[p, i, i, f, i32t, i, i32t], s::fm_sol_eval as *const () as usize, false);
            self.declare("fm_sol_eval_list", Some(p), &[p, i, i, p, i32t, i, i32t], s::fm_sol_eval_list as *const () as usize,
                         false);
            self.declare("fm_sol_list", Some(p), &[p, i, i, i], s::fm_sol_list as *const () as usize, false);
            self.declare("fm_sol_extreme", Some(f), &[p, i, i, f], s::fm_sol_extreme as *const () as usize, false);
            self.declare("fm_pde_eval", Some(f), &[p, i, i, i, f, f, i, i, i, i32t], s::fm_pde_eval as *const () as usize,
                         false);
            self.declare("fm_odelin", Some(i32t), &[i, p, p, p], s::fm_odelin as *const () as usize, false);
            self.declare("fm_solve_error", None, &[p, i, f, f, i, i32t], s::fm_solve_error as *const () as usize, false);
        }
        self.declare("fm_print_msum", None, &[p, i, i, p, i], rt::fm_print_msum as *const () as usize, false);
        self.declare("fm_quad_sf_clear", None, &[p], rt::fm_quad_sf_clear as *const () as usize, false);
        self.declare("fm_print_num_capped", None, &[p, i, f], rt::fm_print_num_capped as *const () as usize, false);
        self.declare("fm_builtin", None, &[p, i, p, p, i32t], rt::fm_builtin as *const () as usize, false);
        self.declare("fm_powf", Some(f), &[f, f], rt::fm_powf as *const () as usize, true);
        self.declare("fm_list_powf", Some(p), &[p, p, f], rt::fm_list_powf as *const () as usize, false);
        self.declare("fm_tlist_new", Some(p), &[p], rt::fm_tlist_new as *const () as usize, false);
        self.declare("fm_tlist_clear", None, &[p], rt::fm_tlist_clear as *const () as usize, false);
        self.declare("fm_powc", Some(f), &[f, f], rt::fm_powc as *const () as usize, true);
        self.declare("fm_atan2", Some(f), &[f, f], rt::fm_atan2 as *const () as usize, true);
        self.declare("fm_hypot", Some(f), &[f, f], rt::fm_hypot as *const () as usize, true);
        for (_, sym, addr) in rt::math1_shims() {
            self.declare(sym, Some(f), &[f], addr, true);
        }
    }

    fn ext(&self, name: &str) -> FunctionValue<'c> {
        self.externs[name]
    }

    // ------------------------------------------------------------ small helpers
    fn f64t(&self) -> inkwell::types::FloatType<'c> {
        self.cx.f64_type()
    }
    fn fconst(&self, x: f64) -> FloatValue<'c> {
        self.f64t().const_float(x)
    }
    fn nan(&self) -> FloatValue<'c> {
        self.fconst(f64::NAN)
    }
    fn i64c(&self, x: i64) -> IntValue<'c> {
        self.cx.i64_type().const_int(x as u64, true)
    }
    fn i32c(&self, x: i64) -> IntValue<'c> {
        self.cx.i32_type().const_int(x as u64, true)
    }
    fn ptrt(&self) -> inkwell::types::PointerType<'c> {
        self.cx.ptr_type(AddressSpace::default())
    }

    fn llty(&self, k: Kind) -> BasicTypeEnum<'c> {
        match k {
            Kind::F => self.f64t().into(),
            Kind::B => self.cx.bool_type().into(),
            Kind::S | Kind::H => self.cx.i64_type().into(),
            Kind::L | Kind::TL => self.ptrt().into(),
            Kind::V(n) => self.f64t().array_type(n as u32).into(),
            Kind::Void => self.cx.i8_type().into(),
        }
    }

    fn default_of(&self, k: Kind) -> Option<BasicValueEnum<'c>> {
        Some(match k {
            Kind::F => self.nan().into(),
            Kind::B => self.cx.bool_type().const_zero().into(),
            Kind::S | Kind::H => self.cx.i64_type().const_zero().into(),
            Kind::L | Kind::TL => self.ptrt().const_null().into(),
            Kind::V(n) => {
                let nan = self.nan();
                self.f64t().const_array(&vec![nan; n]).into()
            }
            Kind::Void => return None,
        })
    }

    fn call(&mut self, name: &str, args: &[BasicValueEnum<'c>]) -> R<Option<BasicValueEnum<'c>>> {
        let f = self.ext(name);
        let a: Vec<BasicMetadataValueEnum> = args.iter().map(|v| (*v).into()).collect();
        let cs = bl!(self.b.build_call(f, &a, ""));
        Ok(cs.try_as_basic_value().left())
    }

    fn fcall(&mut self, name: &str, args: &[BasicValueEnum<'c>]) -> R<FloatValue<'c>> {
        Ok(self.call(name, args)?.unwrap().into_float_value())
    }

    fn intrinsic(&mut self, name: &str, args: &[FloatValue<'c>]) -> R<FloatValue<'c>> {
        let i = Intrinsic::find(name).ok_or_else(|| format!("no intrinsic {name}"))?;
        let f = i.get_declaration(&self.lm, &[self.f64t().into()]).ok_or("intrinsic declaration")?;
        let a: Vec<BasicMetadataValueEnum> = args.iter().map(|v| (*v).into()).collect();
        Ok(bl!(self.b.build_call(f, &a, "")).try_as_basic_value().left().unwrap().into_float_value())
    }

    fn fnv(&self) -> FunctionValue<'c> {
        self.fnv.unwrap()
    }

    fn new_bb(&self, name: &str) -> BasicBlock<'c> {
        self.cx.append_basic_block(self.fnv(), name)
    }

    fn goto(&mut self, bb: BasicBlock<'c>) {
        self.b.position_at_end(bb);
        self.known_line = None;
    }

    fn terminated(&self) -> bool {
        self.b.get_insert_block().and_then(|b| b.get_terminator()).is_some()
    }

    /// A conditional branch whose `cold` side is rarely taken (errors).
    fn cold_br(&mut self, cond_cold: IntValue<'c>, cold: BasicBlock<'c>, hot: BasicBlock<'c>) -> R<()> {
        let br = bl!(self.b.build_conditional_branch(cond_cold, cold, hot));
        let kind = self.cx.get_kind_id("prof");
        let md = self.cx.metadata_node(&[self.cx.metadata_string("branch_weights").into(),
                                         self.cx.i32_type().const_int(1, false).into(),
                                         self.cx.i32_type().const_int(1 << 20, false).into()]);
        let _ = br.set_metadata(md, kind);
        Ok(())
    }

    fn alloca(&mut self, ty: BasicTypeEnum<'c>, name: &str) -> R<PointerValue<'c>> {
        let eb = self.entry_b.as_ref().unwrap();
        let entry = self.fnv().get_first_basic_block().unwrap();
        match entry.get_first_instruction() {
            Some(i) => eb.position_before(&i),
            None => eb.position_at_end(entry),
        }
        Ok(bl!(eb.build_alloca(ty, name)))
    }

    // ------------------------------------------------------------ type-based alias analysis
    /// The TBAA access tag of one kind of memory: "var" (variable slots), "elem" (list elements), "hdr" (list
    /// headers), "line" (fm.line), "flag" (the error flag), "env" (env arrays). They never overlap, so a store to
    /// a list element doesn't make LLVM reload variables or list lengths (loops keep them in registers).
    fn tbaa(&self, name: &str) -> inkwell::values::MetadataValue<'c> {
        let i64t = self.cx.i64_type();
        let root = self.cx.metadata_node(&[self.cx.metadata_string("fermium tbaa").into()]);
        let ty = self.cx.metadata_node(&[self.cx.metadata_string(name).into(), root.into(), i64t.const_zero().into()]);
        self.cx.metadata_node(&[ty.into(), ty.into(), i64t.const_zero().into()])
    }


    pub(super) fn ld(&self, ty: impl BasicType<'c>, p: PointerValue<'c>, name: &str, tag: &str) -> R<BasicValueEnum<'c>> {
        let v = bl!(self.b.build_load(ty, p, name));
        if let Some(i) = v.as_instruction_value() {
            let _ = i.set_metadata(self.tbaa(tag), self.cx.get_kind_id("tbaa"));
        }
        Ok(v)
    }

    pub(super) fn st(&self, p: PointerValue<'c>, v: impl BasicValue<'c>, tag: &str) -> R<()> {
        let i = bl!(self.b.build_store(p, v));
        let _ = i.set_metadata(self.tbaa(tag), self.cx.get_kind_id("tbaa"));
        Ok(())
    }

    // ------------------------------------------------------------ lines and errors
    fn set_line(&mut self, line: u32) -> R<()> {
        if line != 0 && self.known_line != Some(line) {
            self.st(self.line_g.as_pointer_value(), self.i32c(line as i64), "line")?;
            self.known_line = Some(line);
        }
        Ok(())
    }

    fn line_val(&mut self) -> R<IntValue<'c>> {
        Ok(self.ld(self.cx.i32_type(), self.line_g.as_pointer_value(), "line", "line")?.into_int_value())
    }

    /// Stop with a run-time error unless `ok`.
    fn guard(&mut self, ok: IntValue<'c>, kind: i64, a: FloatValue<'c>, b: FloatValue<'c>) -> R<()> {
        let fail = self.new_bb("fail");
        let cont = self.new_bb("ok");
        let notok = bl!(self.b.build_not(ok, "notok"));
        self.cold_br(notok, fail, cont)?;
        self.b.position_at_end(fail);
        let line = self.line_val()?;
        let ctx = self.ctx_ptr;
        self.call("fm_error", &[ctx.into(), self.i64c(kind).into(), a.into(), b.into(), line.into()])?;
        bl!(self.b.build_unconditional_branch(self.err_bb.unwrap()));
        self.b.position_at_end(cont);
        Ok(())
    }

    /// After a call that may have stopped the program: return if the context's error flag is set.
    fn check_err(&mut self) -> R<()> {
        let flag = self.ld(self.cx.i32_type(), self.ctx_ptr, "err", "flag")?.into_int_value();
        let bad = bl!(self.b.build_int_compare(IntPredicate::NE, flag, self.cx.i32_type().const_zero(), "bad"));
        let cont = self.new_bb("ok");
        self.cold_br(bad, self.err_bb.unwrap(), cont)?;
        self.b.position_at_end(cont);
        Ok(())
    }

    // ------------------------------------------------------------ variables
    fn slot(&mut self, sym: SymId) -> R<(PointerValue<'c>, Kind)> {
        if let Some(x) = self.overrides.get(&sym) {
            return Ok(*x);
        }
        let s = &self.m.syms[sym];
        if s.storage == Storage::Arena {
            return Err("REPL arena variables aren't compiled".into());
        }
        let k = kind_of(&s.ty)?;
        if k == Kind::Void {
            return Err(format!("variable {} has no value type", s.name));
        }
        if s.func.is_none() {
            if let Some(x) = self.globals.get(&sym) {
                return Ok(*x);
            }
            let ty = self.llty(k);
            let g = self.lm.add_global(ty, None, &format!("g{}.{}", sym, s.name));
            g.set_linkage(Linkage::Internal);
            let init = match k {
                Kind::F => self.fconst(0.0).as_basic_value_enum(),
                _ => ty.const_zero(),
            };
            g.set_initializer(&init);
            let r = (g.as_pointer_value(), k);
            self.globals.insert(sym, r);
            return Ok(r);
        }
        if let Some(x) = self.locals.get(&sym) {
            return Ok(*x);
        }
        let ty = self.llty(k);
        let name = s.name.clone();
        let p = self.alloca(ty, &name)?;
        // a fresh local starts as 0 / null, like a zeroed frame slot
        let eb = self.entry_b.as_ref().unwrap();
        bl!(eb.build_store(p, match k {
            Kind::F => self.fconst(0.0).as_basic_value_enum(),
            _ => ty.const_zero(),
        }));
        self.locals.insert(sym, (p, k));
        Ok((p, k))
    }

    fn load_var(&mut self, sym: SymId) -> R<Val<'c>> {
        let (p, k) = self.slot(sym)?;
        let v = self.ld(self.llty(k), p, &self.m.syms[sym].name, "var")?;
        Ok(Val { k, v: Some(v) })
    }

    fn store_var(&mut self, sym: SymId, v: Val<'c>) -> R<()> {
        let (p, k) = self.slot(sym)?;
        let v = self.coerce(v, k)?;
        if let Some(x) = v.v {
            self.st(p, x, "var")?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ value conversions (Value::num / truth)
    fn to_f(&mut self, v: Val<'c>) -> R<FloatValue<'c>> {
        Ok(match v.k {
            Kind::F => v.v.unwrap().into_float_value(),
            Kind::B => bl!(self.b.build_unsigned_int_to_float(v.v.unwrap().into_int_value(), self.f64t(), "b2f")),
            _ => self.nan(),
        })
    }

    fn truth(&mut self, v: Val<'c>) -> R<IntValue<'c>> {
        Ok(match v.k {
            Kind::B => v.v.unwrap().into_int_value(),
            Kind::F => bl!(self.b.build_float_compare(FloatPredicate::UNE, v.v.unwrap().into_float_value(),
                                                      self.fconst(0.0), "truth")),
            _ => self.cx.bool_type().const_zero(),
        })
    }

    fn coerce(&mut self, v: Val<'c>, k: Kind) -> R<Val<'c>> {
        if v.k == k {
            return Ok(v);
        }
        Ok(match k {
            Kind::F => Val { k, v: Some(self.to_f(v)?.into()) },
            Kind::B => Val { k, v: Some(self.truth(v)?.into()) },
            _ => return Err(format!("can't use a {:?} value as {:?}", v.k, k)),
        })
    }


    fn vec_elems(&mut self, v: Val<'c>) -> R<Vec<FloatValue<'c>>> {
        let Kind::V(n) = v.k else { return Err("not a vector".into()) };
        let a = v.v.unwrap().into_array_value();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(bl!(self.b.build_extract_value(a, i as u32, "e")).into_float_value());
        }
        Ok(out)
    }

    fn make_vec(&mut self, xs: &[FloatValue<'c>]) -> R<Val<'c>> {
        let ty = self.f64t().array_type(xs.len() as u32);
        let mut a = ty.get_undef();
        for (i, x) in xs.iter().enumerate() {
            a = bl!(self.b.build_insert_value(a, *x, i as u32, "v")).into_array_value();
        }
        Ok(Val { k: Kind::V(xs.len()), v: Some(a.into()) })
    }

    /// A vector stored in a stack slot, for callbacks that take a pointer.
    fn spill(&mut self, v: Val<'c>) -> R<PointerValue<'c>> {
        let ty = self.llty(v.k);
        let p = self.alloca(ty, "spill")?;
        bl!(self.b.build_store(p, v.v.unwrap()));
        Ok(p)
    }

    // ------------------------------------------------------------ IEEE helpers, exactly as eval.rs
    /// eval::fdiv: x/0 is ±∞, 0/0 and NaN/0 are NaN (f64::NAN), else a / b.
    fn fdiv(&mut self, a: FloatValue<'c>, b: FloatValue<'c>) -> R<FloatValue<'c>> {
        let q = bl!(self.b.build_float_div(a, b, "q"));
        let zero = self.fconst(0.0);
        let bz = bl!(self.b.build_float_compare(FloatPredicate::OEQ, b, zero, "bz"));
        let az = bl!(self.b.build_float_compare(FloatPredicate::UEQ, a, zero, "az")); // a == 0 or NaN
        let inf = self.fconst(f64::INFINITY);
        let one = self.fconst(1.0);
        let sa = self.intrinsic("llvm.copysign", &[inf, a])?;
        let sb = self.intrinsic("llvm.copysign", &[one, b])?;
        let signed = bl!(self.b.build_float_mul(sa, sb, "inf"));
        let nan = self.nan();
        let special = bl!(self.b.build_select(az, nan, signed, "sp")).into_float_value();
        Ok(bl!(self.b.build_select(bz, special, q, "fdiv")).into_float_value())
    }

    fn sqrt(&mut self, x: FloatValue<'c>) -> R<FloatValue<'c>> {
        self.intrinsic("llvm.sqrt", &[x])
    }

    fn ge0(&mut self, x: FloatValue<'c>) -> R<IntValue<'c>> {
        Ok(bl!(self.b.build_float_compare(FloatPredicate::OGE, x, self.fconst(0.0), "ge0")))
    }

    /// eval::powc for a compile-time constant p.
    fn powc(&mut self, x: FloatValue<'c>, p: f64) -> R<FloatValue<'c>> {
        let b = &self.b;
        if p == 2.0 {
            return Ok(bl!(b.build_float_mul(x, x, "sq")));
        }
        if p == 3.0 {
            let x2 = bl!(b.build_float_mul(x, x, "sq"));
            return Ok(bl!(b.build_float_mul(x2, x, "cube")));
        }
        if p == 1.0 {
            return Ok(x);
        }
        let nan = self.nan();
        if p == 0.5 {
            let s = self.sqrt(x)?;
            let ge = self.ge0(x)?;
            let isnan = bl!(self.b.build_float_compare(FloatPredicate::UNO, x, x, "isnan"));
            let neg = bl!(self.b.build_select(isnan, x, nan, "neg")).into_float_value();
            return Ok(bl!(self.b.build_select(ge, s, neg, "sqrt")).into_float_value());
        }
        if p == -1.0 {
            return self.fdiv(self.fconst(1.0), x);
        }
        if p == -2.0 {
            let x2 = bl!(self.b.build_float_mul(x, x, "sq"));
            return self.fdiv(self.fconst(1.0), x2);
        }
        if p == 4.0 {
            let x2 = bl!(self.b.build_float_mul(x, x, "sq"));
            return Ok(bl!(self.b.build_float_mul(x2, x2, "p4")));
        }
        if p == -0.5 || p == 1.5 || p == -1.5 {
            let s = self.sqrt(x)?;
            let r = if p == -0.5 {
                self.fdiv(self.fconst(1.0), s)?
            } else {
                let xs = bl!(self.b.build_float_mul(x, s, "xs"));
                if p == 1.5 {
                    xs
                } else {
                    self.fdiv(self.fconst(1.0), xs)?
                }
            };
            let ge = self.ge0(x)?;
            return Ok(bl!(self.b.build_select(ge, r, nan, "powc")).into_float_value());
        }
        let pc = self.fconst(p);
        self.fcall("fm_powc", &[x.into(), pc.into()])
    }

    fn arith(&mut self, op: BinOp, x: FloatValue<'c>, y: FloatValue<'c>) -> R<FloatValue<'c>> {
        Ok(match op {
            BinOp::Add => bl!(self.b.build_float_add(x, y, "add")),
            BinOp::Sub => bl!(self.b.build_float_sub(x, y, "sub")),
            BinOp::Mul => bl!(self.b.build_float_mul(x, y, "mul")),
            BinOp::Div => self.fdiv(x, y)?,
        })
    }

    // ------------------------------------------------------------ module
    pub fn compile_module(&mut self) -> R<()> {
        // uncertain values (±, propagate montecarlo) run in the tree-walker, as v1 runs them in its interpreter
        // (D122): every value may carry an uncertainty there, which the compiled code's f64 can't
        if self.m.uses_uncertainty {
            return Err("uncertain values (±) aren't compiled".into());
        }
        let m = self.m;
        for (fid, f) in m.funcs.iter().enumerate() {
            let mut params: Vec<BasicMetadataTypeEnum> = vec![];
            for p in &f.params {
                let k = kind_of(&m.syms[*p].ty)?;
                if k == Kind::Void {
                    return Err("a parameter without a value type".into());
                }
                params.push(self.llty(k).into());
            }
            let rk = kind_of(&f.ret_ty)?;
            let ft = match rk {
                Kind::Void => self.cx.void_type().fn_type(&params, false),
                k => self.llty(k).fn_type(&params, false),
            };
            let fv = self.lm.add_function(&format!("f{}.{}", fid, f.name), ft, Some(Linkage::Internal));
            self.funcs.push(fv);
        }
        for fid in 0..m.funcs.len() {
            self.compile_func(fid)?;
        }
        self.compile_main()
    }

    fn begin_fn(&mut self, fv: FunctionValue<'c>, ret: Kind) {
        self.fnv = Some(fv);
        let entry = self.cx.append_basic_block(fv, "entry");
        let eb = self.cx.create_builder();
        eb.position_at_end(entry);
        self.entry_b = Some(eb);
        self.locals.clear();
        self.overrides.clear();
        self.loops.clear();
        self.ret_kind = ret;
        let err = self.cx.append_basic_block(fv, "err");
        self.err_bb = Some(err);
        self.b.position_at_end(entry);
        self.known_line = None;
        if let Some(g) = self.aot {
            let p = bl_unwrap(self.b.build_load(self.ptrt(), g.as_pointer_value(), "ctx")).into_pointer_value();
            self.ctx_ptr = p;
        }
    }

    fn ret_default(&mut self) -> R<()> {
        match self.default_of(self.ret_kind) {
            Some(v) => bl!(self.b.build_return(Some(&v))),
            None => bl!(self.b.build_return(None)),
        };
        Ok(())
    }

    fn finish_fn(&mut self) -> R<()> {
        if !self.terminated() {
            self.ret_default()?;
        }
        self.b.position_at_end(self.err_bb.unwrap());
        self.ret_default()
    }

    fn frame_addr(&mut self) -> R<IntValue<'c>> {
        let i = Intrinsic::find("llvm.frameaddress").ok_or("no llvm.frameaddress")?;
        let f = i.get_declaration(&self.lm, &[self.ptrt().into()]).ok_or("frameaddress declaration")?;
        let p = bl!(self.b.build_call(f, &[self.cx.i32_type().const_zero().into()], "fp"))
            .try_as_basic_value().left().unwrap().into_pointer_value();
        Ok(bl!(self.b.build_ptr_to_int(p, self.cx.i64_type(), "sp")))
    }

    fn compile_func(&mut self, fid: usize) -> R<()> {
        let f = &self.m.funcs[fid];
        let fv = self.funcs[fid];
        self.begin_fn(fv, kind_of(&f.ret_ty)?);
        // runaway recursion: a clear error before the stack overflows (v1 stack_check; off while stackbase is 0)
        let sp = self.frame_addr()?;
        let base = bl!(self.b.build_load(self.cx.i64_type(), self.stackbase_g.as_pointer_value(), "base")).into_int_value();
        let used = bl!(self.b.build_int_sub(base, sp, "used"));
        let ok = bl!(self.b.build_int_compare(IntPredicate::SLE, used, self.i64c(STACK_LIMIT as i64), "stackok"));
        let (fa, zero) = (self.fconst(fid as f64), self.fconst(0.0));
        self.guard(ok, rt::E_DEEP, fa, zero)?;
        for (i, p) in f.params.iter().enumerate() {
            let k = kind_of(&self.m.syms[*p].ty)?;
            let v = fv.get_nth_param(i as u32).unwrap();
            self.store_var_local(*p, Val { k, v: Some(v) })?;
        }
        let body = &f.body;
        self.block(body)?;
        self.finish_fn()
    }

    /// Parameters are always locals of the function (even when the checker's owner says otherwise).
    fn store_var_local(&mut self, sym: SymId, v: Val<'c>) -> R<()> {
        if self.m.syms[sym].func.is_none() {
            return Err(format!("parameter {} isn't a local", self.m.syms[sym].name));
        }
        self.store_var(sym, v)
    }

    fn compile_main(&mut self) -> R<()> {
        let fv = self.lm.add_function("fm_main", self.cx.void_type().fn_type(&[], false), Some(Linkage::External));
        self.begin_fn(fv, Kind::Void);
        let sp = self.frame_addr()?;
        bl!(self.b.build_store(self.stackbase_g.as_pointer_value(), sp));
        let main = &self.m.main;
        self.block(main)?;
        self.finish_fn()
    }

    // ------------------------------------------------------------ statements
    fn block(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts {
            if self.terminated() {
                break;
            }
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        self.set_line(s.line)?;
        match &s.kind {
            StmtKind::Assign(sym, e) => {
                let v = self.expr(e)?;
                self.store_var(*sym, v)?;
            }
            StmtKind::Expr(e) => {
                self.expr(e)?;
            }
            StmtKind::IndexAssign(sym, idx, value) => {
                let (p, k) = self.slot(*sym)?;
                if k != Kind::L {
                    return Err("index assignment to a non-list".into());
                }
                let l = self.ld(self.ptrt(), p, "l", "var")?.into_pointer_value();
                let iv = self.expr(idx)?;
                let i = self.to_f(iv)?;
                let ep = self.list_elem_ptr(l, i)?;
                let vv = self.expr(value)?;
                let v = self.to_f(vv)?;
                self.st(ep, v, "elem")?;
            }
            StmtKind::Push(sym, e) => {
                let v = self.expr(e)?;
                let (p, k) = self.slot(*sym)?;
                let l = self.ld(self.ptrt(), p, "l", "var")?;
                match (k, v.k) {
                    (Kind::L, Kind::F) => {
                        self.call("fm_list_push", &[l, v.v.unwrap()])?;
                    }
                    (Kind::L, Kind::L) => {
                        self.call("fm_list_extend", &[l, v.v.unwrap()])?;
                    }
                    (Kind::TL, Kind::S) => {
                        self.call("fm_tlist_push", &[l, v.v.unwrap()])?;
                    }
                    _ => return Err("push of this kind of value isn't compiled".into()),
                }
            }
            StmtKind::Clear(sym) => {
                let (p, k) = self.slot(*sym)?;
                let l = self.ld(self.ptrt(), p, "l", "var")?;
                match k {
                    Kind::L => self.call("fm_list_clear", &[l])?,
                    Kind::TL => self.call("fm_tlist_clear", &[l])?,
                    _ => return Err("clear of this kind of value isn't compiled".into()),
                };
            }
            StmtKind::If(c, then, other) => {
                let cv = self.expr(c)?;
                let t = self.truth(cv)?;
                let (tb, eb, join) = (self.new_bb("then"), self.new_bb("else"), self.new_bb("endif"));
                bl!(self.b.build_conditional_branch(t, tb, eb));
                self.goto(tb);
                self.block(then)?;
                if !self.terminated() {
                    bl!(self.b.build_unconditional_branch(join));
                }
                self.goto(eb);
                self.block(other)?;
                if !self.terminated() {
                    bl!(self.b.build_unconditional_branch(join));
                }
                self.goto(join);
            }
            StmtKind::While(c, body) => {
                let (head, bodyb, exit) = (self.new_bb("while"), self.new_bb("wbody"), self.new_bb("wend"));
                bl!(self.b.build_unconditional_branch(head));
                self.goto(head);
                let cv = self.expr(c)?;
                let t = self.truth(cv)?;
                bl!(self.b.build_conditional_branch(t, bodyb, exit));
                self.goto(bodyb);
                self.loops.push((head, exit));
                self.block(body)?;
                self.loops.pop();
                if !self.terminated() {
                    bl!(self.b.build_unconditional_branch(head));
                }
                self.goto(exit);
            }
            StmtKind::For { sym, lo, hi, step, body, par, .. } => {
                if let Some(info) = par {
                    return self.parallel_for(*sym, lo, hi, step.as_ref(), body, info);
                }
                self.for_range(*sym, lo, hi, step.as_ref(), body)?;
            }
            StmtKind::ForIn(sym, lst, body) => self.for_in(*sym, lst, body)?,
            StmtKind::Print(items) => self.print(items)?,
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                // a function returns its value (no conversion: the checker made it the function's type)
                match (self.ret_kind, v) {
                    (Kind::Void, _) => bl!(self.b.build_return(None)),
                    (k, Some(v)) => {
                        let v = self.coerce(v, k)?;
                        bl!(self.b.build_return(Some(&v.v.unwrap())))
                    }
                    (k, None) => {
                        let d = self.default_of(k).unwrap();
                        bl!(self.b.build_return(Some(&d)))
                    }
                };
            }
            StmtKind::Break => {
                let (_, exit) = *self.loops.last().ok_or("break outside a loop")?;
                bl!(self.b.build_unconditional_branch(exit));
            }
            StmtKind::Continue => {
                let (next, _) = *self.loops.last().ok_or("continue outside a loop")?;
                bl!(self.b.build_unconditional_branch(next));
            }
            StmtKind::Assert(c, msg) => {
                let cv = self.expr(c)?;
                let t = self.truth(cv)?;
                let (a, z) = (self.fconst(*msg as f64), self.fconst(0.0));
                self.guard(t, rt::E_ASSERT, a, z)?;
            }
            StmtKind::Plot(..) => return Err("plot isn't compiled yet".into()),
            StmtKind::Solve { .. } => self.solve_stmt(s)?,
            StmtKind::Fit { .. } => return Err("fit isn't compiled yet".into()),
            StmtKind::Animate { .. } => return Err("animate isn't compiled yet".into()),
            StmtKind::Propagate { .. } => return Err("propagate isn't compiled yet".into()),
        }
        Ok(())
    }

    /// `for sym from lo to hi step st`: inclusive, lo + i·st, as eval.rs (n = ⌊(hi − lo)/st + 1e-9⌋ + 1 when
    /// finite and ≥ 0, else no iterations; step 0 is an error).
    fn for_range(&mut self, sym: SymId, lo: &Expr, hi: &Expr, step: Option<&Expr>, body: &[Stmt]) -> R<()> {
        let (lo, st, count) = self.range_count(lo, hi, step)?;
        self.counted_loop(count, body, |g, i| {
            let fi = bl!(g.b.build_signed_int_to_float(i, g.f64t(), "fi"));
            let x = bl!(g.b.build_float_mul(fi, st, "ist"));
            let x = bl!(g.b.build_float_add(lo, x, "x"));
            g.store_var(sym, fv(x))
        })
    }

    /// (lo, step, number of iterations) of `for … from lo to hi step st`, with its run-time errors.
    fn range_count(&mut self, lo: &Expr, hi: &Expr, step: Option<&Expr>)
                   -> R<(FloatValue<'c>, FloatValue<'c>, IntValue<'c>)> {
        let lov = self.expr(lo)?;
        let lo = self.to_f(lov)?;
        let hiv = self.expr(hi)?;
        let hi = self.to_f(hiv)?;
        let st = match step {
            Some(e) => {
                let v = self.expr(e)?;
                self.to_f(v)?
            }
            None => self.fconst(1.0),
        };
        // eval.rs for_count: a zero or NaN step and a NaN count are errors, a negative count is 0, at most 2⁶²
        let nz = bl!(self.b.build_float_compare(FloatPredicate::ONE, st, self.fconst(0.0), "stepnz"));
        self.guard(nz, rt::E_STEP0, lo, hi)?;
        let d = bl!(self.b.build_float_sub(hi, lo, "d"));
        let q = bl!(self.b.build_float_div(d, st, "q"));
        let q = bl!(self.b.build_float_add(q, self.fconst(1e-9), "q"));
        let n = self.intrinsic("llvm.floor", &[q])?;
        let n = bl!(self.b.build_float_add(n, self.fconst(1.0), "n1"));
        let neg = bl!(self.b.build_float_compare(FloatPredicate::OLT, n, self.fconst(0.0), "neg"));
        let n = bl!(self.b.build_select(neg, self.fconst(0.0), n, "n0")).into_float_value();
        let known = bl!(self.b.build_float_compare(FloatPredicate::ORD, n, n, "known"));
        self.guard(known, rt::E_RANGE, lo, hi)?;
        let cap = self.fconst(2f64.powi(62));
        let big = bl!(self.b.build_float_compare(FloatPredicate::OGT, n, cap, "big"));
        let n = bl!(self.b.build_select(big, cap, n, "ncap")).into_float_value();
        let count = self.fptosi_sat(n)?;
        Ok((lo, st, count))
    }

    fn fptosi_sat(&mut self, x: FloatValue<'c>) -> R<IntValue<'c>> {
        let i = Intrinsic::find("llvm.fptosi.sat").ok_or("no fptosi.sat")?;
        let f = i.get_declaration(&self.lm, &[self.cx.i64_type().into(), self.f64t().into()]).ok_or("fptosi.sat")?;
        Ok(bl!(self.b.build_call(f, &[x.into()], "sat")).try_as_basic_value().left().unwrap().into_int_value())
    }

    /// for i in 0..count { set(i); body } with break / continue.
    fn counted_loop(&mut self, count: IntValue<'c>, body: &[Stmt],
                    set: impl Fn(&mut Self, IntValue<'c>) -> R<()>) -> R<()> {
        let i64t = self.cx.i64_type();
        let ip = self.alloca(i64t.into(), "i")?;
        bl!(self.b.build_store(ip, i64t.const_zero()));
        let (head, bodyb, next, exit) = (self.new_bb("for"), self.new_bb("fbody"), self.new_bb("fnext"), self.new_bb("fend"));
        bl!(self.b.build_unconditional_branch(head));
        self.goto(head);
        let i = bl!(self.b.build_load(i64t, ip, "i")).into_int_value();
        let more = bl!(self.b.build_int_compare(IntPredicate::SLT, i, count, "more"));
        bl!(self.b.build_conditional_branch(more, bodyb, exit));
        self.goto(bodyb);
        set(self, i)?;
        self.loops.push((next, exit));
        self.block(body)?;
        self.loops.pop();
        if !self.terminated() {
            bl!(self.b.build_unconditional_branch(next));
        }
        self.goto(next);
        let i = bl!(self.b.build_load(i64t, ip, "i")).into_int_value();
        let i1 = bl!(self.b.build_int_add(i, i64t.const_int(1, false), "i1"));
        bl!(self.b.build_store(ip, i1));
        bl!(self.b.build_unconditional_branch(head));
        self.goto(exit);
        Ok(())
    }

    /// `for x in list`: over a copy taken at the start (the tree-walker iterates a snapshot).
    fn for_in(&mut self, sym: SymId, lst: &Expr, body: &[Stmt]) -> R<()> {
        let v = self.expr(lst)?;
        let ctx = self.ctx_ptr;
        match v.k {
            Kind::L => {
                let cp = self.call("fm_list_copy", &[ctx.into(), v.v.unwrap()])?.unwrap().into_pointer_value();
                let (data, len) = self.list_parts(cp)?;
                self.counted_loop(len, body, |g, i| {
                    let ep = unsafe { bl!(g.b.build_gep(g.f64t(), data, &[i], "ep")) };
                    let x = g.ld(g.f64t(), ep, "x", "elem")?.into_float_value();
                    g.store_var(sym, fv(x))
                })
            }
            Kind::TL => {
                let cp = self.call("fm_tlist_copy", &[ctx.into(), v.v.unwrap()])?.unwrap();
                let len = self.call("fm_tlist_len", &[cp])?.unwrap().into_int_value();
                self.counted_loop(len, body, |g, i| {
                    let id = g.call("fm_tlist_at", &[cp, i.into()])?.unwrap();
                    g.store_var(sym, Val { k: Kind::S, v: Some(id) })
                })
            }
            Kind::V(n) => {
                let p = self.spill(v)?;
                let ty = self.f64t().array_type(n as u32);
                self.counted_loop(self.i64c(n as i64), body, |g, i| {
                    let ep = unsafe { bl!(g.b.build_gep(ty, p, &[g.i64c(0), i], "ep")) };
                    let x = bl!(g.b.build_load(g.f64t(), ep, "x")).into_float_value();
                    g.store_var(sym, fv(x))
                })
            }
            _ => Err("for … in over this kind of value isn't compiled".into()),
        }
    }

    fn list_parts(&mut self, l: PointerValue<'c>) -> R<(PointerValue<'c>, IntValue<'c>)> {
        let data = self.ld(self.ptrt(), l, "data", "hdr")?.into_pointer_value();
        let lenp = unsafe { bl!(self.b.build_gep(self.cx.i8_type(), l, &[self.i64c(8)], "lenp")) };
        let len = self.ld(self.cx.i64_type(), lenp, "len", "hdr")?.into_int_value();
        Ok((data, len))
    }

    /// The address of element i (1-based, a float) of a list, with the tree-walker's index check.
    fn list_elem_ptr(&mut self, l: PointerValue<'c>, i: FloatValue<'c>) -> R<PointerValue<'c>> {
        let (data, len) = self.list_parts(l)?;
        let k = self.index_check(i, len)?;
        Ok(unsafe { bl!(self.b.build_gep(self.f64t(), data, &[k], "ep")) })
    }

    /// Check a 1-based index against a length (eval list_index); the 0-based position.
    fn index_check(&mut self, i: FloatValue<'c>, len: IntValue<'c>) -> R<IntValue<'c>> {
        // valid iff 1 <= i <= n and i is whole (eval elem_index), computed on integers: k = the saturated
        // conversion of i is exact only for a whole i (NaN gives 0), and 1 <= k <= n is one unsigned compare
        let k = self.fptosi_sat(i)?;
        let back = bl!(self.b.build_signed_int_to_float(k, self.f64t(), "back"));
        let whole = bl!(self.b.build_float_compare(FloatPredicate::OEQ, back, i, "whole"));
        let k0 = bl!(self.b.build_int_sub(k, self.i64c(1), "k0"));
        let inside = bl!(self.b.build_int_compare(IntPredicate::ULT, k0, len, "inside"));
        let ok = bl!(self.b.build_and(whole, inside, "ok"));
        let fail = self.new_bb("fail");
        let cont = self.new_bb("ok");
        let notok = bl!(self.b.build_not(ok, "notok"));
        self.cold_br(notok, fail, cont)?;
        self.b.position_at_end(fail);
        let n = bl!(self.b.build_unsigned_int_to_float(len, self.f64t(), "n"));
        let line = self.line_val()?;
        let ctx = self.ctx_ptr;
        self.call("fm_error", &[ctx.into(), self.i64c(rt::E_INDEX).into(), i.into(), n.into(), line.into()])?;
        bl!(self.b.build_unconditional_branch(self.err_bb.unwrap()));
        self.b.position_at_end(cont);
        Ok(k0)
    }

    fn print(&mut self, items: &[PrintItem]) -> R<()> {
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        for it in items {
            match it {
                PrintItem::Num(e, f) if crate::eval_calc::measured_sum(e) => {
                    // the decimal-place rule (spec B2, eval_calc sum_sf): the operands here, the rule at run time
                    let mut vals = vec![];
                    let node = self.msum_operands(e, &mut vals)?;
                    self.tables.msum_sites.push(node);
                    let site = self.tables.msum_sites.len() as i64 - 1;
                    let arr = self.f64t().array_type(vals.len().max(1) as u32);
                    let p = self.alloca(arr.into(), "msum")?;
                    for (i, v) in vals.iter().enumerate() {
                        let at = unsafe { bl!(self.b.build_gep(arr, p, &[self.i64c(0), self.i64c(i as i64)], "m")) };
                        bl!(self.b.build_store(at, *v));
                    }
                    self.call("fm_print_msum", &[ctx, self.i64c(*f as i64).into(), self.i64c(site).into(), p.into(),
                                                 self.i64c(vals.len() as i64).into()])?;
                }
                PrintItem::Num(e, f) if crate::eval_calc::integral_shaped(e) => {
                    // capped at the figures its integrals support (eval.rs print, take_quad_sf)
                    self.call("fm_quad_sf_clear", &[ctx])?;
                    let v = self.expr(e)?;
                    let x = self.to_f(v)?;
                    self.call("fm_print_num_capped", &[ctx, self.i64c(*f as i64).into(), x.into()])?;
                }
                PrintItem::Num(e, f) => {
                    let v = self.expr(e)?;
                    let x = self.to_f(v)?;
                    self.call("fm_print_num", &[ctx, self.i64c(*f as i64).into(), x.into()])?;
                }
                PrintItem::List(e, f) => {
                    let v = self.expr(e)?;
                    if v.k != Kind::L {
                        return Err("print of a list that isn't one".into());
                    }
                    self.call("fm_print_list", &[ctx, self.i64c(*f as i64).into(), v.v.unwrap()])?;
                }
                PrintItem::Vec(e, f) => {
                    let v = self.expr(e)?;
                    let Kind::V(n) = v.k else { return Err("print of a vector that isn't one".into()) };
                    let p = self.spill(v)?;
                    self.call("fm_print_vec", &[ctx, self.i64c(*f as i64).into(), p.into(), self.i64c(n as i64).into()])?;
                }
                PrintItem::MixedVec(e, fs) => {
                    let v = self.expr(e)?;
                    let Kind::V(n) = v.k else { return Err("print of a vector that isn't one".into()) };
                    let p = self.spill(v)?;
                    self.tables.mvec_fmts.push(fs.clone());
                    let site = self.tables.mvec_fmts.len() as i64 - 1;
                    self.call("fm_print_mvec", &[ctx, self.i64c(site).into(), p.into(), self.i64c(n as i64).into()])?;
                }
                PrintItem::Mat(e, f) => {
                    let (r, c) = match &e.ty {
                        Ty::Mat { r, c, .. } => (*r, *c),
                        _ => return Err("print of a matrix that isn't one".into()),
                    };
                    let v = self.expr(e)?;
                    if v.k != Kind::V(r * c) {
                        return Err("print of a matrix that isn't one".into());
                    }
                    let p = self.spill(v)?;
                    self.call("fm_print_mat", &[ctx, self.i64c(*f as i64).into(), p.into(), self.i64c(r as i64).into(),
                                                self.i64c(c as i64).into()])?;
                }
                PrintItem::Complex(e, f) => {
                    let v = self.expr(e)?;
                    if v.k != Kind::V(2) {
                        return Err("print of a complex number that isn't one".into());
                    }
                    let xs = self.vec_elems(v)?;
                    self.call("fm_print_complex", &[ctx, self.i64c(*f as i64).into(), xs[0].into(), xs[1].into()])?;
                }
                PrintItem::Bool(e) => {
                    let v = self.expr(e)?;
                    let t = self.truth(v)?;
                    let t = bl!(self.b.build_int_z_extend(t, self.cx.i32_type(), "b"));
                    self.call("fm_print_bool", &[ctx, t.into()])?;
                }
                PrintItem::Text(i) | PrintItem::Data(_, i) => {
                    self.call("fm_print_text", &[ctx, self.i64c(*i as i64).into()])?;
                }
                PrintItem::TextVar(e) => {
                    let v = self.expr(e)?;
                    if v.k != Kind::S {
                        return Err("print of a text that isn't one".into());
                    }
                    self.call("fm_print_text", &[ctx, v.v.unwrap()])?;
                }
                PrintItem::TextList(e) => {
                    let v = self.expr(e)?;
                    if v.k != Kind::TL {
                        return Err("print of a text list that isn't one".into());
                    }
                    self.call("fm_print_tlist", &[ctx, v.v.unwrap()])?;
                }
                PrintItem::ComplexList(..) => return Err("lists of complex numbers aren't compiled yet".into()),
            }
        }
        self.call("fm_print_end", &[ctx])?;
        Ok(())
    }

    /// The operands of a printed measured sum, compiled left to right (eval_calc sum_place evaluates only them).
    fn msum_operands(&mut self, e: &Expr, vals: &mut Vec<FloatValue<'c>>) -> R<rt::MNode> {
        if let ExprKind::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) = &e.kind {
            if matches!(e.ty, Ty::Num(_)) {
                let na = self.msum_operands(a, vals)?;
                let nb = self.msum_operands(b, vals)?;
                return Ok(rt::MNode::Op(*op == BinOp::Add, Box::new(na), Box::new(nb)));
            }
        }
        let v = self.expr(e)?;
        vals.push(self.to_f(v)?);
        Ok(rt::MNode::Leaf(e.sf))
    }

    // ------------------------------------------------------------ expressions
    pub fn expr(&mut self, e: &Expr) -> R<Val<'c>> {
        self.set_line(e.line)?;
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        Ok(match &e.kind {
            ExprKind::Const(x) => fv(self.fconst(*x)),
            ExprKind::Bool(b) => bv(self.cx.bool_type().const_int(u64::from(*b), false)),
            ExprKind::Str(s) => {
                let id = self.intern(s);
                Val { k: Kind::S, v: Some(self.i64c(id).into()) }
            }
            ExprKind::Var(sym) => self.load_var(*sym)?,
            ExprKind::Bin(op, a, b) => {
                let va = self.expr(a)?;
                let vb = self.expr(b)?;
                self.bin(*op, va, vb)?
            }
            ExprKind::PowC(a, p) => {
                let v = self.expr(a)?;
                match v.k {
                    Kind::F => {
                        let x = v.v.unwrap().into_float_value();
                        fv(self.powc(x, *p)?)
                    }
                    Kind::L => {
                        let pc = self.fconst(*p);
                        let r = self.call("fm_list_powc", &[ctx, v.v.unwrap(), pc.into()])?;
                        Val { k: Kind::L, v: r }
                    }
                    _ => return Err("** of this kind of value isn't compiled".into()),
                }
            }
            ExprKind::Pow(a, b) => {
                let va = self.expr(a)?;
                let vb = self.expr(b)?;
                let y = self.to_f(vb)?;
                match va.k {
                    Kind::F => {
                        let x = va.v.unwrap().into_float_value();
                        fv(self.fcall("fm_powf", &[x.into(), y.into()])?)
                    }
                    Kind::L => Val { k: Kind::L, v: self.call("fm_list_powf", &[ctx, va.v.unwrap(), y.into()])? },
                    _ => return Err("** of this kind of value isn't compiled".into()),
                }
            }
            ExprKind::Neg(a) => {
                let v = self.expr(a)?;
                match v.k {
                    Kind::F => fv(bl!(self.b.build_float_neg(v.v.unwrap().into_float_value(), "neg"))),
                    Kind::L => Val { k: Kind::L, v: self.call("fm_list_neg", &[ctx, v.v.unwrap()])? },
                    Kind::V(_) => {
                        let xs = self.vec_elems(v)?;
                        let mut out = vec![];
                        for x in xs {
                            out.push(bl!(self.b.build_float_neg(x, "neg")));
                        }
                        self.make_vec(&out)?
                    }
                    Kind::B => v,
                    _ => return Err("negation of this kind of value isn't compiled".into()),
                }
            }
            ExprKind::Cmp(op, a, b) => {
                let va = self.expr(a)?;
                let vb = self.expr(b)?;
                if !matches!(va.k, Kind::F | Kind::B) || !matches!(vb.k, Kind::F | Kind::B) {
                    return Err("comparison of non-numbers isn't compiled".into());
                }
                let x = self.to_f(va)?;
                let y = self.to_f(vb)?;
                let pred = match op {
                    CmpOp::Eq => FloatPredicate::OEQ,
                    CmpOp::Ne => FloatPredicate::UNE,
                    CmpOp::Lt => FloatPredicate::OLT,
                    CmpOp::Gt => FloatPredicate::OGT,
                    CmpOp::Le => FloatPredicate::OLE,
                    CmpOp::Ge => FloatPredicate::OGE,
                };
                bv(bl!(self.b.build_float_compare(pred, x, y, "cmp")))
            }
            ExprKind::Approx { a, b, rtol, atol } => {
                let va = self.expr(a)?;
                let x = self.to_f(va)?;
                let vb = self.expr(b)?;
                let y = self.to_f(vb)?;
                let at = match atol {
                    Some(t) => {
                        let v = self.expr(t)?;
                        self.to_f(v)?
                    }
                    None => self.fconst(0.0),
                };
                let d = bl!(self.b.build_float_sub(x, y, "d"));
                let d = self.intrinsic("llvm.fabs", &[d])?;
                let ax = self.intrinsic("llvm.fabs", &[x])?;
                let ay = self.intrinsic("llvm.fabs", &[y])?;
                let m = self.intrinsic("llvm.maxnum", &[ax, ay])?;
                let rt = bl!(self.b.build_float_mul(self.fconst(*rtol), m, "rt"));
                let tol = self.intrinsic("llvm.maxnum", &[at, rt])?;
                let eq = bl!(self.b.build_float_compare(FloatPredicate::OEQ, x, y, "eq"));
                let fin = bl!(self.b.build_float_compare(FloatPredicate::OLT, d, self.fconst(f64::INFINITY), "fin"));
                let le = bl!(self.b.build_float_compare(FloatPredicate::OLE, d, tol, "le"));
                let near = bl!(self.b.build_and(fin, le, "near"));
                bv(bl!(self.b.build_or(eq, near, "approx")))
            }
            ExprKind::Logic { and, a, b } => {
                let va = self.expr(a)?;
                let x = self.truth(va)?;
                let from = self.b.get_insert_block().unwrap();
                let (rhs, join) = (self.new_bb("rhs"), self.new_bb("logic"));
                if *and {
                    bl!(self.b.build_conditional_branch(x, rhs, join));
                } else {
                    bl!(self.b.build_conditional_branch(x, join, rhs));
                }
                self.goto(rhs);
                let vb = self.expr(b)?;
                let y = self.truth(vb)?;
                let rhs_end = self.b.get_insert_block().unwrap();
                bl!(self.b.build_unconditional_branch(join));
                self.goto(join);
                let phi = bl!(self.b.build_phi(self.cx.bool_type(), "l"));
                let short = self.cx.bool_type().const_int(u64::from(!*and), false);
                phi.add_incoming(&[(&short, from), (&y, rhs_end)]);
                bv(phi.as_basic_value().into_int_value())
            }
            ExprKind::Not(a) => {
                let v = self.expr(a)?;
                let t = self.truth(v)?;
                bv(bl!(self.b.build_not(t, "not")))
            }
            ExprKind::If(c, a, b) => {
                let k = kind_of(&e.ty)?;
                let cv = self.expr(c)?;
                let t = self.truth(cv)?;
                let (tb, eb, join) = (self.new_bb("ethen"), self.new_bb("eelse"), self.new_bb("eif"));
                bl!(self.b.build_conditional_branch(t, tb, eb));
                self.goto(tb);
                let va = self.expr(a)?;
                let va = self.coerce(va, k)?;
                let ta = self.b.get_insert_block().unwrap();
                bl!(self.b.build_unconditional_branch(join));
                self.goto(eb);
                let vb = self.expr(b)?;
                let vb = self.coerce(vb, k)?;
                let tb2 = self.b.get_insert_block().unwrap();
                bl!(self.b.build_unconditional_branch(join));
                self.goto(join);
                if k == Kind::Void {
                    Val { k, v: None }
                } else {
                    let phi = bl!(self.b.build_phi(self.llty(k), "if"));
                    phi.add_incoming(&[(&va.v.unwrap(), ta), (&vb.v.unwrap(), tb2)]);
                    Val { k, v: Some(phi.as_basic_value()) }
                }
            }
            ExprKind::Let(binds, value) => {
                for (sym, v) in binds {
                    let x = self.expr(v)?;
                    self.store_var(*sym, x)?;
                }
                self.expr(value)?
            }
            ExprKind::Call(f, args) => {
                let func = &self.m.funcs[*f];
                let mut vals: Vec<BasicMetadataValueEnum> = vec![];
                for (a, p) in args.iter().zip(func.params.iter()) {
                    let v = self.expr(a)?;
                    let k = kind_of(&self.m.syms[*p].ty)?;
                    let v = self.coerce(v, k)?;
                    vals.push(v.v.unwrap().into());
                }
                if args.len() != func.params.len() {
                    return Err("a call with the wrong number of arguments".into());
                }
                let rk = kind_of(&func.ret_ty)?;
                let into_module = func.body.first().is_some_and(|s| s.line > crate::eval::MODLINE_MAX);
                let saved = match self.known_line {
                    Some(l) => self.i32c(l as i64),
                    None => self.line_val()?,
                };
                if into_module {
                    // eval.rs call(): 0 < caller line <= MODLINE_MAX → call_line (D185)
                    let pos = bl!(self.b.build_int_compare(IntPredicate::SGT, saved, self.i32c(0), "pos"));
                    let prog = bl!(self.b.build_int_compare(IntPredicate::ULE, saved,
                                                            self.i32c(crate::eval::MODLINE_MAX as i64), "prog"));
                    let yes = bl!(self.b.build_and(pos, prog, "yes"));
                    let clp = unsafe { bl!(self.b.build_gep(self.cx.i8_type(), self.ctx_ptr, &[self.i64c(4)], "clp")) };
                    let old = bl!(self.b.build_load(self.cx.i32_type(), clp, "old")).into_int_value();
                    let nv = bl!(self.b.build_select(yes, saved, old, "cl"));
                    bl!(self.b.build_store(clp, nv));
                }
                let cs = bl!(self.b.build_call(self.funcs[*f], &vals, "call"));
                self.check_err()?;
                // eval.rs call(): later errors in the caller are on its own line
                self.st(self.line_g.as_pointer_value(), saved, "line")?;
                Val { k: rk, v: cs.try_as_basic_value().left() }
            }
            ExprKind::List(items) if matches!(e.ty, Ty::TextList) => {
                let l = self.call("fm_tlist_new", &[ctx])?.unwrap();
                for it in items {
                    let v = self.expr(it)?;
                    if v.k != Kind::S {
                        return Err("a text list item that isn't a text".into());
                    }
                    self.call("fm_tlist_push", &[l, v.v.unwrap()])?;
                }
                Val { k: Kind::TL, v: Some(l) }
            }
            ExprKind::List(items) => {
                let cap = self.i64c(items.len() as i64);
                let l = self.call("fm_list_new", &[ctx, cap.into()])?.unwrap();
                for it in items {
                    let v = self.expr(it)?;
                    match v.k {
                        Kind::F => {
                            self.call("fm_list_push", &[l, v.v.unwrap()])?;
                        }
                        Kind::L => {
                            self.call("fm_list_extend", &[l, v.v.unwrap()])?;
                        }
                        _ => return Err("a list item that isn't a number or a list".into()),
                    }
                }
                Val { k: Kind::L, v: Some(l) }
            }
            ExprKind::Vec(items) => {
                let Kind::V(n) = kind_of(&e.ty)? else { return Err("a vector literal that isn't a vector".into()) };
                let mut xs = vec![];
                for it in items {
                    let v = self.expr(it)?;
                    match v.k {
                        Kind::F => xs.push(v.v.unwrap().into_float_value()),
                        Kind::V(_) => xs.extend(self.vec_elems(v)?),
                        _ => return Err("a vector item that isn't a number or a vector".into()),
                    }
                }
                if xs.len() != n {
                    return Err("a vector literal of the wrong size".into());
                }
                self.make_vec(&xs)?
            }
            ExprKind::VecElem(v, k) => {
                let vv = self.expr(v)?;
                let Kind::V(n) = vv.k else { return Err("a component of a non-vector".into()) };
                if *k >= n {
                    return Err("a component out of range".into());
                }
                let a = vv.v.unwrap().into_array_value();
                fv(bl!(self.b.build_extract_value(a, *k as u32, "c")).into_float_value())
            }
            ExprKind::Index(l, i) => {
                let lv = self.expr(l)?;
                let iv = self.expr(i)?;
                let i = self.to_f(iv)?;
                match lv.k {
                    Kind::L => {
                        let ep = self.list_elem_ptr(lv.v.unwrap().into_pointer_value(), i)?;
                        fv(self.ld(self.f64t(), ep, "x", "elem")?.into_float_value())
                    }
                    Kind::TL => {
                        let len = self.call("fm_tlist_len", &[lv.v.unwrap()])?.unwrap().into_int_value();
                        let k = self.index_check(i, len)?;
                        let id = self.call("fm_tlist_at", &[lv.v.unwrap(), k.into()])?;
                        Val { k: Kind::S, v: id }
                    }
                    _ => return Err("indexing this kind of value isn't compiled".into()),
                }
            }
            ExprKind::Builtin(name, args) => self.builtin(e, name, args)?,
            ExprKind::Integral { .. } | ExprKind::Sum { .. } | ExprKind::Root { .. } => self.calculus(e)?,
            ExprKind::SolEval { .. } | ExprKind::SolList { .. } | ExprKind::PdeEval { .. }
            | ExprKind::OdeLinSolve { .. } => self.solution_expr(e)?,
            other => return Err(format!("{} isn't compiled yet", expr_name(other))),
        })
    }

    fn intern(&mut self, s: &str) -> i64 {
        if let Some(i) = self.tables.texts.iter().position(|t| &**t == s) {
            return i as i64;
        }
        self.tables.texts.push(Rc::from(s));
        self.tables.texts.len() as i64 - 1
    }

    fn bin(&mut self, op: BinOp, a: Val<'c>, b: Val<'c>) -> R<Val<'c>> {
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        let opc = self.i32c(match op {
            BinOp::Add => 0,
            BinOp::Sub => 1,
            BinOp::Mul => 2,
            BinOp::Div => 3,
        });
        Ok(match (a.k, b.k) {
            (Kind::F, Kind::F) => {
                let (x, y) = (a.v.unwrap().into_float_value(), b.v.unwrap().into_float_value());
                fv(self.arith(op, x, y)?)
            }
            (Kind::L, Kind::F) | (Kind::F, Kind::L) => {
                let swap = a.k == Kind::F;
                let (l, x) = if swap { (b, a) } else { (a, b) };
                let r = self.call("fm_list_binls", &[ctx, opc.into(), l.v.unwrap(), x.v.unwrap(),
                                                      self.i32c(i64::from(swap)).into()])?;
                Val { k: Kind::L, v: r }
            }
            (Kind::L, Kind::L) => {
                let line = self.line_val()?;
                let r = self.call("fm_list_binll", &[ctx, opc.into(), a.v.unwrap(), b.v.unwrap(), line.into()])?;
                self.check_err()?;
                Val { k: Kind::L, v: r }
            }
            (Kind::V(n), Kind::F) | (Kind::F, Kind::V(n)) => {
                let (xs, y, swap) = if a.k == Kind::F {
                    (self.vec_elems(b)?, a.v.unwrap().into_float_value(), true)
                } else {
                    (self.vec_elems(a)?, b.v.unwrap().into_float_value(), false)
                };
                let mut out = Vec::with_capacity(n);
                for x in xs {
                    out.push(if swap { self.arith(op, y, x)? } else { self.arith(op, x, y)? });
                }
                self.make_vec(&out)?
            }
            (Kind::V(n), Kind::V(m)) if n == m => {
                let xs = self.vec_elems(a)?;
                let ys = self.vec_elems(b)?;
                let mut out = Vec::with_capacity(n);
                for (x, y) in xs.into_iter().zip(ys) {
                    out.push(self.arith(op, x, y)?);
                }
                self.make_vec(&out)?
            }
            _ => return Err(format!("arithmetic on {:?} and {:?} isn't compiled", a.k, b.k)),
        })
    }

    fn builtin(&mut self, e: &Expr, name: &str, args: &[Expr]) -> R<Val<'c>> {
        // max / min of a solution component: refined between the steps (eval_solve sol_extreme), args unevaluated
        if let ([a], "max_list" | "min_list") = (args, name) {
            if let ExprKind::SolList { sol, comp, what: 0 } = &a.kind {
                let v = self.load_var(*sol)?;
                let sg = self.fconst(if name == "max_list" { 1.0 } else { -1.0 });
                let ctx: BasicValueEnum = self.ctx_ptr.into();
                return Ok(fv(self.fcall("fm_sol_extreme", &[ctx, v.v.unwrap(), self.i64c(*comp as i64).into(),
                                                            sg.into()])?));
            }
        }
        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.expr(a)?);
        }
        let all_num = !vals.is_empty() && vals.iter().all(|v| matches!(v.k, Kind::F | Kind::B));
        // plain math on numbers, compiled in place (the functions eval.rs's fallback uses)
        if all_num {
            let x = self.to_f(vals[0])?;
            if vals.len() == 1 {
                if SHIM1.contains(&name) {
                    let sym = format!("fm_{name}");
                    let f = self.externs.iter().find(|(k, _)| **k == sym).map(|(k, _)| *k).unwrap();
                    return Ok(fv(self.fcall(f, &[x.into()])?));
                }
                let r = match name {
                    "sqrt" => Some(self.powc(x, 0.5)?),
                    "abs" => Some(self.intrinsic("llvm.fabs", &[x])?),
                    "floor" => Some(self.intrinsic("llvm.floor", &[x])?),
                    "ceil" => Some(self.intrinsic("llvm.ceil", &[x])?),
                    "round" => Some(self.intrinsic("llvm.round", &[x])?),
                    "sign" => {
                        let z = self.fconst(0.0);
                        let pos = bl!(self.b.build_float_compare(FloatPredicate::OGT, x, z, "pos"));
                        let neg = bl!(self.b.build_float_compare(FloatPredicate::OLT, x, z, "neg"));
                        let p = bl!(self.b.build_unsigned_int_to_float(pos, self.f64t(), "p"));
                        let n = bl!(self.b.build_unsigned_int_to_float(neg, self.f64t(), "n"));
                        Some(bl!(self.b.build_float_sub(p, n, "sign")))
                    }
                    _ => None,
                };
                if let Some(r) = r {
                    return Ok(fv(r));
                }
            }
            if vals.len() == 2 && (name == "atan2" || name == "hypot") {
                let y = self.to_f(vals[1])?;
                let f = if name == "atan2" { "fm_atan2" } else { "fm_hypot" };
                return Ok(fv(self.fcall(f, &[x.into(), y.into()])?));
            }
            if name == "max" || name == "min" {
                // eval_core: f64::max / min (llvm.maxnum / minnum) from the first argument
                let mut acc = x;
                for v in vals[1..].to_vec() {
                    let b = self.to_f(v)?;
                    acc = self.intrinsic(if name == "max" { "llvm.maxnum" } else { "llvm.minnum" }, &[acc, b])?;
                }
                return Ok(fv(acc));
            }
            if name == "mod" && vals.len() == 2 {
                // a − c·⌊a/c⌋
                let c = self.to_f(vals[1])?;
                let q = bl!(self.b.build_float_div(x, c, "q"));
                let fl = self.intrinsic("llvm.floor", &[q])?;
                let m = bl!(self.b.build_float_mul(c, fl, "m"));
                return Ok(fv(bl!(self.b.build_float_sub(x, m, "mod"))));
            }
            if name == "clamp" && vals.len() == 3 {
                let lo = self.to_f(vals[1])?;
                let hi = self.to_f(vals[2])?;
                let a = self.intrinsic("llvm.maxnum", &[x, lo])?;
                return Ok(fv(self.intrinsic("llvm.minnum", &[a, hi])?));
            }
        }
        if name == "len" && vals.len() == 1 {
            match vals[0].k {
                Kind::L => {
                    let (_, len) = self.list_parts(vals[0].v.unwrap().into_pointer_value())?;
                    return Ok(fv(bl!(self.b.build_unsigned_int_to_float(len, self.f64t(), "len"))));
                }
                Kind::V(n) => return Ok(fv(self.fconst(n as f64))),
                _ => {}
            }
        }
        // anything else: the tree-walker's own implementation, through a callback
        let ret = kind_of(&e.ty)?;
        let kinds: Vec<Kind> = vals.iter().map(|v| v.k).collect();
        if kinds.iter().any(|k| matches!(k, Kind::Void | Kind::H)) || ret == Kind::H {
            return Err(format!("built-in {name} with an argument without a value"));
        }
        self.tables.builtins.push(BuiltinSite { name: name.to_string(), args: kinds, ret });
        let site = self.tables.builtins.len() as i64 - 1;
        let i64t = self.cx.i64_type();
        let n = vals.len().max(1) as u32;
        let argp = self.alloca(i64t.array_type(n).into(), "args")?;
        for (i, v) in vals.iter().enumerate() {
            let slot = match v.k {
                Kind::F => bl!(self.b.build_bit_cast(v.v.unwrap(), i64t, "bits")).into_int_value(),
                Kind::B => bl!(self.b.build_int_z_extend(v.v.unwrap().into_int_value(), i64t, "b")),
                Kind::S => v.v.unwrap().into_int_value(),
                Kind::L | Kind::TL => bl!(self.b.build_ptr_to_int(v.v.unwrap().into_pointer_value(), i64t, "p")),
                Kind::V(_) => {
                    let p = self.spill(*v)?;
                    bl!(self.b.build_ptr_to_int(p, i64t, "p"))
                }
                Kind::Void | Kind::H => unreachable!(),
            };
            let sp = unsafe { bl!(self.b.build_gep(i64t.array_type(n), argp, &[self.i64c(0), self.i64c(i as i64)], "a")) };
            bl!(self.b.build_store(sp, slot));
        }
        let outty: BasicTypeEnum = match ret {
            Kind::V(m) => self.f64t().array_type(m as u32).into(),
            _ => i64t.into(),
        };
        let outp = self.alloca(outty, "out")?;
        let line = self.line_val()?;
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        self.call("fm_builtin", &[ctx, self.i64c(site).into(), argp.into(), outp.into(), line.into()])?;
        self.check_err()?;
        Ok(match ret {
            Kind::V(_) => Val { k: ret, v: Some(bl!(self.b.build_load(outty, outp, "r"))) },
            Kind::Void => Val { k: ret, v: None },
            _ => {
                let raw = bl!(self.b.build_load(i64t, outp, "r")).into_int_value();
                let v: BasicValueEnum = match ret {
                    Kind::F => bl!(self.b.build_bit_cast(raw, self.f64t(), "f")),
                    Kind::B => bl!(self.b.build_int_compare(IntPredicate::NE, raw, i64t.const_zero(), "b")).into(),
                    Kind::S => raw.into(),
                    Kind::L | Kind::TL => bl!(self.b.build_int_to_ptr(raw, self.ptrt(), "p")).into(),
                    _ => unreachable!(),
                };
                Val { k: ret, v: Some(v) }
            }
        })
    }
}

fn bl_unwrap<T>(r: Result<T, inkwell::builder::BuilderError>) -> T {
    r.expect("LLVM builder")
}

fn fv(x: FloatValue<'_>) -> Val<'_> {
    Val { k: Kind::F, v: Some(x.into()) }
}

fn bv(x: IntValue<'_>) -> Val<'_> {
    Val { k: Kind::B, v: Some(x.into()) }
}

fn expr_name(k: &ExprKind) -> &'static str {
    match k {
        ExprKind::Map { .. } => "a function applied to a list",
        ExprKind::VecSet { .. } => "setting a vector component",
        ExprKind::VecIndex { .. } => "a vector index picked at run time",
        ExprKind::Integral { .. } => "an integral",
        ExprKind::Sum { .. } => "a sum",
        ExprKind::Root { .. } => "an equation (solve)",
        ExprKind::SolEval { .. } | ExprKind::SolList { .. } => "an ODE solution",
        ExprKind::Load(_) | ExprKind::Table(_) | ExprKind::Column(..) => "data",
        ExprKind::PdeEval { .. } => "a PDE solution",
        ExprKind::Uncertain(..) => "an uncertain value",
        _ => "this expression",
    }
}

#[path = "par.rs"]
mod par;
#[path = "lam.rs"]
mod lam;
#[path = "ode.rs"]
mod ode;
