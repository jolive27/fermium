//! Build script: embeds Fermium's standard library, which stays Fermium source (`stdlib/*.fm`, spec §B4), in
//! the binary, so `import mechanics` works with no files installed next to it.
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..");
    let candidates = [root.join("stdlib"), root.join("fermium/stdlib"), root.join("legacy/fermium/stdlib")];
    let dir = candidates.iter().find(|d| d.is_dir());
    let mut entries = vec![];
    if let Some(dir) = dir {
        println!("cargo:rerun-if-changed={}", dir.display());
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "fm") {
                println!("cargo:rerun-if-changed={}", p.display());
                let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
                let abs = fs::canonicalize(&p).unwrap();
                entries.push((stem, abs));
            }
        }
    }
    entries.sort();
    let mut out = String::from("/// The standard library modules: (name, Fermium source).\npub static STDLIB: &[(&str, &str)] = &[\n");
    for (stem, p) in entries {
        out += &format!("    ({stem:?}, include_str!({:?})),\n", p.to_string_lossy());
    }
    out += "];\n";
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("stdlib.rs"), out).unwrap();
}
