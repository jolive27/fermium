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
| ± | `+-` (reserved) | `\pm` | ⌥⇧= |
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
| `fermium build file.fm` | make a standalone program `./file` (needs a C compiler; not for plot/load/fit) |
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
solve m x'' = -k x - b x'
  with x(0) = 0.1 m, x'(0) = 0 m/s
  for t from 0 s to 5 s         # add "step 1 ms" for fixed-step RK4
data = load "data/pendulum.csv" # header: L [m], T [s]  ->  data.L, data.T
fit T = 2 pi sqrt(L / g) to data    # add: with g = 9 m/s^2
fit T^2 = k L to data           # the left side can be a formula of a column
plot data.T vs data.L to "p.png"    # plot x vs t, plot y in AU vs x in AU

r = <1, 0> AU                   # a vector; v = <0, 30> km/s
print |r|, r.x, a · b, a × b    # length, component, dot, cross
```

**Constants:** `c h hbar e k_B N_A G g_n m_e m_p m_n m_u epsilon_0 mu_0 sigma alpha a_0 b_W M_sun R_sun L_sun M_earth R_earth AU`
**Units:** `m g s A K mol` + prefixes · `N J W Pa C V ohm Hz` · `min hr day yr` · `eV MeV fm u barn` · `AU ly pc Msun` · `inch ft mi mph lb` · `deg rad` · `degC`

### ⚠️ Gotchas
1. **A unit name right after a number is a unit.** `2 g h` = 2 *grams* × Planck's h. `0.5 m v^2` = half a *metre*. Write `2 * g * h`, `½ m v^2`, `0.5 * m * v^2`.
2. **`/` right after a unit (no space) continues the unit:** `50 N/m`, `3 m/s`. With a space before the `/`, one of *your* variables wins: `20 m/s / g` divides by your `g`. If in doubt, use parentheses: `(20 m/s) / g`.
3. `1/2 m v^2` means 1/(2mv²). Write `½ m v^2` or `(1/2) m v^2`.
4. `LT` is one name; `L T` is L × T. `ωt` is one name; write `ω t`.
5. `e` is the elementary charge (so `e^2` is the charge squared): write `exp(x)`, not `e^x`. Angles are radians: `sin(30 deg)`.
6. Lists start at 1. `hr` is hours (`h` is Planck's constant).
