# Fermium cheat sheet

**Three ways to type a symbol:** the ASCII spelling (always works) · `\name` then **Tab** in the REPL or VS Code · Mac **Option** key. `fermium fmt file.fm --pretty` turns ASCII into symbols; `--ascii` turns them back.

### Symbols

| Symbol | ASCII | `\name` Tab | Mac |
|---|---|---|---|
| π | `pi` | `\pi` | ⌥P |
| √x | `sqrt(x)` | `\sqrt` | ⌥V |
| ∛x | `cbrt(x)` | `\cbrt` | |
| ∫ | `integral` | `\int` | ⌥B |
| ∂ | `partial` | `\partial` | ⌥D |
| ∞ | `inf` | `\infty` `\inf` | ⌥5 |
| ± | `+-` (uncertainty: `1.20 ± 0.01 m`) | `\pm` | ⌥⇧= |
| ħ | `hbar` | `\hbar` | |
| ° | `deg` | `\deg` `\degree` | ⌥⇧8 |
| °C | `degC` | `\celsius` `\degC` | |
| Å | `angstrom` | `\AA` `\angstrom` | |
| μ (micro) | `u` (`um`, `uF`) | `\mu` `\micro` | ⌥M |
| M☉ ☉ | `Msun` | `\Msun` `\sun` `\odot` | |
| ½ | `(1/2)` | `\half` | |
| · × | `*` | `\cdot` `\times` | |
| ≤ ≥ | `<=` `>=` | `\le` `\ge` (`\leq` `\geq`) | ⌥, ⌥. |
| ≠ ≈ | `!=` `~=` | `\ne` `\approx` (`\neq`) | ⌥= ⌥X |
| ′ | `'` | `\prime` | |
| x² x³ x⁻¹ | `x^2` `x^3` `x^-1` | `\^2` `\^3` `\^-1` (`\^0`…`\^9`, `\^-`) | |
| x₀ x₁ | `x_0` `x_1` | `\_0` … `\_9` | |

**Greek letters:** type the name. α `alpha` β `beta` γ `gamma` δ `delta` ε `epsilon` ζ `zeta` η `eta` θ `theta` ι `iota` κ `kappa` λ `lambda` μ `mu` ν `nu` ξ `xi` π `pi` ρ `rho` σ `sigma` τ `tau` υ `upsilon` φ `phi` χ `chi` ψ `psi` ω `omega`; capitals Γ `Gamma` Δ `Delta` Θ `Theta` Λ `Lambda` Ξ `Xi` Π `Pi` Σ `Sigma` Υ `Upsilon` Φ `Phi` Ψ `Psi` Ω `Omega`. Same names with `\` + Tab (`\varepsilon`, `\vartheta`, `\varphi` also work). `omega_0` = `ω_0` = `ω₀`.

### Commands (in the Terminal)

| | |
|---|---|
| `fermium run file.fm` | run a program |
| `fermium` | interactive prompt (`:help`, `:vars`, `:quit`) |
| `fermium check file.fm` | check units without running |
| `fermium fmt file.fm --pretty` / `--ascii` / `-w` | symbols ↔ ASCII (`-w` saves the file) |
| `fermium doctor` | check the installation |
| `cd folder` · `ls` · `open file.png` | change folder · list files · open a picture |
| `fermium build file.fm` | make a standalone program `./file` (not yet for programs that use `plot`, `fit` or `load`) |
| Control+C | stop a running program |

### The language on one page

```
# comment
L = 1.20 m                      # a unit goes right after its number
g = 4 pi^2 L / T^2              # k x, 2 pi r: no * needed between names
print g, g in ft/s^2            # convert with in
print g to 6 digits             # more digits
name = "Mars"                   # a variable can hold text
x += 1 cm                       # also -= *= /=
E = ½ m v^2 where m = 2 kg, v = 3 m/s

f(x) = k x                      # one-line function
f(x [m]) = ...                  # require a length
speed(h) =                      # multi-line function (indent 4 spaces)
    g = 9.81 m/s^2
    return sqrt(2 * g * h)

if x > 0 m                      # else if / else; == != < > <= >= ~=; and or not
    print "positive"
for i from 1 to 10              # both ends included
for t from 0 s to 1 s step 0.1 s
for x in xs
while n < 100                   # break, continue
y = if x > 0 m then x else -x

xs = [1 m, 2 m, 3 m]            # xs[1] first, xs[end] last, len(xs)
push(xs, 4 m)                   # 2 xs, xs^2, sin(xs), f(xs): element by element
linspace(0 s, 1 s, 11)          # sum mean std min max sort reverse cumsum diff

v = x'                          # derivative (also d/dt x, dx/dt); x'' or d²/dt² x second
partial/partial x f             # ∂/∂x f
integral F(x) dx from 0 m to 1 m    # ∫ ... dx; limits may be inf
solve mass x'' = -k x - b x'
  with x(0) = 0.1 m, x'(0) = 0 m/s
  for t from 0 s to 5 s         # add "step 1 ms" for fixed-step RK4
data = load "data/pendulum.csv" # header: L [m], T [s]  ->  data.L, data.T
fit T = 2 pi sqrt(L / g) to data    # add: with g = 9 m/s^2
fit T^2 = k L to data           # the left side can be a formula of a column
plot data.T vs data.L to "p.png"    # plot x vs t, plot y in AU vs x in AU

r = <1, 0> AU                   # a vector; v = <0, 30> km/s
print |r|, r.x, a · b, a × b    # length, component, dot, cross
xs[2:4], xs[3:end]              # slices (both ends included)

L = 1.000 +- 0.002 m            # uncertainty (±): propagates, keeps correlations
propagate montecarlo            # re-run the indented formulas with random samples
z = 3 + 4i                      # complex numbers; also polar(r, θ), conj(z), abs(z)
import mechanics                # the standard library: mechanics em nuclear astro quantum stats
use python numpy as np          # call Python (plain numbers only, unless you declare units)
units natural(hbar = c = 1)     # natural units, still unit-checked; also units nuclear, units astro
analyze pend: T [s] depends on L [m], g [m/s^2]   # dimensional analysis
solve -hbar^2/(2 m_e) * psi'' + V(x) psi = E psi with psi(0 nm) = 0, psi(1 nm) = 0 for x from 0 nm to 1 nm lowest 3
seed(42)                        # reproducible rand(), randn(μ, σ)
```

**Printing:** a result shows as many significant figures as its least precise input (`1.20 m` has 3). If that is unknown (whole numbers, π, constants) it shows **3**: `1/2` prints `0.500`. `print x to 6 digits` for more. Calculations always use full precision.

**Constants:** `c h hbar e k_B N_A G g_n m_e m_p m_n m_u epsilon_0 mu_0 sigma alpha a_0 b_W M_sun R_sun L_sun M_earth R_earth AU`
**Units:** `m g s A K mol` + prefixes · `N J W Pa C V ohm Hz` · `min hr day yr` · `eV MeV fm u barn` · `AU ly pc Msun` · `inch ft mi mph lb` · `deg rad` · `degC`

### Good to know
1. **The unit rule:** right after a number comes a unit (`3 m`, `9.81 m/s²`, `50 N/m`). If that unit is a single name that is also your variable (`2 g` with your own `g`), Fermium asks: `2*g` for your variable, `2 [g]` for the unit. In a longer unit the first name is always a unit; a later name that is your variable gets the same question (`(20 m/s)/g` divides by your `g`). Spaces never change the meaning. `fermium fmt --fix file.fm` adds the brackets for you.
2. **Kinetic energy:** `½ m v^2` or `(1/2) m v^2`. A fraction of plain numbers is one coefficient (`1/2 mass v^2`, `73/24 x²`); for a sphere write `(4/3) π r^3`.
3. **Gravity:** `g = 9.81 m/s^2` in your file, or the built-in `g_n`.
4. `LT` is one name; `L T` is L × T. `ωt` is one name; write `ω t`.
5. `e` is the elementary charge (so `e^2` is the charge squared): write `exp(x)`, not `e^x`. Angles are radians: `sin(30 deg)`.
6. Lists start at 1. `hr` is hours (`h` is Planck's constant).
