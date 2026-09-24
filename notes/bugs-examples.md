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
