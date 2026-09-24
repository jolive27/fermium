# BACKLOG

Tier 5 ideas and anything cut from earlier tiers. Pick the highest-value item first.

## High value
- [ ] Cross-check derivatives, integrals and ODEs against SymPy/SciPy on randomized inputs (property tests).
- [ ] Fuzz the lexer and parser (random and mutated programs): no Python exceptions, only FermiumErrors.
- [ ] Property tests for units: dimension algebra laws, conversion round-trips, `fmt` round-trips.
- [ ] Raise test coverage toward 95%.
- [ ] Better symbolic simplification. Example: `d/dt (2 m cos(t/(1 s)))` currently prints `-(2 m sin(t/(1 s))·(1/(1 s)))`.
- [ ] Better dense output for DP45: use the method's own 4th-order interpolant instead of cubic Hermite.

## Features
- [ ] Vectors and matrices: `<3, 4> m/s`, `|v|`, dot and cross products.
- [ ] Uncertainties (§3.7): the `NumTy` flavour `{value, sigma}` with first-order propagation. `±` is already reserved in the lexer and parser.
- [ ] AOT standalone executables (`fermium build`). Needs the globals and callbacks to be linkable, not arena addresses.
- [ ] Browser playground (Pyodide can't run llvmlite's JIT; would need an interpreter fallback).
- [ ] Partial derivatives of multi-line functions.
- [ ] Stiff ODE solver (implicit method). Events and root finding in `solve` ("stop when x < 0").
- [ ] `solve` with a parameter sweep. `plot` with a log scale.
- [ ] `print` with a digits option (`print x to 6 digits`).

## Known issues
- `2 g h` means 2 grams times h (spec rule: a unit right after a number). There is a warning, but it's a trap for beginners.
- Lists created inside loops are never freed (no GC). Fine for scripts, bad for very long loops that allocate.
