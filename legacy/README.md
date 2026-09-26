# Fermium 1.5 (legacy, deprecated)

**Deprecated.** This folder holds Fermium 1.5, the Python implementation of Fermium. Since v2.0 the language is
the `fermium` binary built from [`rust/`](../rust/) (Fermium 2): install it as the top-level
[README](../README.md) says. Fermium 1.5 stays here, frozen, for one more phase (spec §B8) because it is the
**oracle** of the conformance suite: `conformance/` holds each program with the output Fermium 1.5 gives, and the
Rust binary is scored against it ([CONFORMANCE.md](../CONFORMANCE.md); deliberate differences in
[rust/DIVERGENCES.md](../rust/DIVERGENCES.md)). It will be removed after the next phase (DECISIONS D269).

- `legacy/fermium/`: the package. Its import name is still `fermium` (`conformance/run --impl legacy`,
  `conformance/harvest.py` and the test suite import it).
- `legacy/tests/`: its pytest suite (runs every docs and bootcamp code block, the examples, gauntlet and
  research programs with Fermium 1.5). `make check` still runs it and it must stay green.
- `legacy/tools/`: one-off migration scripts of the 1.5 cycle (D235).

## Installing it

From the repository root (Python 3.10 or newer):

```
python3 -m pip install -e ".[full,dev]"     # [full] is enough to run programs; [dev] adds the test tools
fermium-legacy doctor
fermium-legacy run program.fm               # or: python3 -m fermium run program.fm
```

The console script is `fermium-legacy`, so it never shadows the Rust `fermium`. `python3 -m fermium` works too.
Its output is unchanged from 1.5 (the conformance goldens come from it), so `fermium-legacy --version` still
prints `fermium 0.1.0` and its messages still say `fermium`.

## Running its tests

```
python3 -m pytest -q                         # testpaths = legacy/tests (pyproject.toml)
FERMIUM_SKIP_RUST=1 make check               # ruff + this suite, without the Rust part
```

Before a full run, check which `fermium` package Python imports:
`python3 -c "import fermium; print(fermium.__file__)"` must point into this checkout's `legacy/fermium/`.
