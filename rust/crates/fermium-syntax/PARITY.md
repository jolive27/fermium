# fermium-syntax: parity with Fermium 1.5

The lexer and parser are ports of `fermium/lexer.py` and `fermium/parser.py` (frozen, the oracle). They are
checked by running both implementations on the same programs and comparing everything they produce, printed
in one format:

- the tree as S-expressions: every node with its line:col+length, its fields and the attributes the parser
  attaches (`times_unit`, `coefficient`, `div_info`, `sum_info`, `unit_left`, ...);
- every token: kind, value, spelling, position, offsets, spacing, significant figures, and the `role` and
  `extra` the parser gives it (the formatter reads these);
- the warnings in the order they were produced; or, on a parse error, the error (position, message, hint and
  the fix-mode edits attached to it) and the warnings before it;
- in fix mode (`fmt --fix`): the edits and the error the parse stopped on.

Tools (Python runs only as the oracle):

    python3 rust/tools/parse_oracle.py FILE            # the Python side
    rust/target/debug/fermium parse --oracle FILE      # the Rust side (also --tokens, --fix)
    python3 rust/tools/compare_parse.py [--tokens|--fix] [--snippets|--fuzz]
    python3 rust/tools/harvest_snippets.py             # every string literal of tests/*.py as a program
    python3 rust/tools/fuzz_snippets.py N SEED         # mutated conformance lines (seeded)

## Counts (exact agreement / total)

| Corpus | Programs | Tree + tokens + warnings/error | Fix mode | Tokens only |
|---|---|---|---|---|
| conformance cases + every `.fm` in the repo | 3199 | **3199 / 3199** | 3199 / 3199 | 3199 / 3199 |
| string literals of the Python tests (`--snippets`) | 7473 | **7473 / 7473** | 7473 / 7473 | |
| mutated programs, seed 1 (`--fuzz`) | 20000 | **20000 / 20000** | 20000 / 20000 | |
| mutated programs, seed 7 (`--fuzz`) | 30000 | **30000 / 30000** | | |

Of these, about 29 000 are parse errors (lexer and parser), with about 3 400 distinct messages; they agree in
message, hint, line, column, length and fix edits. The hand-written cases in `tests/cases/` cover the few error
messages the corpora never reached (Python signatures, `table(...)`, indented `with` of a fit, animate).

`cargo test -p fermium-syntax` ports the parser-level rows of `tests/test_a1_unit_rule.py` and
`tests/test_a2_fraction_coefficients.py` and a few lexer rules (`tests/unit_rules.rs`).

## Known differences

1. **Nesting depth.** Fermium 1.5 fails with `this program is nested too deeply for Fermium to compile` when the
   parser reaches Python's recursion limit (driver.py raises it to 20000 frames: about 1 300 nested brackets).
   The port stops at a nesting depth of 5000 recursive steps (`MAX_DEPTH`, about 1 250 brackets) with the same
   message, and runs on a thread with a 1 GB (virtual) stack so it never overflows. The exact depth at which the
   two stop differs; no real program comes near it.
2. **Crashes of the oracle.** Where Fermium 1.5 raises a Python exception instead of a Fermium error, the port
   does something sensible instead. Known case: `_warn_divide_by_unknown` reads `info["warned"]` of an integral's
   `div_info`, which has no such key (a `solve` whose right side divides an integral with a spaced `/` after its
   upper limit); the port skips it. None of the corpora reach it.
3. **Unit exponents beyond 64 bits.** `m^1e20` would need a numerator larger than `i64`; the port clamps it
   (Python's `Fraction` is unbounded). Not reachable by sensible programs.
4. **Unicode.** Python 3.11's `str.isalpha`/`isalnum` and `unicodedata.name` (Unicode 14) are reproduced from
   generated tables (`src/tables.rs`, `src/char_names.rs`, by `rust/tools/gen_syntax_tables.py`), so letters and
   the names in `unexpected character` messages agree; characters added to Unicode after version 14 are unnamed
   in both.

Node identity: the Python parser stores node objects in attributes (`unit_left`, `limit_div_of`, the factors of
`div_info`); the port gives every node an `id` (copies share it) and prints references with the referenced
node's final position, as the Python objects do.
