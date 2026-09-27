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
covers 15 of the files completely** (167 templates) — the ones that hold the most common errors: unit and
dimension mismatches, undefined names, arity and argument kinds, dispatch between versions of a function,
conversions with `in`, the built-in functions' arguments, lists and indexing, uncertainties, the calculus
operators (derivatives, integrals, sums, `solve … for x`), complex lists and Fourier transforms, and random
numbers. The other files (`solve` for ODEs, `fit`, PDEs, eigenvalue problems, data tables, vectors and matrices,
complex numbers, arrays, statements, modules, parallel loops, `analyze`, events and the C, C++ and Python
interop; about 410 templates) are still to do; see §4.

Covered files: `arith.rs`, `builtin.rs`, `calculus.rs`, `calls.rs`, `checker.rs`, `clist.rs`, `convert.rs`, `dispatch.rs`, `exprs.rs`, `lists.rs`, `names.rs`, `print.rs`, `rng.rs`, `uncertain.rs`, `units.rs`.

The table in §2 is **normative and complete for the covered files**. The test `spec_checker_errors`
(`cargo test -p fermium-check --test spec_errors`) extracts every message template from those files — the
first argument of each `err(…)` and `Diagnostic::error(…)`, the message of each `unify_or(…, |c| …)` closure,
and the literals that start a `format!(` or a `{ … }` branch of a `let msg = …` that the next lines raise with
`err(msg, …)` — plus two kinds of
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
`fermium run` rejects it with a one-line error and exit status 1. Rows marked — have no example yet: 71 of the
167. Some are internal guards that no program reaches today (rows 67, 68), some need a construct whose own
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
| 37 | `… takes … argument… but was given …` | builtin.rs, calls.rs, dispatch.rs | wrong number of arguments | C37 |
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
| 69 | `can't show … in … (…)` | builtin.rs, convert.rs | `x in u` where u has another dimension than x (units.md §5) | C69 |
| 70 | `can't show a vector with different units per component in …` | builtin.rs, convert.rs | `in` on a vector of mixed dimensions | C70 |
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
| | **Built-in functions: arguments (builtin.rs)** | | | |
| 80 | `… needs at least one argument` | builtin.rs | `min()`, `max()` with no arguments | C80 |
| 81 | `… needs a plain number, but got …` | builtin.rs | a function of plain numbers (`sin`, `log`, `exp`, …) given a value with units | C81 |
| 82 | `… needs plain numbers, but … is …` | builtin.rs | a built-in over plain numbers given a list or value with units | — |
| 83 | `… needs a list, but got …` | builtin.rs | a list built-in (`sum`, `len`, `times`, …) given something else | C83 |
| 84 | `… of a single value needs a list` | builtin.rs | `mean`, `std`, … of one number | — |
| 85 | `… of … would depend on which unit you mean` | builtin.rs | a function whose value depends on the unit chosen (e.g. a logarithm of a dimensioned value inside a statistic) | — |
| 86 | `… needs both values in the same units` | builtin.rs | `hypot`, `mod`, … with two dimensions | C86 |
| 87 | `… needs all values in the same units` | builtin.rs | `max`, `min`, … with mixed dimensions | C87 |
| 88 | `… needs all values in the same units (here … and …)` | builtin.rs | the same for the list-taking built-ins, naming the dimensions | — |
| 89 | `atan2(y, x) needs y and x in the same units` | builtin.rs | `atan2` with two dimensions | C89 |
| 90 | `clamp needs all values in the same units` | builtin.rs | `clamp(x, lo, hi)` with mixed dimensions | C90 |
| 91 | `interp(x, xs, ys) needs a value and two lists` | builtin.rs | `interp` with the wrong kinds | C91 |
| 92 | `interp: x and xs need the same units` | builtin.rs | `interp`: the point and the grid differ in dimension | C92 |
| 93 | `linspace(a, b, n): a and b need the same units` | builtin.rs | the ends of `linspace` differ in dimension | C93 |
| 94 | `linspace(a, b, n): n must be a plain number` | builtin.rs | the count of `linspace` has units | C94 |
| 95 | `range needs a start and an end: range(a, b) or range(a, b, step)` | builtin.rs | `range` with one argument | C95 |
| 96 | `range: all values need the same units` | builtin.rs | `range` with mixed dimensions | C96 |
| 97 | `…(n): n must be a plain number` | builtin.rs | `zeros(n)`, `ones(n)` with a count that has units | C97 |
| 98 | `…(n, x) needs a whole-number order n, not …` | builtin.rs | a Bessel or Legendre function with a non-integer order | C98 |
| 99 | `identity(n) needs a fixed whole number, like identity(3)` | builtin.rs | `identity` of a value known only at run time | — |
| 100 | `identity(n) needs a whole number n from 2 to … (matrices are at most …×…)` | builtin.rs | `identity` of a size out of range | C100 |
| 101 | `trapz(ys, xs) needs two lists` | builtin.rs | `trapz` with values that aren't lists | C101 |
| 102 | `dot(a, b) needs two lists` | builtin.rs | `dot` of two non-lists | C102 |
| 103 | `clock() takes no arguments` | builtin.rs | `clock` with an argument | C103 |
| 104 | `str takes 1 argument but was given …` | builtin.rs | `str` with several arguments | C104 |
| 105 | `str(x) turns a single number into text, not …` | builtin.rs | `str` of a list, vector, … | — |
| 106 | `to needs a value and a unit: to(x, eV)` | builtin.rs | `to` with one argument | C106 |
| 107 | `to(x, unit) needs a number` | builtin.rs | `to` of a non-number | — |
| 108 | `can't add absolute temperatures: … of a list in … would add them as kelvins (10 … + 20 … isn't 30 …)` | builtin.rs | `sum`/`mean` of a list of °C or °F readings | — |
| 109 | `…(list, value) changes a list and doesn't give a value; write it on its own line` | builtin.rs | `push`/`append` used as a value | C109 |
| 110 | `seed(n) is a statement on its own line, like  seed(42)` | builtin.rs | `seed` used as a value | C110 |
| 111 | `times(...) needs an ODE solution` | builtin.rs | `times` of something that isn't an ODE solution | — |
| 112 | `size(A) is the shape of an array (made with fill(value, n1, n2, …)); for a list use len(xs)` | builtin.rs | `size` of a non-array | — |
| 113 | `copy(A) is a new array with the same entries (made with fill(value, n1, n2, …))` | builtin.rs | `copy` of a non-array | — |
| 114 | `… is a function; give it an argument` | builtin.rs | a function passed to a built-in that takes values | — |
| 115 | `… can't be used this way` | builtin.rs | a name that isn't a value, passed to a built-in | — |
| | **Lists and indexing (lists.rs)** | | | |
| 116 | `a list can hold numbers or text, but not both` | lists.rs | `[1, "a"]` | C116 |
| 117 | `all elements of a list must be the same kind of value: this one is … but the first is …` | lists.rs | a list mixing numbers, vectors, … | — |
| 118 | `all elements of a list need the same units; this one is … but earlier ones are …` | lists.rs | a list mixing dimensions | C118 |
| 119 | `a list of vectors needs one unit for all components; this is …` | lists.rs | a list of vectors of mixed dimensions | — |
| 120 | `a list of complex numbers can't hold …` | lists.rs | a non-number in a list of complex numbers | — |
| 121 | `a matrix is written as a list of rows, like [[1, 2], [3, 4]]; lists of lists aren't supported otherwise` | lists.rs | nested lists that aren't a matrix | — |
| 122 | `a list index must be a plain number (1, 2, 3, ...), not …` | lists.rs | an index with units | C122 |
| 123 | `a list index must be a whole number (1, 2, 3, ...), not …` | lists.rs | a constant index that isn't whole | C123 |
| 124 | `there is no index … here: valid indexes are 1 to …` | lists.rs | a constant index out of range where the length is known | — |
| 125 | `only lists can be indexed with [...]` | lists.rs | indexing a number, text, … | C125 |
| 126 | `a complex number can't be indexed with [...]` | lists.rs | indexing a complex number | C126 |
| 127 | `this vector's components have different units, so pick one with a fixed number, like v[1]` | lists.rs | a run-time index into a vector of mixed dimensions | — |
| 128 | `only lists can be sliced with [a:b], and … isn't a list` | lists.rs | slicing a non-list | C128 |
| 129 | `a data table can't be sliced with [a:b]; its columns are lists, and those can be` | lists.rs | slicing a data table | — |
| 130 | `a:b can only be used inside [...] to take part of a list, like xs[2:5]` | lists.rs | a slice outside brackets | — |
| 131 | `'end' can only be used inside [...] to mean the last element` | lists.rs | `end` outside an index | — |
| | **Uncertainties (uncertain.rs)** | | | |
| 132 | `the uncertainty after ± is … but the value is …; both need the same units` | uncertain.rs | `x ± u` with different dimensions (semantics.md §2.3) | C132 |
| 133 | `a relative uncertainty is a plain number, like  x ± 3%` | uncertain.rs | a percentage uncertainty with units | — |
| 134 | `a single value needs a single uncertainty after ±, not a list` | uncertain.rs | `3 ± [1, 2]` | C134 |
| 135 | `… takes 1 argument but was given …` | uncertain.rs | `value`, `uncertainty`, … with several arguments | — |
| 136 | `the number of samples must be a plain number` | rng.rs, uncertain.rs | `propagate montecarlo` or `sample` with a count that has units (also rng.rs) | — |
| 137 | `propagate montecarlo needs at least one formula (name = …) in its block` | uncertain.rs | an empty `propagate montecarlo` block | — |
| 138 | `only formulas (name = …) go inside  propagate montecarlo; print or plot the results after the block` | uncertain.rs | another statement in the block | — |
| 139 | `propagate montecarlo gives uncertainties to plain numbers, but … is …; give each number its own formula, like …1 = …, …2 = …` | uncertain.rs | a formula whose value isn't a number | — |
| | **Calculus (calculus.rs)** | | | |
| 140 | `' (prime) means a derivative; it only works on functions and ODE solutions` | calculus.rs | `x'` of a plain variable (semantics.md §6.1) | C140 |
| 141 | `…' is ambiguous: … has several parameters` | calculus.rs | `f'` of a function of several variables | C141 |
| 142 | `… has no parameter called …` | calculus.rs | `∂f/∂z` where f has no parameter z | — |
| 143 | `… has no parameter called …, so … … would be 0` | calculus.rs | the same, where the derivative would be identically 0 | — |
| 144 | `d/d… …: … …n't defined, so this can't be a function of … alone` | calculus.rs | `d/dx` of an expression that uses undefined names | — |
| 145 | `can't differentiate …: it calls itself` | calculus.rs | the symbolic derivative of a recursive function | — |
| 146 | `… can only differentiate one-line functions like φ(x, y, z) = ..., and … is defined over several lines` | calculus.rs | `∇`, `div`, `curl`, … of a multi-line function | — |
| 147 | `… works on a function of the coordinates, like φ(x, y, z) = ..., and … isn't a function` | calculus.rs | `∇` of a non-function | — |
| 148 | `… needs a function of …; … has …` | calculus.rs | `∇` etc. of a function with the wrong number of coordinates | — |
| 149 | `… needs … to be a vector formula, like …(x, y, z) = <-y, x, 0> T` | calculus.rs | `div`/`curl` of a scalar function | — |
| 150 | `… has … components but … coordinates; … needs them to match` | calculus.rs | `div`/`curl` of a vector field whose length isn't the number of coordinates | — |
| 151 | `the limits of this integral are … and …; they need the same units` | calculus.rs | `∫ … from a to b` with a and b of different dimensions (semantics.md §6.2) | C151 |
| 152 | `the limits of this integral are … and …: the ' / ' after the upper limit divides the whole integral, not the limit` | calculus.rs | `from 0 to L / 2`-style limits that read as a division of the integral | — |
| 153 | `this sum runs from … to …; the start and end need the same units` | calculus.rs | `Σ(… for k from a to b)` with a and b of different dimensions | C153 |
| 154 | `the step of a sum needs the same units as its start` | calculus.rs | a sum's step of another dimension | — |
| 155 | `this sum runs over …, so it needs a step, like Σ(… for … from a to b step 1 …)` | calculus.rs | a sum over a dimensioned range without a step | — |
| 156 | `a sum needs a finite number of terms` | calculus.rs | a sum with an infinite bound | — |
| 157 | `the search range goes from … to …; both ends need the same units` | calculus.rs | `solve … for x from a to b` with a, b of different dimensions (semantics.md §6.3) | C157 |
| 158 | `the two sides of this equation don't match: left is …, right is …` | calculus.rs | an equation to solve whose sides differ in dimension | — |
| 159 | `step, tolerance, absolute and using are for differential equations; an equation is solved to full precision` | calculus.rs | ODE options on an algebraic `solve` | — |
| | **Complex lists and Fourier transforms (clist.rs)** | | | |
| 160 | `complex(re, im) needs both parts in the same units, but they are … and …` | clist.rs | `complex` of two lists of different dimensions | — |
| 161 | `complex(re, im) takes two real numbers, or two lists of real numbers` | clist.rs | `complex` of lists of the wrong kind | — |
| 162 | `ifft(re, im): the real and imaginary parts need the same units` | clist.rs | `ifft` of two lists of different dimensions | — |
| 163 | `frequencies(n, dt): n must be a plain number` | clist.rs | `frequencies` with a count that has units | — |
| 164 | `…(xs): xs must be a list, not …` | clist.rs | `fft` etc. of a non-list | C164 |
| 165 | `…: … must be a list, not …` | clist.rs | a two-argument transform given a non-list | — |
| 166 | `… doesn't work on a list of complex numbers` | clist.rs | a real-only list built-in on complex lists | — |
| 167 | `… takes … argument…: …` | clist.rs | a transform with the wrong number of arguments | — |

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

```fermium-error
# C80
y = min()
```

```fermium-error
# C81
y = log(3 m)
```

```fermium-error
# C83
x = sum(3)
```

```fermium-error
# C86
x = hypot(1 m, 2 s)
```

```fermium-error
# C87
x = max(1 m, 2 s)
```

```fermium-error
# C89
x = atan2(1 m, 2 s)
```

```fermium-error
# C90
x = clamp(1 m, 0 s, 2 m)
```

```fermium-error
# C91
y = interp(1, 2, 3)
```

```fermium-error
# C92
y = interp(1 m, [1 s, 2 s], [3, 4])
```

```fermium-error
# C93
xs = linspace(0 m, 1 s, 10)
```

```fermium-error
# C94
xs = linspace(0 m, 1 m, 10 s)
```

```fermium-error
# C95
xs = range(1)
```

```fermium-error
# C96
xs = range(1 m, 5 s)
```

```fermium-error
# C97
y = zeros(3 m)
```

```fermium-error
# C98
y = besselj(1.5, 2)
```

```fermium-error
# C100
x = identity(40)
```

```fermium-error
# C101
x = trapz(1, 2)
```

```fermium-error
# C102
x = dot(1, 2)
```

```fermium-error
# C103
x = clock(1)
```

```fermium-error
# C104
x = str(1, 2)
```

```fermium-error
# C106
x = to(3 m)
```

```fermium-error
# C109
xs = [1, 2]
y = push(xs, 3)
```

```fermium-error
# C110
y = seed(3)
```

```fermium-error
# C116
xs = [1, "a"]
```

```fermium-error
# C118
xs = [1 m, 2 s]
```

```fermium-error
# C122
xs = [1, 2]
y = xs[1 m]
```

```fermium-error
# C123
xs = [1, 2]
y = xs[1.5]
```

```fermium-error
# C125
x = 3
y = x[1]
```

```fermium-error
# C126
z = 1 + 2i
y = z[1]
```

```fermium-error
# C128
x = 3
y = x[1:2]
```

```fermium-error
# C132
x = 3 m ± 2 s
```

```fermium-error
# C134
x = 3 ± [1, 2]
```

```fermium-error
# C140
x = 3
y = x'
```

```fermium-error
# C141
f(x, y) = x y
g = f'(1, 2)
```

```fermium-error
# C151
y = ∫ x dx from 0 m to 1 s
```

```fermium-error
# C153
y = Σ(k for k from 1 m to 5 s)
```

```fermium-error
# C157
solve x = 2 m for x from 0 m to 3 s
```

```fermium-error
# C164
z = fft(3)
```

## 4. To do

- **The other checker files.** Extend the table (and `COVERED` in spec_errors.rs) to the files not yet
  covered, by size of their error set: vecmat.rs (64 templates), solve.rs (55), stmts.rs (37, including the
  `need_num` helpers used by many covered files), data.rs (33), pde.rs (31), cinterop.rs (25), cplx.rs (20),
  pyinterop.rs (18), modules.rs (15), eigen.rs (14), arrays.rs (13), parallel.rs (12), systems.rs (8),
  analyze.rs (6), events.rs (5), plus cppinterop.rs's `cerr(…)` messages. About 410 templates remain.
- **Messages the extraction can't see.** A few errors in covered files pass on a message made elsewhere:
  builtin.rs's `to(x, unit)` raises the unit parser's own error for an unknown unit (fermium-units), and
  calculus.rs raises a "no version has that parameter" message passed in by its callers (`none_msg`). They are
  not rows yet.
- **Examples for the rows marked —** (71). In the first 79 rows: natural-units regions (rows 9, 30), eigenvalue problems (22),
  function-local names and captures (28, 29, 31, 44, 45), ODE solutions (36, 46), Python modules (32), a
  parameter used as a function (41), recursion returning a non-number (52; today the kinds check of row 50 comes
  first), the dispatch guard (57), a zero-trip `for` loop (26; today row 25's message is given), a chained
  comparison of non-numbers (60; `need_num` comes first), complex values shown in an incompatible unit (71),
  `print` of a value with no printed form (74), and the Rust-only limit (79). Rows 67 and 68 are internal
  guards that no program reaches. In rows 80–167 (builtin.rs, lists.rs, uncertain.rs, calculus.rs, clist.rs), 50
  rows: mostly the rarer argument errors, `propagate montecarlo`'s block, the vector-calculus operators and the
  two-list forms of the Fourier built-ins.
- **Hints.** The table lists messages only; the hints (the second line) are prose in the source.
- **Conformance cross-reference.** Name, per row, the conformance cases that expect that message.
