# The Fermium Language Specification

**Status: draft 0.1 (started 2026-09-27, spec item D1).** This is the first version of a formal specification of
Fermium, the language implemented by the Rust compiler `fermium` (Fermium 2.5). It is incomplete; every section
says what it covers and what is still to do. It describes the language as implemented today; it does not
propose changes.

| File | Contents | State |
|---|---|---|
| [grammar.md](grammar.md) | Lexical structure (characters, names, numbers, superscripts, fractions, strings, comments, indentation, the ASCII ⇄ symbol equivalences) and the syntax in EBNF, with the precedence table and the parser's special cases | Lexical structure: complete. Statements and expressions: the core is complete; the clause-level grammar of `plot`, `analyze`, `propagate`, the interop signatures and `solve … lowest/grid` is summarised, not exhaustive |
| [units.md](units.md) | Dimensions (7 rational exponents), the unit rule (the bootcamp's three sentences and a precise version), unit expressions, conversions with `in`, affine temperatures, angles, natural units, dimension inference by unification, exact exponent arithmetic | Complete for the rules; the unit catalogue is referenced, not reproduced |
| [semantics.md](semantics.md) | Values and types, evaluation order, numeric semantics (IEEE-754 double, significant figures for printing), errors (compile time and run time; one line, a caret, a hint), the meaning of the calculus operators and of `solve` | Partial: a high-level account; see its TODO list |

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

## Notation

- Grammar rules use ISO-style EBNF: `a b` sequence, `a | b` choice, `[ a ]` optional, `{ a }` zero or more,
  `"x"` a literal token, `(* … *)` a comment. Token classes are in `UPPER_CASE`.
- *Must*, *must not*: a program that breaks the rule is rejected with an error before it runs (Fermium checks
  the whole program first; see semantics.md §5).
- Code blocks marked `fermium` are complete programs. They are checked: every one must parse with the Rust front
  end (`cargo test -p fermium-syntax --test spec_examples`), and with `FERMIUM_BIN` set to a `fermium` binary
  every one must also run without an error. Blocks marked `fermium-error` must be rejected: with `FERMIUM_BIN`
  set, the test checks that each exits with status 1 and a one-line `file, line N: …` error. Blocks marked `text`
  are not checked. To run everything:
  `FERMIUM_BIN=rust/target/fast/fermium cargo test --profile fast -p fermium-syntax --test spec_examples`
  (from `rust/`, with an absolute path for the binary).

## What "1.0" needs from this document (not done yet)

1. A full clause grammar for `plot`, `solve` (all options), `analyze`, `propagate` and the interop signatures.
2. The complete static semantics of every built-in function (argument kinds and dimensions), generated from the
   checker's tables rather than written by hand.
3. The run-time semantics of every numerical method (tolerances, error estimates, failure modes) in one place;
   today they are in docs/reference.md and DECISIONS.md.
4. A cross-reference from every rule here to the conformance cases that exercise it.
