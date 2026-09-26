# Building Fermium 2 (Rust)

Users need nothing: `fermium` is one binary. **Building** it needs Rust (stable), the LLVM 18 development
files (static libraries and `llvm-config`), lld 18's static libraries and headers (liblld-18-dev: lld is linked
into fermium for `fermium build`), a C++ compiler (for the small lld shim), and, on Linux, `lld` (the linker
cargo uses) and the C library's start-up files (libc6-dev, libgcc-13-dev: embedded for `fermium build`).

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
sudo apt-get install -y llvm-18-dev libpolly-18-dev lld-18 liblld-18-dev zlib1g-dev libzstd-dev libncurses-dev
echo "LLVM_SYS_180_PREFIX=/usr/lib/llvm-18" >> "$GITHUB_ENV"
```

(Ubuntu 24.04's own packages work too: `sudo apt-get install llvm-18-dev lld-18 liblld-18-dev libzstd-dev`.)
`llvm-18-dev` ships the static `libLLVM*.a` archives. `libpolly-18-dev` is needed because LLVM 18's
`llvm-config --link-static` may list Polly on apt.llvm.org builds.

### macOS (CI: `macos-14`, arm64)

```sh
brew install llvm@18 lld@18 zstd
echo "LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)" >> "$GITHUB_ENV"
```

Homebrew's `llvm@18` ships static libraries (`lib/libLLVM*.a`); `build.rs` links `libzstd.a` from Homebrew's
`zstd` statically, so the binary runs on a Mac without Homebrew. The system linker is used (no lld needed).
lld's static libraries and headers (linked in for `fermium build`) are looked for beside LLVM's, then in
`$LLD_PREFIX`, `/opt/homebrew/opt/lld@18` and `/usr/local/opt/lld@18` (Homebrew's separate `lld@18` formula; CI
sets `LLD_PREFIX=$(brew --prefix lld@18)` when that formula installs).

## Releases

`.github/workflows/release.yml` builds `cargo build --release -p fermium-cli` on Linux x86_64 and macOS arm64,
strips it and publishes `fermium-linux-x86_64` and `fermium-macos-arm64` (plus `SHA256SUMS`) as assets of the
release when a `v*` tag is pushed (`git tag v2.0 && git push origin v2.0`); started by hand (Actions → Release →
Run workflow) it only keeps them as workflow artifacts. bootcamp/lesson00_setup.md is the install guide for them.

## `fermium build`: executables linked with the built-in lld (spec B5.10)

`fermium build prog.fm [-o prog]` compiles the program with the LLVM back end into an object file
(`llvm::build_object`: generic CPU, position independent; the code reads its run-time context from the global
`fm_ctx`, and the object carries `fm_blob`: the module's tables, the code generator's tables and the source, see
`fermium-codegen/src/llvm/blob.rs`), then links it with **lld, linked into the fermium binary**
(`src/llvm/lld_shim.cpp` calls `lld::elf::link` / `lld::macho::link`; `build.rs` compiles it and gives lld's
driver empty initializers for the LLVM targets not linked in) against:

- the run time of executables, `crates/fermium-aotrt` (a static library: the same `native::rt` callbacks, printer
  and numerics as the JIT, and the C `main`). `crates/fermium-cli/build.rs` builds it with a nested
  `cargo build -p fermium-aotrt --profile aotrt` in its own target folder (so it is compiled without LLVM) and
  embeds it; `FERMIUM_NO_AOTRT=1` skips that for a faster build (`fermium build` then says it's unavailable);
- Linux: the C start-up files of the build machine's glibc (Scrt1.o, crti.o, crtn.o, crtbeginS.o, crtendS.o,
  libc_nonshared.a, found with `cc -print-file-name`), embedded too, and the shared libraries every glibc system
  has (libc.so.6, libm.so.6, libgcc_s.so.1), found on the computer the program is built on. The executable is a
  PIE that needs only those (`ldd prog`). No C compiler or system linker is used.
- macOS arm64 (**not yet tested on a Mac**): `ld64.lld` against libSystem from the SDK of Apple's Command Line
  Tools (`xcode-select --install`): libSystem's stubs (`libSystem.tbd`) exist on disk only there, so without
  the Command Line Tools `fermium build` stops and says so; `fermium run` needs nothing.

`python3 rust/tools/aot_diff.py --bin rust/target/fast/fermium -j 2 [--limit N]` builds the conformance programs
the LLVM back end compiles and requires each executable to print what `fermium run` prints.

## Choosing the back end

`fermium run` uses the LLVM JIT when it compiles every construct of the program (`llvm::supports`), else the
tree-walker. `fermium run --backend llvm|interp FILE` or `FERMIUM_BACKEND=llvm|interp` forces one (forcing llvm
on a program it can't compile yet exits with code 3). `FERMIUM_BACKEND_INFO=1` prints which back end ran;
`FERMIUM_LLVM_TIME=1` prints the compile / JIT / run times; `FERMIUM_DUMP_LLVM=1` (`_OPT=1`) prints the LLVM IR
before (after) optimization.

The differential test: `python3 rust/tools/llvm_diff.py --bin rust/target/fast/fermium -j 2` runs every
conformance program with both back ends and requires identical output wherever LLVM compiles the program.
