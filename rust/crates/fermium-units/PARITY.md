# fermium-units: parity with Fermium 1.5

Fermium 1.5 (the Python implementation in `fermium/`) is the oracle. `rust/tools/units_fixtures.py` runs
the Python code and records inputs and outputs in `tests/fixtures/units.json` (about 2.3 MB, seeded, so the
same file comes out every time). `cargo test -p fermium-units --test parity -- --nocapture` replays every
case against this crate and prints the table below.

To regenerate the fixtures: `python3 rust/tools/units_fixtures.py`. It needs only the `fermium/` sources, and
nothing gets installed.

## Result (2026-09-26)

| section | checks | failed |
|---|---:|---:|
| `format_number`: 631 values × sig 0–18, 20 × trim on/off, plus the default | 25871 | 0 |
| `format_default` / `_whole` | 2524 | 0 |
| `format_written` | 3000 | 0 |
| `format_default_seq` | 300 | 0 |
| `lookup_unit`: every unit, every prefix × 32 bases, micro sign, 2022 prefixes, blocked names | 922 | 0 |
| `parse_unit_string`: including error messages and malformed input | 516 | 0 |
| `format_dim`, `dim_name`, `preferred_unit`, `suggest_units`: every preferred dimension plus 200 random ones | 1285 | 0 |
| `format_quantity`: random dim/hint/sf/direct/echo/whole_ok, affine units, `in` conversions, c-unit echo | 4198 | 0 |
| `print_list`, `print_vec`, `print_mat`, `print_mvec`: one style per sequence, 12+ elements elided | 1450 | 0 |
| `print_cplx`, `print_clist` | 850 | 0 |
| `format_pm`, `format_uncertain`, `format_uncertain_list` | 2300 | 0 |
| constants: every name and alias, value bits, unit, description | 53 | 0 |
| natural/nuclear/astro/SI systems: label, canon_dim, factor, invariant, display_unit, describe, canon_unit, const_value, error messages | 7805 | 0 |
| tables: spelled units, long names, pretty names, prefixes, self-hosted factors | 173 | 0 |
| **total** | **51247** | **0 (100%)** |

Separately, `tests/selfhost.rs` checks that the factors build.rs computes from
`fermium/selfhost/units_db.fm` equal `fermium/units_selfhosted.py` bit for bit. All 56 match.

Floats are compared bit for bit and strings character for character.

## How exactness is achieved

- **Decimal rounding.** Rust's `{:.N}` and `{:.Ne}` round correctly from the exact binary value, with ties
  to even, the same as CPython's `f"{x:.Nf}"` / `f"{x:.Ne}"`. `format_number` follows Python's steps:
  format in e-notation, read the mantissa and exponent back from the text, then format in fixed notation.
- **Python's `round(x, n)`** (used by `format_pm`) is `numfmt::py_round`. It is exact, negative `n`
  included.
- **Powers.** Python's `float ** float` and `float ** int` call libm `pow`, and Rust's `f64::powf` calls
  the same function. Unit powers, natural-unit factors and constants are computed in the same order as
  Python.
- **The unit database** is Fermium source, evaluated at build time (spec §B4). build.rs contains a small
  evaluator for the subset `units_db.fm` uses:
  - assignments and numbers with SI base or coherent units and prefixes
  - implicit multiplication, `*`, `/`, superscript powers, parentheses and `π`
  - `print "name", expr to 17 digits`

  Dimensions are checked, so every printed factor must be a plain number. Each value is printed with
  `format_number(x, 17, trim=False)` and read back the way `fermium/selfhost/__main__.py` does
  (`mantissa * 10.0 ** exponent`).

## Known differences (not covered by the fixtures)

- **Character classes.** `parse_unit_string` needs Python's `str.isalpha`, `str.isdigit` and
  `str.isspace`. The port uses Rust's `char::is_alphabetic` minus numeric characters, ASCII and
  superscript digits, and Unicode White_Space plus U+001C–U+001F. The two can disagree on exotic Unicode
  characters (combining marks, non-ASCII decimal digits), which never occur in unit names.
- **Crashes in 1.5.** Where Fermium 1.5 lets a ValueError, IndexError or ZeroDivisionError escape from
  `parse_unit_string` (for example `m^`, `m^(1/0)`, `m^x`), the port returns a `UnitSyntaxError` with
  `python_exception: true` and its own wording. The fixtures check only that an error happens.
- **Integer range.** Exponents written in a unit string must fit in i64. Python accepts any size.
- **Complex values.** `format_complex` / `format_clist` expect finite values or NaN. Python's
  `str(round(inf))` would raise, and that path is unreachable in 1.5 as well.
- **Uncertainties.** `format_uncertain` takes (value, σ) directly. The correlation tracking of `UFloat`
  belongs to the runtime crate.
- **Second reference.** The C runtime's `fmt_num` (fermium/runtime/aot_rt.c) mirrors `format_number` and
  was used as a reference when reading the code. No fixtures are generated from it.
