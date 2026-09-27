//! What an executable made by `fermium build` carries besides its machine code: the parts of the checked module
//! the run time reads (print formats, texts, data/fit/plot tables, the functions' names), the tables the code
//! generator made (built-in call sites, solve sites…), and the program's source (for error messages). A small
//! binary format written by the compiler and read by the executable's run time (fermium-aotrt).
//!
//! The structs are destructured exhaustively, so a field added to them doesn't compile until it is carried here.
use std::rc::Rc;

use fermium_ir::serde_like::Json;
use fermium_ir::{CCallSite, CParam, CParamKind, Fmt, Func, Hint, Module, PyCallSite, Tables, Ty};
use num_rational::Rational64;

use super::rt::{BuiltinSite, Kind, MNode};
use super::solve_rt::OdeSite;

/// The code generator's tables (what `Ctx` gets at run time).
#[derive(Default)]
pub struct GenTables {
    pub texts: Vec<Rc<str>>,
    pub builtins: Vec<BuiltinSite>,
    pub mvec_fmts: Vec<Vec<usize>>,
    pub ode_sites: Vec<OdeSite>,
    pub msum_sites: Vec<MNode>,
    /// constructs run by the tree-walker (never in an executable: build_object refuses them)
    pub interp_sites: Vec<super::delegate::InterpSite>,
}

/// Everything an executable carries.
pub struct Blob {
    pub module: Module,
    pub tables: GenTables,
    pub source: String,
    pub file_name: String,
    /// the checked module's shape when it was built (fingerprint)
    pub fingerprint: (u64, u64),
}

impl Blob {
    /// Does the executable need the program's IR (constructs the tree-walker runs)?
    pub fn needs_ir(&self) -> bool {
        !self.tables.interp_sites.is_empty()
    }
}

const MAGIC: &[u8; 8] = b"FMBLOB01";

// ---------------------------------------------------------------- writer
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u(&mut self, x: u64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn i(&mut self, x: i64) {
        self.u(x as u64)
    }
    fn f(&mut self, x: f64) {
        self.u(x.to_bits())
    }
    fn b(&mut self, x: bool) {
        self.u(u64::from(x))
    }
    fn s(&mut self, x: &str) {
        self.u(x.len() as u64);
        self.0.extend_from_slice(x.as_bytes());
    }
    fn dim(&mut self, d: &fermium_ir::Dim) {
        for r in d.0.iter() {
            self.i(*r.numer());
            self.i(*r.denom());
        }
    }
    fn hint(&mut self, h: &Option<Hint>) {
        match h {
            None => self.b(false),
            Some(Hint { name, factor, offset, dim }) => {
                self.b(true);
                self.s(name);
                self.f(*factor);
                self.f(*offset);
                self.dim(dim);
            }
        }
    }
    fn json(&mut self, j: &Json) {
        match j {
            Json::Null => self.u(0),
            Json::Bool(b) => {
                self.u(1);
                self.b(*b)
            }
            Json::Num(x) => {
                self.u(2);
                self.f(*x)
            }
            Json::Str(s) => {
                self.u(3);
                self.s(s)
            }
            Json::List(v) => {
                self.u(4);
                self.u(v.len() as u64);
                v.iter().for_each(|x| self.json(x));
            }
            Json::Obj(v) => {
                self.u(5);
                self.u(v.len() as u64);
                for (k, x) in v {
                    self.s(k);
                    self.json(x);
                }
            }
        }
    }
    fn kind(&mut self, k: Kind) {
        match k {
            Kind::F => self.u(0),
            Kind::B => self.u(1),
            Kind::S => self.u(2),
            Kind::L => self.u(3),
            Kind::TL => self.u(4),
            Kind::V(n) => {
                self.u(5);
                self.u(n as u64)
            }
            Kind::H => self.u(6),
            Kind::Obj => self.u(8),
            Kind::Void => self.u(7),
        }
    }
    fn mnode(&mut self, n: &MNode) {
        match n {
            MNode::Leaf(sf) => {
                self.u(0);
                self.i(sf.map(i64::from).unwrap_or(-1));
            }
            MNode::Op(add, a, b) => {
                self.u(1);
                self.b(*add);
                self.mnode(a);
                self.mnode(b);
            }
        }
    }
}

/// The bytes an executable carries.
pub fn write(module: &Module, t: &GenTables, source: &str, file_name: &str) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(MAGIC);
    w.s(source);
    w.s(file_name);
    let Tables { fmts, texts, plots, loads, fits, pycalls, py_base_dir, ccalls } = &module.tables;
    w.u(fmts.len() as u64);
    for Fmt { dim, hint, sf, direct, echo, nat } in fmts {
        w.dim(dim);
        w.hint(hint);
        w.i(sf.map(i64::from).unwrap_or(-1));
        w.u(u64::from(*direct));
        w.b(*echo);
        w.b(nat.is_some());
        w.s(nat.as_deref().unwrap_or(""));
    }
    w.u(texts.len() as u64);
    texts.iter().for_each(|s| w.s(s));
    for js in [plots, loads, fits] {
        w.u(js.len() as u64);
        js.iter().for_each(|j| w.json(j));
    }
    w.u(pycalls.len() as u64);
    for PyCallSite { module, func, display, facs, ints, pnames, rlist, rfac, declared } in pycalls {
        w.s(module);
        w.s(func);
        w.s(display);
        w.u(facs.len() as u64);
        facs.iter().for_each(|x| w.f(*x));
        w.u(ints.len() as u64);
        ints.iter().for_each(|x| w.b(*x));
        w.u(pnames.len() as u64);
        pnames.iter().for_each(|x| w.s(x));
        w.b(*rlist);
        w.f(*rfac);
        w.b(*declared);
    }
    w.s(py_base_dir);
    w.u(ccalls.len() as u64);
    for CCallSite { lib, symbol, display, by_ref, params, rint, rfac, map, cpp } in ccalls {
        w.s(lib);
        w.s(symbol);
        w.s(display);
        w.b(*by_ref);
        w.u(params.len() as u64);
        for CParam { name, kind, fac, len_of } in params {
            w.s(name);
            w.u(*kind as u64);
            w.f(*fac);
            w.u(*len_of as u64);
        }
        w.b(*rint);
        w.f(*rfac);
        w.b(*map);
        w.b(*cpp);
    }
    w.b(module.uses_uncertainty);
    w.u(module.funcs.len() as u64);
    for f in &module.funcs {
        w.s(&f.name);
        w.s(&f.display);
        w.u(u64::from(f.def_line));
        w.i(f.sf.map(i64::from).unwrap_or(-1));
    }
    // the code generator's tables
    let GenTables { texts, builtins, mvec_fmts, ode_sites, msum_sites, interp_sites } = t;
    w.u(texts.len() as u64);
    texts.iter().for_each(|s| w.s(s));
    w.u(builtins.len() as u64);
    for BuiltinSite { name, args, ret } in builtins {
        w.s(name);
        w.u(args.len() as u64);
        args.iter().for_each(|k| w.kind(*k));
        w.kind(*ret);
    }
    w.u(mvec_fmts.len() as u64);
    for v in mvec_fmts {
        w.u(v.len() as u64);
        v.iter().for_each(|x| w.u(*x as u64));
    }
    w.u(ode_sites.len() as u64);
    for OdeSite { method, rtol, atol, tname, evtext, tdep, tfmt, env_kinds, nstates, grid, eig_method, order, pmethod,
                  bc, is_complex, xname, pde_line, sw_ops, sw_slot0, nuser, whens } in ode_sites {
        w.s(method);
        w.f(*rtol);
        w.b(atol.is_some());
        let a = atol.clone().unwrap_or_default();
        w.u(a.len() as u64);
        for (v, p) in a {
            w.f(v);
            w.u(u64::from(p));
        }
        w.u(*tname as u64);
        w.i(*evtext);
        w.b(*tdep);
        w.u(*tfmt as u64);
        w.u(env_kinds.len() as u64);
        env_kinds.iter().for_each(|k| w.kind(*k));
        w.u(*nstates as u64);
        w.u(*grid as u64);
        w.u(u64::from(*eig_method));
        w.u(u64::from(*order));
        w.u(u64::from(*pmethod));
        w.u(u64::from(bc.0));
        w.u(u64::from(bc.1));
        w.b(*is_complex);
        w.u(*xname as u64);
        w.u(u64::from(*pde_line));
        w.u(sw_ops.len() as u64);
        sw_ops.iter().for_each(|o| w.u(u64::from(*o)));
        w.u(*sw_slot0 as u64);
        w.u(*nuser as u64);
        w.u(whens.len() as u64);
        for (d, tx) in whens {
            w.u(u64::from(*d));
            w.u(*tx as u64);
        }
    }
    w.u(msum_sites.len() as u64);
    msum_sites.iter().for_each(|n| w.mnode(n));
    // constructs the tree-walker runs: the executable re-checks its program to get the IR (needs_ir)
    w.u(interp_sites.len() as u64);
    for super::delegate::InterpSite { ptr: _, node, is_stmt, syms, writes, ret } in interp_sites {
        w.u(u64::from(*node));
        w.b(*is_stmt);
        w.u(syms.len() as u64);
        for (s, k) in syms {
            w.u(*s as u64);
            w.kind(*k);
        }
        w.u(writes.len() as u64);
        writes.iter().for_each(|s| w.u(*s as u64));
        w.kind(*ret);
    }
    let (nn, ns) = fingerprint(module);
    w.u(nn);
    w.u(ns);
    w.0
}

/// The module's shape (nodes, symbols): an executable that re-checks its program compares it.
pub fn fingerprint(m: &Module) -> (u64, u64) {
    (super::delegate::nodes(m).len() as u64, m.syms.len() as u64)
}

// ---------------------------------------------------------------- reader
struct Rd<'a> {
    b: &'a [u8],
    at: usize,
}

type RR<T> = Result<T, String>;

impl Rd<'_> {
    fn u(&mut self) -> RR<u64> {
        let s = self.b.get(self.at..self.at + 8).ok_or("the executable's data is truncated")?;
        self.at += 8;
        Ok(u64::from_le_bytes(s.try_into().unwrap()))
    }
    fn n(&mut self) -> RR<usize> {
        let n = self.u()? as usize;
        if n > self.b.len() {
            return Err("the executable's data is damaged".into());
        }
        Ok(n)
    }
    fn i(&mut self) -> RR<i64> {
        Ok(self.u()? as i64)
    }
    fn f(&mut self) -> RR<f64> {
        Ok(f64::from_bits(self.u()?))
    }
    fn b(&mut self) -> RR<bool> {
        Ok(self.u()? != 0)
    }
    fn s(&mut self) -> RR<String> {
        let n = self.n()?;
        let s = self.b.get(self.at..self.at + n).ok_or("the executable's data is truncated")?;
        self.at += n;
        String::from_utf8(s.to_vec()).map_err(|_| "the executable's data is damaged".into())
    }
    fn opt_u32(&mut self) -> RR<Option<u32>> {
        let x = self.i()?;
        Ok(if x < 0 { None } else { Some(x as u32) })
    }
    fn dim(&mut self) -> RR<fermium_ir::Dim> {
        let mut d = fermium_ir::DIMLESS;
        for k in 0..7 {
            let (a, b) = (self.i()?, self.i()?);
            d.0[k] = Rational64::new_raw(a, b);
        }
        Ok(d)
    }
    fn hint(&mut self) -> RR<Option<Hint>> {
        if !self.b()? {
            return Ok(None);
        }
        Ok(Some(Hint { name: self.s()?, factor: self.f()?, offset: self.f()?, dim: self.dim()? }))
    }
    fn json(&mut self) -> RR<Json> {
        Ok(match self.u()? {
            0 => Json::Null,
            1 => Json::Bool(self.b()?),
            2 => Json::Num(self.f()?),
            3 => Json::Str(self.s()?),
            4 => {
                let n = self.n()?;
                Json::List((0..n).map(|_| self.json()).collect::<RR<_>>()?)
            }
            _ => {
                let n = self.n()?;
                Json::Obj((0..n).map(|_| Ok((self.s()?, self.json()?))).collect::<RR<_>>()?)
            }
        })
    }
    fn kind(&mut self) -> RR<Kind> {
        Ok(match self.u()? {
            0 => Kind::F,
            1 => Kind::B,
            2 => Kind::S,
            3 => Kind::L,
            4 => Kind::TL,
            5 => Kind::V(self.n()?),
            6 => Kind::H,
            8 => Kind::Obj,
            _ => Kind::Void,
        })
    }
    fn mnode(&mut self) -> RR<MNode> {
        Ok(match self.u()? {
            0 => MNode::Leaf(self.opt_u32()?),
            _ => {
                let add = self.b()?;
                MNode::Op(add, Box::new(self.mnode()?), Box::new(self.mnode()?))
            }
        })
    }
}

/// Read what `write` wrote.
pub fn read(bytes: &[u8]) -> RR<Blob> {
    if bytes.len() < 8 || &bytes[..8] != MAGIC {
        return Err("this executable wasn't made by this version of fermium build".into());
    }
    let mut r = Rd { b: bytes, at: 8 };
    let source = r.s()?;
    let file_name = r.s()?;
    let mut tables = Tables::default();
    let n = r.n()?;
    for _ in 0..n {
        let dim = r.dim()?;
        let hint = r.hint()?;
        let sf = r.opt_u32()?;
        let direct = r.u()? as u8;
        let echo = r.b()?;
        let has_nat = r.b()?;
        let nat = r.s()?;
        tables.fmts.push(Fmt { dim, hint, sf, direct, echo, nat: has_nat.then_some(nat) });
    }
    let n = r.n()?;
    tables.texts = (0..n).map(|_| r.s()).collect::<RR<_>>()?;
    for k in 0..3 {
        let n = r.n()?;
        let js = (0..n).map(|_| r.json()).collect::<RR<Vec<_>>>()?;
        match k {
            0 => tables.plots = js,
            1 => tables.loads = js,
            _ => tables.fits = js,
        }
    }
    let n = r.n()?;
    for _ in 0..n {
        let module = r.s()?;
        let func = r.s()?;
        let display = r.s()?;
        let k = r.n()?;
        let facs = (0..k).map(|_| r.f()).collect::<RR<_>>()?;
        let k = r.n()?;
        let ints = (0..k).map(|_| r.b()).collect::<RR<_>>()?;
        let k = r.n()?;
        let pnames = (0..k).map(|_| r.s()).collect::<RR<_>>()?;
        let (rlist, rfac, declared) = (r.b()?, r.f()?, r.b()?);
        tables.pycalls.push(PyCallSite { module, func, display, facs, ints, pnames, rlist, rfac, declared });
    }
    tables.py_base_dir = r.s()?;
    let n = r.n()?;
    for _ in 0..n {
        let (lib, symbol, display, by_ref) = (r.s()?, r.s()?, r.s()?, r.b()?);
        let k = r.n()?;
        let mut params = vec![];
        for _ in 0..k {
            let name = r.s()?;
            let kind = match r.u()? {
                0 => CParamKind::Num,
                1 => CParamKind::Int,
                2 => CParamKind::List,
                _ => CParamKind::Len,
            };
            let (fac, len_of) = (r.f()?, r.u()? as usize);
            params.push(CParam { name, kind, fac, len_of });
        }
        let (rint, rfac, map, cpp) = (r.b()?, r.f()?, r.b()?, r.b()?);
        tables.ccalls.push(CCallSite { lib, symbol, display, by_ref, params, rint, rfac, map, cpp });
    }
    let uses_uncertainty = r.b()?;
    let n = r.n()?;
    let mut funcs = vec![];
    for _ in 0..n {
        let (name, display, def_line, sf) = (r.s()?, r.s()?, r.u()? as u32, r.opt_u32()?);
        funcs.push(Func { name, params: vec![], ret_ty: Ty::Void, body: vec![], locals: vec![], sf, display, def_line });
    }
    let module = Module { tables, funcs, uses_uncertainty, ..Default::default() };
    let mut t = GenTables::default();
    let n = r.n()?;
    t.texts = (0..n).map(|_| r.s().map(|s| Rc::from(s.as_str()))).collect::<RR<_>>()?;
    let n = r.n()?;
    for _ in 0..n {
        let name = r.s()?;
        let k = r.n()?;
        let args = (0..k).map(|_| r.kind()).collect::<RR<_>>()?;
        let ret = r.kind()?;
        t.builtins.push(BuiltinSite { name, args, ret });
    }
    let n = r.n()?;
    for _ in 0..n {
        let k = r.n()?;
        t.mvec_fmts.push((0..k).map(|_| r.u().map(|x| x as usize)).collect::<RR<_>>()?);
    }
    let n = r.n()?;
    for _ in 0..n {
        let method = r.s()?;
        let rtol = r.f()?;
        let has_atol = r.b()?;
        let k = r.n()?;
        let a = (0..k).map(|_| Ok((r.f()?, r.u()? as u32))).collect::<RR<Vec<_>>>()?;
        let atol = has_atol.then_some(a);
        let tname = r.u()? as usize;
        let evtext = r.i()?;
        let tdep = r.b()?;
        let tfmt = r.u()? as usize;
        let k = r.n()?;
        let env_kinds = (0..k).map(|_| r.kind()).collect::<RR<_>>()?;
        let nstates = r.u()? as usize;
        let grid = r.u()? as usize;
        let eig_method = r.u()? as u8;
        let order = r.u()? as u8;
        let pmethod = r.u()? as u8;
        let bc = (r.u()? as u8, r.u()? as u8);
        let is_complex = r.b()?;
        let xname = r.u()? as usize;
        let pde_line = r.u()? as u32;
        let k = r.n()?;
        let sw_ops = (0..k).map(|_| r.u().map(|x| x as u8)).collect::<RR<_>>()?;
        let sw_slot0 = r.u()? as usize;
        let nuser = r.u()? as usize;
        let k = r.n()?;
        let whens = (0..k).map(|_| Ok((r.u()? as u8, r.u()? as usize))).collect::<RR<Vec<_>>>()?;
        t.ode_sites.push(OdeSite { method, rtol, atol, tname, evtext, tdep, tfmt, env_kinds, nstates, grid, eig_method,
                                   order, pmethod, bc, is_complex, xname, pde_line, sw_ops, sw_slot0, nuser, whens });
    }
    let n = r.n()?;
    t.msum_sites = (0..n).map(|_| r.mnode()).collect::<RR<_>>()?;
    let n = r.n()?;
    for _ in 0..n {
        let node = r.u()? as u32;
        let is_stmt = r.b()?;
        let k = r.n()?;
        let syms = (0..k).map(|_| Ok((r.u()? as usize, r.kind()?))).collect::<RR<Vec<_>>>()?;
        let k = r.n()?;
        let writes = (0..k).map(|_| Ok(r.u()? as usize)).collect::<RR<Vec<_>>>()?;
        let ret = r.kind()?;
        t.interp_sites.push(super::delegate::InterpSite { ptr: 0, node, is_stmt, syms, writes, ret });
    }
    let fp = (r.u()?, r.u()?);
    Ok(Blob { module, tables: t, source, file_name, fingerprint: fp })
}
