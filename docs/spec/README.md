# The Fermium Language Specification

**Status: draft 0.2 (2026-09-27, spec item D1; 0.1 earlier the same day).** This is a formal specification of
Fermium, the language implemented by the Rust compiler `fermium` (Fermium 2.5). It is incomplete; every section
says what it covers and what is still to do. It describes the language as implemented today; it does not
propose changes.

| File | Contents | State |
|---|---|---|
| [grammar.md](grammar.md) | Lexical structure (characters, names, numbers, superscripts, fractions, strings, comments, indentation, the ASCII ⇄ symbol equivalences) and the syntax in EBNF, with the precedence table, the normative list of tokens that start an implicit product, and the parser's special cases | Lexical structure: complete. Statements and expressions: the core is complete, and implicit multiplication is specified token by token (0.2); the clause-level grammar of `plot`, `analyze`, `propagate`, the interop signatures and `solve … lowest/grid` is summarised, not exhaustive |
| [units.md](units.md) | Dimensions (7 rational exponents), the unit rule (the bootcamp's three sentences and a precise version), unit expressions, the unit catalogue, conversions with `in`, affine temperatures, angles, natural units, dimension inference by unification, exact exponent arithmetic | Complete for the rules; the catalogue (§3.1) is a normative table generated from the unit database and tested equal to it (0.2) |
| [semantics.md](semantics.md) | Values and types, scoping, the linear model of uncertainties, evaluation order, numeric semantics (IEEE-754 double, significant figures for printing), errors (compile time and run time; one line, a caret, a hint), the meaning of the calculus operators and of `solve`, and the static semantics of every built-in function | Partial. New in 0.2: scoping (§2.2), uncertainty propagation (§2.3), the built-in table (§7, names tested against the checker). Still a high-level account of the numerical methods, `propagate`, `analyze` and interop; see its TODO list |

## How this document relates to the others

- **[docs/reference.md](../reference.md)** is the *user guide*: it explains every feature with examples, in the
  order a physicist learns them. This specification is the *definition*: shorter on motivation, precise about
  what is and isn't a program and what a program means. When they differ, one of them has a bug; file it.
- **[conformance/](../../conformance/)** is the *executable truth*: 3366 programs with their expected output,
  warnings, errors and exit codes, harvested from Fermium 1.5 (the frozen oracle) and scored in
  [CONFORMANCE.md](../../CONFORMANCE.md). **Where this specification and the conformance suite disagree, the
  suite wins until the specification is fixed** (or until a decision in DECISIONS.md changes the suite, which
  then records the change as a documented divergence).
- **[DECISIONS.md](../../DECISIONS.md)** records *why* each rule is what it is. This specification cites decisions
  as `D235` etc. instead of repeating the arguments.
- The implementation is `rust/crates/fermium-syntax` (lexer and parser), `rust/crates/fermium-check` (names,
  types, units), `rust/crates/fermium-units` (dimensions, the unit catalogue, printing numbers) and
  `rust/crates/fermium-ir/src/types.rs` (dimension inference). Section headings name the file a rule comes from.

## Where the conformance suite exercises each chapter

| Chapter | Conformance areas (conformance/cases/…, number of cases) |
|---|---|
| grammar.md §1–§2.2 (lexing, statements) | every area; especially `control-flow` (137), `functions` (197), `modules` (36) |
| grammar.md §2.3, semantics.md §6.3 (`solve`) | `ode` (341), `algebraic-solve` (46), `pde` (61), `eigen` (63) |
| grammar.md §2.4 (expressions, implicit products, calculus syntax) | `units-and-printing` (1130), `derivatives` (220), `integrals` (291) |
| grammar.md §2.7 (`plot`), `load`, `table` | `data` (93) |
| units.md §1–§5, §7 | `units-and-printing` (1130), `analyze` (34) |
| units.md §6 (natural units) | `natural-units` (69) |
| semantics.md §2 (values and types) | `lists` (134), `vectors-matrices` (173), `complex` (106) |
| semantics.md §2.2 (scoping) | `functions` (197), `control-flow` (137), `modules` (36) |
| semantics.md §2.3 (uncertainties) | `uncertainty` (113) |
| semantics.md §3 (numerics, printing) | `units-and-printing` (1130), `rng` (29), `fft` (39) |
| semantics.md §4 (evaluation order, `parallel for`) | `control-flow` (137), `parallel` (25) |
| semantics.md §5 (errors) | every area: each case fixes its error message, line and hint (D264) |
| semantics.md §6.1–§6.2 (derivatives, integrals, sums) | `derivatives` (220), `integrals` (291) |
| semantics.md §7 (built-in functions) | `lists` (134), `vectors-matrices` (173), `complex` (106), `fft` (39), `rng` (29), `uncertainty` (113) |
| interop (grammar.md §2.8) | `python-interop` (24) |
| (worked appendix programs) | `appendix1` (5) |

The suite has 3366 cases in 22 areas; every count above is checked by the test
`spec_conformance_counts` (spec_examples.rs), so the table can't go stale silently. A case-by-case
cross-reference (which rule each case exercises) is future work.

## Notation

- Grammar rules use ISO-style EBNF: `a b` sequence, `a | b` choice, `[ a ]` optional, `{ a }` zero or more,
  `"x"` a literal token, `(* … *)` a comment. Token classes are in `UPPER_CASE`.
- *Must*, *must not*: a program that breaks the rule is rejected with an error before it runs (Fermium checks
  the whole program first; see semantics.md §5).
- Code blocks marked `fermium` are complete programs. They are checked: every one must parse with the Rust front
  end (`cargo test -p fermium-syntax --test spec_examples`), and with `FERMIUM_BIN` set to a `fermium` binary
  every one must also run without an error. Blocks marked `fermium-error` must be rejected: with `FERMIUM_BIN`
  set, the test checks that each exits with status 1 and a one-line `file, line N: …` error. Blocks marked `text`
  are not checked. Blocks marked `fermium-reads` hold lines `A ≡ B`: A must parse to the same tree as the
  bracketed B (the precedence rules). The same test checks that grammar.md's keyword and operator lists are the
  lexer's. To run everything:
  `FERMIUM_BIN=rust/target/fast/fermium cargo test --profile fast -p fermium-syntax --test spec_examples`
  (from `rust/`, with an absolute path for the binary).

## What "1.0" needs from this document (not done yet)

1. A full clause grammar for `plot`, `solve` (all options), `analyze`, `propagate` and the interop signatures.
2. Machine-checked static semantics of the built-in functions. Since 0.2 semantics.md §7 has the table, written
   by hand from the checker; only its *names* are tested against the checker's list, not each row's dimensions.
3. The run-time semantics of every numerical method (tolerances, error estimates, failure modes) in one place;
   today they are in docs/reference.md and DECISIONS.md.
4. A cross-reference from every rule here to the conformance cases that exercise it (0.2 maps chapters to areas,
   with tested case counts, above).
5. The unit catalogue is done (units.md §3.1, generated and tested); the warnings about `Hz`/`rad/s`, `Gy`/`Sv`
   and `J`/`N m` are still prose.
