# Check errors

**Part of the Fermium language specification, draft 0.4.** See [README.md](README.md) for the notation.

## 1. Scope

A program that parses (grammar.md) is then checked as a whole before anything runs: names are resolved, every
value gets a kind (number, list, vector, …) and a dimension (units.md §4), calls are matched with definitions,
and the calculus, `solve`, `fit` and interop statements are checked. A program that breaks a rule is rejected
with one of the errors below: one line `<file>, line N: <message>`, a caret under the place, and usually a hint
(semantics.md §5). The syntax errors raised before this are grammar.md §3; the warnings (the program still runs)
are not listed here.

The checker is `rust/crates/fermium-check`: about 40 source files and roughly 580 error messages. **This draft
covers ten of the files completely** — the ones that hold the most common errors: unit and dimension
mismatches, undefined names, arity and argument kinds, dispatch between versions of a function, conversions
with `in`, and random numbers. The other files (calculus, `solve`, `fit`, ODE solutions, PDEs, eigenvalue
problems, data tables, lists, vectors and matrices, complex numbers, uncertainties, modules, parallel loops and
the C, C++ and Python interop) are still to do; see §4.

Covered files: `arith.rs`, `calls.rs`, `checker.rs`, `convert.rs`, `dispatch.rs`, `exprs.rs`, `names.rs`, `print.rs`, `rng.rs`, `units.rs`.

The table in §2 is **normative and complete for the covered files**. The test `spec_checker_errors`
(`cargo test -p fermium-check --test spec_errors`) extracts every message template from those files — the
first argument of each `err(…)` and `Diagnostic::error(…)`, the `format!` of each `unify_or(…, |c| format!(…))`,
and the `format!` literals of a `let msg = …` that the next lines raise with `err(msg, …)` — plus two kinds of
message that are built in one place and raised in another: the "might not have a value" messages stored as
`unset_msg` (stmts.rs, parallel.rs; raised in exprs.rs) and the unit-power overflow message (fermium-units
`exact.rs`, raised in checker.rs). It checks that the table lists exactly those templates, and that the
*Covered files* line above is the test's list. In a message, `…` stands for a part filled in from the program
(a name, a unit, a dimension in words such as `length [m]`, a line number, a kind such as `a 3-D vector of …`).
Errors raised in a covered file through a helper defined in an uncovered file (for example `need_num`'s
"… must be a number, but it is …", used for the two sides of a comparison) belong to the helper's file.

The column *Example* names a ```fermium-error example in §3 (its first line is the comment `# C<n>`). The same
test parses and checks each example with the checker (`api::check_keep`) and requires that it is rejected with a
message matching that row; with `FERMIUM_BIN` set, `spec_examples_run` (fermium-syntax) also requires that
`fermium run` rejects it with a one-line error and exit status 1. Rows marked — have no example yet: 26 of the
79. Some are internal guards that no program reaches today (rows 67, 68), some need a construct whose own
errors come first, and the rest are simply not written yet (§4).

## 2. The errors

| # | Message | Raised in | Meaning | Example |
|---|---|---|---|---|
| | **Units and dimensions** | | | |
| 1 | `can't add … to …` | arith.rs | the two sides of `+` have different dimensions (units.md §4) | C1 |
| 2 | `can't subtract … from …` | arith.rs | the two sides of `-` have different dimensions | C2 |
| 3 | `can't compare … with …` | arith.rs | the two sides of a comparison have different dimensions | C3 |
| 4 | `the two branches give … and …; they must match` | arith.rs | the branches of an if-expression have different dimensions | C4 |
| 5 | `the tolerance after 'within' must be …, like the values it compares, but it is …` | arith.rs | `a ≈ b within t`: t has another dimension than a and b | C5 |
| 6 | `an exponent must be a plain number, but this is …` | arith.rs | an exponent has units | C6 |
| 7 | `can't raise … to a power that isn't a fixed number` | arith.rs | a value with units raised to a power known only at run time: its dimension would not be fixed (units.md §7) | C7 |
| 8 | `e is the elementary charge (1.602×10⁻¹⁹ C) in Fermium` | arith.rs | `e^x` with a negative or non-constant x, where `e` is the built-in constant: almost always meant as exp(x) | C8 |
| 9 | `… was defined in … units (… = 1), so it can only be used where those units hold` | calls.rs | a function defined inside a natural-units region called outside it (units.md §6) | — |
| 10 | `this unit's power is too large to track exactly (…)` | fermium-units exact.rs (raised in checker.rs) | a dimension exponent doesn't fit in the exact 64-bit fractions (units.md §7) | C10 |
| | **Absolute temperatures** | | | |
| 11 | `can't add two absolute temperatures (… + …)` | arith.rs | `20 °C + 30 °C`: two readings can't be added (units.md §5) | C11 |
| 12 | `can't negate an absolute temperature (…)` | arith.rs | `-(20 °C)` | C12 |
| 13 | `… looks like a temperature change, but a value in … is an absolute temperature (… … is … K)` | arith.rs | a name starting with Δ (a change) given a °C or °F literal (a reading), D181 | C13 |
| 14 | `… looks like a temperature change, but this value in … is an absolute temperature (Fermium reads … values as absolute temperatures in K)` | arith.rs | the same for a computed value shown in °C or °F | C14 |
| | **Units used as names or values** | | | |
| 15 | `'…' isn't a unit Fermium knows (or a variable you've defined)` | names.rs | a name right after a number that is neither a unit nor a variable | C15 |
| 16 | `'…' is not a unit Fermium knows` | checker.rs | an unknown unit after `in` | C16 |
| 17 | `… is a unit, not a value` | names.rs | a unit expression written where a value is needed (`rate = cm³/(mol s)`), D163 | C17 |
| 18 | `% is the percent unit in Fermium (5 % = 0.05)` | names.rs | `%` used alone as a value | C18 |
| 19 | `this already has units (…), so […] would multiply them` | exprs.rs | a bracketed unit after a value that already has units (units.md §2) | C19 |
| | **Names** | | | |
| 20 | `… isn't defined` | names.rs | a name that isn't a variable, function, constant or unit in scope (semantics.md §2.2); the hint suggests close names | C20 |
| 21 | `g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: g = 9.81 m/s²` | names.rs | the special case of `g` | C21 |
| 22 | `… isn't defined: the eigenvalue problem's states are …` | names.rs | an undefined name inside an eigenvalue problem | — |
| 23 | `… is a built-in function; call it with arguments like …(x)` | names.rs | a built-in function used as a value | C23 |
| 24 | `… is defined as a function on line …, after this line` | calls.rs | a call of a function defined later in the file, where the name is (still) a value | C24 |
| 25 | `… might not have a value here: it is only set inside … on line …` | stmts.rs (raised in exprs.rs) | a variable set only inside an `if` or loop, used after it (semantics.md §2.2) | C25 |
| 26 | `… might not have a value here: it is only set inside the for loop on line …, which may not run at all` | stmts.rs (raised in exprs.rs) | the same for a variable first set in a `for` loop that can run zero times | — |
| 27 | `… has no value here: it belongs to the iterations of the parallel for on line …, so it isn't kept after the loop` | parallel.rs (raised in exprs.rs) | a variable private to a `parallel for`, used after it (semantics.md §4) | C27 |
| 28 | `… belongs to another function and can't be used here` | exprs.rs | a local of another function | — |
| 29 | `… belongs to one iteration of a parallel for and can't be used in another function` | exprs.rs | a loop-private variable captured by a function | — |
| 30 | `… was solved for outside this … region, so it can't be used here` | exprs.rs | an ODE solution from outside a natural-units region, used inside it | — |
| 31 | `… can't be used inside this integral/equation yet (only numbers, vectors, matrices and ODE solutions can be taken in from the function)` | exprs.rs | a captured value of an unsupported kind inside an integrand or equation | — |
| 32 | `… is the Python module …, not a value; call its functions, like ….f(x)` | exprs.rs | a Python module name used as a value (grammar.md §2.8) | — |
| | **Functions used as values** | | | |
| 33 | `… is a function; give it an argument, like …(x)` | exprs.rs | a user function used without arguments where a value is needed | C33 |
| 34 | `… is a function here; call it like …(x)` | exprs.rs | a function parameter used without arguments | C34 |
| 35 | `… is a function defined inside …; it can only be called, like …(…)` | exprs.rs | a local function returned or passed as a value | C35 |
| 36 | `… is the solution of an ODE (a function of …); use …(…) for its value at a time` | exprs.rs | an ODE solution used as a value (semantics.md §6.3) | — |
| | **Calls: arity and arguments** | | | |
| 37 | `… takes … argument… but was given …` | calls.rs, dispatch.rs | wrong number of arguments | C37 |
| 38 | `… expects … in … (…), but got …` | calls.rs | an argument with the wrong dimension for a parameter annotated `[unit]` | C38 |
| 39 | `… expects … in …, but got the function …` | calls.rs | a function passed for a parameter annotated `[unit]` | C39 |
| 40 | `… expects … to be …, but got …` | calls.rs | an argument of the wrong kind for a parameter annotated `: vector`, `: list`, … | C40 |
| 41 | `… uses … as a function (…), but was given …` | calls.rs | a parameter called inside the body, given a value that isn't a function | — |
| 42 | `… isn't a function, so it can't be called with ( )` | calls.rs | a variable that isn't a number called with brackets (for a number, `k(x + 1)` is a product) | C42 |
| 43 | `this can't be called like a function` | calls.rs | brackets after an expression that isn't a name | C43 |
| 44 | `only functions can be called` | calls.rs | a call of something that isn't a function (inside a function body) | — |
| 45 | `the arguments of … must be values (a function defined inside another function can't take a function)` | calls.rs | a function passed to a local function | — |
| 46 | `… is the solution of an ODE; it can't be passed to a function yet` | calls.rs | an ODE solution as an argument | — |
| 47 | `… returns …, so it can't be applied to each element of a list (lists of vectors aren't supported yet)` | calls.rs | elementwise application of a function whose result isn't a number | C47 |
| | **Function bodies, returns and recursion** | | | |
| 48 | `the function … never returns a value` | calls.rs | a function whose body has no `return` and no final expression, used as a value | C48 |
| 49 | `… doesn't return a value on every path (for example when an if is false, or a loop doesn't run)` | calls.rs | some path through the body ends without a value | C49 |
| 50 | `… returns different kinds of values in different places` | calls.rs | two `return`s give different kinds | C50 |
| 51 | `… always calls itself, so it would never finish` | calls.rs | recursion with no path that returns without calling itself | C51 |
| 52 | `… calls itself and returns …; a function that calls itself must return a single real number` | calls.rs | a recursive function returning a vector, list, … | — |
| 53 | `… calls itself; a function defined inside another function can't be recursive (define it at the top level)` | calls.rs | a recursive local function | C53 |
| 54 | `the units of … don't work out recursively` | calls.rs | the result dimension of a recursive function has no solution (units.md §4) | C54 |
| | **Versions of a function (dispatch)** | | | |
| 55 | `no version of … takes (…)` | dispatch.rs | no definition's annotations fit the arguments' dimensions and kinds | C55 |
| 56 | `the call of … is ambiguous: both … (line …) and … (line …) fit these arguments` | dispatch.rs | two versions fit equally well | C56 |
| 57 | `can't tell which version of … to use here: the units of the arguments aren't known yet` | dispatch.rs | a call whose argument dimensions are still unknown when it must be resolved | — |
| | **Kinds of values in operators** | | | |
| 58 | `can't add text and a number` | arith.rs | `"a" + 1` | C58 |
| 59 | `text can only be joined with +, like  "3p" + "1/2"` | arith.rs | another operator on text | C59 |
| 60 | `each side of a comparison must be a number` | arith.rs | a chained comparison with a side that isn't a number | — |
| 61 | `can't compare a …-vector with a …-vector` | arith.rs | `≈` on vectors of different lengths | C61 |
| 62 | `≈ compares a vector with a vector of the same length, but this is …` | arith.rs | `≈` between a vector and something else | C62 |
| 63 | `both branches of an if-expression must give the same kind of value` | arith.rs | the branches have different kinds | C63 |
| 64 | `both branches of an if-expression must give vectors of the same length` | arith.rs | vector branches of different lengths | C64 |
| 65 | `both branches of an if-expression must give matrices of the same size` | arith.rs | matrix branches of different sizes | C65 |
| 66 | `a list of vectors or matrices can only be multiplied or divided by a number (here … … …)` | arith.rs | `+`, `-` or another operator on lists of vectors or matrices | C66 |
| 67 | `unknown comparison …` | arith.rs | internal guard: a comparison operator the parser doesn't produce | — |
| 68 | `unknown operator …` | arith.rs | internal guard: an arithmetic operator the parser doesn't produce | — |
| | **Conversions and printing** | | | |
| 69 | `can't show … in … (…)` | convert.rs | `x in u` where u has another dimension than x (units.md §5) | C69 |
| 70 | `can't show a vector with different units per component in …` | convert.rs | `in` on a vector of mixed dimensions | C70 |
| 71 | `can't show complex numbers in …` | convert.rs | `in` with a unit a complex value can't be shown in | — |
| 72 | `'to N digits' only works on numbers` | convert.rs | `to N digits` on text, a boolean, … | C72 |
| 73 | `the number of digits must be between 1 and 17` | convert.rs | N outside 1…17 (semantics.md §3) | C73 |
| 74 | `can't print this` | print.rs | `print` of a value with no printed form | — |
| | **Random numbers** | | | |
| 75 | `seed takes one whole number: seed(42)` | rng.rs | wrong number of arguments to `seed` | C75 |
| 76 | `the seed must be a plain number, not …` | rng.rs | a seed with units | C76 |
| 77 | `… takes no arguments or two: …` | rng.rs | `rand`/`randn` with one argument, or more than two | C77 |
| 78 | `sample takes an expression and a count: sample(randn(0 m, 1 m), 1000)` | rng.rs | wrong arguments to `sample` | C78 |
| | **Implementation limits** | | | |
| 79 | `… isn't supported by this version of the Rust compiler yet` | checker.rs | a construct the reference interpreter (Fermium 1.5) accepts but the Rust compiler doesn't yet (CONFORMANCE.md) | — |

## 3. Examples

```fermium-error
# C1
x = 1 m + 2 s
```

```fermium-error
# C2
x = 1 m - 2 s
```

```fermium-error
# C3
b = 1 m < 2 s
```

```fermium-error
# C4
x = if true then 1 m else 2 s
```

```fermium-error
# C5
b = 1 m ≈ 1 m within 2 s
```

```fermium-error
# C6
x = 2 ^ (3 m)
```

```fermium-error
# C7
n = 2
x = (3 m)^n
```

```fermium-error
# C8
x = 2
y = e^(-x)
```

```fermium-error
# C10
a = 1 m^4611686018427387904
b = a a a
```

```fermium-error
# C11
T = 20 °C + 30 °C
```

```fermium-error
# C12
T = -(20 °C)
```

```fermium-error
# C13
ΔT = 10 °C
```

```fermium-error
# C14
T0 = 20 °C
ΔT = T0
```

```fermium-error
# C15
x = 3 blargs
```

```fermium-error
# C16
x = 3 m in blargs
```

```fermium-error
# C17
rate = cm³/(mol s)
```

```fermium-error
# C18
y = %
```

```fermium-error
# C19
x = 3 m
y = x [cm]
```

```fermium-error
# C20
y = z + 1
```

```fermium-error
# C21
F = 2 kg * g
```

```fermium-error
# C23
y = sin
```

```fermium-error
# C24
k = 2
y = k(1)
k(x) = x
```

```fermium-error
# C25
if true
    q = 1
print(q)
```

```fermium-error
# C27
parallel for i from 1 to 3
    q = i
print(q)
```

```fermium-error
# C33
f(x) = x
y = 2 f
```

```fermium-error
# C34
apply(f, x) = f + x
y = apply(sin, 2)
```

```fermium-error
# C35
f(x) =
    g(y) = y
    return g
z = f(1)
```

```fermium-error
# C37
f(x) = x^2
y = f(1, 2)
```

```fermium-error
# C38
f(x [m]) = 2 x
y = f(3 s)
```

```fermium-error
# C39
f(x [m]) = 2 x
g(t) = t
y = f(g)
```

```fermium-error
# C40
f(x: vector) = 2 x
y = f(3)
```

```fermium-error
# C42
k = "a"
y = k(1)
```

```fermium-error
# C43
y = "a"(2)
```

```fermium-error
# C47
f(x) = vec(x, x)
ys = [1, 2, 3]
z = f(ys)
```

```fermium-error
# C48
f(x) =
    print(x)
y = f(1)
```

```fermium-error
# C49
f(x) =
    if x > 0
        return x
y = f(1)
```

```fermium-error
# C50
f(x) =
    if x > 0
        return 1
    return "a"
y = f(1)
```

```fermium-error
# C51
f(x) = f(x)
y = f(1)
```

```fermium-error
# C53
f(x) =
    g(y) = g(y - 1)
    return g(x)
z = f(1)
```

```fermium-error
# C54
f(n) =
    if n < 1
        return 1 m
    return f(n - 1) * 2 m
y = f(3)
```

```fermium-error
# C55
f(x [m]) = x
f(x [s]) = x
y = f(3 kg)
```

```fermium-error
# C56
area(x [m], y) = x y
area(x, y [m]) = x y
z = area(1 m, 2 m)
```

```fermium-error
# C58
x = "a" + 1
```

```fermium-error
# C59
x = "a" * "b"
```

```fermium-error
# C61
u = vec(1, 2)
w = vec(1, 2, 3)
b = u ≈ w
```

```fermium-error
# C62
b = vec(1, 2) ≈ 3
```

```fermium-error
# C63
x = if true then 1 else "a"
```

```fermium-error
# C64
x = if true then vec(1, 2) else vec(1, 2, 3)
```

```fermium-error
# C65
x = if true then [[1, 2], [3, 4]] else [[1, 2, 3], [4, 5, 6]]
```

```fermium-error
# C66
v = [vec(1, 2), vec(3, 4)]
w = v + v
```

```fermium-error
# C69
x = 3 m in s
```

```fermium-error
# C70
v = vec(1 m, 2 s)
print(v in cm)
```

```fermium-error
# C72
print "a" to 3 digits
```

```fermium-error
# C73
print 3 m to 30 digits
```

```fermium-error
# C75
seed(1, 2)
```

```fermium-error
# C76
seed(1 m)
```

```fermium-error
# C77
x = randn(1)
```

```fermium-error
# C78
x = sample(1)
```

## 4. To do

- **The other checker files.** Extend the table (and `COVERED` in spec_errors.rs) to the files not yet
  covered, by size of their error set: vecmat.rs (64 templates), solve.rs (55), stmts.rs (37), data.rs (33),
  pde.rs (31), builtin.rs (27), cinterop.rs (25), cplx.rs (20), pyinterop.rs (18), calculus.rs (15),
  lists.rs (15), modules.rs (15), eigen.rs (14), arrays.rs (13), parallel.rs (12), clist.rs (8), systems.rs (8),
  uncertain.rs (8), analyze.rs (6), events.rs (5), plus cppinterop.rs's `cerr(…)` messages and the few
  messages built into a variable before they are raised (`need_num` and similar helpers in builtin.rs, the
  `unify_or` closures in builtin.rs, pde.rs, stmts.rs, …). About 500 templates remain.
- **Examples for the 26 rows marked —**: natural-units regions (rows 9, 30), eigenvalue problems (22),
  function-local names and captures (28, 29, 31, 44, 45), ODE solutions (36, 46), Python modules (32), a
  parameter used as a function (41), recursion returning a non-number (52; today the kinds check of row 50 comes
  first), the dispatch guard (57), a zero-trip `for` loop (26; today row 25's message is given), a chained
  comparison of non-numbers (60; `need_num` comes first), complex values shown in an incompatible unit (71),
  `print` of a value with no printed form (74), and the Rust-only limit (79). Rows 67 and 68 are internal
  guards that no program reaches.
- **Hints.** The table lists messages only; the hints (the second line) are prose in the source.
- **Conformance cross-reference.** Name, per row, the conformance cases that expect that message.
