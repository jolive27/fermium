# Fermium Language Reference

Fermium is a programming language for physics. Numbers carry units, and the compiler checks those units **before your program runs**. Calculus (derivatives, integrals, differential equations) is part of the language. Programs are compiled to native code with LLVM.

Every program example on this page is tested: `tests/test_docs.py` runs each block marked with `fermium`. The other blocks (command lines, the error display in §16, the `load`/`fit`/`plot` sketch in §11 and the grammar) are not run.

## Contents
1. [Running programs](#1-running-programs)
2. [Numbers and units](#2-numbers-and-units)
3. [Variables and formulas](#3-variables-and-formulas)
4. [Printing](#4-printing)
5. [Functions](#5-functions)
6. [Conditions and loops](#6-conditions-and-loops)
7. [Lists and vectors](#7-lists-and-vectors)
8. [Derivatives](#8-derivatives)
9. [Integrals](#9-integrals)
10. [Differential equations: solve](#10-differential-equations-solve)
11. [Data: load, fit, plot](#11-data-load-fit-plot) (and [dimensional analysis](#dimensional-analysis-analyze)); then [Modules: import and the standard library](#modules)
12. [Symbols and their ASCII spellings](#12-symbols-and-ascii-spellings)
13. [Built-in functions](#13-built-in-functions)
14. [Constants](#14-constants)
15. [Units](#15-units) (and [natural units](#natural-units-units-natural-units-nuclear-units-astro))
16. [Errors](#16-errors)
17. [Tools](#17-tools)
18. [Grammar summary](#18-grammar-summary)
19. [Known limitations](#19-known-limitations)
20. [Numerics: random numbers, Fourier transforms, eigenstates, PDEs](#20-numerics-random-numbers-fourier-transforms-eigenstates-pdes)

---

## 1. Running programs

```
fermium run pendulum.fm      # run a program (or just: fermium pendulum.fm)
fermium                      # interactive prompt (REPL)
fermium check pendulum.fm    # check units without running
fermium fmt pendulum.fm --pretty   # ASCII -> symbols;  --ascii for the reverse; -w rewrites the file
fermium doctor               # check the installation
fermium build pendulum.fm    # make a standalone executable ./pendulum (needs a C compiler)
```

A program is a text file ending in `.fm`. Comments start with `#`.

## 2. Numbers and units

A unit goes **right after a number**:

```fermium
L = 1.20 m
v = 3.0 m/s
k = 50 N/m
G_N = 6.674e-11 N m²/kg²
c_light = 3.00×10⁸ m/s
```

Rules (see DECISIONS.md D7):
- A unit name immediately after a number is a unit: `3 m` is 3 metres.
- Units in brackets are always units: `3 [m/s]`, and in a function parameter, `f(x [m]) = ...` (see §5). A variable can't be declared with a bracket: `x [m] = 3` is an error; write `x = 3 m`.
- Anywhere else, a name is a variable. `m v` is m times v.
- A unit expression continues with `/` (`m/s`), with a space (`N m`), or with `·`. Exponents are written `m²` or `m^2`, and `s⁻¹` or `s^-1`.
- **Dividing by your own variable:** a `/` with a space before it, followed by one of *your* variables, divides by that variable. With `g = 9.81 m/s²`, `20 m/s / g` is 2.04 s; `20 m/s/g` (no space) is 20 m/s per gram. Likewise `2.898e-3 m K / T` divides by a temperature `T`, not by tesla. When in doubt, use parentheses: `(20 m/s) / g`.
- If a single unit name right after a number is also one of your variables, there are two cases (DECISIONS D7):
  - **Multiplied or divided by something else** (`2 g h`, `0.5 m v²`, `2 g * h`), it's an **error** that asks which you mean: write `2*g` for your variable or `2 [g]` for the unit.
  - **On its own** (`x(0) = 0.1 m`, `from 0 m to 0.2 m`), it's the unit, with a warning. If that line or a later one then fails its unit check, the error adds a note naming the cause: `'2 L' here is 2 L, volume [m³] (a unit right after a number); for 2 × your variable L write 2*L`.
  - Compound units (`9.81 m/s²`) and bracketed units (`2 [g]`) are always units.
- **Per minute:** right after a number, `/ min` is the minute even with spaces, so `15.3 / min / g` is 15.3 per minute per gram. `min(a, b)` is still the function.

Numbers: `3`, `3.0`, `1.5e-3`, `6.67×10⁻¹¹`, `½`. Numbers written with a decimal point carry **significant figures**, which Fermium uses when printing (`1.20` has 3).

Every value is stored in SI base units. **Units cost nothing at run time:** they are checked by the compiler and then erased.

## 3. Variables and formulas

```fermium
m = 2.0 kg
v = 3.0 m/s
E = ½ m v²
p = m v
print E, p
```

- **Implicit multiplication:** `2x`, `k x` and `4π² L` all multiply. `LT` is a single name. Write `L T` for L times T.
- **Precedence**, from strongest to weakest:
  1. `^` and superscripts
  2. implicit multiplication
  3. unary `-`
  4. `*` and `/`
  5. `+` and `-`
  6. comparisons
  7. `not`, `and`, `or`

  So `h c / λ k_B T` means (h c)/(λ k_B T). Also, `1/2 x` means 1/(2x), and Fermium warns you. Write `½ m v²` or `(1/2) m v²` for one half. Careful: in `1/2 m`, the `m` right after the number is the unit (§2), so it is 0.5 per metre (DECISIONS D8).
- **`a/b (c)` is a/(b c).** Fermium warns when the way a division is written suggests (a/b)·c: a tight `/` followed by factors with spaces between them (`c²/g (√(1 + x) − 1)`, `n R/(γ − 1) (T3 − T2)`, `μ₀ I/(4π) dl`), a bracket next to another factor after a space (`a / (b) c`), or dividing by the unknown of a `solve` (`ψ'' = -2 m E / ħ² ψ`). Write `(c²/g) (…)` or `c²/g * (…)` for the first meaning and `c²/(g (…))` for the second. `h c / λ k_B T`, `a/(b c)`, `x/2π` and `G M m / r²` are not warned about (DECISIONS D34).
- **A variable keeps its units:** assigning a time to a variable that held a length is an error.
- **Update in place:** `x += 1 m`, `x -= ...`, `x *= 2`, `x /= 2`.
- **`where`:** gives names to values used in just one line:

```fermium
E = ½ m v² where m = 2 kg, v = 3 m/s
print E
```

- **Exponents:** an exponent must be a fixed number when the base has units. `x^2` and `x^(1/3)` are fine. `x^n` requires x to be a plain number.
- **`e` is the elementary charge.** `e²` (or `e^2`) is the charge squared, in C², as in e²/(4πε₀r). For the exponential function write `exp(x)`; `e^x` with a variable exponent is an error that says so.

## 4. Printing

```fermium
g = 9.81 m/s²
print g
print g in ft/s²
print "g is", g
print to(g, km/hr^2)
```

- `print a, b, c` prints the values separated by spaces.
- `print x to 6 digits` shows 6 significant figures instead of the automatic choice.
- Text can be stored in a variable and printed: `name = "Mars"`, `print "planet:", name`. Text can't be used in arithmetic.
- `x in unit` shows a value in another unit. The units must measure the same kind of quantity.
- Numbers are printed with sensible significant figures: the fewest significant figures of the inputs, but at least 2.
- Units are shown in the unit you wrote. When there isn't one, Fermium picks a standard SI unit (N, J, W, Pa, ...).

## 5. Functions

One line:

```fermium
F(x) = 50 N/m * x
KE(m, v) = ½ m v²
print F(0.1 m), KE(2 kg, 3 m/s)
```

Several lines. The last line is the result, or you can use `return`:

```fermium
speed(h) =
    g = 9.81 m/s²
    return √(2*g*h)    # note: `2 g` would mean 2 grams!
print speed(10 m)
```

- **You don't write types or units:** each call is checked with the units of its arguments.
- **Optional unit annotations:** `f(x [m]) = ...` requires x to be a length.
- **Calling with a list** applies the function to each element: `F(xs)`.
- **Functions can call each other and themselves.**

**Passing a function to a function.** A parameter can be a function: pass the function's name and
call it inside, like `V(x)`. A derivative (`g'`, `d/dt (3 t²)`, `∇φ`) or a one-argument built-in
(`sin`, `exp`, `sqrt`, …) can be passed too.

```fermium
simpson(f, a, b, n) =
    h = (b - a) / n
    s = f(a) + f(b)
    for i from 1 to n - 1
        s += (if mod(i, 2) == 1 then 4 else 2) * f(a + i h)
    s h / 3
g(t) = 9.81 m/s² * t
print simpson(g, 0 s, 2 s, 10), simpson(sin, 0, π, 100)

force(V, x) = -V'(x)
spring(x) = ½ (4 N/m) x²
print force(spring, 3 m)
```

- Inside the function, the parameter can be called, differentiated (`V'(x)`, `d/dx V`), used in
  `∫`, `solve` and `plot`, and passed on to another function.
- It is resolved when the program is compiled: every function you pass makes its own copy, so units
  are checked for each one (`energy(V2, 1 m)` can fail while `energy(V1, 1 m)` works) and it costs
  nothing at run time (DECISIONS D43).
- Using a function parameter as a number is an error: `V is a function here; call it like V(x)`.
  Passing a number where the body uses `V'` or `V(a, b)` is an error; where the body only writes
  `V(x)`, a number means `V × x` as usual, with a warning.
- Not yet: `x -> x²` (anonymous functions), passing an ODE solution, printing a function parameter.

## 6. Conditions and loops

```fermium
x = 3 m
if x > 2 m
    print "far"
else if x > 1 m
    print "middle"
else
    print "near"

total = 0 m
for i from 1 to 10
    total += i * 1 cm
print total

for t from 0 s to 1 s step 0.25 s
    print t

n = 0
while n < 3
    n += 1
print n

y = if x > 0 m then x else -x
print 1 m < x < 5 m                   # true: a chained comparison
```

- **Ranges include both ends:** `for i from 1 to 10` runs 1, 2, …, 10. With units, a `step` is required.
- **Loop control:** `break` and `continue` work in loops.
- **Loops without an end:** `for i from 1 to inf` (or `∞`) counts 1, 2, 3, … until a `break`:

```fermium
total = 0
terms = 0
for n from 1 to inf
    total += 1/n^2
    terms = n
    if 1/n^2 < 1e-6
        break
print terms, total
```

- **A variable must have a value on every path.** Using a variable after an `if` or a loop that is the only place it was set is a compile error, because the `if` may be false and the loop may not run at all: `y might not have a value here: it is only set inside the if on line 1`. The same goes for a loop variable read after its loop (`for i from 1 to n` … `print i`), even a `for n from 1 to inf` loop. Give the variable a value before the `if` or loop, or copy the loop variable into another variable inside the loop, as `terms` above.
- **Conditions:** comparisons are `==`, `!=` (`≠`), `<`, `>`, `<=` (`≤`), `>=` (`≥`) and `~=` (`≈`, "equal to within 10⁻⁶ relative"). Combine them with `and`, `or` and `not`.
- **Chained comparisons:** `a < x < b` means `a < x and x < b`, and `x` is computed only once; any number of `<`, `<=`, `>`, `>=` can be chained (`E1 < E2 <= E3 < E4`), or only `==`. Mixing `==` or `!=` with `<` is an error.
- **An if-expression over several lines:** continue it on indented lines that start with `else`, as a piecewise function is written on paper:

```fermium
g = 9.8 m/s^2
t1 = 2 s
u(t) = if t < t1 then g t
       else if t < 3 t1 then g t1
       else g t1 - g (t - 3 t1)
print u(1 s), u(3 s), u(7 s)
```

- **`assert condition, "message"`** stops the program if the condition is false.

## 7. Lists and vectors

```fermium
xs = [1 m, 2 m, 3 m]
print xs[1], xs[end], len(xs)
ys = 2 xs + 1 m
print sum(ys), mean(ys), max(ys)
zs = []
push(zs, 5 s)
push(zs, 7 s)
print zs
ts = linspace(0 s, 1 s, 5)
print ts
```

- **Elements share units:** all elements of a list have the same units.
- **Indexing starts at 1:** `xs[1]` is the first element and `xs[end]` the last. An index out of range stops the program with a clear message.
- **Arithmetic works element by element:** `xs + ys`, `2 xs` and `xs^2`. Functions such as `sin(xs)` work on each element.
- **Growing a list:** `push(xs, value)` (or `append`) adds an element to the end.
- **Size:** a list holds at most 10⁹ numbers. Asking for more (`zeros(2e9)`, or a range that long) stops the program with `not enough memory for a list of 2×10⁹ numbers (the most is 10⁹)`.
- **Sorting:** `sort(xs)` sorts from smallest to largest and puts `NaN` values last.
- **Setting an element:** `xs[i] = value`.
- **Looping:** `for x in xs`.

### Vectors

```fermium
v = <3, 4> m/s
print v, |v|, v.x
a = <1, 0, 0> m
b = <0, 2, 0> m
print a · b, a × b, unit(v)
print 2 v + <1, 1> m/s
```

- **Making a vector:** `<3, 4> m/s` or `<1 m, 2 m, 3 m>` (2, 3 or 4 components). `vec(3, 4)` is the same as `<3, 4>`.
- **A number times a vector** can be written side by side: `R <cos(φ), sin(φ), 0>`, `v0 <cos(θ), sin(θ)>`. This needs a space before `<` and none after it, and the `>` right after the last component; otherwise `<` is less-than (`a < b`, `x <y`). `R * <…>` always works.
- **Operations:** `+`, `-`, multiplying or dividing by a number, `|v|` or `norm(v)` for the length, `unit(v)` for the unit vector, `a · b` (or `dot(a, b)`) for the dot product, `a × b` (or `cross(a, b)`) for the cross product (a number in 2-D; not defined in 4-D).
- **Components:** `v.x`, `v.y`, `v.z`, or `v[1]`, `v[2]`, `v[3]`, `v[4]`. The index can be any whole number known only when the program runs, like a loop variable: `for i from 1 to 3` … `v[i]`; an index out of range stops the program (`index 4 is out of range: valid indexes here are 1 to 3`). A vector with a different unit on each component needs a fixed index.
- **More functions:** `angle(a, b)` is the angle between two 2- or 3-vectors, computed as atan2(\|a × b\|, a · b) (accurate near 0 and π; the vectors may have different units). `abs(v)` takes the absolute value of each component (of a matrix too).
- **A unit after a vector:** `<3, 4> m/s`, `<0, 0> /s` and `<1, 2> 1/s` all work, as after a number.
- **Units** are checked as for numbers: adding a velocity vector to an acceleration vector is an error.
- **In `solve`:** unknowns can be vectors, in 2-D or 3-D (see §10 and `examples/26_orbit_3d.fm`).
- **Not yet:** lists of vectors (push the components into separate lists instead).

**A different unit on each component.** A state vector such as position and velocity keeps one unit per component:

```fermium
q = <1 m, 2 m/s>
print q, q.x, q[2]
print q + <3 m, 4 m/s>
print 2 q, q / (2 s)
```

- `+` and `-` need the same units component by component: `<1 m, 2 m/s> + <1, 2> m` is an error ("component 2 is speed [m/s] on one side and length [m] on the other").
- Multiplying or dividing by a number changes every component's units.
- `|v|`, `norm`, `unit`, `·` and `×` need all components in the same units, and say so otherwise: a state vector has no length.
- `in` (unit conversion) works on one component at a time: `q.x in cm`.
- A vector whose components all have the same units behaves exactly as before; `<1 m, 2 m>` is `<1, 2> m`.

### Matrices

```fermium
K = [[2, -1], [-1, 2]] N/m
x = <1, 2> cm
print K
print K x in N
print det(K), inverse(K)
print Kᵀ, K K
print solve_linear(K, <1, 0> N)
print K[1, 2], K[2]
```

- **Making a matrix:** a list of rows, `[[1, 2], [3, 4]] N/m`, or with a unit on every entry, `[[1 N/m, 0 N/m], [0 N/m, 2 N/m]]`. All entries share one unit. From 1 to 4 rows and 1 to 4 columns. `identity(n)` is the n×n identity matrix (n = 2, 3 or 4).
- **Printing:** the rows on one line, `[[1, 2], [3, 4]] N/m`.
- **Arithmetic:** `A + B`, `A - B` (same size, same units), `2 A`, `A / 2`, `-A`.
- **Products:** `M v`, `M * v` or `M · v` is a matrix times a vector (a vector); `A B` or `A * B` is the matrix product. The units multiply: a stiffness matrix in N/m times a displacement in m gives a force in N. Write the matrix first; `v M` is an error.
- **Functions:** `transpose(M)` (also `Mᵀ`), `det(M)` (units to the power n: a 2×2 in N/m has a determinant in N²/m²), `inverse(M)` (units to the power −1: m/N), `solve_linear(M, b)` solves M x = b (x has the units of b divided by those of M; Gaussian elimination with partial pivoting).
- **Entries:** `M[i, j]` (1-based; also `M[i][j]`), and `M[i]` is row i as a vector. `row(M, i)` and `column(M, j)` are row i and column j as vectors, and `trace(M)` is the sum of the diagonal of a square matrix. Indexes can be loop variables (checked when the program runs).
- **Errors:** sizes that don't fit (`a 2×2 matrix times a 3-vector`), mixed units in a literal, and a singular matrix passed to `inverse` or `solve_linear` (a runtime error: "this matrix is singular").
- Matrices can be function arguments and results: `rot(θ) = [[cos(θ), -sin(θ)], [sin(θ), cos(θ)]]`.

#### Eigenvalues and normal modes

```fermium
k = 80 N/m
kc = 20 N/m
K = [[k + kc, -kc], [-kc, k + kc]]
print eigenvalues(K)                  # <80, 120> N/m
print eigenvectors(K)                 # columns: the unit eigenvectors
M = [[0.2, 0], [0, 0.4]] kg
ω2 = eigenvalues(K, M)                # K v = ω² M v
print ω2
print √(ω2[1]), √(ω2[2])              # the normal-mode angular frequencies
print eigenvectors(K, M)              # the mode shapes
```

- `eigenvalues(M)` of a **symmetric** 2×2, 3×3 or 4×4 matrix is a vector of its eigenvalues, **sorted from smallest to largest**, in the matrix's units (a stiffness matrix in N/m has eigenvalues in N/m). `eigenvectors(M)` is a matrix whose **columns** are the matching unit eigenvectors (column j belongs to eigenvalue j; each one's largest entry is positive). Computed by Jacobi rotations, to machine precision.
- `eigenvalues(K, M)` and `eigenvectors(K, M)` solve the **generalized** problem K v = λ M v with symmetric K and a symmetric, positive-definite M (a mass matrix). For springs and masses λ = ω², in 1/s². The mode shapes are scaled to unit length. Don't write `eigenvalues(inverse(M) K)`: M⁻¹K is not symmetric, so it is an error that points to `eigenvalues(K, M)`.
- **Errors** (when the program runs): a matrix that isn't symmetric (entries may differ by at most 10⁻¹⁰ of the largest entry), and a second matrix that isn't positive definite.

## 8. Derivatives

```fermium
A = 0.1 m
ω = 10 rad/s
x(t) = A cos(ω t)
v = d/dt x
a = x''
print v
print a
print v(0.1 s)
```

- **Syntax:** `x'`, `x''`, `dx/dt`, `d²x/dt²`, `d/dt x` and `d²/dt² x` all differentiate a one-line function. The result is a new function, whose units are the numerator's units divided by the denominator's.
- **Printing a function** shows its formula and units, for example `v(t) = -A ω sin(ω t)   [m/s, for t in s]`.
- **Formulas:** `d/dt (formula)` also works on a formula in `t`. The derivatives are exact and symbolic (sum, product, quotient and chain rules, and all the standard functions), then simplified.
- **Partial derivatives:** `∂/∂x f` (ASCII `partial/partial x f`) differentiates a function of several variables with respect to one parameter.
- **Functions defined by an integral** can be differentiated too, under the integral sign (the Leibniz rule): for `V(x, y, z) = ∫ … ds from a to b`, `∂/∂x V` is the function `∫ ∂/∂x(…) ds from a to b`. When a limit depends on the variable, its boundary term is added: `G(x) = ∫ x s² ds from 0 to x` gives `G'(x) = (∫ s² ds from 0 to x) + x³`. So `∇V`, `∇²V` and `x'` all work on such functions.

```fermium
λ = 2 nC/m
L = 0.5 m
V(x, y, z) = ∫ λ / (4π ε₀ √((x - s)^2 + y^2 + z^2)) ds from -L to L
print -∇V(0.3 m, 0.4 m, 0 m)          # the field of a finite line charge
print ∂/∂x V
```

- **Sums** written in one line with `Σ(… for k from a to b)` are differentiated term by term (§9), so `f'` and `∇²φ` of a Fourier series work.

### Vector calculus: ∇

```fermium
q = 1 nC
φ(x, y, z) = q / (4π ε₀ √(x² + y² + z²))
E = -∇φ(0 m, 3 m, 4 m)
print E
print ∇φ

B(x, y, z) = <-y, x, 0> T/m
print ∇×B(1 m, 2 m, 3 m)
print ∇·B(1 m, 2 m, 3 m)
f(x, y) = x² y + sin(x)
print ∇²f
```

- `∇f` (gradient), `∇·F` (divergence), `∇×F` (curl) and `∇²f` (Laplacian) of a one-line function of 2 or 3 Cartesian coordinates. The curl needs 3. The result is a new function of the same coordinates, differentiated symbolically, so `print ∇φ` shows its formula (tidied by SymPy when it is installed). Call it at a point like any function: `∇φ(1 m, 0 m, 0 m)`.
- `F` for `∇·F` and `∇×F` must be a vector formula: `F(x, y, z) = <…, …, …>` (a unit after `>` applies to every component).
- Units follow: φ in V with coordinates in m gives ∇φ in V/m. A component that differentiates to 0 fits the other components' units.
- ASCII: `grad(f)`, `div(F)`, `curl(F)`, `laplacian(f)`; `fermium fmt --ascii` writes these. `∇` is typed `\nabla`.
- Only Cartesian coordinates. For spherical or cylindrical coordinates, write the formulas out.

## 9. Integrals

```fermium
k = 50 N/m
F(x) = k x
W = ∫ F(x) dx from 0 m to 0.2 m
print W
print integral exp(-x^2) dx from -inf to inf
```

- **Definite integrals** are computed numerically with adaptive Gauss–Kronrod quadrature (G7/K15, relative tolerance 10⁻¹⁰), compiled to native code.
- **Infinite limits** (`∞` or `inf`) work for most physical scales: Fermium first scans the integrand to find its length scale, so a nanometre-wide decay or a 10⁸ m wide Gaussian both come out right. The scan finds the scale of decays and of peaks near the start of the range (or near 0). A narrow peak far from the start (say 1 μm wide at 1 m, integrated from 0 to ∞) can still be missed, and the result is then too small. Split the range around such a peak: `∫ … from 0 m to 2 m` plus `∫ … from 2 m to ∞`.
- **Singularities:** integrable blow-ups at an end of the range (like 1/√x at 0) work. An integrable blow-up at **0 inside the range** works too: the range is split there. Mild blow-ups like 1/√|x − c| work anywhere, but strong ones away from 0 may fail (see §19).
- **Integrals that don't converge** (like ∫ 1/x dx from -1 to 1, or sin(x) up to ∞) stop with a clear error instead of giving a number.
- **Integrals that are 0** (by symmetry: `∫ sin(x) dx from -1 to 1`, the y component of a field that has none) converge: the error is judged against ∫|f| as well as against the result, so a 0 made of rounding noise is accepted (it prints as something like 10⁻¹⁷). A component of a vector integral that is noise compared with the other components is accepted to 10⁻¹⁰ of the vector's size (DECISIONS D44).
- **NaN or ∞ in the integrand:** a 0/0 at a single point (like sin(x)/x at 0, if a sample lands exactly there) doesn't matter: once the quadrature has narrowed it down to a few floating-point numbers next to finite values, that point counts as 0. A NaN or ∞ over a stretch of the range is an error that says where, for example `the integrand is NaN at x = 767.1 (0/0? ∞/∞? an overflow like exp(710)?)` for `x⁴ exp(x) / (exp(x) - 1)²` up to 800: rewrite such an integrand in an overflow-safe form, `x⁴ exp(-x) / (1 - exp(-x))²`. `1 - cos(θ)` loses all its digits for tiny θ (it is exactly 0 below 10⁻⁸); write `2 sin(θ/2)²` (DECISIONS D45).
- **Integrals that can't be computed numerically** stop with a clear error ("couldn't compute this integral numerically") instead of giving a number. That happens for integrals that diverge (like ∫ 1/x dx from -1 to 1), and also for some that converge but oscillate without decaying fast enough (like ∫ sin(x)/x dx from 0 to ∞, which is π/2).

```fermium
print ∫ exp(-x/(1 nm)) dx from 0 m to ∞          # 1 nm
print ∫ exp(-(x/(1e8 m))^2) dx from -∞ to ∞      # √π × 10⁸ m
print ∫ 1/sqrt(abs(x)) dx from -1 to 1           # 4: a singularity at 0, inside the range
```

- **Units:** the result's units are the integrand's units times the variable's units.
- **Where the upper limit ends:** a `/` with a space before it ends the upper limit, so `∫ B(z) dz from -∞ to ∞ / (μ₀ I)` divides the whole integral by μ₀I. `from 0 to 1/2` (no spaces) and `from 0 to (L / 2)` divide the limit. When the division after a limit is by a plain number or name (`to L / 2`), Fermium warns that it divides the whole integral (DECISIONS D34).
- **Integrals without limits** (`∫ x² dx`) are done symbolically with SymPy and give a function.
- **Vectors:** an integral of a vector is the vector of the integrals of its components, each with its own units: `∫ <cos(φ), sin(φ), 0> dφ from 0 to π/2` is `<1, 1, 0>`. Biot–Savart works as written:

```fermium
R = 0.10 m
I = 2.0 A
ring(φ) = R * <cos(φ), sin(φ), 0>
dl(φ) = R * <-sin(φ), cos(φ), 0>
P = <0 m, 0 m, 0.05 m>
B = μ₀ I / (4π) * ∫ dl(φ) × (P - ring(φ)) / |P - ring(φ)|^3 dφ from 0 to 2π
print B                               # <0, 0, 9.0×10⁻⁶> T (up to rounding in x and y)
```

- **Integrals without limits** (`∫ x² dx`) are done symbolically with SymPy and give a function. Answers with `asinh`, `acosh`, `atanh`, `abs` and `sign` are fine: `a = 0.5 m` then `∫ 1/√(a² + s²) ds` is `asinh(s/a)`. A constant is taken as positive only when that is safe: physical constants, quantities written in the formula (`0.5 m`), and variables that are only ever set to positive numbers (not in a loop, `solve` or `fit`; never in the REPL). A constant that only appears squared, like `b` in `√(b² + s²)`, is replaced by `abs(b)`. Every formula is checked by differentiating it at random points, so a formula that only holds for one sign of a constant is refused. When SymPy can't give a usable formula, the error names the line and suggests limits.

### Sums: Σ

```fermium
print Σ(k² for k from 1 to 10)                          # 385
print sum(1/k² for k from 1 to 1000) to 6 digits         # 1.64393
square(x) = Σ(4/(n π) * sin(n x) for n from 1 to 99 step 2)
print square(1) to 4 digits                              # a square wave: about 1
print square'
a = 1 m
V0 = 10 V
φ(x, y) = Σ(4 V0/(n π) * sin(n π x/a) sinh(n π y/a) / sinh(n π) for n from 1 to 61 step 2)
lap = ∇²φ
print φ(0.5 m, 0.5 m) to 6 digits, lap(0.3 m, 0.7 m)     # V₀/4, and 0: it solves Laplace's equation
```

- **Syntax:** `Σ(term for k from a to b)` or `Σ(term for k from a to b step s)`; ASCII `sum(…)` or `Sigma(…)`. The range counts exactly like `for k from a to b step s` (both ends included; an empty range gives 0). The ends can be variables or a function's parameters: `S(N) = Σ(k for k from 1 to N)`.
- **Units:** the sum has the term's units. A range with units needs a `step` with units (`Σ(f(r) for r from 1 m to 3 m step 1 m)`). A vector term gives the vector of the sums.
- **Derivatives:** d/dx of a sum is the sum of the derivatives, so `f'`, `∂/∂x`, `∇` and `∇²` of a one-line function defined by a sum work (not with respect to a variable in the limits).
- **Only finite sums:** `Σ(1/k² for k from 1 to ∞)` is an error; sum a large fixed number of terms, or use a `for … to inf` loop with a `break` (§6).
- `Σ(xs)` of a list is `sum(xs)`.

## 10. Differential equations: solve

```fermium
m = 0.5 kg
k = 50 N/m
b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 0.1 [m], x'(0) = 0 m/s
  for t from 0 s to 5 s
print x(5 s)
print x'(1 s)
```

- **Writing the equation:** use primes (`x'`, `x''`), `dx/dt`, `d/dt x` or `d²/dt² x`. Each equation is solved for its highest derivative automatically, and it must appear linearly. Initial conditions use primes: `x'(0) = 0 m/s`.
- **Several highest derivatives in one equation** (Lagrange's equations, where θ₁'' and θ₂'' appear in both, M(q, q') q'' = f): write them as on paper. The equations must be linear in the highest derivatives (their coefficients may depend on t and the unknowns); Fermium collects the coefficients symbolically and solves the small linear system (Gaussian elimination with pivoting, up to 4 unknowns that are numbers) at every step. If the matrix is singular at some time, the error says when (DECISIONS D47). In `0.5 b''` with an unknown `b`, `b` is the unknown, not the unit barn.

```fermium
m1 = 1.0 kg
m2 = 0.5 kg
l1 = 1.0 m
l2 = 0.7 m
g = 9.81 m/s²
solve (m1 + m2) l1 θ1'' + m2 l2 θ2'' cos(θ1 - θ2) + m2 l2 θ2'² sin(θ1 - θ2) + (m1 + m2) g sin(θ1) = 0 N,
      l2 θ2'' + l1 θ1'' cos(θ1 - θ2) - l1 θ1'² sin(θ1 - θ2) + g sin(θ2) = 0 m/s²
  with θ1(0 s) = 1.2, θ2(0 s) = -0.5, θ1'(0 s) = 0 /s, θ2'(0 s) = 0 /s
  for t from 0 s to 5 s
print θ1(5 s), θ2(5 s)
```

- **Vector unknowns:** `solve r'' = -G M_sun r / |r|^3 with r(0) = <1, 0> AU, r'(0) = <0, 29.8> km/s for t from 0 yr to 1 yr`. Afterwards `r(t)` is a vector, and `plot r.y vs r.x` draws the path.
- **Systems:** separate equations with commas or `and`, or put them on the indented lines below `solve`. For example: `solve x' = -a x, y' = a x - b y with ...`
- **Initial conditions:** every unknown needs one, and so does every derivative below the highest. They determine the unknowns' units, and both sides of every equation are unit-checked.
- **Methods:**
  - Without `step`, Fermium uses adaptive Dormand–Prince RK45 (relative tolerance 10⁻⁹).
  - With `step 1 ms`, it uses classic fixed-step RK4. After the solve, Fermium checks the step cheaply (step doubling at 8 points, 24 extra evaluations of the right-hand side) and warns when the estimated error is more than 0.1% of the solution's size: `the step is too coarse for this equation: the estimated error is 80% of the solution's size …; use a smaller step, or drop step to use the adaptive solver`. The warning is shown once per `solve` line.
  - **`tolerance 1e-12`** after the range sets the adaptive solver's relative tolerance (a plain number between 0 and 1). It has no effect on RK4.
  - **`using rk4`** or **`using rk45`** (also `method rk4`) picks the method by name. `rk4` needs a `step`. With `using rk45`, the solver chooses its own steps and a `step` is ignored.
  - **`using radau`** is for **stiff** equations: time scales far apart, such as a decay chain with a 164 μs member followed for hours, fast chemistry next to slow chemistry, or a relaxation oscillator. RK45 has to keep its step below the shortest time scale for stability even after that part of the solution has settled, so it takes millions of steps. Radau (implicit Runge–Kutta, Radau IIA of order 5) takes steps sized by accuracy alone: the radon chain below takes about 8 000 steps over 12 hours, where RK45 needs 5×10⁷. `using bdf` is SciPy's variable-order BDF, of lower order (cheaper per step, less accurate at tight tolerances). Both use the same relative tolerance as RK45 (10⁻⁹, or `tolerance r`), and choose their own steps (a `step` is an error). They work with `until`, backwards ranges and vector unknowns, and the solution is used as usual.
  - A long RK45 solve that is held back by stiffness warns: `this equation looks stiff: rk45 has taken 526031 steps, held small by stability rather than accuracy …; add using radau after the range`. The "too many steps" error suggests it too.
  - `radau` and `bdf` run SciPy's solvers (they call the compiled right-hand side), so they need SciPy, and `fermium build` refuses them for now (use `fermium run`).
  - The order is `for t from a to b [step h] [tolerance r] [using method]`:

```fermium
solve x' = -x / (1 s)
  with x(0) = 1 m
  for t from 0 s to 1 s tolerance 1e-12
print x(1 s) to 10 digits

solve y' = -y / (1 s)
  with y(0) = 1 m
  for t from 0 s to 1 s step 1 ms using rk4
print y(1 s) to 10 digits
```

```fermium
# the radon progeny on an air filter: Po-218 → Pb-214 → Bi-214 → Po-214 (164 μs)
λ1 = ln(2) / 3.098 min
λ2 = ln(2) / 26.8 min
λ3 = ln(2) / 19.9 min
λ4 = ln(2) / 164.3 μs
solve
    N1' = -λ1 N1
    N2' = λ1 N1 - λ2 N2
    N3' = λ2 N2 - λ3 N3
    N4' = λ3 N3 - λ4 N4
    with N1(0) = 1e6, N2(0) = 0, N3(0) = 0, N4(0) = 0
    for t from 0 min to 720 min using radau
print "Bi-214 atoms at 1 h:", N3(60 min)
print "A(Po-214) / A(Bi-214) at 5 h:", λ4 N4(300 min) / (λ3 N3(300 min))
print len(times(N1)), "steps"
```

- **Using the result:**
  - `x(t)` gives the value at a time (interpolated), and `x'(t)` the derivative. The highest derivative (`x'(t)` of an unknown in `x' = …`, `x''(t)` in `x'' = …`) is the right-hand side evaluated at the interpolated state, so it is as accurate as `x(t)` itself; the right side uses the values its variables had when the `solve` ran (DECISIONS D46). `plot x' vs t` still draws the interpolant's derivative.
  - Inside a function, a solution can be used in `∫`, in an equation `solve … for T from a to b`, and at any time `x(t)`, but it can't be returned yet: return a number made from it (DECISIONS D48).
  - `plot x vs t` plots against time, and `plot y vs x` plots one unknown against another (an orbit or phase plot).
  - `values(x)` and `times(x)` give lists, and `x[end]` is the final value.
- **Towards smaller t:** the range can go down, `for t from 5 s to 0 s`, with the initial conditions at the start (5 s). This works with both methods (a `step` is always written as a positive size). `times(x)` then decreases, `x[end]` is the value at the end of the range (0 s), and `x(t)` and `plot` work as usual.
- **Stop condition, `until`:** `until lhs = rhs` on a line of its own stops the solve the first time the two sides cross (after the start), for example when a ball lands. The crossing is located to full precision on the solver's dense output (Dormand–Prince's 4th-order interpolant, or the cubic Hermite for RK4), and the solution ends there: `x[end]` is the value at the crossing and `times(x)[end]` the time. The range's end is then only a limit: if the condition never happens before it, that's an error (so a too-short range can't be mistaken for the answer). The condition can use `t`, the unknowns and their derivatives below the highest (`until y' = 0 m/s` for the top of a flight), in matching units. On one line, write it after the range (`… for t from 0 s to 10 s until y = 0 m`) or after the initial conditions.

```fermium
g = 9.81 m/s²
θ = 40°
k = 0.01 / (1 m)
solve r'' = <0 m/s², -g> - k |r'| r'
  with r(0 s) = <0, 0> m, r'(0 s) = 30 m/s * <cos(θ), sin(θ)>
  for t from 0 s to 60 s
  until r.y = 0 m
print "flight time", times(r)[end]
print "range", r.x[end]

solve u' = u / (1 s)
  with u(0 s) = 1 m
  for t from 0 s to -5 s
  until u = 0.5 m
print "u halves at", times(u)[end]
```

- **An `if` on t** (a force that switches on at 0.3 s, a potential step in x): the adaptive solver finds the switch to rounding precision and restarts there, so the requested tolerance holds across it. A jump that depends on the unknowns instead (`if x > 0 m`) is not located this way.
- **Runtime errors** name the equation's own variable and units: `the right side of the equation is NaN or infinite at ξ = 0 (0/0? 1/0?)` when it can't be evaluated at the start (start slightly away from a singular point, with a series), and `the range of t is empty` for a range that starts where it ends.

### Equations: solve … for x from a to b

Without derivatives and without `with`, `solve` finds where the two sides of an equation are equal, and stores the answer in the variable:

```fermium
solve cos(x) = x for x from 0 to 10
print x

g = 9.81 m/s²
solve g t²/2 = 20 m for t from 0 s to 10 s
print t
```

- It finds the **first** solution after `a`, even when the two ends already bracket a later one: it looks for the first crossing at 200 points from `a`, then refines it to full double precision (Illinois regula falsi, which keeps the solution bracketed). `solve sin(x) = 0 for x from 1 to 10` gives π. Two solutions closer together than (b − a)/200 can hide each other; narrow the range to separate them.
- The answer has the units of the range. Inside a loop, each `solve` overwrites the variable.
- **Derivatives of known functions** are values: with `I(θ)` defined, `solve I'(θm) = 0 for θm from a to b` finds a maximum of I, and with `r` an ODE solution, `solve r(t2) · r'(t2) = 0 km²/s for t2 from …` finds where the radial velocity is zero. Only names that aren't defined yet make a `solve` a differential equation.
- If the sides never cross in the range, the error says so. A jump across (like `tan` at 90°) is reported as not a solution; narrow the range. A scan point that lands exactly on a pole (the sides are ∞ there, as in `1/(x - 1.5)` scanned from 1 to 2) is skipped, not taken as a crossing: `solve 1/(x - 1.5) = 2 for x from 1 to 2` gives 2, and `solve 1/(x - 1.5) = 0 …` is the jump error. A point where the sides are undefined (NaN) inside the final bracket is never returned as a solution.
- **Rounding-noise warning:** if large terms cancel so badly that the two sides differ only by rounding error near the crossing (`(E + ε)² − (pc − ε)²` with E ~ 10²⁰ eV), the answer is printed with a warning; expand the expression on paper so the big terms cancel exactly.
- `step`, `tolerance` and `using` are only for differential equations.

```fermium
f(x) = x³ - 3x
solve f'(x) = 0 for x from 0 to 3
print x

solve sin(x) = 0 for x from 1 to 10
print x
```

## 11. Data: load, fit, plot

```
data = load "pendulum.csv"       # header:  L [m], T [s]
print data.L                     # a column, as a list with units
fit T = 2π √(L / g) to data      # fits g; reports g with units
plot data.T vs data.L to "pendulum.png"
```

- **`load "file.csv"`:** reads a CSV whose header gives names and units, like `T [s]`. The header is read when the program is compiled, so the units are checked. Paths are relative to the program's folder.
- **`fit y = model to data`:** nonlinear least squares. The left side can also be a formula of a column, for a linearised fit: `fit T^2 = k L to data`.
  - **Parameters:** the names that are not columns, constants or functions. If there are none, the names that already have values are fitted, starting from those values.
  - **Starting guesses:** set them with `with a = 2 m`.
  - **Report:** each parameter with units, a standard error, and the rms residual. Afterwards the parameters are ordinary variables, and `err(g)` is g's standard error (same units; NaN if it couldn't be estimated), so it can be carried into later results: `print err(g)/g`. Values are shown to the second digit of their standard error.
- **Plots:** each form saves a PNG with labelled axes (units included) and prints where it was saved.
  - `plot ys vs xs` (lists). Columns from `load` are drawn as markers, everything else as lines.
  - `plot x vs t` (an ODE solution)
  - `plot f(x) vs x from 0 m to 1 m` (a formula)
  - `... to "file.png"` chooses the file name.
  - Options go after `with`: `with log y`, `with log x`, `with log` (both axes), `with title "Decay of Ba-137m"`. Separate several options with commas.
  - Several series: `plot a vs t, b vs t`.

### Dimensional analysis: analyze

`analyze name: T [s] depends on L [m], m [kg], g [m/s²]` applies the Buckingham Π theorem: it finds the dimensionless groups that can be made from the target (T) and the quantities it depends on, and says what they imply for the target.

```fermium
analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]
print 2π pendulum(1 m, 9.81 m/s²)
```

prints

```
dimensional analysis of pendulum: T depends on L, m, g
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = T √(g/L)
  so T ∝ √(L/g)   (T = C √(L/g), with C a pure number)
  m drops out: nothing else has mass
  defined pendulum(L, g) = √(L/g), so T = C pendulum(L, g)
2.01 s
```

- **Quantities:** each is a name with a unit in brackets (`L [m]`; only the dimension matters, `[1]` is a pure number), a variable defined earlier, or a built-in constant (`G`, `c`, `ħ`, `e`, `m_e`, `ε₀`, …). A bracketed name is always a new quantity, even if a constant has that name.
- **Groups:** there are n − r of them (n quantities, r = rank of the dimension matrix). The target is in exactly one group, with exponent 1. The groups are built from the *repeating* quantities: the inputs, in the order written, that are dimensionally independent of the ones before them. So the order chooses the form: `F [N] depends on ρ [kg/m³], v [m/s], A [m²], μ [Pa s]` gives `F = ρ v² A · f(Π₂)` with Π₂ = ρ v √A/μ (the Reynolds number); listing μ first gives `F = μ v √A · f(Π₂)`, with Π₂ = v ρ √A/μ. The other groups are scaled to small exponents (integers or halves), mostly positive.
- **Conclusion:** one group: `T ∝ …`; several: `T = … · f(Π₂, …)`. An input that is in no group *drops out*, with the reason (`nothing else has mass`).
- **The result is usable:** with a name, `analyze` defines `name(...)` = the formula for the target without its constant, with the non-constant quantities that appear in it as arguments (bracketed ones keep their units, so `pendulum(1 s, …)` is an error). Use it in formulas or fits: `fit T = C pendulum(L, 9.81 m/s²) to data` fits the pure number C. If the formula has only constants (`analyze planck: ℓ [m] depends on G, ħ, c`), `name` is a plain value and its value is printed. Without a name (`analyze T [s] depends on L [m], g [m/s²]`), nothing is defined.
- **Errors:** a target whose dimension can't be made from the inputs is an error that says which base dimension is missing (`v can't be made from m, t: v has length, but nothing it depends on has length`), and says so when there is no dimensionless group at all. A name with no known units asks for a bracket. `analyze` only works at the top level (not inside `if`, loops or functions).
- **Exact:** the dimension matrix is solved with fractions, so fifth roots and halves are exact: `R [m] depends on E [J], ρ [kg/m³], t [s]` gives `R ∝ (E t²/ρ)^(1/5)` (Taylor's blast wave). The printed formulas are valid Fermium.
- `analyze` stays an ordinary name everywhere else: a line is an analysis only when it has `depends` in it.

See [bootcamp lesson 11](../bootcamp/lesson11_dimensional_analysis.md) for a tutorial and DECISIONS.md D70 for the design.

## Modules

A **module** is a `.fm` file of functions and constants that other programs import. Fermium ships a
standard library of modules: `mechanics`, `em`, `nuclear`, `astro`, `quantum` and `stats`, listed with
every function's units in [stdlib.md](stdlib.md).

```fermium
import mechanics
print mechanics.pendulum_period_large(1 m, g_n, 60°)       # 2.15 s: the exact period at 60°

import astro as a
print a.schwarzschild_radius(10 M☉)

from nuclear import semf_binding, Q_value
print semf_binding(56, 26)                                  # ⁵⁶Fe, about 495 MeV
print Q_value(238.0507884 u, 234.0436014 u + 4.00260325 u)  # α decay of ²³⁸U: 4.27 MeV

from quantum import hydrogen_level as E
print E(2) - E(1)
```

- **Three forms:** `import mechanics` makes the module's names available as `mechanics.name`;
  `import astro as a` gives the module a shorter name (`a.name`); `from nuclear import semf_binding, Q_value`
  makes those names available directly, and `from em import skin_depth as δ` renames one.
- **Import by path:** `import "lib/springs.fm"` (then `springs.name`), or `import "lib/my-springs.fm" as sp`
  when the file name isn't a valid name. The path is relative to the folder of the file doing the import.
- **Where modules are found:** `import springs` looks for `springs.fm` in the folder of the file doing the
  import, then the program's folder, then the folders listed in `fermium.toml` (below), then the
  standard library. So a `mechanics.fm` next to your program is used instead of the standard one.
- **What a module may contain:** function definitions, constants (`a_V = 15.75 MeV`) and imports of other
  modules. A module can't print, plot, solve or loop when it is imported: `a module can only define
  functions and constants, but this line has a print`. Its constants are computed once, where it is
  first imported.
- **Imported functions are ordinary functions:** they can be passed to other functions, integrated and
  differentiated: `astro.wien_peak'(5000 K)` and, after `from astro import wien_peak`, `wien_peak'`.
- **Units are checked across modules** exactly as in one file: a module's functions are checked with the
  units of each call's arguments, and parameters written with units (`kinetic_energy(mass [kg], v [m/s])`)
  must get those units.
- **Each module has its own names.** A module's functions see the module's own functions and constants
  and the built-in constants, never the importing program's variables. Names starting with `_` are private.
- **Clear errors:** a module that can't be found (with the folders searched and a close name), a name the
  module doesn't have, the same name imported from two modules, an imported name that the program also
  defines, a circular import (`circular import: a → b → a`), and an import inside a block or a function.
  An error inside a module names the module file and its line, and points at the line of your program
  that led to it.
- **The REPL, Jupyter and the editor** understand imports too: in the REPL, `import mechanics` works like in
  a program (modules are found from the folder the REPL was started in); the language server finds
  modules from the document's folder.

**`fermium.toml`** marks the folder of a project. `fermium run` (and `check`, the REPL and the editor) reads
the nearest one, in the program's folder or a parent folder:

```
[project]
name = "lab-reports"
version = "0.1.0"

[paths]
modules = ["lib", "../shared"]     # folders searched for modules, relative to fermium.toml
```

See DECISIONS.md D100–D103 for the design.

## 12. Symbols and ASCII spellings

Every symbol has an ASCII spelling that means exactly the same thing.

| Symbol | ASCII | `\name` Tab |
|---|---|---|
| π | `pi` | `\pi` |
| θ, ω, λ, … | `theta`, `omega`, `lambda`, … | `\theta` … |
| ħ | `hbar` | `\hbar` |
| √x | `sqrt(x)` | `\sqrt` |
| ∛x | `cbrt(x)` | `\cbrt` |
| x² | `x^2` | `\^2` |
| ∫ | `integral` | `\int` |
| ∂ | `partial` | `\partial` |
| ± | `+-` (reserved for uncertainties) | `\pm` |
| ° | `deg` | `\deg` |
| °C | `degC` | `\celsius` |
| · × | `*` | `\cdot` `\times` |
| Mᵀ | `transpose(M)` | `\transpose` |
| ≤ ≥ ≠ ≈ | `<=` `>=` `!=` `~=` | `\le` `\ge` `\ne` `\approx` |
| ∞ | `inf` | `\infty` |
| ε₀ | `epsilon_0` | `\epsilon\_0` |
| Å, μm, M☉ | `angstrom`, `um`, `Msun` | `\AA`, `\mu`, `\Msun` |

- **Greek names:** Greek letter names are converted segment by segment, so `omega_0` is the same name as `ω₀`, and `m_pi` is the same name as `m_π` (a `π` after `_` is part of the name; on its own `π` is the constant).
- **Look-alike characters:** the micro sign µ becomes μ. Using two names that look the same (such as `v` and Greek `ν`) gives a warning.

## 13. Built-in functions

| Function | Meaning |
|---|---|
| `sin cos tan asin acos atan sinh cosh tanh exp ln log log10 log2 erf gamma` | need plain numbers (angles are plain numbers) |
| `besselj(n, x) bessely(n, x)` | Bessel functions J_n and Y_n of whole-number order n (plain numbers; the C library's `jn`/`yn`) |
| `besseli(n, x) besselk(n, x)` | modified Bessel functions I_n and K_n of whole-number order n (plain numbers) |
| `ellipk(m) ellipe(m)` | complete elliptic integrals K(m) and E(m) with the **parameter m = k²**, as in SciPy and Abramowitz & Stegun (`ellipk(0.5)` = 1.8541; K(1) = ∞) |
| `sqrt cbrt abs sign` | keep or transform units |
| `floor ceil round` | need plain numbers: `floor(270 cm)` is an error, because the answer depends on the unit. Write `floor(x / (1 cm)) cm` |
| `atan2(y, x) hypot(a, b) mod(a, b) min(a, b, …) max(…) clamp(x, lo, hi)` | arguments in the same units |
| `len sum mean std min max first last cumsum diff reverse sort` | lists |
| `linspace(a, b, n) range(a, b, step) zeros(n) ones(n)` | make lists |
| `push(xs, x)` / `append` | add to a list |
| `dot(a, b) trapz(ys, xs) interp(x, xs, ys)` | list maths |
| `norm(v) unit(v) dot(a, b) cross(a, b) vec(x, y[, z])` | vectors (also `\|v\|`, `a · b`, `a × b`) |
| `angle(a, b)` | the angle between two vectors, from 0 to π |
| `abs(v)` `abs(M)` | the absolute value of each component or entry |
| `Σ(term for k from a to b [step s])` / `sum(…)` | a sum written in one line (§9), differentiable term by term |
| `sign(v)` of a vector | the unit vector v/\|v\|, the same as `unit(v)`: `sign(<3, 4> m/s)` is `<0.6, 0.8>` |
| `transpose(M) det(M) inverse(M) identity(n) solve_linear(M, b)` | matrices (also `Mᵀ`, `M v`, `A B`) |
| `trace(M) row(M, i) column(M, j)` | the sum of the diagonal; a row or a column as a vector |
| `eigenvalues(M) eigenvectors(M) eigenvalues(K, M) eigenvectors(K, M)` | symmetric matrices: eigenvalues sorted ascending, unit eigenvectors as columns; K v = λ M v for normal modes |
| `values(sol) times(sol)` | samples of an ODE solution |
| `to(x, unit)` | same as `x in unit` |
| `factorial(n)` | |
| `rand() rand(a, b) randn() randn(μ, σ) seed(n) sample(expr, N)` | seeded random numbers and Monte Carlo (§20) |
| `fft_re(xs) fft_im(xs) ifft(re, im) amplitude_spectrum(xs) power_spectrum(xs, dt) frequencies(xs, dt)` | Fourier transforms (§20) |
| `argmax(xs) argmin(xs)` | the position (1-based) of the largest / smallest element |
| `clock()` | the time in seconds, from an arbitrary starting point; subtract two readings to time part of a program |

```fermium
print besselj(0, 2.404825557695773) to 3 digits    # a zero of J₀
print besselj(1, 1) to 10 digits, bessely(0, 1) to 10 digits
print besseli(1, 2) to 10 digits, besselk(0, 1) to 10 digits
print ellipk(0.5) to 10 digits, ellipe(0.5) to 10 digits
J1p = d/dx besselj(1, x)                              # (J₀ − J₂)/2
print J1p(2) to 10 digits
```

- An order that isn't a whole number is an error when it is written as a number (`besselj(1.5, x)`); computed at run time, it gives NaN. The derivatives with respect to x (and m) are known to the differentiator: J′ₙ = (Jₙ₋₁ − Jₙ₊₁)/2, I′ₙ = (Iₙ₋₁ + Iₙ₊₁)/2, K′ₙ = −(Kₙ₋₁ + Kₙ₊₁)/2, dK/dm = (E − (1 − m)K)/(2m(1 − m)), dE/dm = (E − K)/(2m).

## 14. Constants

CODATA 2022 values (NIST), with units. You can override any of them by assigning your own value to the name (`h = 10 m`).

| Name | Meaning |
|---|---|
| `c` | speed of light |
| `h`, `ħ` (`hbar`) | Planck constant, reduced Planck constant |
| `e` | elementary charge |
| `k_B` | Boltzmann constant |
| `N_A` | Avogadro constant |
| `R_gas` | molar gas constant |
| `G` | gravitational constant |
| `g_n` | standard gravity |
| `m_e`, `m_p`, `m_n`, `m_u`, `m_α`, `m_d`, `m_μ` | particle masses |
| `ε_0` (`epsilon_0`), `μ_0` (`mu_0`), `k_e` | electromagnetic constants |
| `σ` (`sigma`) | Stefan–Boltzmann constant |
| `α` (`alpha`) | fine-structure constant |
| `a_0`, `R_∞`, `r_e` | Bohr radius, Rydberg constant, classical electron radius |
| `b_W` | Wien displacement constant |
| `μ_B`, `μ_N` | Bohr and nuclear magnetons |
| `M_sun`, `R_sun`, `L_sun`, `M_earth`, `R_earth`, `AU`, `GM_sun` (`GM☉`), `GM_earth` | astronomy (GM is known more precisely than G or M) |
| `π` | pi |
| `∞` | infinity |

## 15. Units

- **SI base units:** `m g s A K mol cd`.
- **SI prefixes:** Q R Y Z E P T G M k h da d c m μ(u) n p f a z y r q, on the prefixable units.
- **Derived units:** `N J W Pa C V F Ω(ohm) S Wb T H Hz Bq Gy Sv lm lx kat`.
- **Physics:** `eV` (`keV MeV GeV`), `u`/`amu`/`Da`, `b`/`barn`, `fm`, `Å`, `erg`, `dyn`, `gauss`, `c` (as a speed unit), `Ci`.
- **Astronomy:** `au`/`AU`, `ly`, `pc` (`kpc Mpc`), `M☉ R☉ L☉` (`Msun Rsun Lsun`), `M_E R_E`, `yr`.
- **Other:** `min hr day year`, `L`, `atm bar Torr mmHg psi`, `inch ft yd mi mph kph lb lbf hp cal Wh` (`kWh MWh`), `rad sr ° arcmin arcsec rev rpm %`.
  - `rad` and `arcsec` take SI prefixes: `mrad`, `μrad`, `krad/s`; `mas` and `μas` are milli- and micro-arcseconds.
  - `rev` = 2π (angles are plain numbers) and `rpm` = rev/min. So `60 rpm in Hz` is 2π Hz = 6.28 Hz, an angular frequency, and Fermium warns about it. To count turns per second, write `in rev/s`: `60 rpm in rev/s` is 1 rev/s. See DECISIONS D27.
  - The other way round, **Hz means rad/s**: `1 Hz in rpm` is 9.55 rpm (not 60) and `1 Hz in rad/s` is 1 rad/s (not 2π). Every conversion between a value written or shown in Hz and rev, rpm, rad/s or °/s warns, with the numbers for that case. For cycles per second, write the value in `rev/s` (`50 rev/s in rpm` is 3000 rpm), or multiply by 2π (`2π f in rpm`; `2π f` and `ω/(2π)` drop the Hz or rad/s they came from). Adding or subtracting a Hz value and a rad/s (or rpm) value warns too. See DECISIONS D95.
  - **Same SI unit, different quantity:** adding or subtracting `Gy` and `Sv` (absorbed and equivalent dose), `Bq` and `Hz`, or `J` and `N m` (energy and torque) warns.
- **Temperatures:** `K`, and `°C`/`°F` (absolute temperatures; see DECISIONS D12). The difference of two temperatures is shown in K (`in °C` shows it without the offset, with a warning). `°C + °C` and `sum` (or `cumsum`) of a list in °C are errors (`mean` works); `2 T` and `T / 2` of a °C value warn that they scale the absolute temperature. Inside a compound unit a degree is a step, so `2 °C/min` and `4.18 J/(g °C)` work.
- **Names left out on purpose, because they collide with common variable names:** `h` for hour (use `hr`), `t` for tonne (use `tonne`), `G` for gauss (use `gauss`), `d` for day (use `day`).

### Natural units: `units natural`, `units nuclear`, `units astro`

Particle and nuclear physicists set ħ = c = 1: then a mass is an energy, and a length or a time is 1/energy.
Write `units natural(ħ = c = 1)` (or just `units natural`) on its own line, and from there on Fermium
works the same way. Units are still checked, and `in` gives the answer back in SI:

```fermium
units natural(ħ = c = 1)
a0 = 1/(α m_e)          # the Bohr radius, as on paper
print a0                # 268.173 MeV⁻¹
print a0 in fm          # 52917.7 fm
print a0 in Å           # 0.529177 Å
m_π = 139.57 MeV
print 1/m_π in fm       # range of the pion-exchange force: 1.4138 fm
print ħ, c              # 1 1
```

- **What is checked:** "modulo ħ and c". A mass plus an energy is fine (`1 kg + 1 J`), and so is a length
  plus a time (1 s is 2.998×10⁸ m). An energy plus a length is still an error, because it is E + 1/E:
  `can't add energy or mass [MeV] to length or time (1/energy) [MeV⁻¹]`. `in` must match the power of energy too.
- **Constants** take their natural-unit values: ħ and c are exactly 1, `m_e` is 0.511 MeV,
  and `G` is 1/M_Planck² = 6.70883×10⁻³⁹ GeV⁻², so `2 G M☉ in km` is the Schwarzschild radius, 2.95325 km.
- **Printing:** a value with no unit of its own is shown in powers of MeV. A unit you wrote is kept
  (`m = 1 kg` prints as `1 kg`). Converting back is exact: `x in fm` multiplies by ħc = 197.327 MeV fm,
  and `m in kg` divides by c². Fermium works out the powers of ħ and c for you.
- **Other constants:** you can set any independent set of ħ, c, k_B, G and ε_0 to 1. With
  `units natural(ħ = c = k_B = 1)` a temperature is an energy (`300 K in meV` is 25.852 meV).
  `units natural(G = c = 1)` gives geometrized units (M☉ is 1476.63 m), and `units natural(ħ = c = G = 1)`
  gives Planck units.
- **`units nuclear`** is ħ = c = 1, with results shown in MeV, and in fm for 1/energy (fm² for cross
  sections): `print 1/(139.57 MeV)` shows `1.4138 fm`.
- **`units astro`** doesn't set anything to 1: it is ordinary SI checking. A value with no unit of its own
  is shown in M☉, AU, yr, L☉ or km/s, so `print G` shows `39.4769 AU³/(M☉ yr²)` (that is 4π²).
- **A region:** end the line with `:` and indent the lines under it. The natural units then hold for
  those lines only. SI variables from before the region convert into it automatically. A value computed
  inside the region leaves it only through `in`, because 1/MeV could be a length or a time:

```fermium
L = 2 m
units nuclear:
    E = 10 MeV
    k = 1/(1 fm)
    print E, k          # 10 MeV 197.327 MeV
r = 1/k in fm           # an ordinary SI length again
print r in m, E in J
```

- **Functions:** a function defined outside a region can be used inside it. It is checked again in natural
  units, and it gives the same physics (`f(m) = m c²` gives the same joules). A function defined inside a
  region can only be used where the same constants are 1.
- **Rules:** a `units` line goes at the top level (not inside a function, loop or `if`), and `units SI`
  switches back. A variable set outside a region can't be changed inside it. An ODE solution can't cross
  a region boundary, and `load` isn't allowed inside a natural region. `fermium build` works as usual.
  See DECISIONS D60 for the design.

## 16. Errors

Fermium reports problems in one line, points at the spot, and suggests a fix:

```
prog.fm, line 3: can't add length [m] to time [s]
    y = x + t
        ^^^^^
  hint: both sides of + and - must have the same units
```

Runtime problems (an index out of range, asking an ODE solution for a time outside its range) stop the program with a one-line message. **Control+C** stops a running program (it prints `stopped by Ctrl+C`). See `bootcamp/TROUBLESHOOTING.md` for the common ones.

## 17. Tools

- **REPL:** run `fermium`. It keeps history (the up arrow), saved between sessions in `~/.fermium_history`. Type `\theta` then Tab to get θ; `\name` is also replaced when you press Enter. `:help` shows help, `:quit` leaves.
- **`fermium fmt file.fm --pretty` / `--ascii`:** converts between ASCII and symbols without changing the program's meaning.
- **`fermium doctor`:** checks the installation and explains fixes. It also reports whether a C compiler is available, which only `fermium build` needs.
- **`fermium build file.fm -o prog`:** compiles ahead of time into a standalone executable. LLVM compiles the program to an object file, which is linked with a small C runtime. This needs a C compiler (on a Mac: `xcode-select --install`). The executable prints exactly what `fermium run` prints, and doesn't need Python. `load`, `fit` and `plot` work too, with three differences:
  - **Files are relative to the folder you run the program in**, not the folder of the `.fm` file. `load "data/pendulum.csv"` in a program built as `./pendulum` reads `data/pendulum.csv` from the current folder, and a plot is saved there too. The CSV is read when the program runs, so new measurements work without rebuilding; but the header must be the one the program was built with (the columns' units are compiled in), otherwise the program stops with a message saying so.
  - **Plots are SVG files.** `plot ... to "decay.png"` writes `decay.svg` and prints `plot saved to decay.svg (standalone programs write SVG)`. The plot has axes, ticks, unit labels, a legend, log scales and the title, but it is simpler than the matplotlib one (no minor ticks). Open it in a web browser.
  - **`fit`** uses Fermium's own Levenberg–Marquardt instead of SciPy. It starts from the same guesses and reports the same numbers to the printed digits. When the fit can't pin a parameter down (the warning *the fit may not have converged*), the value it stops at can differ from `fermium run`.
- **VS Code:** `editors/vscode/` adds syntax highlighting and `\name` completion.

## 18. Grammar summary

```
program    := statement*
statement  := name = expr [where binds] | name op= expr | name[expr] = expr
            | name(params) = expr | name(params) = NEWLINE INDENT block
            | print items | plot series [to "file"] | fit eq to expr [with binds]
            | analyze [name:] q [unit] depends on q [unit], q [unit], ...
            | solve eqs [with eqs] for t from a to b [step h] [tolerance r] [using rk4|rk45|radau|bdf]
            | if expr block [else block] | for x from a to b [step s] block
            | for x in expr block | while expr block | return expr | break | continue
            | assert expr [, "message"] | expr
            | import name [as name] | import "file.fm" [as name]
            | from name import name [as name], name [as name], ...
expr       := if expr then expr [NEWLINE INDENT] else expr | or-expression
comparison := sum (cmp sum)*          a < x < b means a < x and x < b
precedence := or < and < not < comparison < + - < * / < unary - < implicit × < ^ < postfix
postfix    := atom ( (args) | [index] | .name | ' )*
atom       := number [unit] | name | "text" | (expr) | [list] | <expr, expr[, expr]> [unit] | |expr|
            | √atom | ∫ … d x [from a to b] | d/dt atom | dx/dt | ∂/∂x atom | load "file"
            | Σ(expr for x from a to b [step s])
```

## 19. Known limitations

These are known and not yet fixed. None of them is silent about units.

- **A narrow peak in a huge finite range can be missed.** `∫ exp(-x²) dx from -1e6 to 1e6` prints `0` (the right answer is √π ≈ 1.77): the first samples of the quadrature all land where the integrand is 0. A peak that sits exactly in the middle of the range can come out as half its true value. Use a range that fits the peak, or split the range at the peak. Infinite ranges handle decays and peaks near the start, but a narrow peak far from the start can still be missed (§9).
- **Strong blow-ups away from 0 fail.** `∫ abs(x - 0.3)^(-0.8) dx from -1 to 1` stops with "couldn't compute this integral numerically", although it converges (the same happens from 0.3 to 1). Shift the variable so that the blow-up is at 0: `∫ abs(u)^(-0.8) du from -1.3 to 0.7` gives the right answer, 9.9. Blow-ups at 0, and mild ones like 1/√|x − 0.3|, work.
- **No garbage collection.** Memory for lists (including the old blocks left behind when `push` grows a list) is only given back when the program ends. A program that makes many large lists in a loop can run out of memory.
- **Derivatives** (`x'`, `d/dt`, `∂/∂x`) only work on one-line functions and formulas (a series can be one line with `Σ`, §9).
- A jump in an ODE that depends on the unknowns (`if x > 0 m`) isn't located like a jump in t, so it can cost accuracy.
- **Lists of vectors or matrices** don't exist yet. `eigenvalues` needs a symmetric matrix (or the pair K, M).
- **Uncertainties** (`±`) are reserved but not implemented yet (see `docs/uncertainties.md`).
- **Modules** are read again by each compilation (no cached compiled modules), and the REPL keeps a module it
  has imported even if the file changes (restart the REPL to see the change).
- **`fermium build`** writes plots as SVG (not PNG), and reads data files relative to the folder the program is run in (§17).

## 20. Numerics: random numbers, Fourier transforms, eigenstates, PDEs

### Random numbers and Monte Carlo

```fermium
seed(42)                       # the same numbers every run, in fermium run, fermium build and the interpreter
print rand()                   # uniform in [0, 1)
print rand(2 m, 3 m)           # uniform in [2 m, 3 m): both ends in the same units
print randn()                  # standard normal (mean 0, standard deviation 1)
print randn(9.81 m/s², 0.02 m/s²)   # normal with mean μ and standard deviation σ, in their units
```

- **`seed(n)`** is a statement on its own line. It restarts the generator: the same seed gives the same numbers, in `fermium run`, in a `fermium build` executable and in the reference interpreter (the generator, xoshiro256\*\*, is written once in LLVM IR and once in Python, DECISIONS D80). A program that never calls `seed` starts as if it had called `seed(0)`, so it is reproducible too. In the REPL and in Jupyter, the numbers continue from one input to the next.
- **`sample(expr, N)`** evaluates `expr` N times, drawing new random numbers each time, and gives a list with the units of `expr`. With `mean`, `std` and `len`, this is a Monte Carlo estimate with its statistical error:

```fermium
seed(1)
N = 100000
inside = sample(if rand()^2 + rand()^2 < 1 then 1 else 0, N)
p = mean(inside)
print "π ≈", 4 p, "±", 4 sqrt(p (1 - p) / N)

# a pendulum's period when its length is known to ±1 cm (Monte Carlo error propagation)
g = 9.81 m/s²
Ts = sample(2π sqrt(randn(1.00 m, 0.01 m) / g), 20000)
print mean(Ts), "±", std(Ts)
```

- `randn` uses the Box–Muller method (two uniform numbers per normal number). `std` is the sample standard deviation (divides by N − 1).

### Fourier transforms

Fermium has no complex numbers, so a transform comes as its real and imaginary parts, or directly as a spectrum with units:

```fermium
dt = 1 ms                                  # sampling interval
n = 1000
ts = linspace(0 s, (n - 1) dt, n)
xs = zeros(n)
for i from 1 to n
    xs[i] = 3 V * sin(2π * 50 Hz * ts[i]) + 1 V * cos(2π * 120 Hz * ts[i])

A = amplitude_spectrum(xs)                 # in V: a sine of amplitude 3 V gives a peak of 3 V
f = frequencies(xs, dt)                    # in Hz: 0, 1 Hz, 2 Hz, …, 500 Hz (the Nyquist frequency)
k = argmax(A)
print "strongest:", f[k], "with amplitude", A[k]

P = power_spectrum(xs, dt)                 # power spectral density, in V²/Hz
print "Parseval:", sum(P) (f[2] - f[1]), "=", mean(xs * xs)

back = ifft(fft_re(xs), fft_im(xs))        # the inverse transform gives back the signal
print back[10], xs[10]
```

| Function | Gives | Units |
|---|---|---|
| `fft_re(xs)`, `fft_im(xs)` | real and imaginary parts of X_k = Σⱼ xⱼ e^(−2πi jk/n), k = 0 … n − 1 (NumPy's `fft`, not normalised) | those of xs |
| `ifft(re, im)` | the real part of the inverse transform, (1/n) Σₖ Xₖ e^(2πi jk/n) | those of re and im |
| `amplitude_spectrum(xs)` | one-sided amplitudes for k = 0 … n/2: \|Xₖ\|/n, doubled except at 0 Hz and at the Nyquist frequency | those of xs |
| `power_spectrum(xs, dt)` | one-sided power spectral density \|Xₖ\|² dt/n (doubled likewise); Σ P Δf = mean(x²) | xs² × time (V²/Hz) |
| `frequencies(xs, dt)` or `frequencies(n, dt)` | the frequencies of those n/2 + 1 bins, k/(n dt) (NumPy's `rfftfreq`) | 1/time (Hz) |

- Any length works (not only powers of two). A frequency between two bins shows up in the nearest bins, spread out ("leakage"); a longer signal gives finer bins, Δf = 1/(n dt).
- `fermium run` and the interpreter use NumPy's FFT; `fermium build` executables use a built-in C FFT (radix 2, and Bluestein's algorithm for other lengths), which agrees to rounding.

### Bound states: solve … lowest N

An equation that is linear in an unknown function ψ, with one undefined constant (the eigenvalue), zero boundary conditions at both ends and `lowest N`, is an **eigenvalue problem**: `solve` finds the N lowest eigenvalues and their eigenfunctions.

```fermium
m = m_e
ħω = 1 eV
ω = ħω / ħ
V(x) = m ω² x² / 2
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 4
for n from 1 to 4
    print "E", n, "=", E[n] in eV, "  (ħω(n - ½) =", ħω (n - 0.5), ")"
print ψ₁(0 nm), ∫ ψ₁(x)^2 dx from -3 nm to 3 nm
```

- **The eigenvalue** is the one name in the equation that has no value yet (`E` here). Afterwards it is a list, `E[1] < E[2] < …`, with the units the equation gives it (energy).
- **The states** are `ψ₁ … ψ_N` (ASCII `psi_1`): functions of x like an ODE solution, with `ψ₁'(x)`, `ψ₁''(x)`, `values(ψ₁)`, `times(ψ₁)` (the grid) and `plot ψ₁ vs x, ψ₂ vs x`. Each is normalised, ∫ψ² dx = 1 (so ψ has units 1/√length), and its first lobe (from the left) is positive.
- **Boundary conditions:** ψ = 0 at both ends of the range (a hard wall, or far enough into the forbidden region that ψ has died away: check that the energies don't change when you widen the range).
- **Methods:** `using matrix` (the default) and `using shooting`, after `lowest N`:
  - *matrix*: finite differences on a grid of 2 × 2000 intervals (set with `grid 4000`, which doubles it), a symmetric tridiagonal matrix, and LAPACK for the N lowest eigenvalues. The grid is solved at three spacings and Richardson-extrapolated, so smooth potentials give ~10⁻¹⁰ relative accuracy. A jump in V between grid points (a finite well) is located and averaged over its cell; there the accuracy is ~10⁻⁶.
  - *shooting*: Numerov's method from the left end, counting nodes to pick the n-th state, and a root finder for ψ(b) = 0. An independent method, useful as a cross-check (slower).
- The equation may be written in any linear form (`ψ'' = 2m(V - E)/ħ² ψ` works too). A term with ψ' isn't supported yet (for a radial equation, use u = r R), and the eigenvalue must multiply ψ with a coefficient of one sign (`E ψ`, as in Schrödinger's equation).
- These run in Python (NumPy and SciPy, like `using radau`), so `fermium build` refuses them for now.


### Partial differential equations: heat, waves, Schrödinger

With two ranges, `for x from a to b, t from t0 to t1`, `solve` takes a **PDE** for an unknown u(x, t). Write the derivatives with ∂ (ASCII `partial`): `∂u/∂t`, `∂²u/∂x²`, `∂u/∂x`. The conditions after `with` are the initial value `u(x, t0) = …` and a boundary condition at each end, either a value `u(a, t) = …` or a slope `∂u/∂x(a, t) = …` (0 for an insulated end); both may depend on t.

```fermium
L = 1 m
D = 0.01 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 2 K * sin(π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 10 s
print u(0.5 m, 10 s), "  exact:", 2 K exp(-D π² 10 s / L²)
print ∂u/∂x(0 m, 5 s), ∂u/∂t(0.5 m, 5 s)
```

- **Using the solution:** `u(x, t)` anywhere in the range (cubic interpolation in x, Hermite in t), `∂u/∂x(x, t)`, `∂u/∂t(x, t)`, integrals like `∫ u(x, 2 s) dx from 0 m to L`, and plots of a formula like `plot u(x, 2 s) vs x from 0 m to L`.
- **Pictures:** `plot u vs x` draws u at 6 times in one plot; `plot u vs x animate over t to "heat.gif"` writes an animated GIF (with `frames 30` to choose the number of frames, 60 by default). With a file name that does not end in `.gif` (or without the pillow package) it writes the frames as PNG files in a folder, `heat_frames/`.
- **Waves:** an equation with `∂²u/∂t²` also needs the initial velocity `∂u/∂t(x, t0) = …`:

```fermium
c = 2 m/s
f(x) = 1 cm * exp(-((x - 0.5 m) / 0.05 m)^2)
solve ∂²u/∂t² = c² ∂²u/∂x²
    with u(x, 0 s) = f(x), ∂u/∂t(x, 0 s) = 0 m/s, u(0 m, t) = 0 m, u(1 m, t) = 0 m
    for x from 0 m to 1 m, t from 0 s to 0.1 s
    grid 1000
print u(0.7 m, 0.1 s), "  d'Alembert:", (f(0.5 m) + f(0.9 m)) / 2
```

- **The Schrödinger equation:** `i` in the equation is the imaginary unit, and a complex initial value is written `A(x) exp(i φ(x))`. The solution is complex, so `ψ(x, t)` is the 2-vector <Re ψ, Im ψ>: `|ψ(x, t)|^2` is the probability density, and `ψ(x, t).x`, `ψ(x, t).y` are the real and imaginary parts. An animation shows |ψ|².

```fermium
m = m_e
σ = 1 nm
k0 = 2 / (1 nm)
solve i ħ ∂ψ/∂t = -ħ²/(2*m) * ∂²ψ/∂x²
    with ψ(x, 0 fs) = (2π σ²)^(-1/4) exp(-x² / (4σ²)) exp(i k0 x), ψ(-40 nm, t) = 0 nm^(-1/2), ψ(40 nm, t) = 0 nm^(-1/2)
    for x from -40 nm to 40 nm, t from 0 fs to 30 fs
    grid 2000
print "norm:", ∫ |ψ(x, 30 fs)|^2 dx from -40 nm to 40 nm
print "centre:", ∫ x |ψ(x, 30 fs)|^2 dx from -40 nm to 40 nm, "  (ħ k0 t / m =", ħ k0 30 fs / m in nm, ")"
```

- **Methods:** second-order differences in x on `grid N` intervals (400 by default), and in t:
  - first order in t: **Crank–Nicolson** (the default: second order, stable for any step, and it conserves ∫|ψ|² exactly for the Schrödinger equation), `using implicit` (backward Euler: first order, very robust) or `using explicit` (forward Euler: needs dt ≤ h²/(2D), which Fermium checks and chooses by default). The default is 1000 time steps; `t from 0 s to 10 s step 1 ms` sets the step.
  - second order in t (waves): the explicit central-difference scheme. It needs c dt ≤ h (Courant number ≤ 1); by default Fermium takes the largest such step, where it is exact for a constant wave speed.
- **What is supported:** one unknown; linear equations (each term has one factor u, ∂u/∂x or ∂²u/∂x², like `D ∂²u/∂x² - k u + S(x)`); coefficients that depend on x but not on t (a source term without u may depend on t). All units are checked before the program runs, like every other equation.
- These run in Python (NumPy/SciPy), so `fermium build` refuses them for now. A narrow feature in a wide range can be missed by `∫` (§19): integrate over the part where the solution lives.
