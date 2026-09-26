//! Parity with Fermium 1.5: every case in tests/fixtures/units.json (generated from the Python
//! implementation by rust/tools/units_fixtures.py) must be reproduced exactly.

mod common;

use common::json::{parse, J};
use fermium_units::natural::UnitSystem;
use fermium_units::*;

fn fl(j: &J) -> f64 {
    j.str().parse().unwrap_or_else(|_| panic!("float {j:?}"))
}

fn dim(j: &J) -> Dim {
    let e: Vec<Rational64> = j
        .arr()
        .iter()
        .map(|s| {
            let s = s.str();
            match s.split_once('/') {
                Some((n, d)) => Rational64::new(n.parse().unwrap(), d.parse().unwrap()),
                None => Rational64::from_integer(s.parse().unwrap()),
            }
        })
        .collect();
    Dim(e.try_into().unwrap())
}

fn unit(j: &J) -> Option<Unit> {
    if j.is_null() {
        return None;
    }
    let a = j.arr();
    Some(Unit { name: a[0].str().to_string(), dim: dim(&a[1]), factor: fl(&a[2]), offset: fl(&a[3]) })
}

fn same_unit(a: &Option<Unit>, b: &Option<Unit>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.name == b.name && a.dim == b.dim && a.factor.to_bits() == b.factor.to_bits() && a.offset.to_bits() == b.offset.to_bits()
        }
        _ => false,
    }
}

fn pfmt(j: &J) -> PrintFmt {
    let a = j.arr();
    PrintFmt {
        rdim: dim(&a[0]),
        hint: unit(&a[1]),
        sf: if a[2].is_null() { None } else { Some(a[2].int()) },
        direct: a[3].int() as u8,
        echo: a[4].b(),
    }
}

fn floats(j: &J) -> Vec<f64> {
    j.arr().iter().map(fl).collect()
}

#[derive(Default)]
struct Tally {
    rows: Vec<(String, usize, usize)>,
    examples: Vec<String>,
}

impl Tally {
    fn section(&mut self, name: &str) {
        self.rows.push((name.to_string(), 0, 0));
    }
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        let r = self.rows.last_mut().unwrap();
        r.1 += 1;
        if !ok {
            r.2 += 1;
            if self.examples.len() < 60 {
                let s = what();
                self.examples.push(format!("[{}] {s}", r.0));
            }
        }
    }
    fn eq(&mut self, got: &str, want: &str, ctx: impl FnOnce() -> String) {
        self.check(got == want, || format!("{}: got {got:?}, want {want:?}", ctx()));
    }
}

#[test]
fn parity_with_fermium_1_5() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/units.json")).unwrap();
    let data = parse(&text);
    let mut t = Tally::default();

    // ---- format_number
    t.section("format_number");
    let fnum = data.get("format_number");
    let sigs: Vec<i64> = fnum.get("sigs").arr().iter().map(|j| j.int()).collect();
    for c in fnum.get("cases").arr() {
        let c = c.arr();
        let x = fl(&c[0]);
        for (k, &sig) in sigs.iter().enumerate() {
            t.eq(&format_number(x, sig, false), c[1].arr()[k].str(), || format!("format_number({x:?}, {sig}, False)"));
            t.eq(&format_number(x, sig, true), c[2].arr()[k].str(), || format!("format_number({x:?}, {sig}, True)"));
        }
        t.eq(&format_number(x, 6, true), c[3].str(), || format!("format_number({x:?})"));
    }

    t.section("format_default / _whole");
    for c in data.get("format_default").arr() {
        let c = c.arr();
        let x = fl(&c[0]);
        t.eq(&format_default(x, 3, true), c[1].str(), || format!("format_default({x:?})"));
        t.eq(&format_default(x, 3, false), c[2].str(), || format!("format_default({x:?}, 3, False)"));
        t.eq(&format_default(x, 5, true), c[3].str(), || format!("format_default({x:?}, 5)"));
        t.check(is_whole(x) == c[4].b(), || format!("_whole({x:?})"));
    }

    t.section("format_written");
    for c in data.get("format_written").arr() {
        let c = c.arr();
        let (x, sf) = (fl(&c[0]), c[1].int());
        t.eq(&format_written(x, sf, false), c[2].str(), || format!("format_written({x:?}, {sf}, False)"));
        t.eq(&format_written(x, sf, true), c[3].str(), || format!("format_written({x:?}, {sf}, True)"));
    }

    t.section("format_default_seq");
    for c in data.get("format_default_seq").arr() {
        let c = c.arr();
        let xs = floats(&c[0]);
        let want: Vec<&str> = c[1].arr().iter().map(|j| j.str()).collect();
        let got = format_default_seq(&xs, 3);
        t.check(got == want, || format!("format_default_seq({xs:?}): got {got:?}, want {want:?}"));
    }

    // ---- units
    t.section("lookup_unit");
    for c in data.get("lookup_unit").arr() {
        let c = c.arr();
        let name = c[0].str();
        let got = lookup_unit(name);
        let want = unit(&c[1]);
        t.check(same_unit(&got, &want), || format!("lookup_unit({name:?}): got {got:?}, want {want:?}"));
    }

    t.section("parse_unit_string");
    for c in data.get("parse_unit_string").arr() {
        let c = c.arr();
        let text = c[0].str();
        let r = c[1].arr();
        let got = parse_unit_string(text);
        let ok = match (r[0].str(), &got) {
            ("ok", Ok(u)) => same_unit(&Some(u.clone()), &unit(&J::Arr(r[1..].to_vec()))),
            ("err", Err(e)) => !e.python_exception && e.message == r[1].str(),
            ("exc", Err(e)) => e.python_exception,
            _ => false,
        };
        t.check(ok, || format!("parse_unit_string({text:?}): got {got:?}, want {r:?}"));
    }

    t.section("dims (format_dim, dim_name, preferred_unit, suggest_units)");
    for c in data.get("dims").arr() {
        let c = c.arr();
        let d = dim(&c[0]);
        t.eq(&format_dim(&d), c[1].str(), || format!("format_dim({d:?})"));
        t.eq(&format_dim_style(&d, false), c[2].str(), || format!("format_dim({d:?}, pretty=False)"));
        t.eq(&dim_name(&d), c[3].str(), || format!("dim_name({d:?})"));
        let pu = Some(preferred_unit(&d));
        t.check(same_unit(&pu, &unit(&c[4])), || format!("preferred_unit({d:?}): got {pu:?}, want {:?}", c[4]));
        let su = suggest_units(&d);
        let want = if c[5].is_null() {
            None
        } else {
            let a = c[5].arr();
            Some((a[0].str().to_string(), a[1].arr().iter().map(|j| j.str().to_string()).collect::<Vec<_>>()))
        };
        t.check(su == want, || format!("suggest_units({d:?}): got {su:?}, want {want:?}"));
    }

    // ---- printing
    t.section("format_quantity");
    for c in data.get("format_quantity").arr() {
        let c = c.arr();
        let v = fl(&c[0]);
        let f = pfmt(&c[1]);
        let whole_ok = c[2].b();
        let got = format_quantity(v, &f.rdim, f.hint.as_ref(), f.sf, f.direct, f.echo, whole_ok);
        t.eq(&got, c[3].str(), || format!("format_quantity({v:?}, {f:?}, whole_ok={whole_ok})"));
    }

    let seq = data.get("seq");
    t.section("print_list");
    for c in seq.get("list").arr() {
        let c = c.arr();
        let (xs, f) = (floats(&c[0]), pfmt(&c[1]));
        t.eq(&format_list(&xs, &f), c[2].str(), || format!("print_list({xs:?}, {f:?})"));
    }
    t.section("print_vec");
    for c in seq.get("vec").arr() {
        let c = c.arr();
        let (xs, f) = (floats(&c[0]), pfmt(&c[1]));
        t.eq(&format_vec(&xs, &f), c[2].str(), || format!("print_vec({xs:?}, {f:?})"));
    }
    t.section("print_mat");
    for c in seq.get("mat").arr() {
        let c = c.arr();
        let (xs, r, k, f) = (floats(&c[0]), c[1].int() as usize, c[2].int() as usize, pfmt(&c[3]));
        t.eq(&format_mat(&xs, r, k, &f), c[4].str(), || format!("print_mat({xs:?}, {r}x{k}, {f:?})"));
    }
    t.section("print_mvec");
    for c in seq.get("mvec").arr() {
        let c = c.arr();
        let xs = floats(&c[0]);
        let fs: Vec<PrintFmt> = c[1].arr().iter().map(pfmt).collect();
        t.eq(&format_mvec(&xs, &fs), c[2].str(), || format!("print_mvec({xs:?}, {fs:?})"));
    }

    let cx = data.get("complex");
    t.section("print_cplx");
    for c in cx.get("cplx").arr() {
        let c = c.arr();
        let (re, im, f) = (fl(&c[0]), fl(&c[1]), pfmt(&c[2]));
        let got = format_complex(re, im, &f.rdim, f.hint.as_ref(), f.sf, f.direct);
        t.eq(&got, c[3].str(), || format!("print_cplx({re:?}, {im:?}, {f:?})"));
    }
    t.section("print_clist");
    for c in cx.get("clist").arr() {
        let c = c.arr();
        let (p, f) = (floats(&c[0]), pfmt(&c[1]));
        let n = p.len() / 2;
        let idx: Vec<usize> = if n > 12 { (0..5).chain(n - 3..n).collect() } else { (0..n).collect() };
        let pairs: Vec<(f64, f64)> = idx.iter().map(|&i| (p[2 * i], p[2 * i + 1])).collect();
        let got = format_clist(&pairs, &f.rdim, f.hint.as_ref(), f.sf, f.direct, Some(n));
        t.eq(&got, c[2].str(), || format!("print_clist({p:?}, {f:?})"));
    }

    let un = data.get("uncertain");
    t.section("format_pm");
    for c in un.get("pm").arr() {
        let c = c.arr();
        let (x, s) = (fl(&c[0]), fl(&c[1]));
        let (text, sci) = format_pm(x, s);
        t.check(text == c[2].str() && sci == c[3].b(), || format!("format_pm({x:?}, {s:?}): got {text:?} {sci}, want {:?}", c[2]));
    }
    t.section("format_uncertain");
    for c in un.get("uncertain").arr() {
        let c = c.arr();
        let (v, s, d, h) = (fl(&c[0]), fl(&c[1]), dim(&c[2]), unit(&c[3]));
        t.eq(&format_uncertain(v, s, &d, h.as_ref()), c[4].str(), || format!("format_uncertain({v:?}, {s:?}, {d:?}, {h:?})"));
    }
    t.section("format_uncertain_list");
    for c in un.get("list").arr() {
        let c = c.arr();
        let items: Vec<(f64, Option<f64>)> = c[0]
            .arr()
            .iter()
            .map(|it| {
                let a = it.arr();
                (fl(&a[0]), if a[1].is_null() { None } else { Some(fl(&a[1])) })
            })
            .collect();
        let u = unit(&c[1]).unwrap();
        t.eq(&format_uncertain_list(&items, &u), c[2].str(), || format!("format_uncertain_list({items:?}, {u:?})"));
    }

    // ---- constants
    t.section("constants");
    let want_consts = data.get("constants").arr();
    t.check(want_consts.len() == constants().len(), || format!("{} constants, want {}", constants().len(), want_consts.len()));
    for (c, got) in want_consts.iter().zip(constants()) {
        let c = c.arr();
        let ok = got.name == c[0].str()
            && got.value.to_bits() == fl(&c[1]).to_bits()
            && same_unit(&Some(got.unit.clone()), &unit(&c[2]))
            && got.description == c[3].str();
        t.check(ok, || format!("constant {}: got {got:?}, want {c:?}", c[0].str()));
    }

    // ---- natural units
    t.section("natural/nuclear/astro systems");
    for c in data.get("natural").arr() {
        let c = c.arr();
        let name = c[0].str();
        let consts: Option<Vec<String>> =
            if c[1].is_null() { None } else { Some(c[1].arr().iter().map(|j| j.str().to_string()).collect()) };
        let cref: Option<Vec<&str>> = consts.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());
        let res = make_system(name, cref.as_deref());
        let r = c[2].arr();
        match (r[0].str(), res) {
            ("err", Err(msg)) => t.eq(&msg, r[1].str(), || format!("make_system({name}, {consts:?})")),
            ("ok", Ok(sys)) => check_system(&mut t, &sys, r, name),
            (_, res) => t.check(false, || format!("make_system({name}, {consts:?}): got {res:?}, want {r:?}")),
        }
    }

    t.section("tables");
    let tables = data.get("tables");
    for (k, v) in tables.get("spelled").pairs() {
        t.check(spelled_unit(k) == Some(v.str()), || format!("SPELLED_UNITS[{k}]"));
    }
    t.check(tables.get("spelled").pairs().len() == SPELLED_UNITS.len(), || "SPELLED_UNITS size".into());
    for (k, v) in tables.get("long").pairs() {
        t.check(unit_name_long(k) == Some(v.str()), || format!("UNIT_NAMES_LONG[{k}]"));
    }
    for (k, v) in tables.get("pretty").pairs() {
        t.check(unit_pretty(k) == Some(v.str()), || format!("UNIT_PRETTY[{k}]"));
    }
    let pf = tables.get("prefixes").pairs();
    t.check(pf.len() == PREFIXES.len(), || "PREFIXES size".into());
    for ((k, v), (rk, rv)) in pf.iter().zip(PREFIXES.iter()) {
        t.check(k == rk && fl(v) == *rv, || format!("PREFIXES {k}"));
    }
    let sh = tables.get("self_hosted").pairs();
    t.check(sh.len() == self_hosted_factors().len(), || "self-hosted size".into());
    for ((k, v), (rk, rv)) in sh.iter().zip(self_hosted_factors()) {
        t.check(k == rk && fl(v).to_bits() == rv.to_bits(), || format!("self-hosted {k}"));
    }

    // ---- report
    let (mut total, mut fail) = (0, 0);
    println!("\n{:<58} {:>8} {:>8}", "section", "cases", "failed");
    for (name, n, f) in &t.rows {
        println!("{name:<58} {n:>8} {f:>8}");
        total += n;
        fail += f;
    }
    println!("{:<58} {total:>8} {fail:>8}", "TOTAL");
    println!("match rate: {:.4}%", 100.0 * (total - fail) as f64 / total as f64);
    for e in &t.examples {
        println!("  {e}");
    }
    assert_eq!(fail, 0, "{fail} of {total} fixture checks differ from Fermium 1.5");
}

fn check_system(t: &mut Tally, sys: &UnitSystem, r: &[J], name: &str) {
    t.eq(&sys.label(), r[1].str(), || format!("{name}.label"));
    t.eq(&sys.display, r[2].str(), || format!("{name}.display"));
    let consts: Vec<&str> = r[3].arr().iter().map(|j| j.str()).collect();
    t.check(sys.consts == consts, || format!("{name}.consts: got {:?}, want {consts:?}", sys.consts));
    for row in r[4].arr() {
        let row = row.arr();
        let d = dim(&row[0]);
        let cd = sys.canon_dim(&d);
        t.check(cd == dim(&row[1]), || format!("{name}.canon_dim({d:?})"));
        let f = sys.factor(&d);
        t.check(f.to_bits() == fl(&row[2]).to_bits(), || format!("{name}.factor({d:?}): got {f:?}, want {:?}", row[2]));
        t.check(sys.invariant(&d) == row[3].b(), || format!("{name}.invariant({d:?})"));
        let (du, desc) = if sys.natural {
            (sys.display_unit(&cd), sys.describe(&cd))
        } else {
            (if sys.display == "astro" { sys.display_unit(&d) } else { None }, sys.describe(&d))
        };
        t.check(same_unit(&du, &unit(&row[4])), || format!("{name}.display_unit({d:?}): got {du:?}, want {:?}", row[4]));
        t.eq(&desc, row[5].str(), || format!("{name}.describe({d:?})"));
    }
    for row in r[5].arr() {
        let row = row.arr();
        let u = parse_unit_string(row[0].str()).unwrap();
        let cu = Some(sys.canon_unit(&u));
        t.check(same_unit(&cu, &unit(&row[1])), || format!("{name}.canon_unit({}): got {cu:?}, want {:?}", row[0].str(), row[1]));
    }
    for row in r[6].arr() {
        let row = row.arr();
        let c = constant(row[0].str()).unwrap();
        let v = sys.const_value(&c.name, c.value, &c.unit.dim);
        t.check(v.to_bits() == fl(&row[1]).to_bits(), || format!("{name}.const_value({}): got {v:?}, want {:?}", c.name, row[1]));
    }
}
