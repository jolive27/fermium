# Semantics

Draft 0.1, **partial**: a high-level account of what programs mean. Where a detail is not specified here,
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

## 7. TODO for this chapter

- Specify the run-time behaviour of domain errors (`√` and `ln` of negative numbers, `asin(2)`) uniformly; today a
  compile-time constant is an error while a run-time value gives `NaN`.
- The static semantics of every built-in function (generated from the checker's tables).
- ~~Scoping rules~~ (done in 0.2: §2.2). Still open: natural-units regions, the scope of names bound by
  `analyze`/`propagate`, and the interop blocks' names.
- Uncertainty propagation (first-order, exact correlations; Monte Carlo) as a formal model.
- The interop type mappings (`use python`, `import c/cpp/fortran`).
