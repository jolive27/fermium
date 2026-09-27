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
covers 24 of the files completely** (418 templates) — the ones that hold the most common errors: unit and
dimension mismatches, undefined names, arity and argument kinds, dispatch between versions of a function,
conversions with `in`, the built-in functions' arguments, lists and indexing, uncertainties, the calculus
operators (derivatives, integrals, sums, `solve … for x`), complex lists and Fourier transforms, random
numbers, the statements (control flow, reassignment, list entries, `push`, local functions), vectors and
matrices (vecmat.rs), differential equations (`solve … with … for t from …`, solve.rs), data files, `table`, `fit` and
`plot` (data.rs), modules (modules.rs), complex numbers (cplx.rs), arrays of 3 or more dimensions (arrays.rs) ,
events in ODEs (`when`, events.rs) and eigenvalue problems (`solve … lowest N`, eigen.rs). The other files (PDEs,
parallel loops, `analyze` and the C, C++ and Python
interop; about 155 templates) are still to do; see §4.

Covered files: `arith.rs`, `arrays.rs`, `builtin.rs`, `calculus.rs`, `calls.rs`, `checker.rs`, `clist.rs`, `convert.rs`, `cplx.rs`, `data.rs`, `dispatch.rs`, `eigen.rs`, `events.rs`, `exprs.rs`, `lists.rs`, `modules.rs`, `names.rs`, `print.rs`, `rng.rs`, `stmts.rs`, `uncertain.rs`, `units.rs`, `solve.rs`, `vecmat.rs`.

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
Errors raised in a covered file through a helper defined in another file (for example stmts.rs's `need_num`,
"… must be a number, but it is …", used for the two sides of a comparison) are listed under the helper's file, and a template raised in several covered files (such as "can't add … to …") is
listed once, under the first.

The column *Example* names a ```fermium-error example in §3 (its first line is the comment `# C<n>`). The same
test parses and checks each example with the checker (`api::check_keep`) and requires that it is rejected with a
message matching that row; with `FERMIUM_BIN` set, `spec_examples_run` (fermium-syntax) also requires that
`fermium run` rejects it with a one-line error and exit status 1. Rows marked — have no example yet: 146 of the
418. Some are internal guards that no program reaches today (rows 67, 68), some need a construct whose own
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
| 119 | `a list of vectors needs one unit for all components; this is …` | lists.rs, stmts.rs | a list of vectors of mixed dimensions | — |
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
| | **Statements: control flow (stmts.rs)** | | | |
| 168 | `a condition must be true or false, but this is …` | stmts.rs | an `if`/`while` condition that isn't a boolean | C168 |
| 169 | `break can only be used inside a loop` | stmts.rs | `break` outside a loop | C169 |
| 170 | `continue can only be used inside a loop` | stmts.rs | `continue` outside a loop | C170 |
| 171 | `return can only be used inside a function` | stmts.rs | `return` at the top level | C171 |
| 172 | `return needs a value` | stmts.rs | a bare `return` | C172 |
| 173 | `can't loop over …; 'for x in ...' needs a list` | stmts.rs | `for x in v` where v isn't a list | C173 |
| 174 | `the range goes from … to …; both ends need the same units` | stmts.rs | `for i from a to b` with a, b of different dimensions | C174 |
| 175 | `the step is … but the range is …` | stmts.rs | a loop step of another dimension than the range | C175 |
| 176 | `this range is …, so it needs a step with units` | stmts.rs | a loop over a dimensioned range without a step | C176 |
| 177 | `… was set outside this … region, so it can't be changed here (its units mean something different there)` | stmts.rs | assigning, inside a natural-units region, a variable set outside it (units.md §6) | — |
| | **Statements: variables keep their kind and units (stmts.rs)** | | | |
| 178 | `… is …; it can't now hold …` | stmts.rs | reassigning a variable with a value of another dimension (semantics.md §2.2) | C178 |
| 179 | `… holds …; it can't now hold …` | stmts.rs | reassigning a variable with a value of another kind | C179 |
| 180 | `… holds a …-vector; it can't now hold a …-vector` | stmts.rs | reassigning a vector with one of another length | C180 |
| 181 | `… holds a …×… matrix; it can't now hold a …×… matrix` | stmts.rs | reassigning a matrix with one of another size | C181 |
| 182 | `… holds a …-dimensional array; it can't now hold a …-dimensional one` | stmts.rs | reassigning an array with one of another rank | — |
| 183 | `… needs a value before you can use … on it` | stmts.rs | `x += …` etc. on a variable with no value yet | — |
| 184 | `this doesn't produce a value to store` | stmts.rs | assigning the result of a statement-like call | — |
| 185 | `can't store an ODE solution in a variable this way` | stmts.rs | assigning an ODE solution other than with `solve` | — |
| 186 | `… is already defined here` | stmts.rs | a second definition of a local function in the same body | C186 |
| | **Statements: lists, entries and push (stmts.rs)** | | | |
| 187 | `… isn't a list, so you can't set …[...]` | stmts.rs | `x[i] = …` on a non-list | C187 |
| 188 | `… is a list of …; can't put … in it` | stmts.rs | storing a value of another dimension in a list entry | C188 |
| 189 | `… is …; can't put … in it` | stmts.rs | storing a value of the wrong kind in an entry | — |
| 190 | `…[i, j] = … sets an entry of a matrix, but … is …` | stmts.rs | `x[i, j] = …` on a non-matrix | C190 |
| 191 | `…[i, j, k] = … sets an entry of an array of 3 or more dimensions, but … isn't one` | stmts.rs | `x[i, j, k] = …` on a non-array | C191 |
| 192 | `push needs a list variable and a value: push(xs, x)` | stmts.rs | `push` with the wrong number of arguments | C192 |
| 193 | `push needs a list as its first argument, not …` | stmts.rs | `push` onto a non-list | C193 |
| 194 | `can't add … to a list of …` | stmts.rs | `push` of a value of another dimension | C194 |
| 195 | `can't add … to a list of complex numbers of …` | stmts.rs | the same for a list of complex numbers | — |
| 196 | `this list holds text, so you can only push text onto it` | stmts.rs | `push` of a number onto a list of text | C196 |
| 197 | `this list holds complex numbers, so you can't push … onto it` | stmts.rs | `push` of a non-number onto a complex list | — |
| 198 | `… is …, so you can't push … onto it` | stmts.rs | `push` of a value of the wrong kind | — |
| 199 | `clear needs a list variable: clear(xs)` | stmts.rs | `clear` without a list variable | C199 |
| 200 | `clear empties a list, and … is …` | stmts.rs | `clear` of a non-list | C200 |
| | **Statements: functions (stmts.rs)** | | | |
| 201 | `functions must be defined at the top level of the program (not inside a block)` | stmts.rs | a function definition inside `if`, a loop, … | — |
| 202 | `a function defined inside another function must fit on one line, like  …(x) = …  (define a longer one at the top level)` | stmts.rs | a multi-line local function | — |
| 203 | `a function defined inside another function takes its parameters' units from each call; leave out the [unit]` | stmts.rs | a `[unit]` annotation on a local function's parameter | C203 |
| 204 | `'where' isn't supported on a function defined inside another function; define the helper value first, then …(…) = …` | stmts.rs | `where` on a local function | — |
| 205 | `a function can't return the ODE solution … yet; return a number made from it instead, like …(…), ∫ …(…) d… or a root found with solve` | stmts.rs | returning an ODE solution | — |
| | **Values that must be numbers (stmts.rs helpers, used by many files)** | | | |
| 206 | `… must be a number, but it is …` | stmts.rs | `need_num`: an operand that must be a number (a comparison's side, an operator's operand, a built-in's argument) is a list, vector, text, … | C206 |
| 207 | `… must be a number, but it is a function` | stmts.rs | the same where the operand is a function | — |
| 208 | `… must be a number, but it is an ODE solution` | stmts.rs | the same where the operand is an ODE solution | — |
| | **Vectors and matrices (vecmat.rs)** | | | |
| 209 | `can't mix vectors and lists in arithmetic` | vecmat.rs | a vector and a list on the two sides of an arithmetic operator | C209 |
| 210 | `can't … a vector and a single number` | vecmat.rs | adding or subtracting a vector and a single number | C210 |
| 211 | `can't … a …-vector and a …-vector` | vecmat.rs | adding or subtracting vectors of different lengths | C211 |
| 212 | `can't … these vectors: component … is … on one side and … on the other` | vecmat.rs | adding vectors whose components have different units, component by component | C212 |
| 213 | `can't … vectors of … and …` | vecmat.rs | adding or subtracting vectors of different dimensions | C213 |
| 214 | `can't take the dot product of a …-vector and a …-vector` | vecmat.rs | `·` (dot product) of vectors of different lengths | C214 |
| 215 | `can't take the cross product of a …-vector and a …-vector` | vecmat.rs | `×` (cross product) of vectors of different lengths | C215 |
| 216 | `the cross product needs 3-vectors (or 2-vectors), not 4-vectors` | vecmat.rs | `×` of two 4-vectors | C216 |
| 217 | `× between a vector and a number: use * (or a space) to scale a vector` | vecmat.rs | `×` between a vector and a number | C217 |
| 218 | `can't divide by a vector` | vecmat.rs | a number or vector divided by a vector | C218 |
| 219 | `… needs all components of the vector in the same units, but this one has …` | vecmat.rs | a built-in that needs one unit for the whole vector (such as `norm`) given a vector with a different unit per component | C219 |
| 220 | `can't mix matrices and lists in arithmetic` | vecmat.rs | a matrix and a list on the two sides of an arithmetic operator | C220 |
| 221 | `can't … a matrix and a …` | vecmat.rs | adding or subtracting a matrix and a value of another kind (a number, a vector) | C221 |
| 222 | `can't … a … and a …` | vecmat.rs | adding or subtracting a matrix and a matrix of another size | C222 |
| 223 | `can't … matrices of … and …` | vecmat.rs | adding or subtracting matrices of different dimensions | C223 |
| 224 | `× is the cross product of vectors; multiply matrices with * or a space: A B` | vecmat.rs | `×` between matrices | C224 |
| 225 | `can't multiply a … times a …: the first needs as many columns as the second has rows` | vecmat.rs | a matrix product whose inner sizes differ | C225 |
| 226 | `can't multiply a … times a …-vector: the matrix needs one column per component` | vecmat.rs | a matrix times a vector whose length isn't the number of columns | C226 |
| 227 | `a vector times a matrix isn't defined here; write the matrix first (M v), or use transpose(M) v for the row-vector product` | vecmat.rs | a vector times a matrix | C227 |
| 228 | `can't divide by a matrix` | vecmat.rs | a value divided by a matrix | C228 |
| 229 | `every row of a matrix needs the same number of entries` | vecmat.rs | a matrix literal whose rows have different lengths | C229 |
| 230 | `a matrix can have 1 to … rows and 1 to … columns (at most …×…), not …×…` | vecmat.rs | a matrix literal with too many rows or columns | — |
| 231 | `all entries of a matrix need the same units; this one is … but the others are …` | vecmat.rs | a matrix literal whose entries have different dimensions | C231 |
| 232 | `zeros(r, c) makes an r×c matrix; r and c must be fixed whole numbers from 1 to …, like zeros(8, 8)` | vecmat.rs | `zeros(r, c)` with sizes that aren't fixed whole numbers in range | C232 |
| 233 | `zeros(1, 1) would be a single number; write 0` | vecmat.rs | `zeros(1, 1)` | C233 |
| 234 | `… needs a square matrix, but this one is …×…` | vecmat.rs | a built-in that needs a square matrix (`det`, `inverse`, `trace`, …) given a non-square one | C234 |
| 235 | `… needs a matrix, like [[1, 2], [3, 4]]` | vecmat.rs | a matrix built-in given something that isn't a matrix | C235 |
| 236 | `transpose needs a matrix, like [[1, 2], [3, 4]]` | vecmat.rs | `transpose` of something that isn't a matrix | C236 |
| 237 | `solve_linear(M, b) needs a matrix and a vector, like solve_linear(K, <1, 2> N)` | vecmat.rs | `solve_linear` whose arguments aren't a matrix and a vector | C237 |
| 238 | `solve_linear(M, b) got a …×… matrix and a …-vector; b needs one component per row` | vecmat.rs | `solve_linear(M, b)` with b's length not the number of rows of M | C238 |
| 239 | `… takes a matrix, like …(K), or two, like …(K, M) for K v = λ M v, but was given … arguments` | vecmat.rs | `eigenvalues`/`eigenvectors` with no argument or more than two | C239 |
| 240 | `… needs a square matrix from 2×2 to …×…, not …×…` | vecmat.rs | `eigenvalues`/`eigenvectors` of a square matrix larger than the limit (a non-square one gets row 234's message) | — |
| 241 | `…(K, M) needs K and M of the same size, but they are …×… and …×…` | vecmat.rs | `eigenvalues(K, M)` with K and M of different sizes | C241 |
| 242 | `…(M, …) takes a matrix and a number` | vecmat.rs | `row`/`column` without exactly two arguments | C242 |
| 243 | `…(M, k) needs a matrix, like [[1, 2], [3, 4]]` | vecmat.rs | `row`/`column` of something that isn't a matrix | C243 |
| 244 | `a … of a …×… matrix has … entr…, so it isn't a vector; pick an entry with M[i, j]` | vecmat.rs | `row`/`column` of a matrix whose rows (columns) have a single entry or too many | C244 |
| 245 | `this matrix has … …s, so there is no … …` | vecmat.rs | `row(M, i)`/`column(M, j)` with a fixed index outside the matrix | C245 |
| 246 | `a matrix entry must be picked with fixed numbers, like M[1, 2]` | vecmat.rs | `M[i, j]` with an index that isn't a fixed number where one is needed | — |
| 247 | `this vector has … components, so there is no component …` | vecmat.rs | `v[i]` with a fixed index outside the vector | C247 |
| 248 | `a row of a …×… matrix isn't a vector; pick an entry with M[i, j]` | vecmat.rs | `M[i]` (one index) on a matrix whose rows can't be vectors | — |
| 249 | `… is a matrix: set one entry at a time, like …[i, j] = …` | vecmat.rs | assigning a single number to a matrix variable's whole value by index form `M[i] = …` | C249 |
| 250 | `… is a vector: set one component, like …[i] = …` | vecmat.rs | setting a vector variable with the wrong index form | C250 |
| 251 | `the components of … have different units, so they can't be set one at a time; build the new vector, like … = <…>` | vecmat.rs | `v[i] = …` on a vector whose components have different units | C251 |
| 252 | `the entries of … are …; can't put … in it` | vecmat.rs | storing a value of another dimension in a matrix entry or vector component | C252 |
| 253 | `this vector already has units (a different unit on each component)` | vecmat.rs | a unit written after a vector whose components each already have a unit | C253 |
| 254 | `°C/°F can't be used for matrices` | vecmat.rs | a matrix given the unit °C or °F | C254 |
| 255 | `this already has units (…)` | vecmat.rs | a unit written after a value that already has one | — |
| 256 | `this matrix already has units (…); write the unit once, after the ]]` | vecmat.rs | a unit written after a matrix whose entries already have one | C256 |
| 257 | `°C/°F can't be used for vectors` | vecmat.rs | a vector given the unit °C or °F | C257 |
| 258 | `a complex number's parts are .re and .im (not .…)` | vecmat.rs | `.x`-style access other than `.re`/`.im` on a complex number | C258 |
| 259 | `a vector's components are .x, .y and .z (not .…)` | vecmat.rs | `.name` on a vector with a name other than x, y, z | C259 |
| 260 | `this is a single number (…), not a vector, so it has no .…` | vecmat.rs | `.x`/`.y`/`.z` on a single number | C260 |
| 261 | `'.…' only works on vectors (v.x) and data loaded from a file (data.…)` | vecmat.rs | `.name` on a value that is neither a vector nor loaded data | C261 |
| 262 | `angle(a, b) needs two vectors, like angle(<1, 0> m, <1, 1> m)` | vecmat.rs | `angle(a, b)` whose arguments aren't two vectors | C262 |
| 263 | `angle(a, b) needs two 2-vectors or two 3-vectors, but got a …-vector and a …-vector` | vecmat.rs | `angle(a, b)` of vectors of different or unsupported lengths | C263 |
| 264 | `… needs a vector, like <3, 4> m` | vecmat.rs | `norm`, `unit` or `hat` of something that isn't a vector | C264 |
| 265 | `cross(a, b) needs two vectors` | vecmat.rs | `cross(a, b)` whose arguments aren't two vectors | C265 |
| 266 | `vec(...) takes 2 to … components` | vecmat.rs | `vec(…)` with fewer than 2 or too many components | C266 |
| | **ODEs (solve.rs)** | | | |
| 267 | `this equation is complex, but its unknowns can't be made complex` | solve.rs | an ODE with a complex coefficient whose unknowns can't be complex (a list or vector unknown) | — |
| 268 | `a solve can have only one stop condition (until ...)` | solve.rs | two `until` conditions on one `solve` (today the parser rejects this first: grammar.md §3) | — |
| 269 | `until (a stop condition) is for differential equations` | solve.rs | `until` on a `solve` that isn't a differential equation (an internal guard: the grammar allows `until` only after a range) | — |
| 270 | `this solve has no derivatives in it, so there's no differential equation to solve` | solve.rs | a `solve … with … for t from …` whose equations have no derivatives | C270 |
| 271 | `this solve has … equation… for … unknown function… (…); they must match` | solve.rs | the number of equations isn't the number of unknown functions | C271 |
| 272 | `the range goes from … to …` | solve.rs | the two ends of the range have different dimensions | C272 |
| 273 | `the step is … but … is …` | solve.rs | the step's dimension isn't the variable's | C273 |
| 274 | `unknown method '…' (use rk45, rk4 with a step, or radau for stiff equations)` | solve.rs | `using` names an unknown method | C274 |
| 275 | `… chooses its own steps: remove  step …  (a fixed step is for rk4)` | solve.rs | a `step` with an adaptive method (rk45, radau, bdf) | C275 |
| 276 | `the rk4 method needs a fixed step:  for … from … to … step 0.01…` | solve.rs | `using rk4` without a `step` | C276 |
| 277 | `initial conditions look like  x(0) = 1 m  or  x'(0) = 0 m/s` | solve.rs | an initial condition that isn't of the form `x(t0) = …` or `x'(t0) = …` | C277 |
| 278 | `this initial condition isn't for one of the unknowns (…)` | solve.rs | an initial condition for a name that isn't an unknown | C278 |
| 279 | `…(…) isn't needed: the equation for … is order …` | solve.rs | an initial condition for a derivative the equation's order doesn't need | C279 |
| 280 | `the initial values of … don't match: … and …` | solve.rs | initial values of one unknown (value and derivatives) of different kinds or lengths | — |
| 281 | `an unknown can be a list of numbers or of vectors (with one unit), not a list of matrices` | solve.rs | an unknown whose initial value is a list of matrices | — |
| 282 | `the initial values of … don't match: one is a list, another isn't` | solve.rs | initial values of one unknown where one is a list and another isn't | C282 |
| 283 | `an initial value must be a number or a vector like <1, 0> m, not a matrix` | solve.rs | an initial value that is a matrix | C283 |
| 284 | `a vector unknown needs the same units in every component (write separate unknowns for quantities in different units, like x and v)` | solve.rs | a vector initial value with a different unit per component | C284 |
| 285 | `the initial values of … don't match: one is a …-vector, another a …` | solve.rs | initial values of one unknown: vectors of different lengths, or a vector and a number | C285 |
| 286 | `… can't be both complex and a list (lists of complex unknowns aren't supported yet)` | solve.rs | a complex unknown whose initial value is a list | — |
| 287 | `…(…) should be … but this is …` | solve.rs | an initial derivative whose dimension isn't the unknown's divided by the variable's (to its order) | C287 |
| 288 | `the initial condition is given at … but … is …` | solve.rs | an initial condition given at a value of another dimension than the variable's | C288 |
| 289 | `initial conditions must be at the start of the range (… = start)` | solve.rs | an initial condition given at a time other than the start of the range | C289 |
| 290 | `… can't be both complex and a vector (vectors of complex numbers aren't supported yet)` | solve.rs | a complex unknown whose initial value is a vector | — |
| 291 | `missing initial condition…: …` | solve.rs | an unknown (or one of its lower derivatives) without an initial condition | C291 |
| 292 | `one side of this equation is a vector and the other isn't (or they have different lengths)` | solve.rs | an equation with a vector on one side only, or vectors of different lengths | C292 |
| 293 | `with a list of unknowns, write each equation for one highest derivative, like  N' = …` | solve.rs | with a list of unknowns, an equation not of the form `N' = …` | — |
| 294 | `can't tell which unknown this equation is for` | solve.rs | an equation that names no unknown's highest derivative | — |
| 295 | `… is …, so … must be … too (here it is …)` | solve.rs | an ODE unknown's dimension forces another quantity's (the equation's other side, a derivative) and it doesn't match | — |
| 296 | `… must be … like …` | solve.rs | an equation's side must have a given dimension (shown with an example unit) | — |
| 297 | `… works out to … but should be …` | solve.rs | a term of an equation works out to another dimension than the unknown's derivative needs | — |
| 298 | `when works with the adaptive solver (rk45) for now` | solve.rs | `when` (an event) with a method other than rk45 | C298 |
| 299 | `when doesn't work with a list of unknowns yet` | solve.rs | `when` in a `solve` with a list of unknowns | — |
| 300 | `the tolerance is relative, so it must be a plain number (no units) like 1e-8, but it is …` | solve.rs | `tolerance` with units | C300 |
| 301 | `the tolerance must be a plain number written out, like 1e-8` | solve.rs | `tolerance` that isn't a constant number | C301 |
| 302 | `the tolerance is relative, so it must be between 0 and 1 (like 1e-8), but it is …` | solve.rs | `tolerance` outside (0, 1) | C302 |
| 303 | `absolute sets the error control of the adaptive solvers (rk45, radau, bdf); with  step  the steps are fixed, so leave out one of them` | solve.rs | `absolute` together with a fixed `step` | C303 |
| 304 | `an absolute tolerance must be a positive constant, like 1e-16 or 1e-9 m` | solve.rs | `absolute` that isn't a positive constant | C304 |
| 305 | `two absolute tolerances in …; give one value per unit` | solve.rs | two `absolute` values of the same dimension | C305 |
| 306 | `no absolute tolerance for …, which is … (the values given are in …); add one in its units after a comma, like  absolute …` | solve.rs | an unknown whose dimension has no `absolute` value when others do | C306 |
| 307 | `no unknown of this solve is in …, so this absolute tolerance isn't used` | solve.rs | an `absolute` value whose dimension no unknown has | C307 |
| 308 | `… both appear in one equation, which works only for unknowns that are numbers, but … is a vector; write its components as separate unknowns` | solve.rs | an implicit system (two unknowns' highest derivatives in one equation) where an unknown is a vector | — |
| 309 | `… both appear in one equation; that works for up to 4 unknowns (this solve has …); solve for the highest derivatives yourself, e.g. with solve_linear` | solve.rs | an implicit system with more than 4 unknowns | — |
| 310 | `… both appear in one equation; that works when every equation is linear in … (like m1 a'' + k b'' = F, with coefficients that may depend on … and the unknowns), and this one isn't` | solve.rs | an implicit system that isn't linear in the highest derivatives | C310 |
| 311 | `… drops out of the equations (its coefficients are all 0), so they can't be solved for it` | solve.rs | an implicit system where an unknown's highest derivative has all-zero coefficients | — |
| 312 | `the stop condition can use … (not …)` | solve.rs | an `until` condition that uses an unknown's highest derivative (or a higher one) | C312 |
| 313 | `the two sides of the stop condition don't match: left is …, right is …` | solve.rs | the two sides of an `until` condition have different dimensions | C313 |
| 314 | `… is a …-vector, so it has no .…` | solve.rs | `.name` on an ODE solution vector with a component it doesn't have | C314 |
| 315 | `… is complex, and lists of complex numbers aren't supported yet` | solve.rs | an ODE solution with complex values used where a list is needed | — |
| 316 | `… is a vector; use its components, like ….x` | solve.rs | an ODE vector solution used where a list of numbers is needed (as in `plot`) | — |
| 317 | `… takes one argument (…)` | solve.rs | an ODE solution called with other than one argument | C317 |
| 318 | `… is a function of …, which is …, not …` | solve.rs | an ODE solution evaluated at a value of another dimension than its variable | C318 |
| 319 | `… is complex, so it can't be evaluated at each element of a list (lists of complex numbers aren't supported yet)` | solve.rs | a complex ODE solution evaluated at a list of times | — |
| 320 | `… is a vector, so it can't be evaluated at each element of a list (lists of vectors aren't supported yet)` | solve.rs | an ODE vector solution evaluated at a list of times | C320 |
| 321 | `… is a list of unknowns, so it takes one time, not a list of times` | solve.rs | an ODE solution with a list of unknowns evaluated at a list of times | C321 |
| 322 | `can't take that many derivatives of the solution …` | solve.rs | too many derivatives of an ODE solution (beyond what can be computed from the equation) | — |
| 323 | `… is a list of unknowns: use its value at a time, like …(…) (a list) or …(…)[i], or its last value …[end]` | solve.rs | an ODE solution with a list of unknowns used as a value | — |
| | **Data, fit and plot (data.rs)** | | | |
| 324 | `in …, column '…': …` | data.rs | a data file's column header names a unit that isn't known (the unit parser's message follows) | — |
| 325 | `data files can't be loaded inside a … region (their columns are in SI units)` | data.rs | `load` inside a natural-units region | — |
| 326 | `can't find the file '…'` | data.rs | `load` of a file that doesn't exist | C326 |
| 327 | `a table needs at least one column, like  table(x = xs, y = ys)` | data.rs | `table()` without columns (today a bare `table()` is reported as an undefined name first) | — |
| 328 | `the column … of a table must be a list of numbers, like  … = [1, 2, 3] m` | data.rs | a `table` column that isn't a list of numbers | C328 |
| 329 | `the units of the column … aren't known here` | data.rs | a `table` column whose dimension isn't fixed where the table is built | — |
| 330 | `the data has no column called … (columns: …)` | data.rs | `data.name` for a column the data doesn't have | C330 |
| 331 | `err(x) gives the standard error of a parameter found by fit, like err(g) after fit T = 2π √(L/g) to data` | data.rs | `err(x)` of a name that isn't a parameter found by `fit` | C331 |
| 332 | `fit can only be used at the top level of a program` | data.rs | `fit` inside a block or a function | — |
| 333 | `fit ... to <data>: the data must come from load "file.csv" or table(x = xs, y = ys)` | data.rs | `fit … to d` where d isn't loaded data or a `table` | C333 |
| 334 | `the left side of a fit must use a column of the data (…)` | data.rs | the left side of `fit` isn't one of the data's columns | C334 |
| 335 | `this fit has no unknown parameters to adjust` | data.rs | a `fit` model with no unknown names to adjust | C335 |
| 336 | `the model gives … but … is …` | data.rs | the model's dimension isn't the fitted column's | — |
| 337 | `the starting guess for … is … but … must be …` | data.rs | a starting guess (`with k = …`) of the wrong dimension | — |
| 338 | `can't show … in …` | data.rs | a plot axis unit (`y in unit`) of another dimension than the plotted values | C338 |
| 339 | `all series in one plot need the same … units (here … and …)` | data.rs | two series in one plot with different dimensions on one axis | C339 |
| 340 | `sweep needs a loop like  sweep k in [1, 2, 4]` | data.rs | `sweep` without a loop over values | — |
| 341 | `the … range must be constants, like  … from 1e-12 to 1  or  … from 0 s to 10 s` | data.rs | an axis range (`x from a to b`) whose ends aren't constants | C341 |
| 342 | `the … axis is …, but this end of its range is …` | data.rs | an axis range end of another dimension than the axis | C342 |
| 343 | `the … range must go from the smaller value to the larger one; to have … decrease along the axis, add  reversed …` | data.rs | an axis range that goes from the larger value to the smaller | C343 |
| 344 | `a log … axis can't start at 0 or below` | data.rs | a log axis whose range starts at 0 or below | C344 |
| 345 | `… is a vector; plot its components, e.g.  plot ….y vs ….x` | data.rs | plotting an ODE vector solution directly | C345 |
| 346 | `… is a function of …; plot it  vs …` | data.rs | plotting a function against a name other than its variable | — |
| 347 | `a solution is plotted over the range it was solved for (no 'from ... to' needed)` | data.rs | a `from … to …` range on a plotted ODE solution (with `plot z vs t from …`, the solution-as-value message comes first today) | — |
| 348 | `can only plot two solutions against each other if they come from the same solve` | data.rs | plotting two ODE solutions from different `solve`s against each other | C348 |
| 349 | `… isn't defined; to plot a formula give a range, like plot y vs … from 0 to 10` | data.rs | plotting a formula against an undefined name without a range (today `q isn't defined` usually comes first) | — |
| 350 | `to plot a formula, the thing after 'vs' must be a variable name` | data.rs | plotting a formula against something that isn't a name | C350 |
| 351 | `the two ends of the plot range need the same units` | data.rs | a plot range whose ends have different dimensions | C351 |
| 352 | `can't plot … here; plot needs lists of values (or a solution, or a formula with a range)` | data.rs | plotting a value that isn't a list, a solution or a formula with a range | C352 |
| 353 | `an animation shows one PDE solution:  plot u vs x animate over t` | data.rs | `animate over` with more than one PDE solution | — |
| 354 | `a PDE solution's plot takes only  title  and  animate over t [frames N]  (not axis ranges, labels, log or reversed axes)` | data.rs | a PDE solution's plot with an option other than `title` and `animate` | — |
| 355 | `animate over t works for the solution of a PDE:  plot u vs x animate over t` | data.rs | `animate over` on something that isn't a PDE solution | — |
| 356 | `plot a PDE solution against its space variable:  plot … vs …` | data.rs | a PDE solution plotted against a name other than its space variable | — |
| 357 | `… changes with …: write  animate over …` | data.rs | `animate over` a name other than the PDE's time variable | — |
| 358 | `frames must be from 2 to 1000` | data.rs | `frames N` outside 2 to 1000 | — |
| | **Modules (modules.rs)** | | | |
| 359 | `… isn't a valid fermium.toml (line …: …)` | modules.rs | a `fermium.toml` package file that doesn't parse | — |
| 360 | `import must be at the top level of the program (not inside a block or function)` | modules.rs | `import` inside a block or a function | C360 |
| 361 | `the module file … has a name that can't be used in a program` | modules.rs | importing a module file whose name isn't a valid Fermium name (like `my-mod.fm`) | — |
| 362 | `… is private to the module … (names starting with _ aren't exported)` | modules.rs | `from m import _name`: names starting with `_` are private to their module | — |
| 363 | `can't find the module file "…"` | modules.rs | `import "path.fm"` of a file that doesn't exist | C363 |
| 364 | `can't find a module called …` | modules.rs | `import name` where no module file or package of that name is found | C364 |
| 365 | `… has no …` | modules.rs | `from m import x` or `m.x` where the module m doesn't define x | — |
| 366 | `… is already imported from … (line …); importing it from … too would be ambiguous` | modules.rs | the same name imported from two modules | — |
| 367 | `… already means something in this program, so it can't also be …` | modules.rs | an imported name (or module alias) that is already defined in the program | — |
| 368 | `… is … (imported on line …) and is defined again on line …` | modules.rs | a name imported from a module and then defined again in the program | — |
| 369 | `circular import: …` | modules.rs | modules that import each other in a cycle | — |
| 370 | `can't read the module …: …` | modules.rs | a module file that can't be read | — |
| 371 | `a module can only define functions and constants, but this line has …` | modules.rs | a module line that isn't a function or constant definition (a module can't print, plot or solve) | — |
| 372 | `in the module … (…): …` | modules.rs | an error inside a module file, reported with the module's name and line | — |
| 373 | `… is a module, not a value; use the names it defines, like …` | modules.rs | a module name used as a value | — |
| | **Complex numbers (cplx.rs)** | | | |
| 374 | `°C/°F can't be used for complex numbers (… has an offset)` | cplx.rs | a complex number given the unit °C or °F | — |
| 375 | `can't mix complex numbers and … in arithmetic` | cplx.rs | a complex number and a list, vector or matrix in arithmetic | C375 |
| 376 | `the exponent must be a number` | cplx.rs | a complex power whose exponent isn't a number | C376 |
| 377 | `the base of a complex power must be a number` | cplx.rs | a power with a complex exponent whose base isn't a number | C377 |
| 378 | `… needs a number or a complex number, but got …` | cplx.rs | a complex built-in (`re`, `im`, `abs`, `arg`, `conj`) given something that isn't a number | C378 |
| 379 | `complex(a, b) takes two real numbers: the real and imaginary parts` | cplx.rs | `complex(a, b)` with a complex part | C379 |
| 380 | `complex(a, b) needs both parts in the same units, but they are … and …` | cplx.rs | `complex(a, b)` with parts of different dimensions | C380 |
| 381 | `… takes real numbers` | cplx.rs | `polar`/`cis` with a complex argument | C381 |
| 382 | `the angle in … must be a plain number (radians or degrees), but it is …` | cplx.rs | the angle of `polar`/`cis` with units | C382 |
| 383 | `… doesn't work on complex numbers` | cplx.rs | a built-in that doesn't take complex numbers given one | C383 |
| 384 | `… needs a plain number, but got a complex number of …` | cplx.rs | `exp`, `ln`, `sin`, … of a complex number with units | C384 |
| 385 | `complex numbers can't be compared with … (they aren't ordered)` | cplx.rs | `<`, `>`, `≤`, `≥` with a complex number | C385 |
| 386 | `°C/°F can't be used for complex numbers` | cplx.rs | a complex number (from a computation) given the unit °C or °F | — |
| | **Arrays of 3 or more dimensions (arrays.rs)** | | | |
| 387 | `fill(value, n1, n2, …) makes an array of 1 to … dimensions: give the value, then the size along each (fill(0 K, 50, 50) is a 50×50 grid)` | arrays.rs | `fill` with no size, or too many sizes | C387 |
| 388 | `a size must be a plain whole number, not …` | arrays.rs | a `fill` size with units or not a whole number | C388 |
| 389 | `… can't take an array here` | arrays.rs | a built-in that doesn't take an array given one where an array can't go | — |
| 390 | `size(A, k): this array has … dimensions, so k is 1 to …` | arrays.rs | `size(A, k)` with k outside 1 to the array's number of dimensions | C390 |
| 391 | `… doesn't work on arrays yet (size, sum, mean, max, min, abs and copy do; loop over the entries for the rest)` | arrays.rs | a built-in other than size, sum, mean, max, min, abs and copy applied to an array | C391 |
| 392 | `… is a …-dimensional array, so it takes … indexes, like …[…]` | arrays.rs | an array read with the wrong number of indexes | C392 |
| 393 | `an array index is a whole number (slices and end aren't supported for arrays yet)` | arrays.rs | an array index that is a slice or `end` | C393 |
| 394 | `an array index must be a plain number (1, 2, 3, ...), not …` | arrays.rs | an array index with units | C394 |
| 395 | `an array index must be a whole number (1, 2, 3, ...), not …` | arrays.rs | an array index that isn't a whole number | C395 |
| 396 | `… is a …-dimensional array, so it takes … indexes, like …[…] = …` | arrays.rs | an array entry set with the wrong number of indexes | C396 |
| 397 | `… is an array of …; can't put … in it` | arrays.rs | storing a value of another dimension or kind in an array entry | C397 |
| 398 | `can't combine a …-dimensional array with a …-dimensional one` | arrays.rs | arithmetic between arrays with different numbers of dimensions | — |
| 399 | `an array can be combined with numbers and arrays, not with …` | arrays.rs | arithmetic between an array and a list, vector or matrix | C399 |
| | **Events in ODEs (events.rs)** | | | |
| 400 | `… can use … (not …)` | events.rs | a `when` condition that uses an unknown's highest derivative (or a higher one); it can use the unknowns and their lower derivatives | C400 |
| 401 | `the two sides of the condition of when don't match: left is …, right is …` | events.rs | the two sides of a `when` condition have different dimensions | C401 |
| 402 | `can't set … here: …` | events.rs | a `when` action that sets something other than an unknown or one of its lower derivatives | C402 |
| 403 | `… is set twice in this when` | events.rs | the same unknown set twice in one `when` | C403 |
| 404 | `… is … but this is …` | events.rs | a `when` action's value has another dimension than the value it sets | C404 |
| | **Eigenvalue problems (eigen.rs)** | | | |
| 405 | `… must be a whole number from … to …` | eigen.rs | `lowest N` or `grid N` outside its allowed range | C405 |
| 406 | `step, tolerance, absolute and until are for initial-value problems; an eigenvalue problem (lowest N) takes  grid N  and  using matrix / using shooting` | eigen.rs | `step`, `tolerance`, `absolute` or `until` on an eigenvalue problem | C406 |
| 407 | `an eigenvalue problem (lowest N) has one equation, like  -ħ²/(2m) * ψ'' + V(x) ψ = E ψ` | eigen.rs | an eigenvalue problem with more than one equation | C407 |
| 408 | `an eigenvalue problem needs one unknown function with a second derivative, like ψ''` | eigen.rs | an eigenvalue problem whose equation has no derivative of an unknown function | C408 |
| 409 | `an eigenvalue problem needs the second derivative …'' (this equation has …)` | eigen.rs | an eigenvalue problem whose equation has a derivative of another order | C409 |
| 410 | `unknown method '…' for an eigenvalue problem (use matrix or shooting)` | eigen.rs | `using` names a method other than `matrix` and `shooting` | C410 |
| 411 | `the boundary conditions of an eigenvalue problem look like  …(a) = 0, …(b) = 0 (the ends of the range)` | eigen.rs | a boundary condition not of the form `ψ(a) = 0` | C411 |
| 412 | `only … = 0 at the ends is supported for now (a wall, or far enough out that … has died away)` | eigen.rs | a boundary condition with a value other than 0 | C412 |
| 413 | `the boundary condition is at … but … is …` | eigen.rs | a boundary condition at a value of another dimension than the variable's | C413 |
| 414 | `the boundary conditions must be at the ends of the range (… = start and … = end)` | eigen.rs | a boundary condition at a point other than the ends of the range | C414 |
| 415 | `an eigenvalue problem needs … at both ends:  with …(…) = 0, …(…) = 0` | eigen.rs | an eigenvalue problem without a condition at each end | C415 |
| 416 | `an eigenvalue problem needs an unknown constant, like E in  … = E …; every name here already has a value (use a new name for the eigenvalue)` | eigen.rs | an eigenvalue problem where every name already has a value (no eigenvalue to find) | C416 |
| 417 | `this equation has … undefined names (…); an eigenvalue problem has exactly one unknown constant (the eigenvalue)` | eigen.rs | an eigenvalue problem with more than one undefined name | C417 |
| 418 | `…'' works out to … but should be …` | eigen.rs | the second-derivative term works out to another dimension than the equation needs | — |

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

```fermium-error
# C168
if 3 m
    print(1)
```

```fermium-error
# C169
break
```

```fermium-error
# C170
continue
```

```fermium-error
# C171
return 3
```

```fermium-error
# C172
f(x) =
    return
y = f(1)
```

```fermium-error
# C173
x = 3
for i in x
    print(i)
```

```fermium-error
# C174
for i from 1 m to 5 s
    print(i)
```

```fermium-error
# C175
for i from 1 m to 5 m step 1 s
    print(i)
```

```fermium-error
# C176
for i from 1 m to 5 m
    print(i)
```

```fermium-error
# C178
x = 3 m
x = 2 s
```

```fermium-error
# C179
x = 3
x = "a"
```

```fermium-error
# C180
v = vec(1, 2)
v = vec(1, 2, 3)
```

```fermium-error
# C181
A = [[1, 2], [3, 4]]
A = [[1, 2, 3], [4, 5, 6]]
```

```fermium-error
# C186
f(x) =
    g(y) = y
    g(y) = 2 y
    return g(x)
z = f(1)
```

```fermium-error
# C187
x = 3
x[1] = 2
```

```fermium-error
# C188
xs = [1 m, 2 m]
xs[1] = 3 s
```

```fermium-error
# C190
x = 3
x[1, 2] = 2
```

```fermium-error
# C191
x = 3
x[1, 2, 3] = 2
```

```fermium-error
# C192
xs = [1]
push(xs)
```

```fermium-error
# C193
x = 3
push(x, 3)
```

```fermium-error
# C194
xs = [1 m, 2 m]
push(xs, 3 s)
```

```fermium-error
# C196
xs = ["a"]
push(xs, 3)
```

```fermium-error
# C199
clear()
```

```fermium-error
# C200
x = 3
clear(x)
```

```fermium-error
# C203
f(x) =
    g(y [m]) = y
    return g(x)
z = f(1 m)
```

```fermium-error
# C206
b = [1, 2] < 3
```

```fermium-error
# C209
v = <1, 2> m + [1, 2] m
```

```fermium-error
# C210
v = <1, 2> m + 1 m
```

```fermium-error
# C211
v = <1, 2> m + <1, 2, 3> m
```

```fermium-error
# C212
v = <1 m, 2 s> + <1 s, 2 s>
```

```fermium-error
# C213
v = <1, 2> m + <1, 2> s
```

```fermium-error
# C214
d = <1, 2> m · <1, 2, 3> m
```

```fermium-error
# C215
c = <1, 2> m × <1, 2, 3> m
```

```fermium-error
# C216
c = <1, 2, 3, 4> × <1, 2, 3, 4>
```

```fermium-error
# C217
c = <1, 2, 3> m × 2
```

```fermium-error
# C218
x = 1 / <1, 2> m
```

```fermium-error
# C219
n = norm(<1 m, 2 s>)
```

```fermium-error
# C220
m = [[1, 2], [3, 4]] + [1, 2]
```

```fermium-error
# C221
m = [[1, 2], [3, 4]] + 1
```

```fermium-error
# C222
m = [[1, 2], [3, 4]] + [[1, 2, 3], [4, 5, 6]]
```

```fermium-error
# C223
m = [[1, 2], [3, 4]] m + [[1, 2], [3, 4]] s
```

```fermium-error
# C224
m = [[1, 2], [3, 4]] × [[1, 2], [3, 4]]
```

```fermium-error
# C225
m = [[1, 2], [3, 4]] * [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
```

```fermium-error
# C226
v = [[1, 2], [3, 4]] * <1, 2, 3>
```

```fermium-error
# C227
v = <1, 2> * [[1, 2], [3, 4]]
```

```fermium-error
# C228
m = 1 / [[1, 2], [3, 4]]
```

```fermium-error
# C229
m = [[1, 2], [3]]
```

```fermium-error
# C231
m = [[1 m, 2 s], [3 m, 4 m]]
```

```fermium-error
# C232
m = zeros(0, 3)
```

```fermium-error
# C233
m = zeros(1, 1)
```

```fermium-error
# C234
d = det([[1, 2, 3], [4, 5, 6]])
```

```fermium-error
# C235
d = det(3)
```

```fermium-error
# C236
t = transpose(3)
```

```fermium-error
# C237
x = solve_linear([[1, 2], [3, 4]], 3)
```

```fermium-error
# C238
x = solve_linear([[1, 2], [3, 4]], <1, 2, 3>)
```

```fermium-error
# C239
e = eigenvalues()
```

```fermium-error
# C241
e = eigenvalues([[1, 2], [2, 1]], [[1, 0, 0], [0, 1, 0], [0, 0, 1]])
```

```fermium-error
# C242
r = row([[1, 2], [3, 4]])
```

```fermium-error
# C243
r = row(3, 1)
```

```fermium-error
# C244
r = column([[1, 2]], 1)
```

```fermium-error
# C245
r = row([[1, 2], [3, 4]], 3)
```

```fermium-error
# C247
x = <1, 2>[3]
```

```fermium-error
# C249
M = [[1, 2], [3, 4]]
M[1] = 5
```

```fermium-error
# C250
v = <1, 2>
v[1, 2] = 5
```

```fermium-error
# C251
v = <1 m, 2 s>
v[1] = 3 m
```

```fermium-error
# C252
M = [[1, 2], [3, 4]] m
M[1, 1] = 5 s
```

```fermium-error
# C253
v = <1 m, 2 s> kg
```

```fermium-error
# C254
m = [[1, 2], [3, 4]] °C
```

```fermium-error
# C256
m = [[1 m, 2 m], [3 m, 4 m]] s
```

```fermium-error
# C257
v = <1, 2> °C
```

```fermium-error
# C258
z = (1 + 2i).x
```

```fermium-error
# C259
w = <1, 2>.w
```

```fermium-error
# C260
a = 3 m
w = a.x
```

```fermium-error
# C261
s = [1, 2].x
```

```fermium-error
# C262
a = angle(1, 2)
```

```fermium-error
# C263
a = angle(<1, 0>, <1, 1, 0>)
```

```fermium-error
# C264
n = norm(3)
```

```fermium-error
# C265
c = cross(1, 2)
```

```fermium-error
# C266
v = vec(1)
```

```fermium-error
# C270
solve z = 1 with z(0) = 1 for t from 0 to 1
```

```fermium-error
# C271
solve x' = -x, x' = 1 with x(0) = 1 for t from 0 to 1
```

```fermium-error
# C272
solve z' = -z/(1 s) with z(0) = 1 for t from 0 s to 1 m
```

```fermium-error
# C273
solve z' = -z/(1 s) with z(0) = 1 for t from 0 s to 1 s step 1 m using rk4
```

```fermium-error
# C274
solve z' = -z with z(0) = 1 for t from 0 to 1 using euler
```

```fermium-error
# C275
solve z' = -z with z(0) = 1 for t from 0 to 1 step 0.1 using radau
```

```fermium-error
# C276
solve z' = -z with z(0) = 1 for t from 0 to 1 using rk4
```

```fermium-error
# C277
solve z' = -z with z = 1 for t from 0 to 1
```

```fermium-error
# C278
solve z' = -z with z(0) = 1, w(0) = 2 for t from 0 to 1
```

```fermium-error
# C279
solve z' = -z with z(0) = 1, z'(0) = 1 for t from 0 to 1
```

```fermium-error
# C282
solve x'' = -x with x(0) = [1, 2], x'(0) = 0 for t from 0 to 1
```

```fermium-error
# C283
solve x' = -x with x(0) = [[1, 2], [3, 4]] for t from 0 to 1
```

```fermium-error
# C284
solve x' = -x with x(0) = <1 m, 2 s> for t from 0 to 1
```

```fermium-error
# C285
solve x'' = -x with x(0) = <1, 0>, x'(0) = 0 for t from 0 to 1
```

```fermium-error
# C287
solve y'' = -y/(1 s^2) with y(0) = 1 m, y'(0) = 1 m for t from 0 s to 1 s
```

```fermium-error
# C288
solve x' = -x/(1 s) with x(1 m) = 1 for t from 0 s to 1 s
```

```fermium-error
# C289
solve z' = -z with z(0.5) = 1 for t from 0 to 1
```

```fermium-error
# C291
solve x'' = -x with x(0) = 1 for t from 0 to 1
```

```fermium-error
# C292
solve x' = <1, 2> with x(0) = 1 for t from 0 to 1
```

```fermium-error
# C298
solve y'' = -9.8 with y(0) = 1, y'(0) = 0 for t from 0 to 1 step 0.01 using rk4
  when y = 0: y' = -0.9 y'
```

```fermium-error
# C300
solve z' = -z with z(0) = 1 for t from 0 to 1 tolerance 1e-8 m
```

```fermium-error
# C301
a = 0.001
solve z' = -z with z(0) = 1 for t from 0 to 1 tolerance a
```

```fermium-error
# C302
solve z' = -z with z(0) = 1 for t from 0 to 1 tolerance 2
```

```fermium-error
# C303
solve z' = -z with z(0) = 1 for t from 0 to 1 step 0.1 using rk4 absolute 1e-9
```

```fermium-error
# C304
solve z' = -z with z(0) = 1 for t from 0 to 1 absolute -1
```

```fermium-error
# C305
solve z' = -z with z(0) = 1 for t from 0 to 1 absolute 1e-9, 1e-8
```

```fermium-error
# C306
solve x' = -x/(1 s), y' = -y/(1 s) with x(0) = 1 m, y(0) = 1 kg for t from 0 s to 1 s absolute 1e-9 m
```

```fermium-error
# C307
solve x' = -x/(1 s) with x(0) = 1 m for t from 0 s to 1 s absolute 1e-9 m, 1e-9 kg
```

```fermium-error
# C310
solve a'' + b''^2 = 0, b'' = 1 with a(0) = 0, a'(0) = 0, b(0) = 0, b'(0) = 0 for t from 0 to 1
```

```fermium-error
# C312
solve y'' = -9.8 with y(0) = 1, y'(0) = 0 for t from 0 to 1 until y'' = 0
```

```fermium-error
# C313
solve x' = -x/(1 s) with x(0) = 1 m for t from 0 s to 1 s until x = 1 s
```

```fermium-error
# C314
solve r' = -r with r(0) = <1, 0> for t from 0 to 1
a = r.z
```

```fermium-error
# C317
solve z' = -z with z(0) = 1 for t from 0 to 1
a = z(1, 2)
```

```fermium-error
# C318
solve z' = -z/(1 s) with z(0) = 1 for t from 0 s to 1 s
a = z(1 m)
```

```fermium-error
# C320
solve r' = -r with r(0) = <1, 0> for t from 0 to 1
a = r([0.5, 1])
```

```fermium-error
# C321
solve N' = -N with N(0) = [1, 2] for t from 0 to 1
a = N([0.5, 1])
```

```fermium-error
# C326
d = load "no_such_file_here.csv"
```

```fermium-error
# C328
d = table(x = 3)
```

```fermium-error
# C330
d = table(x = [1, 2] m)
y = d.q
```

```fermium-error
# C331
a = 3
b = err(a)
```

```fermium-error
# C333
fit y = k x to 3
```

```fermium-error
# C334
d = table(x = [1, 2, 3], y = [2, 4, 6])
fit z = k x to d
```

```fermium-error
# C335
d = table(x = [1, 2, 3], y = [2, 4, 6])
fit y = 2 x to d
```

```fermium-error
# C338
xs = [1, 2, 3] s
ys = [1, 2, 3] m
plot ys in kg vs xs
```

```fermium-error
# C339
xs = [1, 2, 3] s
ys = [1, 2, 3] m
zs = [1, 2, 3] kg
plot ys vs xs, zs vs xs
```

```fermium-error
# C341
xs = [1, 2, 3] s
ys = [1, 2, 3] m
a = ys[1]
plot ys vs xs with y from a to 5 m
```

```fermium-error
# C342
xs = [1, 2, 3] s
ys = [1, 2, 3] m
plot ys vs xs with y from 0 s to 5 s
```

```fermium-error
# C343
xs = [1, 2, 3] s
ys = [1, 2, 3] m
plot ys vs xs with y from 5 m to 0 m
```

```fermium-error
# C344
xs = [1, 2, 3] s
ys = [1, 2, 3] m
plot ys vs xs with log y, y from 0 m to 5 m
```

```fermium-error
# C345
solve r' = -r with r(0) = <1, 0> for t from 0 to 1
plot r vs t
```

```fermium-error
# C348
solve a' = -a with a(0) = 1 for t from 0 to 1
solve b' = -b with b(0) = 1 for t from 0 to 1
plot a vs b
```

```fermium-error
# C350
xs = [1, 2]
plot 2 xs vs xs[1] from 0 to 1
```

```fermium-error
# C351
plot sin(q) vs q from 0 s to 1 m
```

```fermium-error
# C352
a = 3
b = 4
plot a vs b
```

```fermium-error
# C360
if true:
    import nosuchmodule
```

```fermium-error
# C363
import "no_such_module_here.fm"
```

```fermium-error
# C364
import no_such_module_here
```

```fermium-error
# C375
z = (1 + 2i) + [1, 2]
```

```fermium-error
# C376
z = (1 + 2i)^[1, 2]
```

```fermium-error
# C377
z = [1, 2]^(1 + 2i)
```

```fermium-error
# C378
z = re([1, 2])
```

```fermium-error
# C379
z = complex(1 + 2i, 3)
```

```fermium-error
# C380
z = complex(1 m, 2 s)
```

```fermium-error
# C381
z = cis(1 + 2i)
```

```fermium-error
# C382
z = cis(2 m)
```

```fermium-error
# C383
z = floor(1 + 2i)
```

```fermium-error
# C384
z = (1 + 2i) * 1 m
w = exp(z)
```

```fermium-error
# C385
b = (1 + 2i) < 3
```

```fermium-error
# C387
A = fill(0)
```

```fermium-error
# C388
A = fill(0, 3 m, 3, 3)
```

```fermium-error
# C390
A = fill(0, 3, 3, 3)
n = size(A, 4)
```

```fermium-error
# C391
A = fill(0, 3, 3, 3)
B = floor(A)
```

```fermium-error
# C392
A = fill(0, 3, 3, 3)
x = A[1, 2]
```

```fermium-error
# C393
A = fill(0, 3, 3, 3)
x = A[1, 2, end]
```

```fermium-error
# C394
A = fill(0, 3, 3, 3)
x = A[1, 2, 3 m]
```

```fermium-error
# C395
A = fill(0, 3, 3, 3)
x = A[1, 2, 1.5]
```

```fermium-error
# C396
A = fill(0, 3, 3, 3)
A[1, 2, 3, 1] = 5
```

```fermium-error
# C397
A = fill(0 K, 3, 3, 3)
A[1, 2, 3] = 5 m
```

```fermium-error
# C399
A = fill(0, 3, 3, 3)
B = A + [1, 2, 3]
```

```fermium-error
# C400
solve y'' = -9.8 with y(0) = 1, y'(0) = 0 for t from 0 to 1
  when y'' = 0: y' = 1
```

```fermium-error
# C401
solve y'' = -9.8 m/s^2 with y(0) = 1 m, y'(0) = 0 m/s for t from 0 s to 1 s
  when y = 0 s: y' = -0.9 y'
```

```fermium-error
# C402
solve y'' = -9.8 with y(0) = 1, y'(0) = 0 for t from 0 to 1
  when y = 0: y'' = 1
```

```fermium-error
# C403
solve y'' = -9.8 with y(0) = 1, y'(0) = 0 for t from 0 to 1
  when y = 0: y' = 1, y' = 2
```

```fermium-error
# C404
solve y'' = -9.8 m/s^2 with y(0) = 1 m, y'(0) = 0 m/s for t from 0 s to 1 s
  when y = 0 m: y' = 1 m
```

```fermium-error
# C405
solve -ψ'' = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 0
```

```fermium-error
# C406
solve -ψ'' = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 tolerance 1e-8 lowest 3
```

```fermium-error
# C407
solve -ψ'' = E ψ, -φ'' = E φ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C408
solve -ψ = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C409
solve -ψ''' = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C410
solve -ψ'' = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 using rk4 lowest 3
```

```fermium-error
# C411
solve -ψ'' = E ψ with ψ'(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C412
solve -ψ'' = E ψ with ψ(0) = 1, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C413
solve -ψ'' = E ψ/(1 m^2) with ψ(0 s) = 0, ψ(1 m) = 0 for x from 0 m to 1 m lowest 3
```

```fermium-error
# C414
solve -ψ'' = E ψ with ψ(0.5) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C415
solve -ψ'' = E ψ with ψ(0) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C416
E = 2
solve -ψ'' = E ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

```fermium-error
# C417
solve -ψ'' = E F ψ with ψ(0) = 0, ψ(1) = 0 for x from 0 to 1 lowest 3
```

## 4. To do

- **The other checker files.** Extend the table (and `COVERED` in spec_errors.rs) to the files not yet
  covered, by size of their error set: pde.rs (31), cinterop.rs (25),
  pyinterop.rs (18), parallel.rs (12), systems.rs (8),
  analyze.rs (6), plus cppinterop.rs's `cerr(…)` messages. About 155 templates remain.
- **Messages the extraction can't see.** A few errors in covered files pass on a message made elsewhere:
  builtin.rs's `to(x, unit)` raises the unit parser's own error for an unknown unit (fermium-units), and
  calculus.rs raises a "no version has that parameter" message passed in by its callers (`none_msg`). They are
  not rows yet.
- **Examples for the rows marked —** (146). In the first 79 rows: natural-units regions (rows 9, 30), eigenvalue problems (22),
  function-local names and captures (28, 29, 31, 44, 45), ODE solutions (36, 46), Python modules (32), a
  parameter used as a function (41), recursion returning a non-number (52; today the kinds check of row 50 comes
  first), the dispatch guard (57), a zero-trip `for` loop (26; today row 25's message is given), a chained
  comparison of non-numbers (60; `need_num` comes first), complex values shown in an incompatible unit (71),
  `print` of a value with no printed form (74), and the Rust-only limit (79). Rows 67 and 68 are internal
  guards that no program reaches. In rows 80–167 (builtin.rs, lists.rs, uncertain.rs, calculus.rs, clist.rs), 50
  rows: mostly the rarer argument errors, `propagate montecarlo`'s block, the vector-calculus operators and the
  two-list forms of the Fourier built-ins. In rows 168–208 (stmts.rs), 15 rows: natural-units regions, arrays,
  complex lists, ODE solutions and local-function limits. In rows 209–266 (vecmat.rs), 5 rows: the size limits
  and index forms that need large or run-time matrices. In rows 267–323 (solve.rs), 21 rows: implicit systems,
  complex and list unknowns, events with lists, and messages another check reaches first.
  In rows 324–358 (data.rs), 17 rows: data files with bad headers, `fit` models and guesses, PDE-solution plots
  and `sweep`. In rows 359–373 (modules.rs), 12 rows: they need module files next to the program, which a
  one-file example can't hold. In rows 374–386 (cplx.rs), 2 rows: °C/°F and a second unit on a complex value.
  In rows 387–399 (arrays.rs), 2 rows; rows 400–404 (events.rs) all have one; in rows 405–418
  (eigen.rs), 1 row.
- **Hints.** The table lists messages only; the hints (the second line) are prose in the source.
- **Conformance cross-reference.** Name, per row, the conformance cases that expect that message.
