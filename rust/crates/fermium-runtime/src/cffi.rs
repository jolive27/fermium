//! Calling C and Fortran functions through the C ABI (`import c` / `import fortran`, spec C3, DECISIONS D275),
//! without libffi.
//!
//! The functions Fermium calls take only doubles, C `int`s and pointers (to doubles, to ints, or to arrays of
//! doubles), and return a double or an `int`. For those, the platform's calling convention can be reached from
//! Rust alone: arguments are split by class (floating point / integer), the first ones of each class go in
//! registers in order, and the rest go on the stack, in the order of the arguments, one 8-byte slot each. So the
//! trampoline calls the function pointer as a Rust `extern "C" fn` with every integer register, every
//! floating-point register and enough 8-byte stack slots, filled from the classified arguments. Registers and
//! stack slots the callee doesn't read are harmless: in the C ABI the caller owns (and cleans up) its outgoing
//! arguments.
//!
//! - x86-64 System V (Linux, macOS on Intel): 6 integer registers (rdi…r9), 8 SSE registers (xmm0…7).
//! - AArch64 AAPCS64 (Linux, macOS on Apple silicon): 8 integer registers (x0…x7), 8 FP registers (d0…d7).
//!   Apple's variant packs stack arguments smaller than 8 bytes, so an `int` that doesn't fit in a register is
//!   refused there (a clear error, not a wrong value).
//!
//! Libraries are opened with `dlopen` once and kept open for the life of the process.
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Mutex, OnceLock};

/// The most arguments a C or Fortran function can take through [`call`].
pub const MAX_ARGS: usize = 16;

/// One argument of a C call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CArg {
    /// a `double`
    F64(f64),
    /// a C `int` (32 bits)
    I32(i32),
    /// a pointer (to a double, an int or an array)
    Ptr(*const c_void),
}

/// What a C function returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CRet {
    F64,
    I32,
}

/// A result: a double, or an `int` widened to f64 (every C `int` is exact in a double).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CVal {
    F64(f64),
    I32(i32),
}

impl CVal {
    pub fn num(self) -> f64 {
        match self {
            CVal::F64(x) => x,
            CVal::I32(n) => n as f64,
        }
    }
}

#[cfg(target_arch = "x86_64")]
const INT_REGS: usize = 6;
#[cfg(target_arch = "aarch64")]
const INT_REGS: usize = 8;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const INT_REGS: usize = 0;
const FP_REGS: usize = 8;
/// Enough stack slots for the worst case (every argument an integer).
const STACK_SLOTS: usize = 10;

/// Is calling C functions supported on this platform?
pub fn supported() -> bool {
    cfg!(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(target_os = "windows")))
}

/// Call the C function at `fp` with `args`, reading a result of kind `ret`.
///
/// # Safety
/// `fp` must be the address of a C function whose parameters are exactly `args` (doubles, `int`s, pointers, in
/// this order) and whose result is `ret`; every pointer must be valid for what the function does with it.
pub unsafe fn call(fp: *const c_void, args: &[CArg], ret: CRet) -> Result<CVal, String> {
    if !supported() {
        return Err("calling C functions isn't supported on this platform yet (only x86-64 and AArch64)".into());
    }
    if args.len() > MAX_ARGS {
        return Err(format!("a C function can take at most {MAX_ARGS} arguments here, not {}", args.len()));
    }
    let mut ints = [0u64; 8];
    let mut fps = [0f64; FP_REGS];
    let mut stack = [0u64; STACK_SLOTS];
    let (mut ni, mut nf, mut ns) = (0usize, 0usize, 0usize);
    for a in args {
        let (is_fp, bits) = match *a {
            CArg::F64(x) => (true, x.to_bits()),
            CArg::I32(n) => (false, n as i64 as u64),
            CArg::Ptr(p) => (false, p as usize as u64),
        };
        if is_fp && nf < FP_REGS {
            fps[nf] = f64::from_bits(bits);
            nf += 1;
        } else if !is_fp && ni < INT_REGS {
            ints[ni] = bits;
            ni += 1;
        } else {
            if cfg!(all(target_arch = "aarch64", target_vendor = "apple")) && matches!(a, CArg::I32(_)) {
                return Err("on this platform an int argument after the eighth integer or pointer argument isn't \
                            supported yet".into());
            }
            stack[ns] = bits;
            ns += 1;
        }
    }
    unsafe { Ok(raw_call(fp, &ints, &fps, &stack, ret)) }
}

#[cfg(target_arch = "x86_64")]
unsafe fn raw_call(fp: *const c_void, i: &[u64; 8], f: &[f64; FP_REGS], s: &[u64; STACK_SLOTS], ret: CRet) -> CVal {
    type Sig<R> = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64,
                                        f64, f64, f64, f64, f64, f64, f64, f64,
                                        u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> R;
    unsafe {
        match ret {
            CRet::F64 => {
                let g: Sig<f64> = std::mem::transmute(fp);
                CVal::F64(g(i[0], i[1], i[2], i[3], i[4], i[5], f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
                            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9]))
            }
            CRet::I32 => {
                let g: Sig<u64> = std::mem::transmute(fp);
                let r = g(i[0], i[1], i[2], i[3], i[4], i[5], f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
                          s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9]);
                CVal::I32(r as u32 as i32) // an int comes back in the low 32 bits of rax
            }
        }
    }
}

#[cfg(target_arch = "aarch64")]
unsafe fn raw_call(fp: *const c_void, i: &[u64; 8], f: &[f64; FP_REGS], s: &[u64; STACK_SLOTS], ret: CRet) -> CVal {
    type Sig<R> = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64,
                                        f64, f64, f64, f64, f64, f64, f64, f64,
                                        u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> R;
    unsafe {
        match ret {
            CRet::F64 => {
                let g: Sig<f64> = std::mem::transmute(fp);
                CVal::F64(g(i[0], i[1], i[2], i[3], i[4], i[5], i[6], i[7],
                            f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
                            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9]))
            }
            CRet::I32 => {
                let g: Sig<u64> = std::mem::transmute(fp);
                let r = g(i[0], i[1], i[2], i[3], i[4], i[5], i[6], i[7],
                          f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
                          s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9]);
                CVal::I32(r as u32 as i32) // an int comes back in w0
            }
        }
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
unsafe fn raw_call(_: *const c_void, _: &[u64; 8], _: &[f64; FP_REGS], _: &[u64; STACK_SLOTS], _: CRet) -> CVal {
    unreachable!("supported() is false here")
}

// ---------------------------------------------------------------- libraries and symbols
#[cfg(not(target_arch = "wasm32"))]
mod dl {
    use super::*;
    const RTLD_NOW: c_int = 2;
    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }
    fn dl_error() -> String {
        let e = unsafe { dlerror() };
        if e.is_null() { "unknown error".into() } else { unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned() }
    }
    pub fn open(path: &str) -> Result<usize, String> {
        let c = CString::new(path).map_err(|_| "the path has a NUL character".to_string())?;
        let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
        if h.is_null() { Err(dl_error()) } else { Ok(h as usize) }
    }
    pub fn sym(h: usize, name: &str) -> Option<usize> {
        let c = CString::new(name).ok()?;
        let p = unsafe { dlsym(h as *mut c_void, c.as_ptr()) };
        (!p.is_null()).then_some(p as usize)
    }
}

#[cfg(target_arch = "wasm32")]
mod dl {
    pub fn open(_: &str) -> Result<usize, String> {
        Err("the browser can't load C libraries".into())
    }
    pub fn sym(_: usize, _: &str) -> Option<usize> {
        None
    }
}

fn libs() -> &'static Mutex<HashMap<String, Result<usize, String>>> {
    static L: OnceLock<Mutex<HashMap<String, Result<usize, String>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Open the shared library at `path` (once; later calls reuse the handle). The error is dlopen's message.
pub fn open_library(path: &str) -> Result<usize, String> {
    let mut m = libs().lock().unwrap_or_else(|e| e.into_inner());
    m.entry(path.to_string()).or_insert_with(|| dl::open(path)).clone()
}

/// The address of `symbol` in the library at `path`, or None when it doesn't define it.
pub fn symbol(path: &str, symbol: &str) -> Result<Option<usize>, String> {
    let h = open_library(path)?;
    Ok(dl::sym(h, symbol))
}

/// The name a Fortran compiler gives `name` in the object file: gfortran's and flang's default (lowercase, one
/// trailing underscore); `bind(C)` without a name keeps the lowercase name; `bind(C, name="…")` is exact.
pub fn fortran_symbol(name: &str, bind_c: bool, bind_name: Option<&str>) -> String {
    match (bind_c, bind_name) {
        (_, Some(n)) => n.to_string(),
        (true, None) => name.to_lowercase(),
        (false, None) => format!("{}_", name.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn add3(a: f64, b: f64, c: f64) -> f64 {
        a + 10.0 * b + 100.0 * c
    }
    extern "C" fn twice(n: c_int) -> c_int {
        2 * n
    }
    extern "C" fn neg(n: c_int) -> c_int {
        -n
    }
    extern "C" fn by_ref(x: *const f64, n: *const c_int) -> f64 {
        unsafe { *x * *n as f64 }
    }
    extern "C" fn sum(x: *const f64, n: c_int) -> f64 {
        (0..n as usize).map(|i| unsafe { *x.add(i) }).sum()
    }
    /// 16 interleaved doubles and ints: every register of both classes, then the stack in argument order.
    #[allow(clippy::too_many_arguments)]
    extern "C" fn mixed16(a0: f64, i0: c_int, a1: f64, i1: c_int, a2: f64, i2: c_int, a3: f64, i3: c_int,
                          a4: f64, i4: c_int, a5: f64, i5: c_int, a6: f64, i6: c_int, a7: f64, i7: c_int) -> f64 {
        let f = [a0, a1, a2, a3, a4, a5, a6, a7];
        let i = [i0, i1, i2, i3, i4, i5, i6, i7];
        (0..8).map(|k| f[k] * (k as f64 + 1.0) + i[k] as f64 * 1000.0 * (k as f64 + 1.0)).sum()
    }
    #[allow(clippy::too_many_arguments)]
    extern "C" fn doubles16(a0: f64, a1: f64, a2: f64, a3: f64, a4: f64, a5: f64, a6: f64, a7: f64, a8: f64,
                            a9: f64, a10: f64, a11: f64, a12: f64, a13: f64, a14: f64, a15: f64) -> f64 {
        let v = [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15];
        v.iter().enumerate().map(|(k, x)| x * 2f64.powi(k as i32)).sum()
    }
    #[allow(clippy::too_many_arguments)]
    extern "C" fn ints16(a0: c_int, a1: c_int, a2: c_int, a3: c_int, a4: c_int, a5: c_int, a6: c_int, a7: c_int,
                         a8: c_int, a9: c_int, a10: c_int, a11: c_int, a12: c_int, a13: c_int, a14: c_int,
                         a15: c_int) -> c_int {
        let v = [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15];
        v.iter().enumerate().map(|(k, x)| x * (k as c_int + 1)).sum()
    }
    #[allow(clippy::too_many_arguments)]
    extern "C" fn ptrs10(p0: *const f64, p1: *const f64, p2: *const f64, p3: *const f64, p4: *const f64,
                         p5: *const f64, p6: *const f64, p7: *const f64, x: f64, p8: *const f64, p9: *const f64) -> f64 {
        let v = [p0, p1, p2, p3, p4, p5, p6, p7, p8, p9];
        x + v.iter().enumerate().map(|(k, p)| unsafe { **p } * (k as f64 + 1.0)).sum::<f64>()
    }

    fn fp<T>(f: T) -> *const c_void {
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
        unsafe { std::mem::transmute_copy(&f) }
    }

    #[test]
    fn doubles_and_ints() {
        let f = fp(add3 as extern "C" fn(f64, f64, f64) -> f64);
        let r = unsafe { call(f, &[CArg::F64(1.0), CArg::F64(2.0), CArg::F64(3.0)], CRet::F64) };
        assert_eq!(r, Ok(CVal::F64(321.0)));
        let f = fp(twice as extern "C" fn(c_int) -> c_int);
        assert_eq!(unsafe { call(f, &[CArg::I32(21)], CRet::I32) }, Ok(CVal::I32(42)));
        let f = fp(neg as extern "C" fn(c_int) -> c_int);
        assert_eq!(unsafe { call(f, &[CArg::I32(7)], CRet::I32) }.map(CVal::num), Ok(-7.0));
    }

    #[test]
    fn pointers() {
        let (x, n) = (2.5f64, 4 as c_int);
        let f = fp(by_ref as extern "C" fn(*const f64, *const c_int) -> f64);
        let args = [CArg::Ptr(&x as *const f64 as _), CArg::Ptr(&n as *const c_int as _)];
        assert_eq!(unsafe { call(f, &args, CRet::F64) }, Ok(CVal::F64(10.0)));
        let xs = [1.0, 2.0, 3.5];
        let f = fp(sum as extern "C" fn(*const f64, c_int) -> f64);
        assert_eq!(unsafe { call(f, &[CArg::Ptr(xs.as_ptr() as _), CArg::I32(3)], CRet::F64) }, Ok(CVal::F64(6.5)));
    }

    #[test]
    fn sixteen_arguments_spill_to_the_stack() {
        type M = extern "C" fn(f64, c_int, f64, c_int, f64, c_int, f64, c_int, f64, c_int, f64, c_int, f64, c_int,
                               f64, c_int) -> f64;
        let f = fp(mixed16 as M);
        let mut args = vec![];
        for k in 0..8 {
            args.push(CArg::F64(0.5 + k as f64));
            args.push(CArg::I32(k + 1));
        }
        let want = mixed16(0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5, 5.5, 6, 6.5, 7, 7.5, 8);
        assert_eq!(unsafe { call(f, &args, CRet::F64) }, Ok(CVal::F64(want)));

        type D = extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64;
        let f = fp(doubles16 as D);
        let args: Vec<CArg> = (0..16).map(|k| CArg::F64(k as f64 + 1.0)).collect();
        let want = doubles16(1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 14., 15., 16.);
        assert_eq!(unsafe { call(f, &args, CRet::F64) }, Ok(CVal::F64(want)));

        type N = extern "C" fn(c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int,
                               c_int, c_int, c_int, c_int) -> c_int;
        let f = fp(ints16 as N);
        let args: Vec<CArg> = (0..16).map(|k| CArg::I32(k - 3)).collect();
        let want = ints16(-3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
        if cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
            assert!(unsafe { call(f, &args, CRet::I32) }.is_err());
        } else {
            assert_eq!(unsafe { call(f, &args, CRet::I32) }, Ok(CVal::I32(want)));
        }

        type P = extern "C" fn(*const f64, *const f64, *const f64, *const f64, *const f64, *const f64, *const f64,
                               *const f64, f64, *const f64, *const f64) -> f64;
        let f = fp(ptrs10 as P);
        let vals: Vec<f64> = (0..10).map(|k| k as f64 * 0.25).collect();
        let mut args: Vec<CArg> = vals[..8].iter().map(|v| CArg::Ptr(v as *const f64 as _)).collect();
        args.push(CArg::F64(100.0));
        args.extend(vals[8..].iter().map(|v| CArg::Ptr(v as *const f64 as _)));
        let want = ptrs10(&vals[0], &vals[1], &vals[2], &vals[3], &vals[4], &vals[5], &vals[6], &vals[7], 100.0,
                          &vals[8], &vals[9]);
        assert_eq!(unsafe { call(f, &args, CRet::F64) }, Ok(CVal::F64(want)));
    }

    #[test]
    fn too_many_arguments() {
        let f = fp(add3 as extern "C" fn(f64, f64, f64) -> f64);
        let args = vec![CArg::F64(0.0); 17];
        assert!(unsafe { call(f, &args, CRet::F64) }.unwrap_err().contains("at most 16"));
    }

    #[test]
    fn fortran_names() {
        assert_eq!(fortran_symbol("Binding_Energy", false, None), "binding_energy_");
        assert_eq!(fortran_symbol("Binding_Energy", true, None), "binding_energy");
        assert_eq!(fortran_symbol("x", true, Some("semf_Sn")), "semf_Sn");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn libm_through_dlopen() {
        let cos = symbol("libm.so.6", "cos").unwrap().expect("cos in libm");
        let r = unsafe { call(cos as *const c_void, &[CArg::F64(0.0)], CRet::F64) };
        assert_eq!(r, Ok(CVal::F64(1.0)));
        assert_eq!(symbol("libm.so.6", "no_such_function_here").unwrap(), None);
        assert!(open_library("/nonexistent/libnothing.so").is_err());
    }
}
