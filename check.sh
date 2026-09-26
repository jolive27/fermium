#!/usr/bin/env bash
# Keep the build green (DECISIONS D269):
#   1. Fermium 1.5 (legacy/, deprecated but kept green for one more phase): lint and its whole test suite
#      (which runs the docs and bootcamp code blocks, the examples, gauntlet and research programs with v1);
#   2. Fermium 2 (rust/, the `fermium` binary): build, cargo test, and the conformance suite against the binary.
#      Every docs, bootcamp, examples, gauntlet and research program is harvested into conformance/, so this is
#      what checks them against the Rust binary. FERMIUM_SKIP_RUST=1 skips this part (it says so; CI runs it in
#      its own job); it is skipped, loudly, when cargo isn't installed.
# Arguments go to pytest (e.g. ./check.sh -k docs).
set -euo pipefail
cd "$(dirname "$0")"
echo "== lint (ruff)"
ruff check legacy/fermium legacy/tests
echo "== legacy tests (Fermium 1.5, deprecated; pytest; includes docs + bootcamp code blocks and examples)"
# Parallel when pytest-xdist is installed (the dev extra has it); a file's tests stay on one worker,
# which the Jupyter kernel fixture needs.
par=()
if python3 -c "import xdist" 2>/dev/null; then par=(-n auto --dist loadfile); fi
python3 -m pytest -q -x -p no:cacheprovider "${par[@]}" "$@"

if [ "${FERMIUM_SKIP_RUST:-}" = "1" ]; then
    echo "== Rust: SKIPPED (FERMIUM_SKIP_RUST=1): the Rust build, cargo test and the conformance suite did not run"
elif ! command -v cargo >/dev/null 2>&1; then
    echo "== Rust: SKIPPED (cargo not found): the Rust build, cargo test and the conformance suite did not run"
    echo "   install Rust and LLVM 18 (rust/BUILD.md) to check Fermium 2, or set FERMIUM_SKIP_RUST=1 to skip it on purpose"
else
    echo "== Rust: build and cargo test (Fermium 2)"
    (cd rust && cargo build --profile fast && cargo test --profile fast \
        && cargo test --profile fast -p fermium-pyapi -p fermium-wasm)
    echo "== conformance: the Rust binary against Fermium 1.5's outputs (never below conformance/RUST_FLOOR)"
    out="$(mktemp "${TMPDIR:-/tmp}/conformance-rust.XXXXXX.md")"
    python3 conformance/run --impl rust --bin rust/target/fast/fermium --out "$out" --min "$(cat conformance/RUST_FLOOR)"
    echo "   report: $out"
fi
echo "== all good"
