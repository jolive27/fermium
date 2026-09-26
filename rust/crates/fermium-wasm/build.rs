//! The WebAssembly module's stack (the "shadow stack" in linear memory, where Rust keeps what it can't keep in
//! wasm locals): 1 MB by default, too little for the recursive parser and evaluator. See lib.rs STACK.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size=33554432");
    }
}
