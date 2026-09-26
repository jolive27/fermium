# Building Fermium 2 (Rust)

Users need nothing: `fermium` is one binary. **Building** it needs Rust (stable), the LLVM 18 development
files (static libraries and `llvm-config`) and, on Linux, `lld`.

```sh
cd rust
CARGO_BUILD_JOBS=2 cargo build --release -p fermium-cli     # → target/release/fermium
CARGO_BUILD_JOBS=2 cargo build --profile fast -p fermium-cli  # no LTO, for iterating → target/fast/fermium
```

## LLVM 18, linked statically

The LLVM back end (`fermium-codegen/src/llvm/`, spec §B4) uses [inkwell](https://crates.io/crates/inkwell)
0.5 (feature `llvm18-0-no-llvm-linking`) over `llvm-sys` 180. `llvm-sys` is told not to link LLVM; instead
`crates/fermium-codegen/build.rs` does it:

1. finds `llvm-config` for LLVM 18: `$LLVM_SYS_180_PREFIX/bin/llvm-config`, else `llvm-config-18`,
   `/usr/lib/llvm-18/bin/llvm-config`, Homebrew's `llvm@18`, or `llvm-config` if it reports 18.x;
2. links the **static** libraries of the components the back end uses (`llvm-config --link-static --libs core
   executionengine mcjit orcjit native passes ipo … x86 aarch64`);
3. links LLVM's system libraries statically when a `lib<name>.a` exists (zlib, zstd, terminfo), skips
   libxml2 (only LLVM's Windows-manifest tool uses it), and links the C++ standard library statically on Linux
   (`libstdc++.a`, found with `c++ -print-file-name`); on macOS libc++, libz and libncurses come from the system
   (every Mac has them).

Check the result: on Linux `ldd target/release/fermium` must list only `libc`, `libm`, `libgcc_s` and the
dynamic loader (no `libLLVM`, `libstdc++`, `libz`, `libzstd`, `libtinfo`); on macOS `otool -L` must list only
`/usr/lib/*` system libraries. `cargo build --no-default-features -p fermium-codegen` builds without LLVM
(tree-walker only).

`rust/.cargo/config.toml` links with `lld` on Linux (`-fuse-ld=lld`): the static LLVM libraries make the final
link heavy, and GNU ld is several times slower. Keep debug info off for the same reason.

### Ubuntu (CI: `ubuntu-latest`)

```sh
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
sudo add-apt-repository -y "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-18 main"
sudo apt-get update
sudo apt-get install -y llvm-18-dev libpolly-18-dev lld-18 zlib1g-dev libzstd-dev libncurses-dev
echo "LLVM_SYS_180_PREFIX=/usr/lib/llvm-18" >> "$GITHUB_ENV"
```

(Ubuntu 24.04's own `llvm-18-dev` package works too: `sudo apt-get install llvm-18-dev lld-18 libzstd-dev`.)
`llvm-18-dev` ships the static `libLLVM*.a` archives. `libpolly-18-dev` is needed because LLVM 18's
`llvm-config --link-static` may list Polly on apt.llvm.org builds.

### macOS (CI: `macos-14`, arm64)

```sh
brew install llvm@18 zstd
echo "LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)" >> "$GITHUB_ENV"
```

Homebrew's `llvm@18` ships static libraries (`lib/libLLVM*.a`); `build.rs` links `libzstd.a` from Homebrew's
`zstd` statically, so the binary runs on a Mac without Homebrew. The system linker is used (no lld needed).

## Choosing the back end

`fermium run` uses the LLVM JIT when it compiles every construct of the program (`llvm::supports`), else the
tree-walker. `fermium run --backend llvm|interp FILE` or `FERMIUM_BACKEND=llvm|interp` forces one (forcing llvm
on a program it can't compile yet exits with code 3). `FERMIUM_BACKEND_INFO=1` prints which back end ran;
`FERMIUM_LLVM_TIME=1` prints the compile / JIT / run times; `FERMIUM_DUMP_LLVM=1` (`_OPT=1`) prints the LLVM IR
before (after) optimization.

The differential test: `python3 rust/tools/llvm_diff.py --bin rust/target/fast/fermium -j 2` runs every
conformance program with both back ends and requires identical output wherever LLVM compiles the program.
