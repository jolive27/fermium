# Fermium 3 (in progress)

Changes on the branch `claude/v3` (spec Phase D). Draft; items are added as they land.

## D1 (started): a formal language specification

- `docs/spec/` (draft 0.1): [README](docs/spec/README.md) (scope, status, relation to docs/reference.md and
  conformance/), [grammar](docs/spec/grammar.md) (lexical structure and EBNF), [units](docs/spec/units.md)
  (dimensions, the unit rule, conversions, temperatures, natural units, inference) and
  [semantics](docs/spec/semantics.md) (partial: types, numerics, printing, errors, calculus, solve). DECISIONS D350.
- The specification's examples are tests: `cargo test -p fermium-syntax --test spec_examples` parses every
  `fermium` block in docs/spec/, and runs them (and checks the `fermium-error` blocks fail) when `FERMIUM_BIN`
  points at a `fermium` binary.
- Not yet specified: the full clause grammar of `plot`, `analyze`, `propagate` and the interop signatures; the
  built-in functions' static semantics; the numerical methods in one place; a cross-reference to conformance cases.
