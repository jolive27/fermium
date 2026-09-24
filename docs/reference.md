# Fermium Language Reference

Fermium is a programming language for physics. Numbers carry units, and the compiler checks those units **before your program runs**. Calculus (derivatives, integrals, differential equations) is part of the language. Programs are compiled to native code with LLVM.

Every example on this page is tested (`tests/test_docs.py` runs each block marked with `fermium`).

## Contents
1. [Running programs](#1-running-programs)
2. [Numbers and units](#2-numbers-and-units)
3. [Variables and formulas](#3-variables-and-formulas)
4. [Printing](#4-printing)
5. [Functions](#5-functions)
6. [Conditions and loops](#6-conditions-and-loops)
7. [Lists](#7-lists)
8. [Derivatives](#8-derivatives)
9. [Integrals](#9-integrals)
10. [Differential equations: solve](#10-differential-equations-solve)
11. [Data: load, fit, plot](#11-data-load-fit-plot)
12. [Symbols and their ASCII spellings](#12-symbols-and-ascii-spellings)
13. [Built-in functions](#13-built-in-functions)
14. [Constants](#14-constants)
15. [Units](#15-units)
16. [Errors](#16-errors)
17. [Tools](#17-tools)
18. [Grammar summary](#18-grammar-summary)

---

## 1. Running programs

```
fermium run pendulum.fm      # run a program (or just: fermium pendulum.fm)
fermium                      # interactive prompt (REPL)
fermium check pendulum.fm    # check units without running
fermium fmt pendulum.fm --pretty   # ASCII -> symbols;  --ascii for the reverse; -w rewrites the file
fermium doctor               # check the installation
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
- Units in brackets are always units: `3 [m/s]`, `x [m]`.
- Anywhere else, a name is a variable. `m v` is m times v.
- A unit expression continues with `/` (`m/s`), with a space (`N m`), or with `·`. Exponents are written `m²` or `m^2`, and `s⁻¹` or `s^-1`.
- If a unit name after a number is also one of your variables (for example `0.2 m` when you have a mass `m`), Fermium warns you once and explains how to write the other meaning (`2*m`).

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

  So `h c / λ k_B T` means (h c)/(λ k_B T). Also, `1/2 m v²` means 1/(2 m v²), and Fermium warns you. Write `½ m v²` instead.
- **A variable keeps its units:** assigning a time to a variable that held a length is an error.
- **Update in place:** `x += 1 m`, `x -= ...`, `x *= 2`, `x /= 2`.
- **`where`:** gives names to values used in just one line:

```fermium
E = ½ m v² where m = 2 kg, v = 3 m/s
print E
```

- **Exponents:** an exponent must be a fixed number when the base has units. `x^2` and `x^(1/3)` are fine. `x^n` requires x to be a plain number.
- **`e` is the elementary charge.** For the exponential function, write `exp(x)`.

## 4. Printing

```fermium
g = 9.81 m/s²
print g
print g in ft/s²
print "g is", g
print to(g, km/hr^2)
```

- `print a, b, c` prints the values separated by spaces.
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
```

- **Ranges include both ends:** `for i from 1 to 10` runs 1, 2, …, 10. With units, a `step` is required.
- **Loop control:** `break` and `continue` work in loops.
- **Conditions:** comparisons are `==`, `!=` (`≠`), `<`, `>`, `<=` (`≤`), `>=` (`≥`) and `~=` (`≈`, "equal to within 10⁻⁶ relative"). Combine them with `and`, `or` and `not`.
- **`assert condition, "message"`** stops the program if the condition is false.

## 7. Lists

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
- **Setting an element:** `xs[i] = value`.
- **Looping:** `for x in xs`.

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

- **Syntax:** `x'`, `x''`, `d/dt x` and `d²/dt² x` differentiate a one-line function. The result is a new function, whose units are the numerator's units divided by the denominator's.
- **Printing a function** shows its formula and units, for example `v(t) = -A ω sin(ω t)   [m/s, for t in s]`.
- **Formulas:** `d/dt (formula)` also works on a formula in `t`. The derivatives are exact and symbolic (sum, product, quotient and chain rules, and all the standard functions), then simplified.
- **Partial derivatives:** `∂/∂x f` (ASCII `partial/partial x f`) differentiates a function of several variables with respect to one parameter.

## 9. Integrals

```fermium
k = 50 N/m
F(x) = k x
W = ∫ F(x) dx from 0 m to 0.2 m
print W
print integral exp(-x^2) dx from -inf to inf
```

- **Definite integrals** are computed numerically with adaptive Gauss–Kronrod quadrature (G7/K15, relative tolerance 10⁻¹⁰), compiled to native code. Infinite limits (`∞` or `inf`) work.
- **Units:** the result's units are the integrand's units times the variable's units.
- **Integrals without limits** (`∫ x² dx`) are done symbolically with SymPy and give a function.

## 10. Differential equations: solve

```fermium
m = 0.5 kg
k = 50 N/m
b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 0.1 m, x'(0) = 0 m/s
  for t from 0 s to 5 s
print x(5 s)
print x'(1 s)
```

- **Writing the equation:** use primes (`x'`, `x''`) or `d/dt x`. Each equation is solved for its highest derivative automatically, and it must appear linearly.
- **Systems:** separate equations with commas or `and`, or put them on the indented lines below `solve`. For example: `solve x' = -a x, y' = a x - b y with ...`
- **Initial conditions:** every unknown needs one, and so does every derivative below the highest. They determine the unknowns' units, and both sides of every equation are unit-checked.
- **Methods:**
  - Without `step`, Fermium uses adaptive Dormand–Prince RK45 (relative tolerance 10⁻⁹).
  - With `step 1 ms`, it uses classic fixed-step RK4.
- **Using the result:**
  - `x(t)` gives the value at a time (interpolated), and `x'(t)` the derivative.
  - `plot x vs t` plots against time, and `plot y vs x` plots one unknown against another (an orbit or phase plot).
  - `values(x)` and `times(x)` give lists, and `x[end]` is the final value.

## 11. Data: load, fit, plot

```
data = load "pendulum.csv"       # header:  L [m], T [s]
print data.L                     # a column, as a list with units
fit T = 2π √(L / g) to data      # fits g; reports g with units
plot data.T vs data.L to "pendulum.png"
```

- **`load "file.csv"`:** reads a CSV whose header gives names and units, like `T [s]`. The header is read when the program is compiled, so the units are checked. Paths are relative to the program's folder.
- **`fit y = model to data`:** nonlinear least squares.
  - **Parameters:** the names that are not columns, constants or functions. If there are none, the names that already have values are fitted, starting from those values.
  - **Starting guesses:** set them with `with a = 2 m`.
  - **Report:** each parameter with units, a standard error, and the rms residual. Afterwards the parameters are ordinary variables.
- **Plots:** each form saves a PNG with labelled axes (units included) and prints where it was saved.
  - `plot ys vs xs` (lists)
  - `plot x vs t` (an ODE solution)
  - `plot f(x) vs x from 0 m to 1 m` (a formula)
  - `... to "file.png"` chooses the file name.
  - Several series: `plot a vs t, b vs t`.

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
| ≤ ≥ ≠ ≈ | `<=` `>=` `!=` `~=` | `\le` `\ge` `\ne` `\approx` |
| ∞ | `inf` | `\infty` |
| ε₀ | `epsilon_0` | `\epsilon\_0` |
| Å, μm, M☉ | `angstrom`, `um`, `Msun` | `\AA`, `\mu`, `\Msun` |

- **Greek names:** Greek letter names are converted segment by segment, so `omega_0` is the same name as `ω₀`.
- **Look-alike characters:** the micro sign µ becomes μ. Using two names that look the same (such as `v` and Greek `ν`) gives a warning.

## 13. Built-in functions

| Function | Meaning |
|---|---|
| `sin cos tan asin acos atan sinh cosh tanh exp ln log log10 log2 erf gamma` | need plain numbers (angles are plain numbers) |
| `sqrt cbrt abs floor ceil round sign` | keep or transform units |
| `atan2(y, x) hypot(a, b) mod(a, b) min(a, b, …) max(…) clamp(x, lo, hi)` | arguments in the same units |
| `len sum mean std min max first last cumsum diff reverse sort` | lists |
| `linspace(a, b, n) range(a, b, step) zeros(n) ones(n)` | make lists |
| `push(xs, x)` / `append` | add to a list |
| `dot(a, b) trapz(ys, xs) interp(x, xs, ys)` | list maths |
| `values(sol) times(sol)` | samples of an ODE solution |
| `to(x, unit)` | same as `x in unit` |
| `factorial(n) rand()` | |

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
| `M_sun`, `R_sun`, `L_sun`, `M_earth`, `R_earth`, `AU` | astronomy |
| `π` | pi |
| `∞` | infinity |

## 15. Units

- **SI base units:** `m g s A K mol cd`.
- **SI prefixes:** Q R Y Z E P T G M k h da d c m μ(u) n p f a z y r q, on the prefixable units.
- **Derived units:** `N J W Pa C V F Ω(ohm) S Wb T H Hz Bq Gy Sv lm lx kat`.
- **Physics:** `eV` (`keV MeV GeV`), `u`/`amu`/`Da`, `b`/`barn`, `fm`, `Å`, `erg`, `dyn`, `gauss`, `c` (as a speed unit), `Ci`.
- **Astronomy:** `au`/`AU`, `ly`, `pc` (`kpc Mpc`), `M☉ R☉ L☉` (`Msun Rsun Lsun`), `M_E R_E`, `yr`.
- **Other:** `min hr day year`, `L`, `atm bar Torr mmHg psi`, `inch ft yd mi mph kph lb lbf hp cal`, `rad sr ° arcmin arcsec rev %`.
- **Temperatures:** `K`, and `°C`/`°F` (absolute temperatures; see DECISIONS D12).
- **Names left out on purpose, because they collide with common variable names:** `h` for hour (use `hr`), `t` for tonne (use `tonne`), `G` for gauss (use `gauss`), `d` for day (use `day`).

## 16. Errors

Fermium reports problems in one line, points at the spot, and suggests a fix:

```
line 3: can't add length [m] to time [s]
    y = x + t
        ^^^^^
  hint: both sides of + and - must have the same units
```

Runtime problems (an index out of range, asking an ODE solution for a time outside its range) stop the program with a one-line message. See `bootcamp/TROUBLESHOOTING.md` for the common ones.

## 17. Tools

- **REPL:** run `fermium`. It keeps history. Type `\theta` then Tab to get θ; `\name` is also replaced when you press Enter. `:help` shows help, `:quit` leaves.
- **`fermium fmt file.fm --pretty` / `--ascii`:** converts between ASCII and symbols without changing the program's meaning.
- **`fermium doctor`:** checks the installation and explains fixes.
- **VS Code:** `editors/vscode/` adds syntax highlighting and `\name` completion.

## 18. Grammar summary

```
program    := statement*
statement  := name = expr [where binds] | name op= expr | name[expr] = expr
            | name(params) = expr | name(params) = NEWLINE INDENT block
            | print items | plot series [to "file"] | fit eq to expr [with binds]
            | solve eqs [with eqs] for t from a to b [step h]
            | if expr block [else block] | for x from a to b [step s] block
            | for x in expr block | while expr block | return expr | break | continue
            | assert expr [, "message"] | expr
expr       := if expr then expr else expr | or-expression
precedence := or < and < not < comparison < + - < * / < unary - < implicit × < ^ < postfix
postfix    := atom ( (args) | [index] | .name | ' )*
atom       := number [unit] | name | "text" | (expr) | [list] | |expr| | √atom | ∫ … d x [from a to b]
            | d/dt atom | ∂/∂x atom | load "file"
```
