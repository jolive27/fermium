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
- `docs/spec/` draft 0.2 (DECISIONS D351): the tokens that start an implicit product (grammar.md §2.4, with
  precedence readings); scoping rules (semantics.md §2.2); a formal model of linear uncertainty propagation
  (§2.3); the static semantics of every built-in function as a table (§7), whose names
  `cargo test -p fermium-check --test spec_builtins` checks against the checker; the unit catalogue generated
  from the unit database (units.md §3.1, kept equal by `cargo test -p fermium-units --test spec_catalogue`);
  chapter-to-conformance-area case counts (README, checked by `spec_conformance_counts`).
- Not yet specified: the full clause grammar of `plot`, `analyze`, `propagate` and the interop signatures; the
  built-in functions' static semantics; the numerical methods in one place; a cross-reference to conformance cases.
