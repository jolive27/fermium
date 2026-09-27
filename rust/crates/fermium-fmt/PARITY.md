# fermium-fmt: parity with Fermium 1.5

`fermium fmt [--pretty|--ascii] [--fix] [-w] FILE` is a port of `fermium/fmt.py` and `cmd_fmt` in
`fermium/cli.py` (frozen, the oracle). It uses the token stream of fermium-syntax (spellings, roles, `extra`), so it
copies everything between tokens unchanged and changes only spellings. `--fix` rewrites unit/variable collisions as
bracketed units (the A1 rule, D235), using the parser's fix mode.

Checked by running both on the same files and comparing **stdout, stderr and the exit code** exactly, in seven
ways: `--pretty`, `--ascii`, `--fix`, `--fix --pretty`, `--fix --ascii`, and the two round trips (`--ascii` of the
`--pretty` output, `--pretty` of the `--ascii` output):

    python3 rust/tools/compare_fmt.py [--snippets|--fuzz] [--only SUBSTR]

## Counts (exact agreement / total)

| Corpus | Files | Runs (7 per file) | Agree |
|---|---|---|---|
| conformance cases + every `.fm` in the repo | 3199 | 22393 | **22386** (every mode 3198 / 3199) |
| string literals of the Python tests (`--snippets`) | 7473 | 52311 | **52311** |

`cargo test -p fermium-fmt` checks a few outputs of Fermium 1.5's formatter (pretty, ASCII, both ways, and `--fix`).

## Known differences

1. **Deep nesting.** `conformance/cases/units-and-printing/c40ce9f2472d.fm` nests 100 brackets. Fermium 1.5's
   `fermium fmt` crashes on it with a Python traceback (`RecursionError`: unlike `fermium run`, `fmt` never raises
   Python's recursion limit); the port formats it. This accounts for the 7 disagreements (one per mode).
