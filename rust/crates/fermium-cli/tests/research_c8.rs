//! Phase C (v2.5) item C8, the research track: each reproduction in research/<name>/ (data downloaded from a
//! cited source, compared with published values in its README) runs with the `fermium` binary and must print
//! exactly its `expected_output.txt`, apart from the `plot saved to <absolute path>` lines, which depend on
//! where it runs. The folder is copied to a temporary directory first, so the committed plots are not rewritten.
//! `FERMIUM_BLESS=1` rewrites the expected outputs (read the diff before committing).
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAMS: &[(&str, &str)] = &[
    ("cmb_firas", "firas.fm"),
    ("charge_radii", "radii.fm"),
    ("gamow_window", "gamow.fm"),
    ("alpha_decay", "alpha.fm"),
    ("pulsar_spindown", "pulsars.fm"),
    ("mass_luminosity", "mlr.fm"),
    ("supernova_hubble", "hubble.fm"),
    ("white_dwarf_cooling", "wd_cooling.fm"),
    ("neutron_star_cooling", "ns_cooling.fm"),
    ("level_density", "level_density.fm"),
];

fn research() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../research")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            std::fs::copy(&p, to.join(p.file_name().unwrap())).unwrap();
        }
    }
}

fn run_one(dir: &str, prog: &str) -> Result<(), String> {
    let src = research().join(dir);
    let tmp = std::env::temp_dir().join(format!("fermium-c8-{}-{}", dir, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&src, &tmp);
    let o = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .arg("run")
        .arg(prog)
        .current_dir(&tmp)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    let stdout = String::from_utf8_lossy(&o.stdout);
    let got: String = stdout
        .lines()
        .filter(|l| !l.starts_with("plot saved to "))
        .map(|l| format!("{l}\n"))
        .collect();
    if !o.status.success() {
        return Err(format!("{dir}/{prog} failed:\n{}{}", got, String::from_utf8_lossy(&o.stderr)));
    }
    let exp_path = src.join("expected_output.txt");
    if std::env::var("FERMIUM_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&exp_path, &got).unwrap();
        return Ok(());
    }
    let want = std::fs::read_to_string(&exp_path).map_err(|e| format!("{}: {e}", exp_path.display()))?;
    if !same_text(&want, &got) {
        let first = got
            .lines()
            .zip(want.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(got.lines().count().min(want.lines().count()));
        return Err(format!(
            "{dir}/{prog}: output differs from expected_output.txt at line {}\n  got:  {:?}\n  want: {:?}",
            first + 1,
            got.lines().nth(first),
            want.lines().nth(first)
        ));
    }
    Ok(())
}

#[test]
fn research_c8_reproductions_print_their_expected_output() {
    let fails: Vec<String> = PROGRAMS.iter().filter_map(|(d, p)| run_one(d, p).err()).collect();
    assert!(fails.is_empty(), "{}", fails.join("\n\n"));
}

#[test]
fn every_c8_folder_cites_its_data() {
    for (d, _) in PROGRAMS {
        let s = std::fs::read_to_string(research().join(d).join("SOURCE.md")).unwrap();
        assert!(s.contains("http"), "{d}/SOURCE.md has no URL");
        assert!(s.contains("Citation") || s.contains("citation"), "{d}/SOURCE.md has no citation");
    }
}

// The conformance runner's comparison (conformance/run same_text, DECISIONS D264): a number with a decimal point
// or a power of ten may differ by one unit in its last digit when it has the same shape (C libraries differ in the
// last bit of exp, sin, …; a fitted parameter that is statistically zero shows it: G_0 = 0.0005831 on macOS,
// 0.0005832 on Linux, standard error 0.028).
fn tokens(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let is_sep = |c: char| c.is_whitespace() || "[](),<>;:=".contains(c);
    let cs: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < cs.len() {
        let (b, c) = cs[i];
        if is_sep(c) {
            if b > start {
                out.push(&s[start..b]);
            }
            let mut j = i;
            // a run of whitespace is one token; each other separator is its own token
            if c.is_whitespace() {
                while j + 1 < cs.len() && cs[j + 1].1.is_whitespace() {
                    j += 1;
                }
            }
            let end = if j + 1 < cs.len() { cs[j + 1].0 } else { s.len() };
            out.push(&s[b..end]);
            start = end;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// (value, unit of the last digit, shape) of a number with a decimal point or a power of ten (conformance/run num_shape).
fn num_shape(t: &str) -> Option<(f64, f64, (usize, usize, i32, i32, bool))> {
    let (mant, exp) = match t.find("×10") {
        Some(k) => (&t[..k], Some(&t[k + "×10".len()..])),
        None => (t, None),
    };
    if !(mant.contains('.') || mant.contains('e') || exp.is_some()) {
        return None;
    }
    let v: f64 = mant.parse().ok()?;
    let digits = mant.split('e').next().unwrap();
    let decimals = digits.split_once('.').map(|(_, d)| d.len()).unwrap_or(0);
    let e10: i32 = match exp {
        Some(e) => {
            let s: String = e.chars().map(|c| match c {
                '⁰' => '0', '¹' => '1', '²' => '2', '³' => '3', '⁴' => '4', '⁵' => '5', '⁶' => '6', '⁷' => '7',
                '⁸' => '8', '⁹' => '9', '⁻' => '-', other => other,
            }).collect();
            s.parse().ok()?
        }
        None => 0,
    };
    let e_part: i32 = match mant.split_once('e') {
        Some((_, e)) => e.parse().ok()?,
        None => 0,
    };
    let scale = 10f64.powi(e10);
    let last = 10f64.powi(e_part - decimals as i32) * scale;
    let ndig = digits.trim_start_matches(['-', '+']).replace('.', "").len();
    Some((v * scale, last, (ndig, decimals, e_part, e10, t.starts_with('-'))))
}

fn same_token(want: &str, got: &str) -> bool {
    if want == got {
        return true;
    }
    let (Some((va, ulp, sa)), Some((vb, _, sb))) = (num_shape(want), num_shape(got)) else { return false };
    // a carry (9.99 -> 10.00) may add one digit
    if (sa.1, sa.2, sa.3, sa.4) != (sb.1, sb.2, sb.3, sb.4) || sa.0.abs_diff(sb.0) > 1 {
        return false;
    }
    (va - vb).abs() <= ulp * 1.0000001
}

fn same_text(want: &str, got: &str) -> bool {
    let (wl, gl): (Vec<&str>, Vec<&str>) = (want.trim_end_matches('\n').split('\n').collect(),
                                             got.trim_end_matches('\n').split('\n').collect());
    wl.len() == gl.len()
        && wl.iter().zip(&gl).all(|(w, g)| {
            let (wt, gt) = (tokens(w), tokens(g));
            w == g || (wt.len() == gt.len() && wt.iter().zip(&gt).all(|(a, b)| same_token(a, b)))
        })
}

#[test]
fn the_comparison_allows_one_unit_in_the_last_digit_only() {
    assert!(same_text("  G_0 = 0.0005832   (standard error 0.028)\n", "  G_0 = 0.0005831   (standard error 0.028)\n"));
    assert!(same_text("x 9.99 m\n", "x 10.00 m\n"));
    assert!(same_text("T = 2.725×10⁻³ K\n", "T = 2.726×10⁻³ K\n"));
    assert!(!same_text("G_0 = 0.0005832\n", "G_0 = 0.0005830\n"));
    assert!(!same_text("n = 12\n", "n = 13\n"));
    assert!(!same_text("a = 0.583\n", "a = 0.5831\n"));
    assert!(!same_text("a\nb\n", "a\n"));
}
