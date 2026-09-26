# Fermium 2 is the Rust binary (rust/crates/fermium-cli); Fermium 1.5 (legacy/, deprecated) is the Python package
# that `pip install -e ".[full,dev]"` installs as fermium-legacy (DECISIONS D269).
.PHONY: check test lint bench cov install build conformance
check:
	./check.sh
# the Rust binary for iterating: rust/target/fast/fermium
build:
	cd rust && cargo build --profile fast -p fermium-cli
# a release build of the Rust binary on your PATH (~/.cargo/bin/fermium); needs LLVM 18 (rust/BUILD.md)
install:
	cd rust && cargo install --locked --path crates/fermium-cli  # from rust/, so rust/.cargo/config.toml applies
conformance: build
	python3 conformance/run --impl rust --bin rust/target/fast/fermium --out "$${TMPDIR:-/tmp}/conformance-rust.md" --min $$(cat conformance/RUST_FLOOR)
# the legacy (Fermium 1.5) test suite
test:
	python3 -m pytest -q
lint:
	ruff check legacy/fermium legacy/tests
cov:
	python3 -m pytest -q --cov=fermium --cov-report=term-missing
bench:
	python3 benchmarks/run.py
