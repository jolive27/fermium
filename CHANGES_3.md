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
- `docs/spec/` draft 0.3 (DECISIONS D352): every row of the built-in table is machine-checked, a call per row
  type-checked with the checker against the documented kind and dimension (semantics.md §7.1,
  `spec_builtins_types`; it corrected the `ifft` row, whose result on a complex list is a complex list, and added
  `sqrt` of a complex); the parser's complete error set as a normative table of 115 message templates, tested
  equal to the templates in the lexer/parser/unit-rule source, with a `fermium-error` example for 113 of them that
  must fail with that row's message (grammar.md §3, `spec_syntax_errors`); conformance cases named for each point
  of the unit rule, tested to exist with the expected outcome (units.md §2.3, `spec_unit_rule_cases`).
- `docs/spec/` draft 0.4 (DECISIONS D353): the checker's compile-time errors, a new chapter
  [errors.md](docs/spec/errors.md): a normative table of 208 message templates in 22 categories (units and
  dimensions, absolute temperatures, units as values, names, functions as values, arity and arguments, returns
  and recursion, dispatch, kinds in operators, conversions and printing, random numbers, built-in arguments,
  lists and indexing, uncertainties, calculus, complex lists and Fourier transforms, statements), complete for 16 of the
  checker's source files (arith, builtin, calculus, calls, checker, clist, convert, dispatch, exprs, lists, names,
  print, rng, stmts, uncertain, units), tested equal to the templates extracted from those files, with a
  `fermium-error` example for 122 rows that the checker must
  reject with that row's message (`cargo test -p fermium-check --test spec_errors`).
- `docs/spec/` draft 0.5 (DECISIONS D354): errors.md complete for all 31 checker files that raise errors — 541
  message templates (vectors and matrices, ODE and PDE `solve`, eigenvalue problems, data, `fit`, `plot`, modules,
  complex numbers, arrays, events, natural-units regions, `analyze`, `parallel for` and the Python, C, Fortran and
  C++ interop added), with an example for 306 rows; the test also checks that the other 9 checker files raise no
  errors.
- Not yet specified: the full clause grammar of `plot`, `analyze`, `propagate` and the interop signatures; examples
  for 235 of the checker's 541 error templates (errors.md §4); the numerical methods in one place; a cross-reference to conformance cases beyond the unit
  rule.
