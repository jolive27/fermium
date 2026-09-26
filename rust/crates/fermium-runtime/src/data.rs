//! The data run time: `load "file.csv"` (CSV with a unit header like `L [m], T [s]`), `table(...)`
//! and the run-time side of `fit` (v1: `Runtime.load`, `Runtime.table`, `Runtime.fit` in
//! fermium/runtime/core.py; `fm_load`, `fm_fit` in aot_data.c).
//!
//! # API (for the checker/evaluator)
//!
//! ```text
//! // compile time: the header (names + unit texts; the caller parses units and canonicalises names)
//! let cols = read_csv_header("data/pendulum.csv")?;          // [HeaderCol { header: "L [cm]", name: "L", unit: Some("cm") }, ...]
//! // run time: the numbers, converted to SI with each column's unit (x·factor + offset)
//! let ds = load("data/pendulum.csv", "pendulum.csv", &[ColUnit { factor: 0.01, offset: 0.0 }, ...])?;
//! ds.cols[k]                                                // column k in SI
//! let ds = table(vec![xs, ys])?;                            // table(x = xs, y = ys)
//! // fit: residuals (model − left side) for every row, v1's report lines and the values for err()
//! let out = run_fit(&mut resid, n, &guesses, &FitInfo { text, path: "pendulum.csv".into(), params, y_unit, y_factor })?;
//! for l in &out.lines { println!("{l}"); }                   // "fit T = 2π √(L/g)   (8 data points from pendulum.csv)" ...
//! ```
//!
//! Errors are v1's messages (Err(String)).

use crate::format::format_number;
use crate::numerics::fit::{fit_sigfigs, least_squares_fit, FitResult};

/// A header column: the header text (trimmed), the name (spaces → `_`; the caller applies the
/// lexer's canonical form) and the unit text inside `[…]` (None for none, `1`, `-` or empty).
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderCol {
    pub header: String,
    pub name: String,
    pub unit: Option<String>,
}

/// A column's unit at run time: SI value = written value × factor + offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColUnit {
    pub factor: f64,
    pub offset: f64,
}

/// A data set: columns in SI units, all the same length.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dataset {
    pub cols: Vec<Vec<f64>>,
}

impl Dataset {
    pub fn len(&self) -> usize {
        self.cols.first().map(|c| c.len()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Records of a CSV text (Excel dialect, like Python's `csv.reader`): quoted fields with `""`,
/// CR/LF/CRLF line ends; a blank line is an empty record. A leading UTF-8 BOM is skipped.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let b: Vec<char> = t.chars().collect();
    let mut rows = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let mut row: Vec<String> = Vec::new();
        let mut buf = String::new();
        let (mut quoted, mut any, mut field_start) = (false, false, true);
        while i < b.len() {
            let c = b[i];
            if quoted {
                if c == '"' {
                    if i + 1 < b.len() && b[i + 1] == '"' {
                        buf.push('"');
                        i += 2;
                        continue;
                    }
                    quoted = false;
                    i += 1;
                    continue;
                }
                buf.push(c);
                i += 1;
                continue;
            }
            if c == '"' && field_start {
                quoted = true;
                field_start = false;
                any = true;
                i += 1;
                continue;
            }
            if c == ',' {
                row.push(std::mem::take(&mut buf));
                field_start = true;
                any = true;
                i += 1;
                continue;
            }
            if c == '\r' || c == '\n' {
                if c == '\r' && i + 1 < b.len() && b[i + 1] == '\n' {
                    i += 1;
                }
                i += 1;
                break;
            }
            buf.push(c);
            field_start = false;
            any = true;
            i += 1;
        }
        if any || !buf.is_empty() {
            row.push(buf);
        }
        rows.push(row);
    }
    rows
}

/// Python's `float(s)`: surrounding whitespace, underscores between digits, inf/infinity/nan.
pub fn py_float(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let cs: Vec<char> = t.chars().collect();
    let mut u = String::new();
    for (i, &c) in cs.iter().enumerate() {
        if c == '_' {
            let ok = i > 0 && i + 1 < cs.len() && cs[i - 1].is_ascii_digit() && cs[i + 1].is_ascii_digit();
            if !ok {
                return None;
            }
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '+' || c == '-') {
            return None;
        }
        u.push(c);
    }
    let low = u.to_ascii_lowercase();
    let body = low.trim_start_matches(['+', '-']);
    if body.chars().any(|c| c.is_ascii_alphabetic()) && !matches!(body, "inf" | "infinity" | "nan") && !body.contains('e') {
        return None;
    }
    if body.contains('x') {
        return None;
    }
    u.parse::<f64>().ok()
}

/// Python's `repr` of a string (for "not a number: ['1', 'x']").
pub fn py_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut o = String::new();
    o.push(q);
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if c == q => {
                o.push('\\');
                o.push(c);
            }
            c if (c as u32) < 32 || c as u32 == 127 => o.push_str(&format!("\\x{:02x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push(q);
    o
}

fn base_name(p: &str) -> &str {
    std::path::Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(p)
}

/// v1's `read_csv_header`: the column names and unit texts of a CSV file's first line.
pub fn read_csv_header(full: &str) -> Result<Vec<HeaderCol>, String> {
    let text = std::fs::read_to_string(full).map_err(|e| format!("can't read {full}: {e}"))?;
    let rows = parse_csv(&text);
    let header = rows.first().ok_or_else(|| format!("the file {} is empty", base_name(full)))?;
    Ok(header
        .iter()
        .map(|h| {
            let h = h.trim();
            let (mut name, mut unit) = (h.to_string(), None);
            if h.contains('[') && h.ends_with(']') {
                let (n, rest) = h.split_once('[').unwrap();
                name = n.trim().to_string();
                let ut = rest[..rest.len() - 1].trim();
                if !matches!(ut, "" | "1" | "-") {
                    unit = Some(ut.to_string());
                }
            }
            HeaderCol { header: h.to_string(), name: name.replace(' ', "_"), unit }
        })
        .collect())
}

/// v1's `Runtime.load`: read the numbers under the header, each column converted to SI.
/// `full` is the file to read, `shown` the path as written (for messages).
pub fn load(full: &str, shown: &str, cols: &[ColUnit]) -> Result<Dataset, String> {
    let text = match std::fs::read_to_string(full) {
        Ok(t) => t,
        Err(_) => {
            let dir = std::path::Path::new(full)
                .parent()
                .map(|d| if d.as_os_str().is_empty() { std::env::current_dir().unwrap_or_default() } else { d.to_path_buf() })
                .unwrap_or_default();
            return Err(format!(
                "can't find the file '{shown}' (looked in {}; data files are read from the folder you run the program in)",
                dir.display()
            ));
        }
    };
    let rows = parse_csv(&text);
    if rows.is_empty() {
        return Err(format!("the file {} is empty", base_name(shown)));
    }
    let nc = cols.len();
    let mut out: Vec<Vec<f64>> = vec![Vec::new(); nc];
    for (k, row) in rows.iter().enumerate().skip(1) {
        let ln = k + 1;
        if row.is_empty() || row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if row.len() != nc {
            return Err(format!("{shown}, line {ln}: expected {nc} values but found {}", row.len()));
        }
        let vals: Option<Vec<f64>> = row.iter().map(|c| py_float(c)).collect();
        match vals {
            None => {
                let r: Vec<String> = row.iter().map(|c| py_repr(c)).collect();
                return Err(format!("{shown}, line {ln}: not a number: [{}]", r.join(", ")));
            }
            Some(v) => {
                for (j, x) in v.into_iter().enumerate() {
                    out[j].push(x * cols[j].factor + cols[j].offset);
                }
            }
        }
    }
    Ok(Dataset { cols: out })
}

/// v1's `Runtime.table`: `table(x = xs, y = ys)` from lists already in SI.
pub fn table(columns: Vec<Vec<f64>>) -> Result<Dataset, String> {
    let n = columns.first().map(|c| c.len()).unwrap_or(0);
    for c in &columns[1.min(columns.len())..] {
        if c.len() != n {
            return Err(format!("the columns of this table have different lengths ({n} and {})", c.len()));
        }
    }
    Ok(Dataset { cols: columns })
}

/// A fitted parameter's display: its name and display unit (SI = shown × factor + offset).
#[derive(Debug, Clone, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    pub unit: String,
    pub factor: f64,
    pub offset: f64,
}

/// What v1's report needs about a fit.
#[derive(Debug, Clone, PartialEq)]
pub struct FitInfo {
    /// the fit as written (`T = 2π √(L/g)`)
    pub text: String,
    /// the data file as shown
    pub path: String,
    pub params: Vec<ParamInfo>,
    /// the left side's display unit and factor (for the rms residual)
    pub y_unit: String,
    pub y_factor: f64,
}

/// The fit's result: the values (SI), the standard errors (NaN where v1 has none, for `err()`),
/// the covariance and v1's report lines.
#[derive(Debug, Clone)]
pub struct FitOutcome {
    pub result: FitResult,
    pub errors_or_nan: Vec<f64>,
    pub lines: Vec<String>,
}

fn with_unit(s: String, unit: &str) -> String {
    match unit {
        "" | "1" => s,
        "°" | "%" | "′" | "″" => format!("{s}{unit}"),
        _ => format!("{s} {unit}"),
    }
}

/// v1's `Runtime.fit` after the data are read: fit, then the report ("fit …", one line per
/// parameter with its standard error, the rms residual, and the non-convergence warning).
pub fn run_fit(resid: &mut dyn FnMut(&[f64], &mut [f64]), n: usize, guess: &[Option<f64>], info: &FitInfo) -> Result<FitOutcome, String> {
    let res = least_squares_fit(resid, n, guess)?;
    let mut lines = vec![format!("fit {}   ({} data points from {})", info.text, n, info.path)];
    for (i, p) in info.params.iter().enumerate() {
        let (best, err) = (res.params[i], res.errors[i].filter(|e| e.is_finite()));
        let sig = fit_sigfigs(best, err);
        let x = (best - p.offset) / p.factor;
        let val = with_unit(format_number(x, sig.max(2) as usize, false), &p.unit);
        match err {
            Some(e) => {
                let se = format_number(e / p.factor, 2, false);
                let unit = if p.unit == "" || p.unit == "1" { String::new() } else { format!(" {}", p.unit) };
                lines.push(format!("  {} = {}   (standard error {}{})", p.name, val, se, unit));
            }
            None => lines.push(format!("  {} = {}   (standard error could not be estimated)", p.name, val)),
        }
    }
    let unit = if info.y_unit == "" || info.y_unit == "1" { String::new() } else { format!(" {}", info.y_unit) };
    lines.push(format!("  rms residual = {}{}", format_number(res.rms / info.y_factor, 3, false), unit));
    if let Some(i) = (0..info.params.len()).find(|&i| !res.errors[i].map(|e| e.is_finite()).unwrap_or(false)) {
        lines.push(format!(
            "  warning: the fit may not have converged; give a starting guess, like  fit ... with {} = ...",
            info.params[i].name
        ));
    }
    let errors_or_nan = res.errors.iter().map(|e| e.filter(|v| v.is_finite()).unwrap_or(f64::NAN)).collect();
    Ok(FitOutcome { result: res, errors_or_nan, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_floats() {
        let r = parse_csv("\u{feff}a [m], \"b, c\" [s]\r\n1,2\n\n\"3\"\"\",x\n");
        assert_eq!(r[0], vec!["a [m]", " \"b", " c\" [s]"]);
        assert_eq!(r[1], vec!["1", "2"]);
        assert!(r[2].is_empty());
        assert_eq!(r[3], vec!["3\"", "x"]);
        assert_eq!(py_float(" 1_000.5 "), Some(1000.5));
        assert_eq!(py_float("1e-3"), Some(1e-3));
        assert!(py_float("-inf").unwrap().is_infinite());
        assert!(py_float("nan").unwrap().is_nan());
        assert_eq!(py_float("1__0"), None);
        assert_eq!(py_float("0x10"), None);
        assert_eq!(py_float("abc"), None);
        assert_eq!(py_float(""), None);
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_repr("a\tb"), "'a\\tb'");
    }
}
