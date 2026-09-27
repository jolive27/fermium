# Semantics

Draft 0.3, **partial**: a high-level account of what programs mean. Where a detail is not specified here,
conformance/ and docs/reference.md are the authority. Source: `rust/crates/fermium-check` (static semantics),
`fermium-ir` (the typed IR), `fermium-codegen` / `fermium-runtime` (execution), `fermium-units/src/numfmt.rs`
(printing numbers), `fermium-sym` (symbolic and automatic differentiation).

## 1. Program execution model

1. **The whole program is checked before any of it runs.** Parsing, name resolution, type and dimension checking
   (units.md) and the other static checks cover every statement; the first error stops compilation and nothing
   is printed (except warnings). So a unit error on the last line prevents the first line's `print`.
2. The checked program is lowered to a typed IR and compiled to native code with LLVM (`fermium run` compiles in
   memory; `fermium build` writes an executable), or, for the constructs the compiler hands over (`plot`, `fit`,
   `load` and a few others, docs/reference.md §17), run by the built-in interpreter over the same IR. Both
   paths must produce the same output (a conformance requirement).
3. Top-level statements run in order. There is one global scope; a function body has its own local scope and
   can read global variables (the exact visibility rules are TODO, §7). Functions are checked at their calls
   (§2.1).
4. Units are erased: at run time a quantity is a double holding its value in SI units (in canonical units inside a
   natural-units region, units.md §6). Display units exist only at compile time and are baked into `print`.

```fermium-error
print 1
y = 2 m + 3 s
```

(Nothing is printed: the error on line 2 is found before line 1 runs.)

## 2. Values and types

The static types (`fermium-ir/src/types.rs`, `Ty`):

| Type | Values | Dimension |
|---|---|---|
| number | an IEEE-754 double | one dimension (units.md §1) |
| bool | `true`, `false` | — |
| text | a string | — |
| list | a growable sequence of numbers, 1-based | one shared dimension |
| vector | 2, 3 or 4 components (`<x, y, z>`) | one shared dimension, or one per component (mixed, D29) |
| matrix | r × c entries | one shared dimension (D195) |
| complex | `a + bi`, stored as two doubles | one dimension (D90) |
| uncertain | a nominal value with first-order error sources (`5.0 ± 0.2 m`) | one dimension (D120) |
| array | 2- to 4-dimensional, shape known at run time (D283) | one shared dimension |
| lists of complex numbers, of texts, of vectors or matrices | | one element type (D243, D281) |
| function | a definition, monomorphised per call signature (D14, D285) | per instance |
| ODE solution | the result of `solve`, callable as `x(t)` | per unknown |
| table | `load "file.csv"`, `table(x = …, y = …)` | per column |

- There is no separate integer type: whole numbers are doubles (loop counters may be kept as 64-bit integers
  internally when that is invisible, D150). A list index must be a whole number
  (`a list index must be a whole number (1, 2, 3, ...), not 1.5`); indexes start at 1, and `end` in an index is
  the last one.
- `and`, `or` short-circuit; conditions must be bool.
- Text is joined with `+` and converted with `str(x)`; it takes part in no other arithmetic.
- A **variable keeps its type and dimension** (D13); `solve` may re-bind a name.

### 2.1 Functions

`f(x) = …` (one line) or `f(x) =` followed by an indented body with `return` defines a function. A function is
checked once for each distinct combination of argument types and dimensions it is called with (monomorphisation,
D14), and each instance is compiled separately. Calling a scalar function with a list maps it over the list.
Recursive functions are allowed. Several definitions of one name with different arities, parameter kinds
(`: number`, `: vector`, `: list`, `: complex`) or bracketed units are *versions*, chosen at compile time (D285).

```fermium
f(x) = 2 x
print f(3 m), f(2)
```

### 2.2 Scoping (fermium-check: `names.rs`, `stmts.rs`, `calls.rs`, `modules.rs`)

Fermium has **one global scope per program, one local scope per function call, and nothing else**: blocks
(`if`, `for`, `while`, `sweep`) do not open scopes. The rules below are normative; each is fixed by an example
that runs or is rejected.

1. **Top level.** An assignment `x = e` at the top level (or inside a block at the top level) defines or
   reassigns the global `x`. A name must be assigned on an earlier line before it is read ("x isn't defined").
   A variable keeps the dimension of its first assignment: assigning a value of another dimension is an error
   ("x is length [m]; it can't now hold time [s]"). A built-in constant (`c`, `h`, `G`, …) may be reassigned:
   from that line on the name means the program's value, with a warning (D34, "c … is now your variable").
2. **Definite assignment.** A name assigned only inside an `if` without an `else` that also assigns it, or only
   inside a `for`/`while` body, may not be read after the block ("z might not have a value here: it is only set
   inside the if on line 2"). A `for` loop's variable after the loop is the same case. If the name had a value
   before the block, the block updates it: after `i = 10` and `for i from 1 to 3`, `i` is 3.
3. **Functions read globals late.** A function body may read global variables; it reads the value the global
   has **when the function is called**, not when it was defined, and the global only has to be defined before
   the first call (a call that happens before the global's first assignment is the error "q isn't defined",
   reported at the body with "this happened when calling f on line N"). The dimension of a global cannot change
   (rule 1), so late reading never changes a function's units.
4. **Locals.** Parameters are local. In a multi-line body, the first assignment to a name makes it a local of
   that call; before that line a read of the name reads the global (rule 3), after it the local. Assignments
   (including `+=`) never change a global: `k += 1` in a body reads the global `k` and creates a local `k`. A
   local is not visible after the function returns, nor to other functions. Definite assignment (rule 2)
   applies inside bodies too.
5. **Nested functions** must be one line (`inner(y) = x y` inside `outer(x)`); they see the enclosing call's
   parameters and locals, and are not visible outside it (D194). A multi-line nested definition is an error.
6. **Functions calling functions:** a body may call any function defined at the top level, before or after it
   (the callee must exist when the call runs), including itself. Functions are passed to functions by name
   and specialised at compile time (D43).
7. **`where`** (`e where a = 1 m, b = 2 m`) binds its names for `e` only; they shadow globals of the same name
   and are not defined after the statement.
8. **Integrals, derivatives and sums** evaluate their bodies in the scope where they are written; their bound
   variable (`u` in `∫ … du`, `i` in `Σ(… for i …)`) is local to the body. Other names are read when the
   integral is evaluated (inside a function: at the call, rule 3).
9. **`solve` with derivatives** makes each unknown a global (or, inside a function, a local) *ODE solution*
   callable as `x(t)`; the independent variable (`t` in `for t from …`) is local to the solve and not defined
   afterwards. The solution is a **snapshot**: the values of every other name the equations read are captured
   when the `solve` runs, so reassigning `k` afterwards does not change `x(t)` (D46, D261).
   A root-finding `solve … for x from a to b` assigns the root to `x` as an ordinary variable.
10. **`fit`** assigns each fitted parameter (a name of the model that is neither a data column nor a defined
    variable) as an ordinary global with its unit; the data column names are local to the model (D193).
11. **Modules** (D101): a module is checked in its own scope whose parent is the built-in constants, so its
    functions never see the importing program's variables. `import m` binds only the name `m` (members are read
    as `m.f`); `import m as n` binds `n`; `from m import f` binds `f`. Defining a name that is also imported,
    importing a module whose name the program already uses, and names starting with `_` from outside the module
    are errors.
12. **`parallel for`** has the scoping of `for`; in addition its body's assignments to names defined before the
    loop are restricted so that iterations are independent: `s += e` on an outer number is a reduction, and
    `xs[i] = e` writes one element (the full restrictions are in docs/reference.md §6; §4 below). `sweep` is a
    `for` over the listed values.

```fermium
k = 2
f(x) = k x
print f(1)
k = 3
print f(1)
g(x) =
    k += 1
    return k x
print g(1), k
outer(x) =
    inner(y) = x y
    return inner(2)
print outer(3)
i = 10
for i from 1 to 3
    j = 0
print i
print a + 1 where a = 2
a = 5
F(t) = ∫ a u² du from 0 to t
print F(1)
r = 2 /s
solve y' = -r y with y(0) = 1 for t from 0 s to 1 s
r = 7 /s
print y(1 s)
solve z² = 2 for z from 0 to 2
print z
import mechanics
print mechanics.pendulum_period(1 m, 9.81 m/s²)
m_s = 2
fit y = m_s x + b to table(x = [1, 2, 3], y = [2, 4, 6.1])
print b
s = 0
parallel for n from 1 to 4
    s += n
print s
```

Each of these is rejected:

```fermium-error
f(x) =
    y = 2 x
    return y
print f(1)
print y
```

```fermium-error
x = 1
if x > 0
    z = 4
print z
```

```fermium-error
for i from 1 to 3
    j = i
print i
```

```fermium-error
f(x) = x + q
print f(1)
q = 2
```

```fermium-error
print a where a = 3
print a
```

```fermium-error
x = 1 m
x = 2 s
```

```fermium-error
f(x) =
    g(y) =
        return 2 y
    return g(x)
print f(1)
```

```fermium-error
solve y' = -y with y(0) = 1 for t from 0 s to 1 s
print t
```

```fermium-error
fit y = m x + b to table(x = [1, 2, 3], y = [2, 4, 6.1])
print x
```

```fermium-error
from mechanics import pendulum_period
pendulum_period(L) = L
```

### 2.3 Uncertain values: the linear model (D120, D276–D279, D300, D304)

**Values.** An uncertain value is a pair u = (v, c) of a nominal value v (a double with a dimension D) and a finite
map c from *sources* to contributions, c = {k ↦ c_k}, every c_k of dimension D. Its standard uncertainty is
σ(u) = √(Σ_k c_k²). A plain number is (v, {}).

**Sources.** Each *evaluation* of `a ± b` creates one fresh source k with c = {k ↦ b} (`a ± p%` gives
b = p/100·|a|); each element of a list written with `±` is its own source; `(a ± b) ± c` adds a second fresh
source to the first; `a ± b ± c` without brackets is an error. A `±` evaluated again (in a loop, a function
called twice) is a new measurement with a new source, except inside an integrand or an ODE's right side, where a
`±` is one source for the whole kernel (D276). Each `fit` creates sources for its parameters (their covariance).
The unit rule applies to `a ± b` as to a number: b must have a's dimension (units.md §7).

**Operations** are first-order (linear) with exact correlations. For a differentiable f of uncertain arguments
u₁ … uₙ, f(u₁, …, uₙ) = (f(v₁, …, vₙ), c) with

  c_k = Σᵢ ∂f/∂xᵢ(v₁, …, vₙ) · (cᵢ)_k        (a missing entry is 0),

that is, forward-mode differentiation with respect to the sources. Every arithmetic operator and every built-in
of §7 that accepts a number uses its analytic partial derivatives (central differences for Bessel and elliptic
functions). Consequences: `x - x` is `0 ± 0`, `x / x` is `1 ± 0`, `x x` has σ = 2|x|σ_x, while `x y` for two
independent measurements of the same size has √2 |x| σ_x. Vectors, matrices and lists of uncertain values
propagate entry by entry with the same rule (D279).

**Comparisons and control** use nominal values only: `<`, `==`, `min`, `max`, `if`, loop bounds. `≈` compares
nominal values.

**Kernels** (integrals and ODE solutions) are differentiated exactly (D276, D277): for I = ∫ₐᵇ f dx,
∂I/∂z_k = ∫ₐᵇ ∂f/∂z_k dx + f(b) ∂b/∂z_k − f(a) ∂a/∂z_k; for y' = f(t, y) the sensitivities S_k = ∂y/∂z_k solve
S_k' = J S_k + ∂f/∂z_k alongside y, with the same solver. After the linear result, each source is moved by ±1σ
and the kernel recomputed (D278, D300): with y₀ the linear value, c the predicted change, y± the recomputed
values, d₁ = (y₊ − y₋)/2 and d₂ = (y₊ + y₋ − 2y₀)/2, the linear result stands when |y₊ − y₀ − c|, |y₀ − y₋ − c|
and |d₂| are all at most 10 % of max(|d₁|, |c|) plus the kernel's own numerical noise. Otherwise the result is
computed by Monte Carlo (10 000 samples for an integral, 2 000 solves for an ODE, from the program's seeded
random numbers), **with a warning**, and linked back to the sources by linear regression, the nonlinear rest
becoming a new source; a Monte Carlo ODE solution reports the nominal solution's value (D304).

**Printing:** σ is rounded to 2 significant figures and the value to the same decimal place (§3.2).

```fermium
x = 2.0 ± 0.1 m
y = 2.0 ± 0.1 m
print x - x, x / x, x x, x y
assert uncertainty(x - x) == 0 m
assert abs(uncertainty(x x) - 0.4 m²) < 1e-12 m²
assert abs(uncertainty(x y) - √2 · 0.2 m²) < 1e-12 m²
z = 5.0 ± 3%
print z, sin(x / (1 m)), x < y, (1.0 ± 0.1) ± 0.2
a = 1.00 ± 0.01
I = ∫ u² du from 0 to a
assert uncertainty(I - a³/3) < 1e-9
k = 1.00 ± 0.01
solve q' = -k q with q(0) = 1 for t from 0 to 2
print q(1), q(1) - exp(-k)
```

```fermium-error
x = 1.0 ± 0.1 ± 0.2
```

```fermium-error
x = 2.0 ± 0.1 m
print 2.0 ± 0.1 s + x
```

## 3. Numeric semantics

### 3.1 Arithmetic

- Every number is an IEEE-754 binary64 double. Literals are rounded to nearest (grammar.md §1.7).
- `+ - * /` and `√` are the correctly rounded IEEE operations; the elementary functions (`sin`, `exp`, `ln`, …)
  come from the platform's libm (results may differ in the last bit between platforms;
  conformance/libm_sensitive.txt lists the affected cases).
- Division by zero follows IEEE: `1/0` is `∞`, `0/0` is `NaN`, and the program continues. Overflow gives `∞`,
  underflow gives a subnormal or 0. `√` of a negative number known at compile time is an error; at run time it
  is `NaN`. These rules are the current behaviour, not a promise: see TODO.
- Floating-point sums may be vectorised in order (`force-ordered-reductions`, D311), so a sum's rounding does not
  depend on the optimiser.
- `a == b` on numbers is exact; `a ≈ b [within t]` is Julia's `isapprox`: without `within`, the allowed difference
  is 10⁻⁶ × the larger magnitude, so `x ≈ 0` is an error that asks for an absolute tolerance (D260).

```fermium
x = 1/3
print x * 3 - 1
print 0.1 + 0.2 == 0.3, 0.1 + 0.2 ≈ 0.3
print 1/0
```

### 3.2 Printing (significant figures)

`print` shows each value in its display unit with a number of significant figures chosen at compile time
(D11, D242). **Printing never changes a value**: every computation uses the full double.

1. A literal with a decimal point carries its significant figures (`1.20` → 3); a literal without one is exact.
2. A computed result shows max(2, the fewest significant figures among its inputs).
3. When every input is exact (integers, π, constants such as c), the result shows **3** significant figures
   with trailing zeros kept (`1/2` → `0.500`, `2π` → `6.28`).
4. A whole number below 10⁷ prints exactly (`4*5` → `20`); a value within 10⁻¹³ (relative) of a whole number
   counts as whole. A value that came straight from a literal prints as written (`12345678`, `3.14159`).
5. When rounding leaves two or more non-significant zeros before the decimal point, the value is shown with a
   power of ten: `1000000/3` → `3.33×10⁵`.
6. Ties round half to even, on the exact binary value (so `0.135` shows `0.14`).
7. `print x to N digits` shows N significant figures.
8. Complex numbers print as `3 + 4i` with the rules applied to each part (D94); uncertain values print with σ
   rounded to 2 significant figures and the value to the same decimal place (D121): `2.400 ± 0.020 m`.
9. `NaN` and `∞` print as `NaN` and `∞`.

```fermium
print 1/3, 2π, 1.20 * 3, 1000000/3, 12345678, 3.14159
L = 1.20 ± 0.01 m
print L * 2
```

The exact number formatter is `fermium-units/src/numfmt.rs`; the conformance runner compares output exactly
except that a number may differ by one unit in its last printed digit when it has the same shape (D264).

## 4. Evaluation order

- Statements run top to bottom; within an expression, operands are evaluated left to right, and a function's
  arguments before the call. Fermium expressions have no side effects of their own (interop calls aside), so
  the order is observable mainly through which run-time error is reported first.
- `e where a = …, b = …` evaluates the bindings in order, then `e`.
- `and`/`or` evaluate their right operand only when needed; `if c then a else b` evaluates one branch.
- `parallel for` runs iterations at the same time, in no fixed order; its body may not `print` (the restrictions
  are listed in docs/reference.md §6).
- `sweep` runs its body once per value like `for`, and collects the plots it makes into one figure (D299).

## 5. Errors and warnings

**Compile-time errors** (lexer, parser, checker) stop compilation at the first error. **Run-time errors** (an
index out of range, asking an ODE solution for a time outside its range, a failed numerical method) stop the
program at that point; output already printed stays. Both exit with status 1.

Every error is one line naming the file and line, then the source line, then a caret under the offending span
(compile-time errors) and usually a hint:

```text
prog.fm, line 3: can't add length [m] to time [s]
    y = x + t
        ^^^^^
  hint: both sides of + and - must have the same units
```

Warnings have the same shape, start with `warning: line N:`, go to standard error, and do not stop the program.
The message texts, lines and hints are part of the language's observable behaviour and are fixed by conformance/
(D264).

```fermium-error
x = 2 m
print x in s
```

## 6. Calculus

### 6.1 Derivatives

`f'`, `f''`, `d/dx f`, `d²x/dt²`, `∂/∂x f`, `∂f/∂x`, `dx/dt` (when `x` is a function or solution), `∇f`, `∇·F`, `∇×F`
and `∇²f` are **exact**: the result is a new function whose dimension is dim(f)/dim(x)ⁿ.

- For one-line functions and formulas, the derivative is **symbolic** (sum, product, quotient and chain rules and
  every standard function), simplified, and printable (`print d/dt x` shows the formula and its units, D298).
- For functions written over several lines (assignments, `if`, loops) it is computed by **forward-mode automatic
  differentiation** as a source transformation: exact to rounding, not printable as a formula (D295).
- Functions defined by an integral are differentiated under the integral sign (Leibniz rule), with boundary
  terms when a limit depends on the variable (D36).
- There is no numerical finite differencing in the language semantics.

### 6.2 Integrals and sums

- `∫ f dx from a to b` is a **numerical** definite integral: adaptive Gauss–Kronrod (G7/K15) to a relative
  tolerance of 10⁻¹⁰, with a scale search for infinite limits and splitting at integrable singularities; an
  integral that doesn't converge is a run-time error (docs/reference.md §9).
- `∫ f dx` without limits is an indefinite integral, which must be found symbolically (TODO: specify the class).
- `Σ(e for k from a to b [step s])` is a finite sum evaluated in order; it is differentiated term by term.

### 6.3 solve

- **Differential equations.** A `solve` whose equations contain primes (or `d/dt`) of names not yet defined is an
  initial-value problem. Its unknowns are those names; every unknown and every derivative below the highest
  needs an initial condition in `with`, which also fixes the unknowns' units, and both sides of each equation are
  dimension-checked. Each equation is solved symbolically for its highest derivative (which must appear
  linearly; several highest derivatives form a mass-matrix system, D47). Afterwards each unknown is an ODE
  solution, callable at any time in the range. Methods (D17): without `step`, Dormand–Prince 5(4) with
  rtol = 10⁻⁹ and a scale-free error norm; `step h` selects fixed-step RK4 (checked for accuracy by step doubling);
  `using radau|bdf` for stiff problems. `when` clauses are events that change the state at a crossing (D297);
  `until` stops at a condition.
- **Equations.** A `solve lhs = rhs for x from a to b` with no derivatives finds the **first** x in [a, b] where the
  two sides are equal: 200-point scan, then Illinois regula falsi to full precision; a pole crossing is reported,
  not returned (D32). The answer has the range's units and is assigned to x.
- Boundary-value and eigenvalue forms (`lowest N`), PDEs (a second range) and Monte Carlo propagation are
  described in docs/reference.md §20–§21 (TODO here).

## 7. Built-in functions (static semantics)

This table is normative for the *static* semantics of every built-in function: how many arguments it takes, what
kind and dimension each must have, and the kind and dimension of the result. It is written from the checker
(`rust/crates/fermium-check/src/builtin.rs`, and `vecmat.rs`, `cplx.rs`, `clist.rs`, `rng.rs`, `uncertain.rs`,
`arrays.rs` for the groups they handle). The test `spec_builtins_table` (fermium-check) checks that the first
column names exactly the checker's list `builtins::BUILTINS`, and §7.1 checks each row's result kind and
dimension against the checker. A program-defined function or variable of the same
name hides the built-in.

Notation: *x* is a number, *xs* a list, *v* a vector, *M* a matrix, *z* a complex number; [x] is the dimension of x
and **1** is dimensionless. "Same class" means a number gives a number and a list gives a list (element by element).
Every function that accepts a number also accepts an uncertain number (first-order propagation, D120) unless
noted. A function of dimensionless arguments applied to a quantity with units is a compile-time error with a hint.

| Name(s) | Arguments | Result |
|---|---|---|
| `sin` `cos` `tan` `cot` `sec` `csc` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` `exp` `ln` `log` `log10` `log2` `erf` `erfc` `gamma` `lgamma` `expm1` `log1p` | one number or list, [x] = **1** (angles in rad or ° are **1**); `exp ln log sin cos tan sinh cosh tanh` also take a complex (giving a complex) | same class, **1**. `log` is the natural logarithm. `sin(10)` with a whole literal ≥ 10 warns (radians) |
| `sqrt` `cbrt` | one number or list, any dimension (exponents must stay rational, units.md §1.1); `sqrt` also takes a complex | same class, [x]^½ resp. [x]^⅓ |
| `abs` | one number or list (any dimension); a vector or matrix | same kind and dimension as x (entry by entry for a vector; the norm is `\|v\|` or `norm`) |
| `floor` `ceil` `round` | one number or list, [x] = **1** (otherwise "would depend on which unit you mean") | same class, **1**, printed exactly |
| `sign` | a number: any dimension; a vector | a number: **1**; a vector: the unit vector v/\|v\|, **1** |
| `isnan` | one number | boolean |
| `besselj` `bessely` `besseli` `besselk` | (n, x): both **1**; a constant n must be a whole number | number, **1** |
| `ellipk` `ellipe` | (m), m = k², **1** | number, **1** |
| `min` `max` | one list: [xs]; or ≥ 2 numbers of one dimension; or numbers and lists of one dimension mixed (element by element, D162) | number [x], or a list for the mixed form |
| `atan2` | (y, x), [y] = [x] | number, **1** |
| `hypot` `mod` | (a, b), [a] = [b] | number, [a] |
| `clamp` | (x, lo, hi), all one dimension | number, [x] |
| `factorial` | one number, **1** | number, **1** |
| `len` | a list (numbers, text or vectors) | number, **1**, exact |
| `sum` `mean` | a list: [xs] (a list of vectors or matrices: that vector or matrix); an array (below) | number [xs]; `sum` of absolute temperatures is an error |
| `std` `first` `last` | a list | number, [xs] (`std` of °C is shown in K) |
| `cumsum` `diff` `reverse` `sort` `values` | a list | list, [xs] (`values` is a copy) |
| `times` | an ODE solution (or its value list) | list of the independent variable's dimension |
| `linspace` | (a, b, n): [a] = [b], n **1** | list, [a] |
| `range` | (a, b) or (a, b, step), all one dimension | list, [a] |
| `zeros` `ones` | (n), n **1**; `zeros(r, c)` is an r×c matrix | list; `zeros`: a dimension inferred from use, `ones`: **1** |
| `interp` | (x, xs, ys), [x] = [xs] | number, [ys] |
| `trapz` | (ys, xs): two lists | number, [ys]·[xs] |
| `dot` | two lists: number [a]·[b]; two vectors: the dot product | number, [a]·[b] |
| `norm` `unit` `hat` | a vector whose components share a dimension | `norm`: number [v]; `unit`, `hat`: vector, **1** |
| `cross` | two 3-vectors | vector, [a]·[b] |
| `vec` | 2 to 16 numbers | a vector (like `<…>`) |
| `angle` | two 2-vectors or two 3-vectors of one shared dimension | number, **1** (rad) |
| `transpose` | a matrix r×c | c×r matrix, [M] |
| `det` | a square matrix n×n | number, [M]ⁿ |
| `inverse` | a square matrix | matrix, [M]⁻¹ |
| `trace` | a square matrix | number, [M] |
| `identity` | (n): a constant whole number 2…16 | n×n matrix, **1** |
| `solve_linear` | (M, b): an n×n matrix and an n-vector | vector, [b]/[M] |
| `eigenvalues` | (K) or (K, M): square matrices of one size, 2×2 to 16×16 | vector, [K] (or [K]/[M]) |
| `eigenvectors` | the same | matrix, **1** (columns are the eigenvectors) |
| `row` `column` | (M, k): a matrix and a whole number | vector, [M] |
| `re` `im` | a complex (or a real) number | number, [z] |
| `conj` | a complex number | complex, [z] |
| `arg` | a complex number | number, **1** |
| `complex` | (a, b), [a] = [b]; two lists: a complex list | complex, [a] |
| `polar` | (r, θ), θ **1** | complex, [r] |
| `cis` | (θ), **1** | complex, **1** |
| `fft` | a list | list of complex numbers, [xs] |
| `ifft` | a complex list; or (re, im) with [re] = [im] (deprecated) | a complex list: complex list, [X] (take `re(…)` for the real signal); (re, im): list, [re] |
| `fft_re` `fft_im` | a list (deprecated: write `re(fft(xs))`) | list, [xs] |
| `amplitude_spectrum` | a list | list, [xs] |
| `power_spectrum` | (xs, dt) | list, [xs]²·[dt] (shown in V²/Hz for volts sampled in s) |
| `frequencies` | (xs, dt) or (n, dt), n **1** | list, 1/[dt] (shown in Hz) |
| `argmax` `argmin` | a list | number, **1** (a 1-based index) |
| `value` `uncertainty` | an uncertain number or list | same class, [x] |
| `rel` | an uncertain number or list | same class, **1** |
| `rand` | () or (a, b) with [a] = [b] | number, **1** resp. [a] |
| `randn` | () or (μ, σ) with [μ] = [σ] | number, **1** resp. [μ] |
| `sample` | (expression, n), n **1**: the expression is evaluated n times | list, [expression] |
| `seed` | (n): a statement on its own line, not a value | — |
| `clock` | () | number, time (seconds; in natural units converted, D60) |
| `str` | one number or text | text (the number as `print` shows it) |
| `to` | (x, unit name): [x] = [unit] | x, displayed in that unit (D216) |
| `push` `append` | (list, value): statements on their own line, not values | — |
| `fill` | (value, n1, n2, …) | an N-dimensional array (D283), [value]; with one size a list |
| `size` | an array | list of its sizes, **1** (`size(A, k)`: number) |
| `copy` | an array | array, [A] |

```fermium
xs = [1, 2, 3, 4] m
ts = [0, 1, 2, 3] s
M = [[2, 1], [1, 3]] N/m
v = <3, 4> m
z = complex(1 V, 2 V)
q = 2.0 ± 0.1 m
print sin(0.5), ln(2), log10(100), erf(0.5), gamma(4), expm1(1e-3), √(4 m²), cbrt(8 m³)
print abs(-2 m), floor(2.5), besselj(1, 2.0), ellipk(0.5)
print len(xs), sum(xs), mean(xs), std(xs), first(xs), last(xs), cumsum(xs), diff(xs), reverse(xs), sort(xs)
print value(q), uncertainty(q), rel(q)
print fft(xs), amplitude_spectrum(xs), power_spectrum(xs, 1 s), frequencies(xs, 1 s), argmax(xs), argmin(xs)
print re(z), im(z), conj(z), arg(z), polar(2 V, 0.5), cis(0.5)
print min(1 m, 2 m), max(xs), atan2(1 m, 2 m), hypot(3 m, 4 m), sign(-2 m), mod(7 m, 3 m)
print linspace(0 m, 1 m, 3), zeros(2), ones(2), range(1 s, 3 s), values(xs), dot(xs, xs), factorial(4)
print clamp(5 m, 0 m, 2 m), isnan(1.0), interp(1.5 m, xs, ts), trapz(ts, xs)
print norm(v), unit(v), hat(v), cross(<1, 0, 0> m, <0, 1, 0> N), vec(1 m, 2 m)
print transpose(M), det(M), inverse(M), identity(2), solve_linear(M, <1, 2> N), eigenvalues(M), eigenvectors(M)
print trace(M), angle(v, <1, 0> m), row(M, 1), column(M, 2), str(2 m), to(1000 m, km), sign(v), abs(v)
A = fill(1 m, 2, 2)
print size(A), copy(A), sum(A)
seed(1)
print rand() < 1, randn(0 m, 1 m) < 10 m, len(sample(randn(0 m, 1 m), 5)), clock() >= 0 s
```

Rejected (one line each):

```fermium-error
print sin(2 m)
```

```fermium-error
print floor(2.5 m)
```

```fermium-error
print atan2(1 m, 2 s)
```

```fermium-error
print linspace(0 m, 1 s, 3)
```

```fermium-error
print det([[1, 2], [3, 4], [5, 6]])
```

### 7.1 The table, checked row by row

The block below states, for each row of the table, the kind and dimension of the result of a call with
arguments of the documented dimensions. It is normative and machine-checked: the test `spec_builtins_types`
(fermium-check) type-checks each line `CALL ⇒ KIND [UNIT]` with the checker (the lines without `⇒` are a prelude
defining the arguments) and compares the result's kind (`number`, `boolean`, `text`, `list`, `complex`,
`complex list`, `vector`, `matrix`, `array`) and dimension (the dimension of `UNIT`; `[1]` is **1**) with the
line. Several calls separated by `;` share one expectation. Every name in the table must appear in some line,
except the statements `seed`, `push` and `append`, which have no value (they are checked by the example above).

```fermium-types
xs = [1, 2, 3, 4] m
ts = [0, 1, 2, 3] s
M = [[2, 1], [1, 3]] N/m
v = <3, 4> m
z = complex(1 V, 2 V)
q = 2.0 ± 0.1 m
A = fill(1 m, 2, 2)
solve y' = -y/(1 s) with y(0) = 1 m for t from 0 s to 1 s
sin(0.5); cos(0.5); tan(0.5); cot(0.5); sec(0.5); csc(0.5); asin(0.5); acos(0.5); atan(0.5) ⇒ number [1]
sinh(0.5); cosh(0.5); tanh(0.5); asinh(0.5); acosh(1.5); atanh(0.5) ⇒ number [1]
exp(0.5); ln(2); log(2); log10(100); log2(8); erf(0.5); erfc(0.5) ⇒ number [1]
gamma(4); lgamma(4); expm1(1e-3); log1p(1e-3); sin(30°); cos(q/(1 m)) ⇒ number [1]
sin([0.1, 0.2]); exp([1, 2]) ⇒ list [1]
exp(z/(1 V)); sin(z/(1 V)) ⇒ complex [1]
sqrt(4 m²); cbrt(8 m³) ⇒ number [m]
sqrt(ts*ts) ⇒ list [s]
abs(-2 m) ⇒ number [m]
abs(xs) ⇒ list [m]
abs(v) ⇒ vector [m]
floor(2.5); ceil(2.5); round(2.5) ⇒ number [1]
sign(-2 m) ⇒ number [1]
sign(v) ⇒ vector [1]
isnan(1.0) ⇒ boolean [1]
besselj(1, 2.0); bessely(1, 2.0); besseli(1, 2.0); besselk(1, 2.0); ellipk(0.5); ellipe(0.5) ⇒ number [1]
min(xs); max(xs); min(1 m, 2 m); max(1 m, 2 m, 3 m) ⇒ number [m]
max(xs, 2 m) ⇒ list [m]
atan2(1 m, 2 m) ⇒ number [1]
hypot(3 m, 4 m); mod(7 m, 3 m); clamp(5 m, 0 m, 2 m) ⇒ number [m]
factorial(4); len(xs) ⇒ number [1]
sum(xs); mean(xs); std(xs); first(xs); last(xs); sum(A) ⇒ number [m]
cumsum(xs); diff(xs); reverse(xs); sort(xs); values(xs); values(y) ⇒ list [m]
times(y) ⇒ list [s]
linspace(0 m, 1 m, 3); range(0 m, 1 m, 0.5 m) ⇒ list [m]
range(1 s, 3 s) ⇒ list [s]
zeros(2); ones(2) ⇒ list [1]
zeros(2, 3) ⇒ matrix [1]
interp(1.5 m, xs, ts) ⇒ number [s]
trapz(ts, xs); dot(xs, ts) ⇒ number [m s]
dot(v, v) ⇒ number [m²]
norm(v) ⇒ number [m]
unit(v); hat(v) ⇒ vector [1]
cross(<1, 0, 0> m, <0, 1, 0> N) ⇒ vector [N m]
vec(1 m, 2 m) ⇒ vector [m]
angle(v, <1, 0> m) ⇒ number [1]
transpose(M) ⇒ matrix [N/m]
det(M) ⇒ number [N²/m²]
inverse(M) ⇒ matrix [m/N]
trace(M) ⇒ number [N/m]
identity(2); eigenvectors(M) ⇒ matrix [1]
solve_linear(M, <1, 2> N) ⇒ vector [m]
eigenvalues(M); row(M, 1); column(M, 2) ⇒ vector [N/m]
eigenvalues(M, [[1, 0], [0, 1]] kg) ⇒ vector [1/s²]
re(z); im(z); re(2 V) ⇒ number [V]
arg(z) ⇒ number [1]
conj(z); complex(1 V, 2 V); polar(2 V, 0.5); sqrt(z*z) ⇒ complex [V]
cis(0.5) ⇒ complex [1]
complex(xs, xs); fft(xs); ifft(fft(xs)) ⇒ complex list [m]
ifft(xs, xs); fft_re(xs); fft_im(xs); amplitude_spectrum(xs) ⇒ list [m]
power_spectrum(xs, 1 s) ⇒ list [m² s]
frequencies(xs, 1 s); frequencies(4, 1 s) ⇒ list [Hz]
argmax(xs); argmin(xs) ⇒ number [1]
value(q); uncertainty(q) ⇒ number [m]
rel(q) ⇒ number [1]
rand(); randn() ⇒ number [1]
rand(1 m, 2 m); randn(0 m, 1 m) ⇒ number [m]
sample(randn(0 m, 1 m), 5) ⇒ list [m]
clock() ⇒ number [s]
str(2 m) ⇒ text [1]
to(1000 m, km) ⇒ number [m]
fill(1 m, 3) ⇒ list [m]
fill(1 m, 2, 2, 2); copy(A) ⇒ array [m]
size(A) ⇒ list [1]
size(A, 1) ⇒ number [1]
```

## 8. TODO for this chapter

- Specify the run-time behaviour of domain errors (`√` and `ln` of negative numbers, `asin(2)`) uniformly; today a
  compile-time constant is an error while a run-time value gives `NaN`.
- ~~The static semantics of every built-in function~~ (done in 0.2: §7, a table written from the checker whose
  names are tested against `builtins::BUILTINS`; since 0.3 every row's kind and dimension is machine-checked by
  §7.1's ```fermium-types block, test `spec_builtins_types`; this found and fixed the `ifft` row, whose result
  is a complex list, and the missing complex case of `sqrt`). Only the statements `seed`, `push` and `append`
  are not checked this way (they have no value; the example in §7 runs them).
- ~~Scoping rules~~ (done in 0.2: §2.2). Still open: natural-units regions, the scope of names bound by
  `analyze`/`propagate`, and the interop blocks' names.
- ~~Uncertainty propagation as a formal model~~ (done in 0.2: §2.3, the linear model and the kernels' ±1σ test).
  Still open: `propagate montecarlo` and `analyze` as formal models.
- The interop type mappings (`use python`, `import c/cpp/fortran`).
