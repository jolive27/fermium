//! Multiple dispatch (spec §C5, DECISIONS D285): one function name with several versions, chosen by the number of
//! arguments, their dimensions and their kinds (number, vector, list, complex).
//!
//! ```text
//! energy(m [kg], v [m/s]) = ½ m v²
//! energy(λ [m]) = h c / λ
//! energy(ν [Hz]) = h ν
//! ```
//!
//! Each definition of a name that is already a top-level function of this scope makes a new `FuncInfo` whose
//! `versions` lists every version visible from there (a definition that covers an earlier one, the same signature
//! or broader on every parameter, replaces it, which is v1's redefinition; D302). A call picks one version from the checked argument types, at compile
//! time, and instantiates it like any function: the IR, both back ends and the run time see ordinary calls.
use fermium_ir::types::Ty;
use fermium_syntax::ast as A;

use crate::checker::*;

/// How one argument fits one parameter: its specificity (0 unannotated, +1 for a kind, +1 for a unit) and whether
/// the fit depends on a dimension that isn't known yet (a generic check).
#[derive(Clone, Copy, Debug)]
struct Fit {
    score: u8,
    unsure: bool,
}

impl Checker {
    /// The versions a call of `info` chooses from (itself alone when it has one).
    pub fn versions_of(&self, info: FuncInfoId) -> Vec<FuncInfoId> {
        if self.funcs[info].versions.len() > 1 { self.funcs[info].versions.clone() } else { vec![info] }
    }

    /// Can a new top-level definition add a version to the function `prev` (rather than replace it)?
    pub fn can_add_version(&self, prev: FuncInfoId, new: FuncInfoId) -> bool {
        let (p, n) = (&self.funcs[prev], &self.funcs[new]);
        p.name == n.name && p.fdef.is_some() && p.parent.is_none() && p.anon_label.is_none() && p.module == n.module
            && p.nat == n.nat && !p.name.starts_with("builtin.")
    }

    /// A version's signature, for comparing definitions: arity, and each parameter's kind and dimension.
    fn signature_key(&self, info: FuncInfoId) -> Vec<(Option<String>, Option<String>)> {
        self.func_params(info)
            .iter()
            .map(|p| {
                let u = p.unit.as_ref().map(|u| match self.resolve_unit(u) {
                    Ok(r) => format!("{:?}", r.dim),
                    Err(_) => format!("?{}", u.text),
                });
                (p.kind.clone(), u)
            })
            .collect()
    }

    /// `energy(m [kg], v [m/s])`: how a version is shown in messages.
    pub fn version_sig(&self, info: FuncInfoId) -> String {
        let ps = self
            .func_params(info)
            .iter()
            .map(|p| {
                let mut s = p.name.clone();
                if let Some(k) = &p.kind {
                    s += &format!(": {k}");
                }
                if let Some(u) = &p.unit {
                    s += &format!(" [{}]", u.text);
                }
                s
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({ps})", self.funcs[info].display_name)
    }

    fn version_line(&self, info: FuncInfoId) -> u32 {
        self.funcs[info].fdef.as_ref().map(|f| f.span.line).unwrap_or(0)
    }

    /// Does a parameter (kind, dimension) take at least every argument that another one takes? Unannotated takes
    /// anything; a kind covers the same kind with any or the same unit; a unit alone covers the same dimension
    /// with any kind but vector (a vector needs `: vector`).
    fn param_covers(new: &(Option<String>, Option<String>), old: &(Option<String>, Option<String>)) -> bool {
        match (&new.0, &new.1) {
            (None, None) => true,
            (Some(k), u) => old.0.as_ref() == Some(k) && (u.is_none() || *u == old.1),
            (None, Some(u)) => old.1.as_ref() == Some(u) && old.0.as_deref() != Some("vector"),
        }
    }

    /// Called by s_funcdef for a new top-level function `id` named `name`: when `name` already is a function here,
    /// the new definition becomes a version of it, or replaces every earlier version it covers: the same number of
    /// parameters, each at least as broad as the earlier one's (D302). So a redefinition with the same signature
    /// (v1's `f(x) = 2 x` then `f(x) = 3 x`) and a less specific one written later (`force(x [m]) = …` then
    /// `force(x) = …`, which v1 also treats as a replacement) replace; a more specific one written later adds a
    /// version.
    pub fn add_version(&mut self, prev: FuncInfoId, id: FuncInfoId) {
        if !self.can_add_version(prev, id) {
            return;
        }
        let sig = self.signature_key(id);
        let covers = |old: &[(Option<String>, Option<String>)]| {
            old.len() == sig.len() && sig.iter().zip(old).all(|(n, o)| Self::param_covers(n, o))
        };
        let olds = self.versions_of(prev);
        let mut vs: Vec<FuncInfoId> = vec![];
        let mut replaced: Vec<FuncInfoId> = vec![];
        for v in olds {
            if covers(&self.signature_key(v)) {
                replaced.push(v);
            } else {
                vs.push(v);
            }
        }
        self.warn_respelt(id, &replaced);
        if vs.is_empty() {
            return; // a redefinition, as in v1
        }
        vs.push(id);
        self.funcs[id].versions = vs;
    }

    /// A replaced version whose units have the same dimensions but mean different things (`E(f [Hz])` then
    /// `E(ω [rad/s])`: Hz and rad/s are both 1/s, so the second replaces the first and `E(1 GHz)` is off by 2π)
    /// was probably meant as another version: warn (red team 14 #7, D306). The pairs are those of adding such
    /// values (Hz/rad/s, Bq/Hz, Gy/Sv, J/N m); `[m]` then `[km]` is an ordinary redefinition and says nothing.
    fn warn_respelt(&mut self, id: FuncInfoId, replaced: &[FuncInfoId]) {
        // the kinds of unit_kind, and "plain" for a bare inverse (1/s, 1/m, s⁻¹): Bq vs 1/s and rad/m vs 1/m are
        // pairs too (red team 15 #7), but not Hz vs 1/s (a hertz is one per second)
        let kind = |text: &str| {
            let k = crate::units::unit_kind(&fermium_ir::Hint { name: text.to_string(), factor: 1.0, offset: 0.0,
                                                                dim: fermium_ir::DIMLESS });
            let t = text.trim();
            if k.is_empty() && (t.starts_with("1/") || t.starts_with("1 /") || t.ends_with("⁻¹") || t.ends_with("^-1")) {
                "plain"
            } else {
                k
            }
        };
        let clash = |a: &str, b: &str| {
            !a.is_empty() && !b.is_empty() && a != b
                && !((a == "plain" && b == "cycles") || (a == "cycles" && b == "plain"))
        };
        let newp = self.func_params(id);
        for &v in replaced {
            let oldp = self.func_params(v);
            if oldp.len() != newp.len() {
                continue;
            }
            let pair = oldp.iter().zip(&newp).find_map(|(o, n)| match (&o.unit, &n.unit) {
                (Some(a), Some(b)) => {
                    let (ka, kb) = (kind(&a.text), kind(&b.text));
                    clash(ka, kb).then_some((ka, kb))
                }
                _ => None,
            });
            let Some((ka, kb)) = pair else {
                continue;
            };
            let (Some(fd), line) = (self.funcs[id].fdef.as_ref().map(|f| f.span), self.version_line(v)) else {
                continue;
            };
            // the REPL numbers each input from 1, so a line number there says nothing
            let at = if self.opts.repl { String::new() } else { format!(" (line {line})") };
            let msg = format!("{} replaces {}{at}: their units have the same dimensions, so they can't be two \
                               versions", self.version_sig(id), self.version_sig(v));
            let example = match (ka, kb) {
                ("angular", "cycles") | ("cycles", "angular") => ", e.g. ω = 2π f",
                ("angular", "plain") | ("plain", "angular") => ": an angle in rad counts as a plain number",
                _ => "",
            };
            let hint = format!("to keep both, give one of them another name (or convert inside one definition{example})");
            self.warn(msg, fd, Some(hint));
        }
    }

    fn fit(&mut self, p: &A::Param, a: &Checked, uses: &std::collections::HashMap<String, (bool, String)>)
           -> CResult<Option<Fit>> {
        let annotated = p.unit.is_some() || p.kind.is_some();
        let v = match a {
            Checked::Func { .. } => {
                // a function argument fits a plain parameter that the body calls
                return Ok((!annotated && uses.contains_key(&p.name)).then_some(Fit { score: 0, unsure: false }));
            }
            Checked::Sol(_) => return Ok(None),
            Checked::Val(x) => x,
        };
        if uses.get(&p.name).is_some_and(|(sure, _)| *sure) {
            return Ok(None); // the body calls this parameter: it needs a function
        }
        let mut score = 0;
        match p.kind.as_deref() {
            None => {}
            Some("number") => {
                if !matches!(v.ty, Ty::Num(_) | Ty::List(_)) {
                    return Ok(None);
                }
                // a list fits a number parameter element by element, less specifically than a list parameter
                if matches!(v.ty, Ty::Num(_)) {
                    score += 1;
                }
            }
            Some("list") => {
                if !matches!(v.ty, Ty::List(_) | Ty::ComplexList(_)) {
                    return Ok(None);
                }
                score += 1;
            }
            Some("vector") => {
                if !matches!(v.ty, Ty::Vec { .. }) {
                    return Ok(None);
                }
                score += 1;
            }
            Some("complex") => {
                if !matches!(v.ty, Ty::Complex(_)) {
                    return Ok(None);
                }
                score += 1;
            }
            Some(_) => return Ok(None),
        }
        let mut unsure = false;
        if let Some(uexpr) = &p.unit {
            let u = self.resolve_unit(uexpr)?;
            let dims = match &v.ty {
                Ty::Num(d) | Ty::List(d) | Ty::Complex(d) => vec![d.clone()],
                Ty::ComplexList(d) if p.kind.is_some() => vec![d.clone()],
                Ty::Vec { .. } if p.kind.is_some() => crate::vecmat::comp_dims(&v.ty),
                _ => return Ok(None),
            };
            for d in &dims {
                if self.u.is_concrete(d) {
                    if self.u.resolve(d) != u.dim {
                        return Ok(None);
                    }
                } else {
                    unsure = true;
                }
            }
            score += 1;
        }
        Ok(Some(Fit { score, unsure }))
    }

    /// The version of `info` that a call with these arguments uses (itself when it has one version).
    pub fn pick_version(&mut self, info: FuncInfoId, args: &[Checked], node: &A::Expr) -> CResult<FuncInfoId> {
        let versions = self.versions_of(info);
        if versions.len() == 1 {
            return Ok(info);
        }
        let mut cands: Vec<(FuncInfoId, Vec<u8>, bool)> = vec![];
        for &v in &versions {
            let params = self.func_params(v);
            if params.len() != args.len() {
                continue;
            }
            let uses = self.func_param_uses(v);
            let mut scores = vec![];
            let mut unsure = false;
            let mut ok = true;
            for (p, a) in params.iter().zip(args) {
                match self.fit(p, a, &uses)? {
                    Some(f) => {
                        scores.push(f.score);
                        unsure |= f.unsure;
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                cands.push((v, scores, unsure));
            }
        }
        let dn = self.funcs[info].display_name.clone();
        let listing = |c: &Checker| {
            versions.iter().map(|&v| format!("{} (line {})", c.version_sig(v), c.version_line(v))).collect::<Vec<_>>()
                    .join(", ")
        };
        if cands.is_empty() {
            let mut arities: Vec<usize> = versions.iter().map(|&v| self.func_params(v).len()).collect();
            arities.sort();
            arities.dedup();
            if !arities.contains(&args.len()) {
                let ns = match arities.as_slice() {
                    [a] => a.to_string(),
                    [rest @ .., last] => format!("{} or {last}", rest.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")),
                    [] => "0".into(),
                };
                let s = if arities == [1] { "" } else { "s" };
                return Err(self.err(format!("{dn} takes {ns} argument{s} but was given {}", args.len()), node.span,
                                    Some(format!("the versions of {dn} are {}", listing(self)))));
            }
            let got = args
                .iter()
                .map(|a| match a {
                    Checked::Val(v) => self.type_desc(&v.ty),
                    Checked::Func { name, .. } => format!("the function {name}"),
                    Checked::Sol(_) => "an ODE solution".into(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(self.err(format!("no version of {dn} takes ({got})"), node.span,
                                Some(format!("the versions of {dn} are {}; call one of them, or define a version for \
                                              these arguments", listing(self)))));
        }
        if cands.len() > 1 && cands.iter().any(|c| c.2) {
            return Err(self.err(format!("can't tell which version of {dn} to use here: the units of the arguments \
                                         aren't known yet"), node.span,
                                Some(format!("the versions of {dn} are {}; give the arguments units, or annotate \
                                              the parameters of the function that passes them on", listing(self)))));
        }
        // the most specific version: at least as specific as every other one in each parameter, and more
        // specific than each in at least one
        let dominates = |a: &[u8], b: &[u8]| a.iter().zip(b).all(|(x, y)| x >= y) && a.iter().zip(b).any(|(x, y)| x > y);
        let best = cands.iter().find(|c| cands.iter().all(|o| o.0 == c.0 || dominates(&c.1, &o.1)));
        let Some(best) = best else {
            // two versions that no other beats: name them
            let top: Vec<&(FuncInfoId, Vec<u8>, bool)> =
                cands.iter().filter(|c| !cands.iter().any(|o| o.0 != c.0 && dominates(&o.1, &c.1))).collect();
            let (a, b) = (top[0].0, top.get(1).map(|t| t.0).unwrap_or(cands[1].0));
            return Err(self.err(format!("the call of {dn} is ambiguous: both {} (line {}) and {} (line {}) fit these \
                                         arguments", self.version_sig(a), self.version_line(a), self.version_sig(b),
                                        self.version_line(b)), node.span,
                                Some("add units or a kind (: number, : vector, : list, : complex) to the \
                                      parameters so that one version is more specific, or define a version for \
                                      exactly these arguments".into())));
        };
        let chosen = best.0;
        self.dispatch_sites.push((node.span, chosen));
        Ok(chosen)
    }
}
