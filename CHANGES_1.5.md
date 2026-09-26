# What changed in Fermium 1.5

Fermium 1.5 fixes what John found by using Fermium 1 for real, then freezes the language so the Rust compiler
(Fermium 2) can match it exactly. This page lists every change a program or a reader can notice, and why.
Design details are in `DECISIONS.md` (D234 onwards).

## The unit rule is three sentences, and spaces never matter (D235)

1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
2. If that unit is a single name that is also one of your variables (`2 g` with your own `g`, `0.1 m` with a mass `m`), Fermium stops and asks which you mean: `2*g` for your variable, `2 [g]` for the unit.
3. In a compound unit (`3 m/s`, `2 kg m²`) the first name is always a unit; any later name that is also your variable is an error that asks the same question.

What that changes:
- `20 m/s/g` with your own `g` used to mean *per gram*, silently. Now it asks. `20 m/s / g` (with spaces) asks too: spaces never change meaning. Write `(20 m/s)/g`.
- `x(0) = 0.1 m` next to a mass `m` used to be the metre with a warning. Now it asks: write `0.1 [m]`.
- `k = 50 N/m` after a mass `m` asks (the later `m` is your variable): write `50 [N/m]`.
- `36 km / h` is an error like `36 km/h` (h is Planck's constant; write `km/hr`).
- `2 m c²` with a mass `m` asks (`c` continues a unit only after `/`, as in `938 MeV/c²`).
- **To update an old program:** `fermium fmt --fix yourfile.fm` adds the brackets and keeps what the program meant before. Editors with the Fermium extension offer the same fix.

## A fraction of plain numbers is one coefficient (D236)

- `73/24 x²` is (73/24)·x², `π²/12 t²` is (π²/12)·t², `1/2 x` is x/2.
- **Deliberate change:** `1/2 kg` is now 0.5 kg. It used to be 0.5 per kilogram.
- `4/3 π r³` asks whether you mean (4/3)·π or 4/(3π): write `(4/3) π r³`.
- `h / m_e v` still means h/(m_e v): implicit multiplication still binds tighter than `/` when a name follows.

## Clearer error messages (D237)

- `ħ = c = 1` now suggests `units natural(ħ = c = 1)`.
- Using `g` without defining it says: *g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: g = 9.81 m/s².*
- `print h c in J/m` says *h c is energy × length; try in J m or in eV nm*.
- Typing a terminal command like `fermium run ke.fm` at the `fm>` prompt tells you to type `:quit` first.
- `sqrt(-4)` written with a number is an error that suggests the complex square root `√(-4 + 0i)`.

## `≈` near zero, and choosing the tolerance with `within` (D260)

- `a ≈ b` still means "equal to within 10⁻⁶ of the larger size", so programs that compare non-zero values print what they printed.
- **New:** `x ≈ 2 m within 1 mm` checks |x − 2 m| ≤ 1 mm. The tolerance must have the same units as the values. `100 ≈ 101 within 2%` is a relative tolerance.
- **Comparing with zero:** `v ≈ 0 m/s` could only ever be true for exactly zero (10⁻⁶ of 0 is 0), so it is now a compile error that shows the fix: write `v ≈ 0 m/s within 1e-9 m/s`, with the difference you can accept.
- `≈` works for vectors (it compares lengths: `|a − b|` against `|a|` and `|b|`) as well as numbers and complex numbers.
- `∞ ≈ ∞` is true; `∞ ≈ 1e308` is now false (it used to be true).

## Under the hood

- An ODE solution used inside an integral or equation in a function is now passed to it as a pointer, not disguised as a number. Nothing prints differently; it just can't break under fast-math or flush-to-zero settings (D261).

## Tools

- `fermium fmt` says on stderr that it formatted the file and didn't run it.
- `fermium doctor` prints one command to copy when packages are missing: `python3 -m pip install -e ".[full]"` (run it in the fermium folder), instead of one `pip install` per package (D250).

## Plots (D251–D253)

- `plot saved to` shows the full path of the picture, like `plot saved to /Users/ada/lab/gallery/pendulum.png`, so you can find it whichever folder you ran the program from. (It used to show the name relative to the program's folder.) The same for animations and for programs made with `fermium build`.
- Axis labels use the column's name: `T [s]`, not `data.T [s]`. When you draw a fitted curve over your data, the y axis says `T [s]` and the legend names the curve.
- `examples/01_pendulum.fm` draws the fitted curve over the measurements: `plot data.T vs data.L, 2π √(L / g) vs L from 20 cm to 120 cm`.

## Research reproductions (D254, D255)

- The research inputs that had been typed from memory are now checked against cited sources, and the citations sit next to the data. The Geiger–Marsden table was right. Two ²⁰⁸Pb level energies moved by 5–6 keV (rms 0.476 → 0.478 MeV). One Big Bang nucleosynthesis rate coefficient was wrong: fixing it moves ⁷Li/H from 5.10×10⁻¹⁰ to 4.36×10⁻¹⁰; helium, deuterium and ³He did not change.
