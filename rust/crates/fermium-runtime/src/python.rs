//! Python interop at run time (`use python numpy as np`, DECISIONS D140, spec §B5.14).
//!
//! libpython is loaded with `dlopen` the first time a program uses Python, never before: the fermium binary has
//! no link-time dependency on Python, and runs on a machine without it as long as the program doesn't use it.
//! The CPython C API is reached through function pointers found with `dlsym` (only functions of the stable
//! ABI). The conversions and messages live in a small Python module (`BRIDGE` below), a port of Fermium 1.5's
//! `fermium/runtime/pycall.py`, so they are v1's exactly (NumPy arrays in, lists out, the same wording).
//!
//! Which Python: `FERMIUM_LIBPYTHON` (a path to libpython3.x.so) if set, else the interpreter named by
//! `FERMIUM_PYTHON`, else `python3` on the PATH, then /usr/local/bin/python3 and /usr/bin/python3: its
//! `sysconfig` says where its shared library is, and the interpreter's own path is given to Python as the
//! program name, so it finds its standard library and site-packages (NumPy, SciPy) as that `python3` does.
//! When the process already runs Python (the ctypes API, D142), that interpreter is used.

/// A loaded Python and the bridge module. Every operation holds the GIL (PyGILState_Ensure/Release), so any
/// thread may call it.
#[cfg(not(target_arch = "wasm32"))]
pub use imp::*;

#[cfg(target_arch = "wasm32")]
mod wasm {
    /// The outcome of importing a Python module (see the native version).
    pub enum Import {
        Ok,
        Missing(String),
        Failed(String),
    }
    /// What a module attribute is.
    pub enum Attr {
        Missing(Option<String>),
        Callable(String),
        Number(f64),
        Other(String),
    }
    pub enum Arg<'a> {
        Num(f64),
        List(&'a [f64]),
    }
    pub enum PyValue {
        Num(f64),
        List(Vec<f64>),
    }
    pub struct CallSite<'a> {
        pub module: &'a str,
        pub func: &'a str,
        pub display: &'a str,
        pub facs: &'a [f64],
        pub ints: &'a [bool],
        pub pnames: &'a [String],
        pub rlist: bool,
        pub rfac: f64,
        pub declared: bool,
    }
    const NO: &str = "Python can't be used here (the browser playground has no Python); run the program with  fermium run";
    pub fn import_module(_name: &str, _base_dir: &str) -> Result<Import, String> {
        Err(NO.into())
    }
    pub fn attribute(_module: &str, _attr: &str, _ascii: &str) -> Result<Attr, String> {
        Err(NO.into())
    }
    pub fn call(_site: &CallSite, _args: &[Arg], _base_dir: &str) -> Result<PyValue, String> {
        Err(NO.into())
    }
    pub fn is_loaded() -> bool {
        false
    }
}
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::ffi::{c_char, c_int, c_long, c_void, CStr, CString};
    use std::sync::OnceLock;

    type Obj = *mut c_void;

    const RTLD_NOW: c_int = 2;
    const RTLD_GLOBAL: c_int = 0x100;
    const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
    const PY_FILE_INPUT: c_int = 257;

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }

    /// The v1 conversion code (pycall.py), run inside Python. Every entry point returns a tuple and never raises.
    const BRIDGE: &str = r#"
import sys, importlib, math
try:
    import numpy as np
except Exception:
    np = None


class PyCallError(Exception):
    pass


_fns = {}


def _import(name, base_dir):
    added = False
    if base_dir and base_dir not in sys.path:
        sys.path.insert(0, base_dir)
        added = True
    try:
        return importlib.import_module(name)
    finally:
        if added:
            try:
                sys.path.remove(base_dir)
            except ValueError:
                pass


def fm_import(name, base_dir):
    try:
        _import(name, base_dir)
        return ("ok", "")
    except ModuleNotFoundError as ex:
        return ("missing", getattr(ex, "name", None) or name)
    except Exception as ex:
        return ("fail", f"{type(ex).__name__}: {ex}")
    except BaseException as ex:
        return ("fail", f"{type(ex).__name__}: {ex}")


def _is_number(x):
    import numbers
    return isinstance(x, numbers.Real) and not isinstance(x, bool)


def fm_attr(module, attr, ascii_attr):
    try:
        pymod = sys.modules[module]
        try:
            obj = getattr(pymod, attr)
            used = attr
        except AttributeError:
            obj = None
            used = ascii_attr
            if ascii_attr != attr:
                obj = getattr(pymod, ascii_attr, None)
            if obj is None:
                from difflib import get_close_matches
                names = [n for n in dir(pymod) if not n.startswith("_")]
                close = get_close_matches(attr, names, n=1, cutoff=0.6)
                return ("missing", close[0] if close else "")
        if callable(obj):
            return ("callable", used)
        if _is_number(obj):
            return ("number", float(obj))
        return ("other", type(obj).__name__)
    except BaseException as ex:
        return ("error", f"{type(ex).__name__}: {ex}")


def _function(module, func, display, base_dir):
    key = (module, func)
    fn = _fns.get(key)
    if fn is None:
        try:
            mod = _import(module, base_dir)
            fn = getattr(mod, func)
        except Exception as ex:
            raise PyCallError(f"can't load the Python function {display}: {ex}") from None
        _fns[key] = fn
    return fn


def _whole(x):
    return x == x and abs(x) < math.inf and x == math.floor(x)


def _what(r):
    if isinstance(r, str):
        return f"the text {r!r}"
    if isinstance(r, (tuple, list)) or (np is not None and isinstance(r, np.ndarray)):
        if np is None:
            return f"a list of {len(r)} numbers"
        a = np.asarray(r, dtype=object)
        if a.ndim == 1:
            return f"a list of {len(a)} numbers"
        return f"an array of shape {a.shape}"
    return f"a {type(r).__name__}"


def _call(module, func, display, facs, ints, pnames, rlist, rfac, declared, args, base_dir):
    name = display
    fn = _function(module, func, display, base_dir)
    conv = []
    for k, a in enumerate(args):
        fac, is_int, pname = facs[k], ints[k], pnames[k]
        if isinstance(a, (bytes, bytearray)):
            if np is not None:
                arr = np.frombuffer(a, dtype=np.float64).copy()
                if fac != 1.0:
                    arr = arr / fac
                if is_int:
                    if not all(_whole(x) for x in arr):
                        raise PyCallError(f"{name}: {pname} must be a list of whole numbers (it is passed as ints)")
                    arr = arr.astype(np.int64)
            else:
                import array
                arr = list(array.array("d", a))
                if fac != 1.0:
                    arr = [x / fac for x in arr]
                if is_int:
                    if not all(_whole(x) for x in arr):
                        raise PyCallError(f"{name}: {pname} must be a list of whole numbers (it is passed as ints)")
                    arr = [int(x) for x in arr]
            conv.append(arr)
        else:
            x = float(a) / fac if fac != 1.0 else float(a)
            if is_int:
                if not _whole(x):
                    raise PyCallError(f"{name}: {pname} must be a whole number (it is passed as an int), "
                                      f"not {x:g}")
                x = int(x)
            conv.append(x)
    try:
        if np is not None:
            with np.errstate(all="ignore"):
                r = fn(*conv)
        else:
            r = fn(*conv)
    except Exception as ex:
        msg = str(ex).strip().splitlines()[0] if str(ex).strip() else ""
        hint = ""
        if isinstance(ex, TypeError) and "integer" in msg and not any(ints):
            hint = (f" (Fermium passes numbers as floats; mark a whole-number parameter in the use line, like  "
                    f"{func}(a, b, n: int))")
        raise PyCallError(f"the Python function {name} failed: {type(ex).__name__}" + (f": {msg}" if msg else "")
                          + hint) from None
    finally:
        try:
            sys.stdout.flush()
        except Exception:
            pass
    return _convert(name, func, rlist, rfac, declared, r)


def _iscomplex(r):
    if np is not None:
        return np.iscomplexobj(r)
    return isinstance(r, complex) or (isinstance(r, (list, tuple)) and any(isinstance(x, complex) for x in r))


def _convert(name, func, rlist, rfac, declared, r):
    if r is None:
        raise PyCallError(f"the Python function {name} returned nothing (None), but Fermium needs a number")
    if rlist:
        if isinstance(r, str) or _iscomplex(r):
            raise PyCallError(f"{name} returned {_what(r) if isinstance(r, str) else 'complex numbers'}, but "
                              f"Fermium expected a list of real numbers here")
        if np is None:
            if not isinstance(r, (list, tuple)):
                why = "declared in the use line" if declared else "one of its arguments is a list"
                raise PyCallError(f"{name} returned a single number, but Fermium expected a list here ({why}); "
                                  f"declare the result as a number in the use line, like  {func}(...) -> number")
            import array
            try:
                out = array.array("d", [float(x) * rfac for x in r])
            except (TypeError, ValueError):
                raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a list of numbers here") from None
            return out.tobytes()
        try:
            arr = np.asarray(r, dtype=float)
        except (TypeError, ValueError):
            raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a list of numbers here") from None
        if arr.ndim == 0:
            why = "declared in the use line" if declared else "one of its arguments is a list"
            raise PyCallError(f"{name} returned a single number, but Fermium expected a list here ({why}); "
                              f"declare the result as a number in the use line, like  {func}(...) -> number")
        if arr.ndim > 1:
            raise PyCallError(f"{name} returned {_what(r)}; Fermium lists have one dimension")
        if rfac != 1.0:
            arr = arr * rfac
        return np.ascontiguousarray(arr, dtype=np.float64).tobytes()
    if isinstance(r, bool) or (np is not None and isinstance(r, np.bool_)):
        r = float(r)
    if _iscomplex(r):
        raise PyCallError(f"{name} returned a complex number, but Fermium expected a real number here")
    if isinstance(r, str) or (np.ndim(r) != 0 if np is not None else isinstance(r, (list, tuple))):
        raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a number here; if it returns a "
                          f"list, declare it in the use line, like  {func}(...) -> list")
    try:
        x = float(r)
    except (TypeError, ValueError):
        raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a number here") from None
    return x * rfac if rfac != 1.0 else x


def fm_call(*a):
    try:
        return (0, _call(*a))
    except PyCallError as ex:
        return (1, str(ex))
    except BaseException as ex:
        return (1, f"calling Python failed: {type(ex).__name__}: {ex}")
"#;

    macro_rules! api {
        ($($name:ident : fn($($a:ty),*) -> $r:ty;)*) => {
            #[allow(non_snake_case)]
            struct Api { $($name: unsafe extern "C" fn($($a),*) -> $r,)* }
            impl Api {
                unsafe fn load(h: *mut c_void) -> Result<Api, String> {
                    Ok(Api { $($name: {
                        let p = unsafe { dlsym(h, concat!(stringify!($name), "\0").as_ptr() as *const c_char) };
                        if p.is_null() {
                            return Err(format!("the Python library has no {}", stringify!($name)));
                        }
                        unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($a),*) -> $r>(p) }
                    },)* })
                }
            }
        };
    }

    api! {
        Py_IsInitialized: fn() -> c_int;
        Py_InitializeEx: fn(c_int) -> ();
        PyEval_SaveThread: fn() -> Obj;
        PyGILState_Ensure: fn() -> c_int;
        PyGILState_Release: fn(c_int) -> ();
        Py_CompileString: fn(*const c_char, *const c_char, c_int) -> Obj;
        PyImport_ExecCodeModule: fn(*const c_char, Obj) -> Obj;
        PyObject_GetAttrString: fn(Obj, *const c_char) -> Obj;
        PyObject_CallObject: fn(Obj, Obj) -> Obj;
        PyTuple_New: fn(isize) -> Obj;
        PyTuple_SetItem: fn(Obj, isize, Obj) -> c_int;
        PyTuple_GetItem: fn(Obj, isize) -> Obj;
        PyList_New: fn(isize) -> Obj;
        PyList_SetItem: fn(Obj, isize, Obj) -> c_int;
        PyUnicode_FromString: fn(*const c_char) -> Obj;
        PyUnicode_AsUTF8String: fn(Obj) -> Obj;
        PyBytes_FromStringAndSize: fn(*const c_char, isize) -> Obj;
        PyBytes_AsString: fn(Obj) -> *mut c_char;
        PyBytes_Size: fn(Obj) -> isize;
        PyFloat_FromDouble: fn(f64) -> Obj;
        PyFloat_AsDouble: fn(Obj) -> f64;
        PyLong_AsLong: fn(Obj) -> c_long;
        PyBool_FromLong: fn(c_long) -> Obj;
        PyErr_Occurred: fn() -> Obj;
        PyErr_Fetch: fn(*mut Obj, *mut Obj, *mut Obj) -> ();
        PyErr_Clear: fn() -> ();
        PyObject_Str: fn(Obj) -> Obj;
        Py_DecRef: fn(Obj) -> ();
    }

    struct Python {
        api: Api,
        bridge: Obj,
    }
    // the bridge module is only touched with the GIL held
    unsafe impl Send for Python {}
    unsafe impl Sync for Python {}

    static PY: OnceLock<Result<Python, String>> = OnceLock::new();

    fn dl_error() -> String {
        let e = unsafe { dlerror() };
        if e.is_null() { "unknown error".into() } else { unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned() }
    }

    /// (library candidates, the interpreter's path) from a python3 executable's sysconfig.
    fn ask_python(exe: &str) -> Option<(Vec<String>, String)> {
        let script = "import sys, sysconfig\nv = sysconfig.get_config_var\n\
                      print(v('LIBDIR') or ''); print(v('INSTSONAME') or ''); print(v('LDLIBRARY') or '')\n\
                      print(sys.executable or ''); print(v('MULTIARCH') or '')\n\
                      print(sys.base_prefix or ''); print(v('PYTHONFRAMEWORKPREFIX') or '')";
        let out = std::process::Command::new(exe).args(["-c", script]).stderr(std::process::Stdio::null())
            .output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let l: Vec<&str> = text.lines().collect();
        let (libdir, inst, ld, exe_path, multi) = (l.first()?, l.get(1)?, l.get(2)?, l.get(3)?, l.get(4).unwrap_or(&""));
        let (base, fw_prefix) = (l.get(5).unwrap_or(&""), l.get(6).unwrap_or(&""));
        let mut cands = vec![];
        // a macOS framework build (python.org, setup-python): LDLIBRARY is "Python.framework/Versions/X.Y/Python"
        // and LIBDIR the build machine's path, which a relocated install (a CI tool cache) doesn't have; the
        // library is the framework's `Python` file, at sys.base_prefix (…/Python.framework/Versions/X.Y)
        if ld.contains(".framework/") {
            if !base.is_empty() {
                cands.push(format!("{base}/Python"));
            }
            if !fw_prefix.is_empty() {
                cands.push(format!("{fw_prefix}/{ld}"));
            }
        }
        for name in [inst, ld] {
            if name.is_empty() || name.ends_with(".a") {
                continue;
            }
            if !libdir.is_empty() {
                cands.push(format!("{libdir}/{name}"));
                if !multi.is_empty() {
                    cands.push(format!("{libdir}/{multi}/{name}"));
                }
            }
            // a relocated install: the library under the interpreter's own prefix
            if !base.is_empty() && !name.contains('/') {
                cands.push(format!("{base}/lib/{name}"));
            }
            cands.push(name.to_string());
        }
        Some((cands, if exe_path.is_empty() { exe.to_string() } else { exe_path.to_string() }))
    }

    /// Find and dlopen libpython; returns the handle and the interpreter path (for the program name).
    fn open_library() -> Result<(*mut c_void, Option<String>), String> {
        if let Some(p) = std::env::var_os("FERMIUM_LIBPYTHON") {
            let p = p.to_string_lossy().into_owned();
            let c = CString::new(p.clone()).map_err(|_| "bad FERMIUM_LIBPYTHON".to_string())?;
            let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
            if h.is_null() {
                return Err(format!("can't load {p} (FERMIUM_LIBPYTHON): {}", dl_error()));
            }
            return Ok((h, std::env::var("FERMIUM_PYTHON").ok()));
        }
        let mut exes: Vec<String> = vec![];
        if let Ok(p) = std::env::var("FERMIUM_PYTHON") {
            exes.push(p);
        }
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                let c = dir.join("python3");
                if c.is_file() {
                    exes.push(c.to_string_lossy().into_owned());
                    break;
                }
            }
        }
        for c in ["/usr/local/bin/python3", "/usr/bin/python3", "/opt/homebrew/bin/python3"] {
            if std::path::Path::new(c).is_file() && !exes.iter().any(|e| e == c) {
                exes.push(c.to_string());
            }
        }
        let mut tried = vec![];
        for exe in &exes {
            let Some((cands, exe_path)) = ask_python(exe) else {
                tried.push(format!("{exe} didn't run"));
                continue;
            };
            for cand in &cands {
                let c = CString::new(cand.clone()).unwrap();
                let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
                if !h.is_null() {
                    return Ok((h, Some(exe_path)));
                }
                tried.push(format!("{cand}: {}", dl_error()));
            }
            if cands.is_empty() {
                tried.push(format!("{exe} has no shared library (it was built without --enable-shared)"));
            }
        }
        if exes.is_empty() {
            return Err("no python3 was found".into());
        }
        Err(format!("its shared library wasn't found ({})", tried.join("; ")))
    }

    fn set_program_name(h: *mut c_void, exe: &str) {
        // Py_SetProgramName(wchar_t*) so Python finds the standard library and site-packages next to that
        // interpreter (deprecated in 3.11, but present; skipped where it is gone)
        unsafe {
            let decode = dlsym(h, c"Py_DecodeLocale".as_ptr());
            let setname = dlsym(h, c"Py_SetProgramName".as_ptr());
            if decode.is_null() || setname.is_null() {
                return;
            }
            let decode: unsafe extern "C" fn(*const c_char, *mut usize) -> *mut c_void = std::mem::transmute(decode);
            let setname: unsafe extern "C" fn(*const c_void) = std::mem::transmute(setname);
            let Ok(c) = CString::new(exe) else { return };
            let w = decode(c.as_ptr(), std::ptr::null_mut());
            if !w.is_null() {
                setname(w); // Python keeps the pointer: never freed
            }
        }
    }

    fn load() -> Result<Python, String> {
        // already inside a Python process (fermium's ctypes API, D142)?
        let own = unsafe { dlsym(RTLD_DEFAULT, c"Py_IsInitialized".as_ptr()) };
        let running = !own.is_null() && unsafe {
            std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> c_int>(own)()
        } != 0;
        let h = if running {
            RTLD_DEFAULT
        } else {
            let (h, exe) = open_library()?;
            let api = unsafe { Api::load(h)? };
            if unsafe { (api.Py_IsInitialized)() } == 0 {
                if let Some(exe) = exe {
                    set_program_name(h, &exe);
                }
                unsafe {
                    (api.Py_InitializeEx)(0);
                    (api.PyEval_SaveThread)(); // release the GIL: every call below takes it
                }
            }
            h
        };
        let api = unsafe { Api::load(h)? };
        let g = unsafe { (api.PyGILState_Ensure)() };
        let src = CString::new(BRIDGE).unwrap();
        let bridge = unsafe {
            let code = (api.Py_CompileString)(src.as_ptr(), c"<fermium python bridge>".as_ptr(), PY_FILE_INPUT);
            if code.is_null() {
                std::ptr::null_mut()
            } else {
                let m = (api.PyImport_ExecCodeModule)(c"_fermium_bridge".as_ptr(), code);
                (api.Py_DecRef)(code);
                m
            }
        };
        let err = if bridge.is_null() { Some(fetch_error(&api)) } else { None };
        unsafe { (api.PyGILState_Release)(g) };
        match err {
            Some(e) => Err(format!("Python started, but Fermium's bridge module failed: {e}")),
            None => Ok(Python { api, bridge }),
        }
    }

    fn fetch_error(api: &Api) -> String {
        unsafe {
            if (api.PyErr_Occurred)().is_null() {
                return "unknown error".into();
            }
            let (mut t, mut v, mut tb) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (api.PyErr_Fetch)(&mut t, &mut v, &mut tb);
            let msg = if v.is_null() { "error".to_string() } else { str_of(api, v) };
            for o in [t, v, tb] {
                if !o.is_null() {
                    (api.Py_DecRef)(o);
                }
            }
            (api.PyErr_Clear)();
            msg
        }
    }

    unsafe fn str_of(api: &Api, o: Obj) -> String {
        unsafe {
            let s = (api.PyObject_Str)(o);
            if s.is_null() {
                (api.PyErr_Clear)();
                return "?".into();
            }
            let r = utf8(api, s);
            (api.Py_DecRef)(s);
            r
        }
    }

    /// A Python str as a Rust String (borrowed reference in).
    unsafe fn utf8(api: &Api, s: Obj) -> String {
        unsafe {
            let b = (api.PyUnicode_AsUTF8String)(s);
            if b.is_null() {
                (api.PyErr_Clear)();
                return "?".into();
            }
            let p = (api.PyBytes_AsString)(b);
            let n = (api.PyBytes_Size)(b);
            let r = String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, n.max(0) as usize)).into_owned();
            (api.Py_DecRef)(b);
            r
        }
    }

    fn python() -> Result<&'static Python, String> {
        match PY.get_or_init(load) {
            Ok(p) => Ok(p),
            Err(e) => Err(e.clone()),
        }
    }

    /// Has a program loaded Python in this process?
    pub fn is_loaded() -> bool {
        matches!(PY.get(), Some(Ok(_)))
    }

    /// Holding the GIL for the lifetime of the guard.
    struct Gil<'a> {
        py: &'a Python,
        state: c_int,
    }
    impl<'a> Gil<'a> {
        fn new(py: &'a Python) -> Gil<'a> {
            Gil { py, state: unsafe { (py.api.PyGILState_Ensure)() } }
        }
        fn str(&self, s: &str) -> Obj {
            let c = CString::new(s.replace('\0', "")).unwrap();
            unsafe { (self.py.api.PyUnicode_FromString)(c.as_ptr()) }
        }
        fn tuple(&self, items: Vec<Obj>) -> Obj {
            unsafe {
                let t = (self.py.api.PyTuple_New)(items.len() as isize);
                for (i, o) in items.into_iter().enumerate() {
                    (self.py.api.PyTuple_SetItem)(t, i as isize, o); // steals o
                }
                t
            }
        }
        fn list(&self, items: Vec<Obj>) -> Obj {
            unsafe {
                let t = (self.py.api.PyList_New)(items.len() as isize);
                for (i, o) in items.into_iter().enumerate() {
                    (self.py.api.PyList_SetItem)(t, i as isize, o);
                }
                t
            }
        }
        /// Call a bridge function with (new references to) its arguments; the result tuple, or the error.
        fn call(&self, func: &CStr, args: Vec<Obj>) -> Result<Obj, String> {
            unsafe {
                let api = &self.py.api;
                let f = (api.PyObject_GetAttrString)(self.py.bridge, func.as_ptr());
                if f.is_null() {
                    return Err(fetch_error(api));
                }
                let t = self.tuple(args);
                let r = (api.PyObject_CallObject)(f, t);
                (api.Py_DecRef)(t);
                (api.Py_DecRef)(f);
                if r.is_null() {
                    return Err(format!("calling Python failed: {}", fetch_error(api)));
                }
                Ok(r)
            }
        }
        fn item(&self, t: Obj, i: isize) -> Obj {
            unsafe { (self.py.api.PyTuple_GetItem)(t, i) }
        }
        fn text(&self, o: Obj) -> String {
            unsafe { utf8(&self.py.api, o) }
        }
        fn decref(&self, o: Obj) {
            unsafe { (self.py.api.Py_DecRef)(o) }
        }
    }
    impl Drop for Gil<'_> {
        fn drop(&mut self) {
            unsafe { (self.py.api.PyGILState_Release)(self.state) }
        }
    }

    /// The outcome of importing a Python module when the program is checked.
    pub enum Import {
        Ok,
        /// ModuleNotFoundError: the name of the missing module (maybe a dependency of the one imported)
        Missing(String),
        /// another exception: "TypeName: message"
        Failed(String),
    }

    /// Import a Python module, also looking in the program's folder (for the user's own mylib.py). Err: Python
    /// itself couldn't be loaded.
    pub fn import_module(name: &str, base_dir: &str) -> Result<Import, String> {
        let py = python()?;
        let g = Gil::new(py);
        let r = g.call(c"fm_import", vec![g.str(name), g.str(base_dir)])?;
        let kind = g.text(g.item(r, 0));
        let val = g.text(g.item(r, 1));
        g.decref(r);
        Ok(match kind.as_str() {
            "ok" => Import::Ok,
            "missing" => Import::Missing(val),
            _ => Import::Failed(val),
        })
    }

    /// What an attribute of an imported module is.
    pub enum Attr {
        /// no such attribute; the closest name (difflib, as v1)
        Missing(Option<String>),
        /// a callable, and the name to call it by (the ASCII spelling when only that exists: sp.γ → gamma)
        Callable(String),
        Number(f64),
        /// something else: its Python type name
        Other(String),
    }

    /// Look up `module.attr` (`ascii`: the attribute with Greek letters spelled out, tried when `attr` is missing).
    pub fn attribute(module: &str, attr: &str, ascii: &str) -> Result<Attr, String> {
        let py = python()?;
        let g = Gil::new(py);
        let r = g.call(c"fm_attr", vec![g.str(module), g.str(attr), g.str(ascii)])?;
        let kind = g.text(g.item(r, 0));
        let v = g.item(r, 1);
        let out = match kind.as_str() {
            "missing" => {
                let s = g.text(v);
                Attr::Missing(if s.is_empty() { None } else { Some(s) })
            }
            "callable" => Attr::Callable(g.text(v)),
            "number" => Attr::Number(unsafe { (py.api.PyFloat_AsDouble)(v) }),
            "other" => Attr::Other(g.text(v)),
            _ => {
                let m = g.text(v);
                g.decref(r);
                return Err(m);
            }
        };
        g.decref(r);
        Ok(out)
    }

    /// An argument passed to Python: a number or a list, in SI units.
    pub enum Arg<'a> {
        Num(f64),
        List(&'a [f64]),
    }

    /// What Python gave back, in SI units.
    pub enum PyValue {
        Num(f64),
        List(Vec<f64>),
    }

    /// One call site of a Python function (the checker's `tables.pycalls` entry).
    pub struct CallSite<'a> {
        pub module: &'a str,
        pub func: &'a str,
        pub display: &'a str,
        pub facs: &'a [f64],
        pub ints: &'a [bool],
        pub pnames: &'a [String],
        pub rlist: bool,
        pub rfac: f64,
        pub declared: bool,
    }

    /// Call the Python function of a call site. Err: the one-line message the program stops with.
    pub fn call(site: &CallSite, args: &[Arg], base_dir: &str) -> Result<PyValue, String> {
        let py = python()?;
        let g = Gil::new(py);
        let api = &py.api;
        let (facs, ints, pnames, argv) = unsafe {
            let facs = g.list(site.facs.iter().map(|f| (api.PyFloat_FromDouble)(*f)).collect());
            let ints = g.list(site.ints.iter().map(|b| (api.PyBool_FromLong)(*b as c_long)).collect());
            let pnames = g.list(site.pnames.iter().map(|p| g.str(p)).collect());
            let argv = g.list(args.iter().map(|a| match a {
                Arg::Num(x) => (api.PyFloat_FromDouble)(*x),
                Arg::List(xs) => (api.PyBytes_FromStringAndSize)(xs.as_ptr() as *const c_char, (xs.len() * 8) as isize),
            }).collect());
            (facs, ints, pnames, argv)
        };
        let items = unsafe {
            vec![g.str(site.module), g.str(site.func), g.str(site.display), facs, ints, pnames,
                 (api.PyBool_FromLong)(site.rlist as c_long), (api.PyFloat_FromDouble)(site.rfac),
                 (api.PyBool_FromLong)(site.declared as c_long), argv, g.str(base_dir)]
        };
        let r = g.call(c"fm_call", items)?;
        let status = unsafe { (api.PyLong_AsLong)(g.item(r, 0)) };
        let v = g.item(r, 1);
        let out = if status != 0 {
            Err(g.text(v))
        } else if site.rlist {
            unsafe {
                let p = (api.PyBytes_AsString)(v) as *const u8;
                let n = (api.PyBytes_Size)(v).max(0) as usize / 8;
                let mut xs = vec![0f64; n];
                std::ptr::copy_nonoverlapping(p, xs.as_mut_ptr() as *mut u8, n * 8);
                Ok(PyValue::List(xs))
            }
        } else {
            Ok(PyValue::Num(unsafe { (api.PyFloat_AsDouble)(v) }))
        };
        g.decref(r);
        out
    }
}
