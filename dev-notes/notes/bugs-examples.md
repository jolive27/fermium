# Bugs and limitations found while writing examples/

Reported by the examples agent. Each entry: minimal repro, expected, actual, and the workaround used (if any).
Status re-checked against the compiler at the end of the examples work: [FIXED] = repro now behaves as expected; workarounds for fixed items were removed from the examples.

## 1. [FIXED] (minor) Values read from an ODE solution ignore significant figures
```
solve x' = -x / (2.0 s) with x(0) = 1.0 m for t from 0 s to 1 s
print x(1 s)
print 1.0 m * exp(-0.5)
```
- Expected: both print with 2 significant figures (inputs have 2), e.g. `0.61 m`.
- Actual: `x(1 s)` prints 6 significant figures (`0.606531 m`), the formula prints `0.61 m`. Looks inconsistent next to each other in examples (e.g. 02_projectile: "range 64.8579 m").
- Workaround: none needed (cosmetic).

## 2. [FIXED] (feature) `plot` always uses SI base units on the axes; no way to plot in AU, fm, MeV, ...
```
solve x'' = -x/(1 s)^2, y'' = -y/(1 s)^2 with x(0) = 1 AU, y(0) = 0 AU, x'(0) = 0 AU/s, y'(0) = 1 AU/s for t from 0 s to 7 s
plot y in AU vs x in AU to "orbit.png"
```
- Expected: axes labelled `x [AU]`, `y [AU]` (either via `in`, or by keeping the unit the user wrote, as `print` does).
- Actual: parse error "expected 'vs' ... but found 'in'"; `plot y vs x` labels the axes in m with ×10¹¹ offsets.
- Workaround in examples: divide by the unit (`xs / (1 AU)`) to plot plain numbers and say the unit in a comment, or live with SI axes.

## 3. [FIXED] (bug) `plot f(x) vs x from a to b` — the form documented in docs/reference.md §11 — fails
```
f(x) = 3 x^2
plot f(x) vs x from 0 m to 1 m to "e.png"
```
- Expected: plots the formula (reference.md §11 lists "`plot f(x) vs x from 0 m to 1 m` (a formula)").
- Actual: `line 2: x isn't defined`. Same for `plot x^2 vs x from 0 m to 1 m`.
- `plot f vs x from 0 m to 1 m` (bare function name) works.
- Workaround in examples: `plot B vs λ from 50 nm to 3000 nm` (06_blackbody, 09_binding_energy...).

## 4. [FIXED] (usability) The display unit is lost when a value comes back from a user function
```
a = 15.75 MeV
A = 56
print a * A        # 882.0 MeV   (good)
f(A) = a A
print f(56)        # 1.413×10⁻¹⁰ J   <- expected 882.0 MeV
```
- Expected: the function result keeps the display unit the user wrote (MeV), as `print a * A` does.
- Actual: falls back to SI (J). In nuclear-physics examples every function result then needs `in MeV`.
- Workaround in examples: `print ... in MeV` everywhere.

## 5. [FIXED] (cosmetic) `round`, `floor`, `ceil` results print with a trailing `.0`
```
print round(25.2), round(25.2) + 1      # 25.0 26.0   expected: 25 26
```

## 6. [FIXED] (minor) Python traceback on a closed stdout pipe
```
fermium run examples/09_binding_energy.fm | head -1
```
- Expected: quiet exit (like other CLI tools), no traceback.
- Actual: several "Exception ignored on calling ctypes callback ... BrokenPipeError" tracebacks, then "internal error in Fermium: BrokenPipeError".

## 7. [OPEN (backlog)] (feature) No lists of text
```
names = ["H-1", "He-4", "C-12"]
```
- Actual: `a list element must be a number, but it is text`.
- Would be nice for labelled tables (isotope names). Workaround in examples: print `A =` numbers instead of names, or write one `print` per isotope.

## 8. [FIXED] (usability) The `4/3 π r³` warning is not shown when the same program then fails the unit check
```
m_u2 = 1.0 kg
ρ(A) = A m_u2 / (4/3 π (1.2 fm)³)
print ρ(56) in kg/m³
```
- Expected: the D8 warning ("this is read as a/(b c) ...") is printed together with the unit error, since it explains it. Ideally the hint would show the user's own text: "write (4/3) π r³".
- Actual: only `can't show a quantity with units [kg m³] in kg/m³`; the warning (which does appear when the program is otherwise valid) is dropped. A beginner has no clue why the units are upside down.
- Workaround in examples: write `4π/3 R³`... careful: that is also 4π/(3 R³)! Use `(4/3) π R³` or `4/3*π*R³`.

## 9. [FIXED] (bug, important for nuclear physics) `e²` is rejected as if it were exp
```
print e² / (4π ε_0) in MeV fm
```
- Expected: 1.43996 MeV fm. `e²/(4πε₀)` is one of the most common expressions in nuclear/atomic physics. D16 only says `e^x` (a *variable/non-constant* exponent) should be an error. A fixed numeric exponent (`e²`, `e^2`, `(e)^2`) can only mean the charge squared (exp(2) would be dimensionless and nobody writes it as e²... and even if they did, the units check would catch it).
- Actual: `line 1: e is the elementary charge (1.602×10⁻¹⁹ C) in Fermium ... hint: for the exponential function write exp(x)`.
- Workaround in examples: `e e` / `e*e` (12_coulomb_barrier, 19_rutherford, 23_alpha_decay).

## 10. [FIXED] (cosmetic) Charge squared prints as `s² A²` instead of `C²`
```
print e e       # 2.56697×10⁻³⁸ s² A²
```

## 11. [FIXED] (usability) Unhelpful error when `2 T[i]` reads T as tesla
```
T = [1 K, 2 K, 3 K]
print 2 T[2]
```
- Expected: an error/hint saying "`2 T` here means 2 tesla (a unit right after a number); write 2*T[2] to use your variable T".
- Actual: `only lists can be indexed with [...]` + `hint: to call a function use parentheses: f(x)`, which sends the beginner the wrong way. The unit/variable warning (D7 rule 5) is not shown either (see #8: warnings are dropped when there's an error).
- Workaround in examples: `2*T[i]` (17_heat_equation).

## 12. [OPEN] (minor) `fit ... with` can't continue on the next line (unlike `solve ... with`)
```
data = load "data/ba137m_decay.csv"
fit rate = R0 exp(-ln(2) t / t_half) + B to data
  with R0 = 80 s⁻¹, t_half = 100 s, B = 1 s⁻¹
```
- Expected: same layout as `solve` (reference.md §10 shows `with` on an indented line).
- Actual: `this line is indented but isn't inside a block`.
- Workaround: put `with` on the same line (18_fit_decay_data).

## 13. [OPEN] (feature) `plot` can only draw lines: no markers for measured data, no log axis
- `plot data.rate vs data.t, model(data.t) vs data.t` draws the noisy data as a zig-zag line. Measured points are normally drawn as dots (with the fit as a line), and decay data on a log-y axis. Something like `plot data.rate vs data.t as points` / `log y` would help. (BACKLOG already lists log scale.)

## 14. [OPEN] (cosmetic) Look-alike normalisation also rewrites text inside strings
```
print "Pound–Rebka"       # prints Pound-Rebka (en dash replaced by a hyphen)
```
- Expected: string literals printed exactly as written. (D10 normalisation should apply to code, not text.)

## 15. [OPEN] (cosmetic) List elements lose their significant figures
```
for ρ in [1.0e7 kg/m³, 1.0e8 kg/m³]
    print ρ, 2 ρ
```
- Expected: `1.0×10⁷ kg/m³ 2.0×10⁷ kg/m³` (literal printed as written, D11).
- Actual: `1×10⁷ kg/m³ 2×10⁷ kg/m³`, and results computed from list elements print 6 significant figures.

## Observation (not a bug): the "unit right after a number" rule bit the examples author 7 times
All were caught by the unit checker (good!), but each cost a minute even for an experienced programmer, so a beginner will hit them constantly:
- `γ = b / (2m)` → 2 metres (03_damped_spring); `p(m, E) = √(2 m E)` inside a function whose *parameter* is `m` (21_compton_debroglie)
- `√(2 W / (1 kg))` → 2 watts (05_escape_velocity)
- `√(2K / m_e)` with a kinetic energy `K` → 2 kelvin (15_relativity)
- `exp(-8849 m / H)` with a scale height `H` → metres per henry (14_hydrostatic_equilibrium)
- `2 T[i]` → 2 tesla (17_heat_equation)
- `4/3 π R³` → 4/(3πR³) (10_nuclear_radius, D8 precedence)
The warnings + hints (now shown together with the error, #8) make them fixable. A possible further step: when a unit right after a number is *also* a parameter/variable in scope and the unit reading leads to a unit error, say so in the error itself ("did you mean 2*m? `2m` is read as 2 metres").
