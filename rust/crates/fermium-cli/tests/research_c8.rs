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
    if got != want {
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
