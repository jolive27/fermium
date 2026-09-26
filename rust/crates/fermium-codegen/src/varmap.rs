//! The evaluator's variable storage, faster than a HashMap (SipHash) on the hot paths.
//!
//! - [`VarMap`]: the variables of one call (a function's locals, a lambda's parameters): few entries, so a
//!   linear scan over the symbol ids (most recently inserted first) beats hashing.
//! - [`GlobalMap`]: the main program's variables, indexed directly by symbol id.
//!
//! Both have the HashMap methods the evaluator uses (get, insert, remove, contains_key, iter), with the same
//! meaning, so they are drop-in replacements.
use fermium_ir::SymId;

use crate::eval::Value;

#[derive(Clone, Debug, Default)]
pub struct VarMap {
    items: Vec<(SymId, Value)>,
}

impl VarMap {
    #[inline]
    fn pos(&self, k: SymId) -> Option<usize> {
        self.items.iter().rposition(|x| x.0 == k)
    }
    #[inline]
    pub fn get(&self, k: &SymId) -> Option<&Value> {
        self.pos(*k).map(|i| &self.items[i].1)
    }
    #[inline]
    pub fn get_mut(&mut self, k: &SymId) -> Option<&mut Value> {
        self.pos(*k).map(move |i| &mut self.items[i].1)
    }
    #[inline]
    pub fn insert(&mut self, k: SymId, v: Value) -> Option<Value> {
        match self.pos(k) {
            Some(i) => Some(std::mem::replace(&mut self.items[i].1, v)),
            None => {
                self.items.push((k, v));
                None
            }
        }
    }
    pub fn remove(&mut self, k: &SymId) -> Option<Value> {
        let i = self.pos(*k)?;
        Some(self.items.remove(i).1)
    }
    pub fn contains_key(&self, k: &SymId) -> bool {
        self.pos(*k).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&SymId, &Value)> {
        self.items.iter().map(|(k, v)| (k, v))
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn with_capacity(n: usize) -> VarMap {
        VarMap { items: Vec::with_capacity(n) }
    }
    pub fn capacity(&self) -> usize {
        self.items.capacity()
    }
}

impl FromIterator<(SymId, Value)> for VarMap {
    fn from_iter<I: IntoIterator<Item = (SymId, Value)>>(it: I) -> Self {
        let mut m = VarMap::default();
        for (k, v) in it {
            m.insert(k, v);
        }
        m
    }
}

#[derive(Clone, Debug, Default)]
pub struct GlobalMap {
    vals: Vec<Option<Value>>,
}

impl GlobalMap {
    #[inline]
    pub fn get(&self, k: &SymId) -> Option<&Value> {
        self.vals.get(*k).and_then(|v| v.as_ref())
    }
    #[inline]
    pub fn insert(&mut self, k: SymId, v: Value) -> Option<Value> {
        if k >= self.vals.len() {
            self.vals.resize(k + 1, None);
        }
        self.vals[k].replace(v)
    }
    pub fn remove(&mut self, k: &SymId) -> Option<Value> {
        self.vals.get_mut(*k).and_then(|v| v.take())
    }
    pub fn contains_key(&self, k: &SymId) -> bool {
        self.get(k).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (SymId, &Value)> {
        self.vals.iter().enumerate().filter_map(|(k, v)| v.as_ref().map(|v| (k, v)))
    }
}

thread_local! {
    /// Frames and argument vectors of finished calls, kept for the next ones (a hot function called millions of
    /// times from an ODE's right side then allocates nothing for them).
    static FRAMES: std::cell::RefCell<Vec<VarMap>> = const { std::cell::RefCell::new(Vec::new()) };
    static ARGS: std::cell::RefCell<Vec<Vec<Value>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// An empty frame with room for n variables.
#[inline]
pub fn take_frame(n: usize) -> VarMap {
    match FRAMES.with(|f| f.borrow_mut().pop()) {
        Some(mut m) => {
            m.items.reserve(n);
            m
        }
        None => VarMap::with_capacity(n),
    }
}

/// Give a finished call's frame back (its values are dropped now).
#[inline]
pub fn give_frame(mut m: VarMap) {
    m.clear();
    FRAMES.with(|f| {
        let mut f = f.borrow_mut();
        if f.len() < 64 {
            f.push(m);
        }
    });
}

/// An empty vector for n argument values.
#[inline]
pub fn take_args(n: usize) -> Vec<Value> {
    match ARGS.with(|f| f.borrow_mut().pop()) {
        Some(mut v) => {
            v.reserve(n);
            v
        }
        None => Vec::with_capacity(n),
    }
}

/// Give an argument vector back.
#[inline]
pub fn give_args(mut v: Vec<Value>) {
    v.clear();
    ARGS.with(|f| {
        let mut f = f.borrow_mut();
        if f.len() < 64 {
            f.push(v);
        }
    });
}
