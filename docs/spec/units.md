# Units and dimensions

Draft 0.1. Source: `rust/crates/fermium-units` (`dim.rs`, `exact.rs`, `db.rs`, `natural.rs`, `quantity.rs`),
`rust/crates/fermium-syntax/src/unitrule.rs` and `expr.rs` (the unit rule), `rust/crates/fermium-ir/src/types.rs`
(inference) and `rust/crates/fermium-check` (checking). Decisions: D5, D6, D12, D13, D14, D60, D235, D236, D238.

Units are checked when the program is compiled and erased afterwards: at run time every quantity is a plain
double in SI (or canonical natural) units, so units cost nothing (semantics.md §3).

## 1. Dimensions

A **dimension** is a vector of seven exponents over the SI base quantities, in this order:

| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| quantity | length | mass | time | current | temperature | amount | luminous intensity |
| base unit | m | kg | s | A | K | mol | cd |

Each exponent is an exact rational number p/q with p, q 64-bit integers (`Rational64`), always reduced. The
dimensions form a group: multiplying quantities adds exponent vectors, dividing subtracts, raising to a rational
power r multiplies every exponent by r. A **plain number** has the zero vector (*dimensionless*).

- **Angles are dimensionless** (D6): `rad` is the unit 1, `°` is π/180. So `sin`, `cos`, `exp`, `ln` and the like
  take a plain number, `30°` is 0.5236, and Hz and rad/s are the same dimension (Fermium warns when a value
  written in Hz is converted to rpm, rad/s or rev, D27, D95, but can't reject it).
- Fractional exponents are allowed: `√(2 m)` has dimension m^(1/2).

### 1.1 Exact exponent arithmetic and its limit

Every exponent computed from a program goes through checked arithmetic (`fermium-units/src/exact.rs`): the exact
result is worked out in 128-bit integers, reduced, and kept only if numerator and denominator fit in 64 bits.
Otherwise the program is rejected with one error at the innermost expression that produced it: *this unit's
power is too large to track exactly (…)*, with the hint to raise a plain number and attach the unit afterwards.
(Red team 13 #1–#2: plain `Ratio<i64>` arithmetic wrapped silently or panicked; see rust/DIVERGENCES.md.)

A written exponent is turned into a fraction by the best rational approximation with denominator at most 10000
(Python's `Fraction.limit_denominator(10000)`; `fermium-check/src/arith.rs`), so `x^0.5` is `x^(1/2)`. An exponent
applied to a quantity with a dimension must be a compile-time constant: a literal, or `+ - * /` of literals.
`x^n` with a variable `n` requires `x` to be dimensionless ("can't raise length [m] to a power that isn't a fixed
number").

```fermium
x = 2 m
print x^(1/2), (x^2)^(1/2), x^0.5
```

## 2. The unit rule

Physicists use `m` for a mass and `g` for gravity; Fermium also has the units metre and gram. One rule decides
whether a name is a unit or a variable. The bootcamp states it in three sentences
(bootcamp/lesson02_variables_formulas.md, quoted verbatim):

> 1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
> 2. If that unit is a single name that is also one of your variables (`2 g` with your own `g`), Fermium stops and asks which you mean: `2*g` for your variable, `2 [g]` for the unit.
> 3. In a longer unit (`3 m/s`, `2 kg m²`) the first name is always a unit; a later name that is also your variable gets the same question.
>
> Anywhere else, a name is your variable, and spaces never change the meaning.

### 2.1 Precise statement

Definitions:

- A **digit literal** is a NUM token written with decimal digits (`3`, `9.81`, `6.674×10⁻¹¹`), optionally raised
  by a superscript or `^` (`10² m`). A vulgar fraction (`½`), `π` and a name are not digit literals.
- **Your variables** at a point of the program (the parser's set `known`) are the names assigned, defined as a
  function, used as a parameter or as a loop variable *before* that point in program order, the parameters of the
  function being defined, the bindings of an enclosing `where` that come before (§2.2), the variable of an
  enclosing `d/dx` or `∂/∂x`, the integration variable of an enclosing `∫ … dx`, and the unknowns of an
  enclosing `solve` inside its equations. In the REPL, the names of earlier inputs.
- A **unit name** is a name in the unit catalogue (§3), possibly with an SI prefix.

The rule (D235, D238):

1. **Explicit positions.** Inside brackets `[ … ]` (after a number, after an expression, or in a parameter
   `x [m]`) and after `in`, every name is a unit. Brackets are how one writes a unit that collides with a
   variable: `2 [g]`.
2. **Right after a digit literal**, a unit name starts a *unit expression* (grammar.md §2.5). If the unit
   expression is a single name that is one of your variables, the program is rejected:
   *'2 g' is ambiguous: right after a number, g is a unit (grams), but g is also your variable g*, with the hint
   *write 2\*g for 2 × your variable g, or 2 [g] for the unit*.
3. **Continuing a unit.** The unit expression continues through spaces, `·` and `/` with a following unit name.
   It ends at an explicit `*`, at a name followed immediately by `(`, at `c` except right after `/`, and at
   anything that isn't a unit name. If a later name of the unit expression is one of your variables, the program
   is rejected with the same question (whatever the spacing: `20 m/s/g` and `20 m/s / g` both ask).
4. **Reciprocal units.** Right after a digit literal, `/` followed by a unit name that isn't your variable makes a
   reciprocal unit: `0.5 /s`, `0.5/s` and `0.5 / s` are all 0.5 per second. `/` followed by your variable divides.
   A bracket after `/` that mixes your variables and units asks (`0.300 /(m s²)` with your `m`).
5. **Everywhere else a name is a variable** (or a function or constant). A unit name that is not defined as
   anything is then an error: *kg isn't defined*, hint *kg is a unit; units go right after a number, like 1 kg,
   or in brackets [kg]*. Two exceptions read a unit that no variable, function, parameter or constant anywhere
   in the program shares: after a bracket holding a pure number (`(1/2) kg`) and after a vulgar fraction
   (`½ kg`); after any other bracket such a unit is an error asking for brackets, `(51 - 33 (N-Z)/A) [MeV]`.
6. **Spacing never changes which reading applies.** (It does decide calls and indexing, grammar.md.)
7. A list or vector literal takes a unit like a number: `[1, 2, 3] m`, `<3, 4> m/s`.

Names left out of the catalogue on purpose because they collide with common variables: `h` (hour; write `hr`),
`t` (tonne), `G` (gauss), `d` (day). `36 km/h` is an error that says `h` is Planck's constant (D180).

```fermium
g = 9.81 m/s^2
h = 20 m
print 2*g*h, 2 [g]
print 0.5 /s, 50 N/m
```

```fermium-error
g = 9.81 m/s^2
h = 20 m
print 2 g h
```

### 2.2 `where`, `solve` and calculus

- In `e where a = …, b = …`, the names `a`, `b` are your variables inside `e`; a binding's own right side sees
  only the bindings before it: `ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg` is valid.
- In a `solve`, a name written with a prime anywhere in it is an unknown, and is your variable inside the
  equations but not in the `with` clause (D211).
- In `d/dx f` and `∂/∂x f`, `x` is your variable inside the operand (D231); in `∫ … dx`, `x` is your variable in the
  integrand, and a trailing `dx` is never a unit (decimetre·x).

```fermium
KE = ½ m v² where m = 2 kg, v = 3 m/s
print KE
```

## 3. The unit catalogue

The catalogue (`fermium-units/src/db.rs`, the generated table `fermium-syntax/src/tables.rs`) maps each unit name to
a dimension and a factor to SI (and, for `°C` and `°F`, an offset). It holds the SI base and derived units
(`N J W Pa C V F Ω S Wb T H Hz Bq Gy Sv lm lx kat`), physics, astronomy and everyday units, and constants that
double as units after a number (`c`). The list is in docs/reference.md §15; this specification treats it as data.

- **Prefixes:** `Q R Y Z E P T G M k h da d c m μ u n p f a z y r q` apply to units marked prefixable. Of the
  2022 prefixes only `Rg Qg qg Qm` exist (D174). A few prefixed names are blocked because the unprefixed reading
  is the useful one (`Gs Pa cd dam ft ha mi min nmi pc` are their own units, not prefixed ones).
- `u` is both the micro prefix (ASCII `um` = μm) and the atomic mass unit.
- A name that is a unit and could split into a prefix and a unit that are both your variables (`1.5 kT` with your
  `k` and `T`) is the unit, with a warning (D203).

## 4. Unit expressions and conversion

A unit expression (grammar.md §2.5) denotes a *display unit*: a dimension, a factor to SI and a text. `3 km` is
the SI value 3000 with dimension length and display unit `km`.

**Conversion** `expr in unit` requires the dimension of `expr` to equal the unit's dimension ("can't show length
[m] in s") and changes only how the value is printed: `v in km/hr` is still the SI value. `in` is also the way a
value leaves a natural-units region (§6). A `print … in unit to N digits` rounds the displayed value.

```fermium
v = 3 m/s
print v in km/hr, v in km/hr to 2 digits
print 1 eV in J, 1 u c^2 in MeV, 90° in rad
```

**Arithmetic rules** (checked on dimensions):

| Operation | Rule |
|---|---|
| `a + b`, `a - b`, comparison, `≈`, both branches of `if`, list elements | dim(a) = dim(b) |
| `a * b`, `a / b`, implicit product | dimensions multiply / divide |
| `a ^ r` | r a compile-time rational constant if dim(a) ≠ 1; dim(a)·r |
| `√a`, `∛a` | dim(a)/2, dim(a)/3 |
| `sin`, `exp`, `ln`, … | argument dimensionless |
| assignment to an existing variable | same dimension as before (D13; the REPL may redefine) |
| `x' `, `d/dt x` | dim(x)/dim(t) |
| `∫ f dx` | dim(f)·dim(x) |
| `a ± b` | dim(a) = dim(b) |

## 5. Affine temperatures

`°C` and `°F` are *affine*: `20 °C` is the absolute temperature 293.15 K (D12). Inside a compound unit a degree
is a step (no offset): `2 °C/min`, `4.18 J/(g °C)`. Rules (D12, D181):

- `a - b` with `b` in °C/°F is a temperature *difference*, shown in K: `300 K - 20 °C` is 6.85 K.
- `T + ΔT` with `ΔT` in K keeps the °C display: `10 °C + 5 K` is 15 °C.
- `°C + °C`, negating a °C value and `sum` of a list in °C are errors; `2 T` and `T / 2` of a °C value warn.
- A temperature change written in °C to a name starting with `Δ`, `δ`, `delta` or `dT` is an error.
- `-40 °C` is minus forty degrees, not −(233.15 K).

```fermium
print 20 °C in K, 300 K - 20 °C, 10 °C + 5 K
```

```fermium-error
print 20 °C + 20 °C
```

## 6. Natural units

`units natural(ħ = c = 1)` (or `units natural`), `units natural(ħ = c = k_B = 1)`, `units natural(G = c = 1)`,
`units nuclear` (ħ = c = 1 shown in MeV and fm), `units astro` (SI checking, astronomical display) and `units SI`
switch the unit system for the rest of the program, or for an indented block after `:` (D60). Any independent
subset of {ħ, c, k_B, G, ε₀} may be set to 1.

**Semantics (D60).** For a set S of constants set to 1, every SI dimension D splits uniquely and exactly (a 7×7
rational solve) as D = Σᵢ aᵢ dim(Cᵢ) + Σⱼ βⱼ Bⱼ over the constants Cᵢ ∈ S and the kept base dimensions Bⱼ.
The *canonical* dimension is Σ βⱼ Bⱼ and a quantity is stored as φ(x) = SI value · Πᵢ Cᵢ^(−aᵢ). φ is a group
homomorphism, so ordinary dimension checking on canonical dimensions *is* checking modulo the constants: with
ħ = c = 1 a mass and an energy have the same canonical dimension, length and time are both 1/energy, and
length + energy is still an error. Boundaries: an SI value used inside a region converts automatically; a value
computed inside a region can leave it only through `x in unit` (natural → SI is not unique); `units` lines are
top-level only.

```fermium
units natural(ħ = c = 1)
m_π = 139.57 MeV
print 1/m_π in fm
```

## 7. Dimension inference

Users almost never write a unit annotation (D5). The checker infers dimensions by unification over **dimension
expressions** (`fermium-ir/src/types.rs`):

- A dimension expression is `D₀ · x₁^c₁ · … · xₙ^cₙ`, a concrete dimension times rational powers of *dimension
  variables*, written additively on exponents: `DExpr { konst, terms }`.
- Fresh variables are created for values of unknown dimension: a literal `0`, an empty list, a function
  parameter without a bracketed unit, a recursive call's result, the parameters of a `fit`.
- Every rule of §4 that requires two dimensions to be equal calls `unify(a, b)`. It normalises `a / b` under the
  current substitution. If no variables remain, it succeeds exactly when the result is dimensionless.
  Otherwise it picks the variable with the simplest coefficient (|c| = 1 first, then the oldest variable) and
  solves `c·x + rest = 0` for `x = rest^(−1/c)`, adding it to the substitution. This is Kennedy-style
  inference for dimension types: unification in the free abelian group, where every equation is linear in
  the exponents.
- A variable never constrained resolves to dimensionless.
- **Functions are monomorphised** (D14): `F(x) = k x` is checked again for each combination of argument types
  and dimensions it is called with, and each instance is compiled separately. Parameters with a bracketed unit
  (`f(x [m])`) are checked against it at each call.
- **Variables keep their dimension** (D13): after `E = 0` (a fresh variable), `E += ½ m v²` fixes E to energy,
  and a later `E = 3 s` is an error.

```fermium
E = 0
E += 3 J
print E
```

```fermium-error
x = 3 m
x = 2 s
```

## 8. TODO for this chapter

- The complete unit catalogue as a normative table (generated from `db.rs`).
- The rules for `Hz`/`rad/s`/`rev`, `Gy`/`Sv`, `J`/`N m` warnings (D27, D95, D306, D324) as a table.
- Units of the built-in functions (min/max/mean/…; linear algebra; FFT) and of `fit` results.
- Mixed-unit vectors (one dimension per component).
