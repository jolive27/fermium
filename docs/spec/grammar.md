# Grammar

Draft 0.1. Source: `rust/crates/fermium-syntax/src/lexer.rs` (tokens), `parser.rs` (statements), `expr.rs`
(expressions) and `unitrule.rs` (unit expressions and the unit rule). The Rust front end is a port of Fermium
1.5's `lexer.py` and `parser.py`, the frozen oracle; conformance/ holds its behaviour.

Fermium's grammar is *not* context-free in three places, and this document says so where it matters:

1. **The unit rule** (units.md §2): whether a name after a number is a unit or a variable depends on the names
   the program has assigned so far (and, in a few places, anywhere in the program).
2. **Spacing** decides a small number of tokenisation questions (a call `f(x)` versus a product `f (x)`, an index
   `xs[1]` versus a bracketed unit `3 [m]`, `R <cos φ, sin φ>` as a vector). Spacing never changes which of
   *unit* or *variable* a name is (D235).
3. **Contextual keywords**: `sweep`, `parallel`, `import`, `use`, `units`, `analyze`, `propagate`, `when`,
   `until`, `tolerance`, `absolute`, `using`, `method`, `lowest`, `grid`, `within`, `table` and `end` are
   ordinary names except in the positions below, and a program that assigns one (`when = 3 s`) keeps it a name.

## 1. Lexical structure

### 1.1 Source text

A program is UTF-8 text. Before tokenising, the lexer *prepares* the source (`prepare_source`):

- line endings are normalised to `\n` and a byte-order mark is removed;
- characters that are the same letter under another code point are replaced silently: `µ` (micro sign) → `μ`,
  `Ω` (ohm sign) → `Ω`, `K` (kelvin sign) → `K`, `Å` (angstrom sign) → `Å`, `ϵ` → `ε`, `ϕ` → `φ`, `ϑ` → `θ`, `ℏ` → `ħ`;
- typographic punctuation is replaced silently: `−` and `–` → `-`, `÷` → `/`, `′` and `’` → `'`, `″` → `''`,
  `“ ”` → `"`;
- Greek and Cyrillic capitals and lower-case letters that look like Latin letters (`Α`, `Μ`, `Τ`, `а`, `о`, `с`, …;
  the table `LOOKALIKE_REPLACE`) are replaced by the Latin letter **with a warning**.

None of these replacements happens inside a string. Columns in messages count code points.

### 1.2 Tokens

```text
token   = NUM | IMAG | NAME | KW | STR | OP | SUP | PRIME | NEWLINE | INDENT | DEDENT | EOF
```

Whitespace (space, tab) separates tokens; each token records whether whitespace preceded it (`ws_before`), which
the parser uses for the spacing rules above.

### 1.3 Comments and line structure

- `#` starts a comment that runs to the end of the line (there is no block comment, and `#` inside a string is
  text).
- A line ends a statement (`NEWLINE`) unless (a) it is inside `(`, `[` or `{` (brackets continue lines), (b) its
  last token is one of the binary operators `+ - * / ^ × ± == <= >= !=` or a comma, or (c) it ends in a backslash
  `\`. `;` also separates statements on one line.
- Blank lines and lines holding only a comment produce no tokens.

### 1.4 Indentation

Blocks are marked by indentation, as in Python. A tab counts as 4 spaces. At the start of each logical line the
lexer compares the indentation with a stack of open widths: deeper pushes and emits `INDENT`; shallower pops and
emits one `DEDENT` per popped width, and the new width must equal one on the stack ("this line's indentation
doesn't match any block above it"). Two special cases (lexer.rs):

- an indented line that starts with `else` directly after a line containing `then` continues that line's
  if-expression (no `INDENT` is emitted);
- after a dedent, a line indented deeper than the enclosing block that starts with `with` or `for` opens an
  indented clause block (used by `solve` and `fit`).

### 1.5 Names

```text
NAME        = name_start { name_char }
name_start  = letter | "_" | "ħ" | "°" | "%"       (* letter: Unicode alphabetic, as Python's str.isalpha *)
name_char   = letter | digit | "_" | "☉" | subscript_digit
subscript_digit = "₀" | "₁" | … | "₉"
standalone  = "π" | "∞" | "𝑖"                        (* each a NAME on its own *)
```

- **Subscripts:** subscript digits are spelled `_N`: `ε₀` is the name `ε_0`, `x₁₂` is `x_12`.
- **Greek spellings:** a name is split at `_` and every segment that is the ASCII name of a Greek letter (`alpha`
  … `omega`, `Gamma` … `Omega`, plus `hbar` → `ħ`, `inf`/`infinity` → `∞`) is replaced by the letter
  (`canonical_name`). So `omega_0`, `ω_0` and `ω₀` are one name, and `theta` is `θ`. `pi` is `π`.
- **Symbols as subscripts:** `π` and `∞` directly after `_` are part of the name: `m_π` is `m_pi`, `R_∞`.
  Elsewhere `π` is its own token, so `2πf` is `2 · π · f`.
- **Degrees and percent:** `°`, `°C`, `°F` and `%` are single names (units). `30°` is thirty degrees.
- **Primes** are not part of a name: `x'` is the name `x` followed by `PRIME(1)`; `x''` is `PRIME(2)`.
- Two names that differ only by look-alike letters (`v` and `ν`, `p` and `ρ`) give a warning.

### 1.6 Keywords

```text
if else elif then for from to step in while return break continue print plot vs solve with fit load
and or not where true false integral partial sqrt cbrt assert nabla
```

Keyword aliases: `∫` is `integral`, `∂` is `partial`, `√` is `sqrt`, `∛` is `cbrt`, `∇` is `nabla`. A keyword
can't be assigned to (`'print' is a reserved word …`); `integral(b, T) = …` gets a message explaining that
`integral` is the ASCII spelling of `∫`.

### 1.7 Numbers

```text
NUM       = mantissa [ exponent ] [ vulgar ]            (* the vulgar suffix only without '.' or exponent *)
          | vulgar
mantissa  = digits [ "." [ digits ] ] | "." digits
digits    = digit { digit | "_" digit }                  (* 1_000_000 *)
exponent  = ( "e" | "E" ) [ "+" | "-" ] digit { digit }
          | "×10" sup_int                                (* 6.674×10⁻¹¹ *)
          | "×10^" [ "+" | "-" ] digit { digit }         (* 3×10^8 *)
vulgar    = "½" | "⅓" | "⅔" | "¼" | "¾" | "⅕" | "⅖" | "⅗" | "⅘" | "⅙" | "⅚" | "⅐" | "⅛" | "⅜" | "⅝" | "⅞" | "⅑" | "⅒"
IMAG      = mantissa [ exponent ] "i"                    (* 4i; not followed by a name character *)
```

- The value is the IEEE-754 double nearest the decimal value (semantics.md §3). A literal whose value overflows
  (`1e400`) is an error; one that underflows is 0.
- **Significant figures:** a mantissa with a decimal point carries significant figures, counted after removing
  leading zeros (`1.20` → 3, `0.050` → 2, `100.` → 3). A mantissa without a point is *exact* (`12`, `1e3`).
  They only affect printing (semantics.md §3.2).
- `2½` is the mixed number 2.5. A vulgar fraction alone is a number that is **not** a digit literal: the unit rule
  (units.md §2) applies only after a literal written with digits.
- `3.4.5` is an error ("this number has two decimal points").

### 1.8 Superscripts

```text
SUP     = sup_int
sup_int = [ "⁻" | "⁺" ] sup_digit { sup_digit }          (* ⁰ ¹ ² ³ ⁴ ⁵ ⁶ ⁷ ⁸ ⁹ *)
```

A superscript is an exponent: `x²` is `x^2`, `m⁻¹` is `m^-1`. Two superscripts in a row are an error.

### 1.9 Strings

```text
STR = '"' { any character except '"' and newline } '"'
```

Strings are single-line and have no escape sequences. They are used by `print`, `plot`, `load`, `import`,
`assert` and the interop statements.

### 1.10 Operators

```text
OP = "+-" | "==" | "!=" | "<=" | ">=" | "+=" | "-=" | "*=" | "/=" | "~=" | "+" | "-" | "*" | "/" | "^"
   | "(" | ")" | "[" | "]" | "{" | "}" | "," | "=" | "<" | ">" | "." | ":" | "·" | "×" | "≤" | "≥" | "≠"
   | "±" | "≈" | "|" | ";" | "ᵀ"
```

The longest operator wins. Canonical spellings: `·` → `*`, `≤` → `<=`, `≥` → `>=`, `≠` → `!=`, `±` → `+-`,
`≈` → `~=`. `×` stays `×` (it is multiplication, and the cross product of vectors). Any other character is an
error naming the character ("unexpected character '§' (Section Sign)").

### 1.11 ASCII ⇄ symbol equivalence

Every symbol has an ASCII spelling that produces the same token, so `fermium fmt --pretty` and `--ascii` convert a
program without changing its meaning:

| Symbol | ASCII | Where the equivalence lives |
|---|---|---|
| `π θ ω λ Δ …` | `pi theta omega lambda Delta …` | name canonicalisation (§1.5) |
| `ħ` | `hbar` | name canonicalisation |
| `∞` | `inf`, `infinity` | name canonicalisation |
| `ε₀` | `epsilon_0`, `ε_0` | subscripts and Greek segments |
| `√ ∛` | `sqrt cbrt` | keyword aliases (§1.6) |
| `∫ ∂ ∇` | `integral partial nabla` | keyword aliases |
| `x²` | `x^2` | SUP token (§1.8) |
| `6.674×10⁻¹¹` | `6.674e-11` | number syntax (§1.7) |
| `· ×` | `*` (`×` is kept for the cross product) | operator canonicalisation |
| `≤ ≥ ≠ ≈ ±` | `<= >= != ~= +-` | operator canonicalisation |
| `Mᵀ` | `transpose(M)` | postfix `ᵀ` (§2.5) |
| `𝑖` | `1i` | IMAG literal |
| `°` `°C` | `deg` `degC` (unit names) | the unit catalogue |
| `Å` `μm` `M☉` | `angstrom` `um` `Msun` | the unit catalogue |

The editor input method `\name` + Tab (`rust/crates/fermium-syntax/src/symbols.rs`) is not part of the language: it
inserts the symbol before the program is read.

```fermium
ω₀ = 2 rad/s
print omega_0, ω_0
θ = 30°
print sin(θ), sin(theta), √(16 m²), sqrt(16 m^2)
```

## 2. Syntax

### 2.1 Programs and blocks

```text
program    = { NEWLINE } { statement } EOF
block      = [ ":" ] ( simple_stmt                         (* on the same line *)
                     | NEWLINE INDENT { statement } DEDENT )
statement  = compound_stmt | simple_stmt end
end        = NEWLINE | ";" | (* before DEDENT or EOF *)
```

A line that is indented but not inside a block is an error.

### 2.2 Statements

```text
compound_stmt = if_stmt | for_stmt | while_stmt | sweep_stmt | solve_stmt | funcdef_block
              | units_block | propagate_stmt

simple_stmt   = assign | aug_assign | index_assign | funcdef_line | print_stmt | plot_stmt | fit_stmt
              | analyze_stmt | return_stmt | assert_stmt | "break" | "continue" | import_stmt
              | use_python_stmt | import_native_stmt | units_stmt | expr_where

assign        = NAME "=" expr_where
aug_assign    = NAME ( "+=" | "-=" | "*=" | "/=" ) expr_where
index_assign  = NAME "[" expr { "," expr } "]" ( "=" | "+=" | "-=" | "*=" | "/=" ) expr_where
                                              (* no space before '['; a slice can't be assigned *)
expr_where    = expr_in [ "where" binding { "," binding } ]
binding       = NAME "=" expr

funcdef_line  = NAME "(" [ params ] ")" "=" expr_where end
funcdef_block = NAME "(" [ params ] ")" "=" NEWLINE INDENT { statement } DEDENT
params        = param { "," param }
param         = NAME [ ":" kind ] [ "[" unit_expr "]" ]
kind          = "number" | "vector" | "list" | "complex"   (* multiple dispatch, D285 *)

print_stmt    = "print" [ print_item { "," print_item } ] [ "where" binding { "," binding } ]
print_item    = expr_in [ "to" NUM ( "digits" | "digit" ) ]

if_stmt       = "if" expr [ "then" ] block { "elif" expr [ "then" ] block } [ "else" ( if_stmt | block ) ]
for_stmt      = [ "parallel" ] "for" NAME "from" expr "to" expr [ "step" expr ] block
              | "for" NAME "in" expr block
sweep_stmt    = "sweep" NAME ( "in" expr | "from" expr "to" expr [ "step" expr ] ) block
while_stmt    = "while" expr block
return_stmt   = "return" [ expr_where ]
assert_stmt   = "assert" expr [ "," STR ]

fit_stmt      = "fit" equation "to" expr [ [ NEWLINE INDENT ] ( "with" | "starting" ) binding { "," binding } ]
equation      = expr "=" expr

units_stmt    = "units" system [ "(" consts { "," consts } ")" ]
units_block   = units_stmt ":" block
system        = "natural" | "nuclear" | "astro" | "SI"
consts        = NAME "=" { NAME "=" } NUM                 (* the number must be 1: ħ = c = 1 *)

propagate_stmt = "propagate" ( "montecarlo" | "monte_carlo" | "MonteCarlo" ) [ sample_count [ "samples" ] ] block

import_stmt   = "import" ( NAME | STR ) [ "as" NAME ]
              | "from" ( NAME | STR ) "import" NAME [ "as" NAME ] { "," NAME [ "as" NAME ] }
use_python_stmt    = "use" "python" NAME [ "as" NAME ] [ ":" signature_block ]
import_native_stmt = "import" ( "c" | "fortran" ) STR ":" signature_block
                   | "import" "cpp" [ STR ] [ "header" STR ] ":" signature_block
```

Notes and special cases (parser.rs):

- **Function definition versus call:** `NAME(` with no space before `(` starts a function definition when the
  matching `)` is followed by `=`. Otherwise it is an expression statement.
- **Assignment targets:** only a name, an indexed name or a function head can be assigned. `h² = …` is an error
  whose hint points to `solve` (FRICTION #28). `a = b = 1` is an error; `ħ = c = 1` suggests `units natural`.
- `def`, `function`, `fn` before a name are errors with a hint ("a function is written like a formula").
- `parallel for` requires the `from … to` form (`for x in …` is an error).
- `sweep` is a keyword only when followed by `NAME in` or `NAME from` and not assigned by the program.
- `plot` is in §2.7; `analyze` and the interop signature blocks are summarised in §2.8.

```fermium
f(x [m], n: number) = n x
g(x) =
    y = 2 x
    return y + 1
for i from 1 to 3 step 2
    print i, f(2 m, i), g(i)
n = 0
while n < 2
    n += 1
if n > 2
    print "big"
elif n == 2
    print "two"
else
    print "small"
print a + b where a = 1 m, b = 2 m
xs = [1, 2, 3]
xs[1] = 5
if xs[1] > 1 then print xs
x = 1 + \
    2
for v in [1, 2]: print v, x
```

Each of these is rejected by the parser, with a one-line message and a hint:

```fermium-error
a = b = 1
```

```fermium-error
h² = 3
```

```fermium-error
def f(x) = 2 x
```

```fermium-error
xs = [1, 2, 3]
xs[1:2] = 5
```

```fermium-error
integral(b, T) = b T
```

### 2.3 solve

```text
solve_stmt = "solve" [ equations ] { solve_clause } end
           | "solve" [ equations ] { solve_clause } NEWLINE INDENT { ( equations | solve_clause ) { solve_clause } end } DEDENT
equations  = equation { ( "," | "and" ) equation }
solve_clause = "with" equation { ( "," | "and" ) equation }             (* initial conditions *)
             | "for" NAME "from" expr "to" expr [ "step" expr ]
                     [ "," NAME "from" expr "to" expr [ "step" expr ] ] (* a second variable: PDEs *)
             | "tolerance" expr | "absolute" expr { "," expr } | ( "using" | "method" ) NAME
             | "until" equation                                        (* stop when the sides cross *)
             | "lowest" expr [ "states" | "levels" ] | "grid" expr
             | "when" expr ( "=" | "<" | ">" | "<=" | ">=" ) expr ":" equation { ( "," | "and" ) equation }
                                                                    (* events, D297 *)
```

- A `solve` must have at least one equation and a `for` range ("solve needs a range for the independent
  variable").
- **Unknowns:** before parsing, the parser collects the names written with a prime anywhere in the solve (`x''`,
  `y'`); inside the equations these count as the program's variables for the unit rule (D211), so `u'' = -u`
  works although `u` is a unit (atomic mass unit). In the `with` clause they don't: `u(0) = 2 u` is 2 u.
- `solve f(x) = g(x) for x from a to b` with no derivative is a root search (semantics.md §6.3).
- Dividing by an unknown in an ODE (`ψ'' = -2 m_e E / ħ² ψ`) warns, because implicit multiplication binds
  tighter than `/` (§2.5).

```fermium
solve y' = -y with y(0) = 1 for t from 0 to 10
    when y = 0.5: y = 1
print y(1)
solve x^2 = 2 for x from 0 to 2
print x
k = 4 N/m
m_b = 1 kg
solve z'' = -(k/m_b) z with z(0) = 1 m, z'(0) = 0 m/s for t from 0 s to 10 s until z = 0 m
print z(0.5 s)
solve u'' = -u with u(0) = 1, u'(0) = 0 for t from 0 to 1
print u(1)
```

### 2.4 Expressions

```text
expr_in    = expr [ "in" unit_expr ]                          (* conversion for display, units.md §4 *)
expr       = "if" expr "then" expr "else" expr | or_expr
or_expr    = and_expr { "or" and_expr }
and_expr   = not_expr { "and" not_expr }                     (* see the separator rule below *)
not_expr   = "not" not_expr | comparison
comparison = sum [ cmp_op sum { cmp_op sum } ] [ "within" sum ]  (* within only after a single ≈ *)
cmp_op     = "==" | "!=" | "<" | ">" | "<=" | ">=" | "~="
sum        = pm_term { ( "+" | "-" ) pm_term }
pm_term    = product { "+-" product }                        (* a ± b *)
product    = unary { ( "*" | "/" | "×" ) unary }
unary      = ( "-" | "+" ) unary | juxt
juxt       = power { [ bracket_unit ] power }                (* implicit multiplication *)
power      = postfix [ SUP | "^" exponent ] [ unit_expr ]    (* the unit only after a digit literal: 10² m *)
exponent   = ( "-" | "+" ) exponent | ( NUM | IMAG | postfix ) [ "^" exponent | SUP ]
postfix    = atom { call | index | "ᵀ" | "." NAME | PRIME | bracket_unit }
call       = "(" [ expr_in { "," expr_in } ] ")"              (* no space before '(' *)
index      = "[" ( expr | [ expr ] ":" [ expr ] ) { "," expr } "]"   (* no space before '[' *)
atom       = quantity | NAME | STR | "true" | "false" | "(" expr_in ")" | list | vector | "|" expr "|"
           | ( "sqrt" | "cbrt" ) power | integral | derivative | partial | nabla | sum_for
           | "load" STR | "end"                               (* end only inside an index *)
quantity   = NUM ( bracket_unit | unit_expr | reciprocal_unit ) | NUM | IMAG
list       = "[" [ expr { "," expr } ] "]" [ unit_expr ]
vector     = "<" expr "," expr [ "," expr [ "," expr ] ] ">" [ unit_expr ]
bracket_unit = "[" unit_expr "]"
```

#### Precedence (lowest to highest)

| Level | Operators | Associativity | Notes |
|---|---|---|---|
| 1 | `where` | — | binds the whole expression before it |
| 2 | `in` unit | — | conversion of the whole expression |
| 3 | `if … then … else` | right | |
| 4 | `or` | left | |
| 5 | `and` | left | |
| 6 | `not` | prefix | |
| 7 | `== != < > <= >= ≈` | chained | `a < x < b` is `a < x and x < b`; a chain may mix `< <= > >=` or be all `==` |
| 8 | `+ -` | left | |
| 9 | `±` | left | tighter than `+`, looser than `*` (D120): `2 x ± 0.1` is `(2 x) ± 0.1` |
| 10 | `* / × ·` | left | |
| 11 | unary `-` `+` | prefix | `-A ω sin(ω t)` negates the whole product |
| 12 | implicit multiplication | left | **tighter than `/`** (D8): `h c / λ k_B T` = (h c)/(λ k_B T) |
| 13 | `^`, superscripts | right | `2^3^2` = 512; `-2^2` = -4; `4π² L` = 4·π²·L |
| 14 | call, index, `ᵀ`, `.name`, `'` | left | |

```fermium
print (2 + 3) * 4, 2^3^2, -2^2
print 1 < 2 < 3, 2 ≈ 2.0001 within 0.01, not true or false
print if 2 > 1 then 5 m else 6 m
h_P = 6.626e-34 J s
c_0 = 3.00e8 m/s
λ = 500 nm
T = 5000 K
print h_P c_0 / λ k_B T
```

Readings that follow from the table (each line: the expression on the left parses to the same tree as the
bracketed one on the right; the test `spec_precedence_readings` checks every line):

```fermium-reads
-2^2 ≡ -(2^2)
-x² ≡ -(x^2)
2^3^2 ≡ 2^(3^2)
h c / λ k T ≡ (h c)/(λ k T)
a b / c d ≡ (a b)/(c d)
a / b * c ≡ (a / b) * c
a - b - c ≡ (a - b) - c
-a b ≡ -(a b)
a b^2 ≡ a (b^2)
4π² r ≡ 4 (π^2) r
√x y ≡ (√x) y
√x² ≡ √(x^2)
2 x ± 0.1 ≡ (2 x) ± 0.1
1 + 2.0 ± 0.1 ≡ 1 + (2.0 ± 0.1)
not a and b ≡ (not a) and b
a < b and c < d or e ≡ ((a < b) and (c < d)) or e
1/2 x ≡ (1/2) x
f(x)^2 ≡ (f(x))^2
a + b in km ≡ (a + b) in km
```

(With `L` instead of `r`, the bracketed form would read `L` as litres: a free unit name after a bracket holding a
pure number is a unit, §2.6. `4π² L` itself is 4·π²·L.)

#### Special cases in expressions (expr.rs)

- **Implicit multiplication** (`juxt`) happens when the next token can start a term (a number, name, `(`, `√`,
  `∫`, `d/dx`, …) and isn't a keyword that ends the expression. `f(x)` with no space is a call when `f` can be
  called; `k(x + 1)` with a number `k` is a product. `f (x)` with a space is a product unless `f` is a function.
- **A fraction of pure numbers is one coefficient** (D236): in `a / b rest`, when `a` is a pure number (digits,
  π, √ and powers and products of those) and the denominator is an implicit product starting with a pure number
  `b`, the result is `(a/b)·rest`: `1/2 x` is x/2 and `π²/12 t²` is (π²/12) t². Dividing by exactly `1` is never
  a coefficient (`0.04 / 1 s` is 0.04 per second). Another pure number after the denominator (`4/3 π r³`,
  `1/2π √(k/m)`) is an error that asks which grouping is meant.
- **Ambiguous denominators warn** (D34): `c²/g (…)`, `μ₀ I/(4π) dl`, `a / (b) x` parse as `a/(b · …)` and warn
  with both spellings. `h c / λ k_B T` doesn't warn.
- **Integral limits:** the upper limit of `∫ … from a to b` is a `sum`; a `/` with a space before it, outside
  brackets, ends the limit (`∫ B dz from -∞ to ∞ / (μ₀ I)` divides the integral, D34), and a spaced binary `+`
  or `-` at the top level of the limit warns (D173).
- **Vector literal by juxtaposition** (D34 #7): `R <cos(φ), sin(φ), 0>` multiplies by a vector when `<` has a
  space before and none after and a matching `>` follows on the line with a top-level comma; otherwise `<` is
  a comparison.
- `and` inside `solve` and `plot` separates items when an `=` or `vs` follows at the same bracket depth
  (`solve x' = v and v' = -x`).
- `sum(` / `Σ(` followed by a top-level `for` is a one-line sum (below); otherwise `sum(xs)` is a call.
- `table(x = xs, y = ys)` is a table with named columns (D193).
- `|v|` is the absolute value or the norm; inside `| |` a `|` closes.
- `a ≈ b within t` compares with an absolute (`1 mm`) or relative (`2%`) tolerance; a single `≈` without
  `within` uses a default relative tolerance (D260).
- `print x to 3 digits` rounds for display.

```fermium
x = 3
r = 2 m
φ = 0.5
print 1/2 x, π²/12 x², 0.04 / 1 s
print r <cos(φ), sin(φ), 0>
xs = [1, 2]
ys = [3, 4]
data = table(x = xs, y = ys)
print data, |<3, 4> m|
```

```fermium-error
print 4/3 π
```

#### Calculus syntax

```text
integral   = "integral" sum_with_dvar [ "from" sum "to" sum ]  (* ∫ F(x) dx from a to b *)
derivative = "d" [ SUP | "^" NUM ] "/" DNAME [ SUP | "^" NUM ] power   (* d/dt x, d²/dt² x *)
           | "d" ( SUP | "^" NUM ) NAME "/" DNAME ( SUP | "^" NUM )    (* d²x/dt² *)
partial    = "partial" [ SUP ] "/" "partial" NAME [ SUP ] power       (* ∂/∂x f *)
           | "partial" [ SUP ] NAME "/" "partial" NAME [ SUP ]        (* ∂f/∂x *)
nabla      = "nabla" ( SUP(2) | "^" "2" | "*" | "×" | ε ) NAME         (* ∇²φ  ∇·E  ∇×B  ∇φ *)
sum_for    = ( "Σ" | "sum" ) "(" expr "for" NAME "from" expr "to" expr [ "step" expr ] ")"
DNAME      = a NAME starting with "d" followed by the variable: dt, dx, dθ
```

- The integrand is a `sum` whose trailing implicit factor `dv` names the variable of integration; while it is
  parsed, `v` counts as a variable (so `∫ 2 [m] dm` is not decimetres). A missing `dx` is an error.
- `d/dt` and `∂/∂x`: the variable counts as the program's variable inside the operand (D231).
- **First-order Leibniz fractions** `dx/dt` are *not* parsed specially: they are the names `dx` and `dt` divided,
  and the checker turns the quotient into a derivative when neither `dx` nor `dt` is defined and `x` is a
  function or an ODE solution (fermium-check `calculus.rs`, `leibniz`).
- `f'`, `f''` (PRIME after a name) differentiate a function; `∇f`, `∇·F`, `∇×F`, `∇²f` (ASCII `nabla f`,
  `nabla*F`, `nabla×F`, `nabla^2 f`) apply to a function's name. `grad(f)`, `div(F)`, `curl(F)`, `laplacian(f)`
  called on a single name mean the same, unless the program defines those names.

```fermium
x(t) = 0.1 m cos(2 rad/s t)
print d/dt x
print d²x/dt²
ψ(x) = exp(-x^2)
print ∂/∂x ψ
φ(x, y, z) = x^2 + y^2
print ∇²φ, nabla^2 phi
print ∫ x^2 dx from 0 to 1, integral exp(-x^2) dx from -inf to inf
print Σ(k^2 for k from 1 to 10)
```

### 2.5 Unit expressions

```text
unit_expr      = unit_factor { unit_join unit_factor }
unit_join      = (* nothing: juxtaposition *) | "·" | "/"          (* "*" only inside [ ] and after in *)
unit_factor    = UNIT_NAME [ unit_exponent ] | "(" unit_expr ")" [ unit_exponent ] | "1"   (* "1" only explicit *)
unit_exponent  = SUP | "^" [ "-" ] ( NUM | "(" NUM "/" NUM ")" )
reciprocal_unit = [ "1" ] "/" ( UNIT_NAME | "(" unit_expr ")" )   (* 0.5 /s, 0.1 1/s *)
```

`UNIT_NAME` is a name in the unit catalogue (units.md §3), with an optional SI prefix. Inside `[ ]` and after
`in` (*explicit* position) every name is a unit. Right after a digit literal (*implicit* position) the unit rule
decides where the unit ends (units.md §2). In implicit position a unit ends at an explicit `*`, at `c` except
after `/` (`938 MeV/c²` but `0.9 c` is the speed of light), at a name followed by `(` with no space, and at the
differential `dv` of an integrand. `W/m² K` warns that `K` multiplies (it is not below the line).

### 2.6 Units after brackets and fractions

- After `)`, a name is a variable (`KE = (1/2) m v²` is ½·m·v²), except that a unit name that no variable,
  function, parameter or constant of the program shares, after a bracket holding a *pure number*, is a unit;
  after any other bracket it is an error that asks for brackets: `(51 - 33 (N-Z)/A) [MeV]` (D235, D238).
- After a vulgar fraction (`½ kg`) the same rule applies: a free unit name is a unit, a name the program uses is
  the variable (`½ m v²`).
- A list literal takes a unit as a number does (`[1, 2, 3] m`); so does a vector literal (`<3, 4> m/s`).

```fermium
print 938 MeV/c², 0.9 c, 2 N·m, [1, 2, 3] s, 10² m
m = 2 kg
v = 3 m/s
print ½ m v², ½ kg, (1/2) kg
```

```fermium-error
a = 2
print (a + 1) kg
```

```fermium-error
print 5 N*m
```

(`5 N*m`: the explicit `*` ends the unit, and `m` alone is then a variable that isn't defined.)

### 2.7 plot

```text
plot_stmt   = "plot" series { ( "," | "and" ) series } { plot_tail }
              [ NEWLINE INDENT { ( series_list | plot_tail ) end } DEDENT ]    (* continued lines, D216 *)
series      = expr_in "vs" expr_in [ "from" expr "to" expr ]
plot_tail   = "to" STR [ "," ] [ options ]                     (* the output file *)
            | "with" options
            | options                                           (* after the last series, "with" may be left out *)
options     = option { "," option }
option      = "log" [ "x" | "y" ] | "points" | "dots" | "markers" | "title" STR | "xlabel" STR | "ylabel" STR
            | ( "x" | "y" ) "from" expr "to" expr | "reversed" ( "x" | "y" )
            | "animate" "over" NAME [ "frames" NUM ]
```

- While a series is parsed, option words (`title`, `log`, …) that aren't the program's variables end it; an option
  word that *is* a variable is read as the variable, and then `with` is needed.
- An unknown option is an error listing the valid ones.

### 2.8 Other statements (summary; TODO: full clause grammar)

- `analyze [NAME ":"] q [ "[" unit "]" ] "depends" "on" q [unit] { "," q [unit] }` (D70; `analyze` is a keyword
  only when `depends` follows on the line).
- Interop signature blocks: `f(x [m], n: int) -> [J]` per line, with `: list`, `bind(C, name="…")`, and for C++
  qualified names `ns::Class::f(…) -> […] [as name]` (C3/C4, D275, D290); `use python numpy as np: …` (D140).

## 3. TODO for this chapter

- The `solve` clause-ordering rules: the range comes first, then `step`; `tolerance`, `absolute`, `using` and
  `until` may follow in any order, on the range's line or each on an indented line of its own.
- A normative list of which tokens *start a term* for implicit multiplication (`starts_term` in expr.rs).
- The exact error set of the parser (each message is fixed by conformance/ today).
