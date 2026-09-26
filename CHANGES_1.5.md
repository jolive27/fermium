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

## Printing (D240–D242)

- `print h c` shows `1.99×10⁻²⁵ J m` instead of `1.99×10⁻²⁵ N m²` (an energy times a length). `print h c in eV nm` still shows `1240 eV nm`, and `in MeV fm` works too.
- A torque you write in `N m` stays in N m. A force times a length that the program computes is shown in J (Fermium can't tell a torque from work); write `in N m` if you want N m.
- `½`, `⅓` and the other fraction characters now mean exactly `(1/2)`, `(1/3)`, …: `print ½` shows `0.500` (it showed `0.5`), `print ⅓` shows `0.333` (it showed 15 digits), and `½ kg` is 0.5 kg (it was an error). New characters: ⅖ ⅗ ⅘ ⅚ ⅐ ⅜ ⅝ ⅞ ⅑ ⅒.
- `for E in [0.50 eV, 0.75 eV, 1 eV]` prints `0.50 eV` (it lost the zero and printed `0.5 eV`): each element prints the way the whole list prints.
- `[1.20 mm, 1.30 mm]` style lists keep their written zeros even after a unit conversion.

## Tools

- `fermium fmt --pretty` writes `(1/2)` as `½` (and `(3/4)` as `¾`, and so on); `--ascii` writes it back. It leaves `x^(1/3)`, calls like `f(1/2)`, and `(0.5)` alone (0.5 is a measured value with one significant figure, ½ is exact).

- `fermium fmt` says on stderr that it formatted the file and didn't run it.
