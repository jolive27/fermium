//! Runs python/test_fermium2.py (the D142 API tests of Fermium 1.5, ported) against the shared library cargo just
//! built. Skipped, with a note, when python3 with NumPy isn't available.
use std::path::PathBuf;
use std::process::Command;

fn library() -> Option<PathBuf> {
    // target/<profile>/deps/python-<hash> → target/<profile>/libfermium_pyapi.so
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    ["libfermium_pyapi.so", "libfermium_pyapi.dylib", "fermium_pyapi.dll"].iter().map(|n| dir.join(n))
        .find(|p| p.exists())
}

#[test]
fn the_python_api_tests_pass() {
    let ok = Command::new("python3").args(["-c", "import numpy"]).output().map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: python3 with numpy isn't available");
        return;
    }
    let Some(lib) = library() else {
        panic!("libfermium_pyapi wasn't built next to the test");
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("python").join("test_fermium2.py");
    let o = Command::new("python3").arg(&script).env("FERMIUM_PYAPI_LIB", &lib).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    eprintln!("{out}{}", String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "python/test_fermium2.py failed");
    assert!(out.contains("all passed"));
}
