# Bugs and confusing behaviour found while writing the bootcamp

Written by the bootcamp agent. Each entry: minimal repro, expected, actual. Workarounds used in the lessons are noted.

## B1. `x^2 L` : a number in an exponent grabs the following name as a unit (ASCII spelling breaks)

```
L = 1.20 m
T = 2.21 s
print 4 pi^2 L / T^2
```
- **Expected:** `9.70 m/s²` — the same as `4π² L / T²` (spec §3.4.3: the ASCII form is first-class).
- **Actual:**
```
e.fm, line 3: an exponent must be a plain number, but this is a quantity with units [m³ s⁴ A²/kg²]
    print 4 pi^2 L / T^2
               ^^^^^^^^^
```
  `^2 L / T^2` is read as `^(2 litres / tesla^2)`. Same for `v^2 m` (→ 2 metres) and `pi^2 L`. `x^2 k` works only because `k` is not a unit.
- **Suggested fix:** a number directly after `^` should never take a unit (the exponent is a bare number; `x^2 L` should be `x² · L`, as DECISIONS D8 says for `x^2y`).
- **Workaround in lessons:** write `4 pi^2 * L / T^2` or use `²`.

## B2. "unit after number" warning fires once per name, on the harmless line, and not on the dangerous one

```
m = 2 kg
v = 3 m/s
E = 0.5 m v^2
print E
```
- **Actual:** one warning on line 2 (`3 m/s`, which obviously means metres per second), then **no warning** on line 3, where `0.5 m v^2` silently means 0.5 metres × v² and prints `4.5 m³/s²`.
- **Expected:** at least warn on line 3 (the case that actually changes meaning). Ideally don't warn for `3 m/s` at all: `m/s` is unambiguous. Every beginner will define `m = ... kg` and then write speeds in m/s, so the current warning trains them to ignore warnings.
- **Workaround in lessons:** teach `½ m v²` / `0.5 * m * v^2`, and the gotcha explicitly.
