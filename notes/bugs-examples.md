# Bugs and limitations found while writing examples/

Reported by the examples agent. Each entry: minimal repro, expected, actual, and the workaround used (if any).

## 1. (minor) Values read from an ODE solution ignore significant figures
```
solve x' = -x / (2.0 s) with x(0) = 1.0 m for t from 0 s to 1 s
print x(1 s)
print 1.0 m * exp(-0.5)
```
- Expected: both print with 2 significant figures (inputs have 2), e.g. `0.61 m`.
- Actual: `x(1 s)` prints 6 significant figures (`0.606531 m`), the formula prints `0.61 m`. Looks inconsistent next to each other in examples (e.g. 02_projectile: "range 64.8579 m").
- Workaround: none needed (cosmetic).

## 2. (feature) `plot` always uses SI base units on the axes; no way to plot in AU, fm, MeV, ...
```
solve x'' = -x/(1 s)^2, y'' = -y/(1 s)^2 with x(0) = 1 AU, y(0) = 0 AU, x'(0) = 0 AU/s, y'(0) = 1 AU/s for t from 0 s to 7 s
plot y in AU vs x in AU to "orbit.png"
```
- Expected: axes labelled `x [AU]`, `y [AU]` (either via `in`, or by keeping the unit the user wrote, as `print` does).
- Actual: parse error "expected 'vs' ... but found 'in'"; `plot y vs x` labels the axes in m with ×10¹¹ offsets.
- Workaround in examples: divide by the unit (`xs / (1 AU)`) to plot plain numbers and say the unit in a comment, or live with SI axes.

## 3. (bug) `plot f(x) vs x from a to b` — the form documented in docs/reference.md §11 — fails
```
f(x) = 3 x^2
plot f(x) vs x from 0 m to 1 m to "e.png"
```
- Expected: plots the formula (reference.md §11 lists "`plot f(x) vs x from 0 m to 1 m` (a formula)").
- Actual: `line 2: x isn't defined`. Same for `plot x^2 vs x from 0 m to 1 m`.
- `plot f vs x from 0 m to 1 m` (bare function name) works.
- Workaround in examples: `plot B vs λ from 50 nm to 3000 nm` (06_blackbody, 09_binding_energy...).

## 4. (usability) The display unit is lost when a value comes back from a user function
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

## 5. (cosmetic) `round`, `floor`, `ceil` results print with a trailing `.0`
```
print round(25.2), round(25.2) + 1      # 25.0 26.0   expected: 25 26
```

## 6. (minor) Python traceback on a closed stdout pipe
```
fermium run examples/09_binding_energy.fm | head -1
```
- Expected: quiet exit (like other CLI tools), no traceback.
- Actual: several "Exception ignored on calling ctypes callback ... BrokenPipeError" tracebacks, then "internal error in Fermium: BrokenPipeError".

## 7. (feature) No lists of text
```
names = ["H-1", "He-4", "C-12"]
```
- Actual: `a list element must be a number, but it is text`.
- Would be nice for labelled tables (isotope names). Workaround in examples: print `A =` numbers instead of names, or write one `print` per isotope.

## 8. (usability) The `4/3 π r³` warning is not shown when the same program then fails the unit check
```
m_u2 = 1.0 kg
ρ(A) = A m_u2 / (4/3 π (1.2 fm)³)
print ρ(56) in kg/m³
```
- Expected: the D8 warning ("this is read as a/(b c) ...") is printed together with the unit error, since it explains it. Ideally the hint would show the user's own text: "write (4/3) π r³".
- Actual: only `can't show a quantity with units [kg m³] in kg/m³`; the warning (which does appear when the program is otherwise valid) is dropped. A beginner has no clue why the units are upside down.
- Workaround in examples: write `4π/3 R³`... careful: that is also 4π/(3 R³)! Use `(4/3) π R³` or `4/3*π*R³`.
