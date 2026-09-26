//! Reading the fixtures written by rust/tools/numerics_fixtures.py.
#![allow(dead_code)]

use std::collections::BTreeMap;

/// One fixture line: the case name and its fields (numbers, or `ERR` then kind a b ...).
#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub fields: Vec<String>,
}

impl Row {
    pub fn is_err(&self) -> bool {
        self.fields.first().map(|s| s == "ERR").unwrap_or(false)
    }
    pub fn f(&self, i: usize) -> f64 {
        parse(&self.fields[i])
    }
    pub fn floats(&self) -> Vec<f64> {
        self.fields.iter().map(|s| parse(s)).collect()
    }
    pub fn floats_from(&self, i: usize) -> Vec<f64> {
        self.fields[i..].iter().map(|s| parse(s)).collect()
    }
}

pub fn parse(s: &str) -> f64 {
    match s {
        "nan" => f64::NAN,
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        _ => s.parse::<f64>().unwrap_or_else(|_| panic!("bad number {s:?}")),
    }
}

pub fn load(file: &str) -> Vec<Row> {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next().unwrap().to_string();
            Row { name, fields: it.map(str::to_string).collect() }
        })
        .collect()
}

pub fn by_name(file: &str) -> BTreeMap<String, Row> {
    load(file).into_iter().map(|r| (r.name.clone(), r)).collect()
}

/// Relative difference, with an absolute floor.
pub fn rel(a: f64, b: f64) -> f64 {
    if a == b {
        return 0.0;
    }
    (a - b).abs() / a.abs().max(b.abs()).max(1e-300)
}

/// Assert a and b agree to rtol (relative) or atol (absolute).
#[track_caller]
pub fn close(what: &str, got: f64, want: f64, rtol: f64, atol: f64) {
    if got == want || (got.is_nan() && want.is_nan()) {
        return;
    }
    let d = (got - want).abs();
    assert!(d <= atol || d <= rtol * want.abs().max(got.abs()), "{what}: got {got:e}, want {want:e} (rel {:e})", rel(got, want));
}

/// Whether this platform's libm is the one the fixtures were generated with (glibc on Linux). There the
/// bit-identity assertions hold; with another libm (Apple's) results that go through sin, cos, exp, log... may
/// differ in their last bits, and the tests compare within a few ulp instead.
pub const FIXTURE_LIBM: bool = cfg!(all(target_os = "linux", target_env = "gnu"));
