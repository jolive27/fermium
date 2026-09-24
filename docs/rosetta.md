# Rosetta: the same physics in Fermium, Julia and Python

Seven small physics programs, each written three times: in Fermium, in Julia and in Python. You can compare how each language reads.

Every program on this page is a real file in [`examples/rosetta/`](../examples/rosetta/), and the test suite (`tests/test_examples.py`) checks three things:
- all three versions run;
- they print the same numbers (to 0.2%);
- the code on this page is the same as the code in the files.

How to run them yourself:

```
fermium run examples/rosetta/01_pendulum.fm
python3 examples/rosetta/01_pendulum.py
julia --project=examples/rosetta examples/rosetta/01_pendulum.jl
```

The Julia versions use [Unitful.jl](https://github.com/PainterQubits/Unitful.jl) for units where it fits, and [QuadGK.jl](https://github.com/JuliaMath/QuadGK.jl) for integrals. Install them once with `julia --project=examples/rosetta -e 'using Pkg; Pkg.instantiate()'`. The Python versions use NumPy and SciPy.

Julia users would normally solve ODEs with DifferentialEquations.jl. That package is large, so the ODE examples here use a small hand-written RK4 loop instead, which keeps them free of extra dependencies.

## What to look for

| | Fermium | Julia | Python |
|---|---|---|---|
| Units | Built in and checked before the program runs. `9.81 m/s²`, `in MeV`. | Unitful.jl checks units while the program runs. Printing in a chosen unit needs `ustrip(u"MeV", x)`. | None: you keep track of units in comments and in your head. |
| Physical constants | Built in (`G`, `ħ`, `k_B`, `M_sun`, …). | Partly built in (`Unitful.G`, `c`, `k`); astronomical ones you type yourself. | `scipy.constants`; astronomical ones you type yourself. |
| Derivatives | Symbolic: `d/dλ B`. | A package (ForwardDiff) or finite differences. | Finite differences or SymPy. |
| Integrals | `∫ f(x) dx from a to ∞`, with units. | `quadgk`; infinite limits don't combine with Unitful quantities. | `scipy.integrate.quad`; it can go wrong if the numbers are far from 1 (e.g. wavelengths in metres). |
| ODEs | `solve m x'' = -k x - b x' with …`. | DifferentialEquations.jl (here: hand-written RK4). | `solve_ivp`: you rewrite the equation as first-order and handle the state vector yourself. |
| Printing | Significant figures and units chosen for you, or `to 5 digits`. | `@printf`. | f-strings. |
| Vectors | `<3, 4> m/s`, `\|v\|`, `a · b`, `a × b`, unit-checked; ODEs can have vector unknowns. | `StaticArrays` or plain arrays of Unitful quantities. | NumPy arrays, no units. |
| Standalone program | `fermium build prog.fm` (needs a C compiler). | PackageCompiler.jl. | PyInstaller or similar. |

Things we ran into while writing these (they are left in the code, with a comment):
- **Julia, escape velocity:** `quadgk` refused an infinite upper limit with Unitful units, so the units have to be stripped for that one integral.
- **Python, blackbody:** `quad(B, 0, np.inf)` with λ in metres quietly returned a wrong answer, about 10⁻¹⁶ of the right one, with only an `IntegrationWarning`. It works once λ is measured in μm.

---

## 1. Pendulum: measuring g

Measure g from a pendulum's length and period, then compute the exact period for large swings with an elliptic integral. The longer version is [`examples/01_pendulum.fm`](../examples/01_pendulum.fm).

**Fermium**

```fermium
# Measure g with a pendulum, then find the exact period of big swings
L = 1.20 m
T = 2.21 s
g = 4π² L / T²
print "g =", g to 4 digits
print "g =", g in ft/s² to 4 digits

period(θ0) = 4 √(L/g) * ∫ 1/√(1 - sin(θ0/2)² sin(φ)²) dφ from 0 to π/2

for θ0 in [10°, 45°, 90°]
    print "amplitude", θ0 in deg, "period", period(θ0) to 5 digits
```

**Julia**

```julia
# Measure g with a pendulum, then find the exact period of big swings
using Unitful, QuadGK, Printf

L = 1.20u"m"
T = 2.21u"s"
g = 4π^2 * L / T^2
@printf("g = %.4g m/s²\n", ustrip(u"m/s^2", g))
@printf("g = %.4g ft/s²\n", ustrip(u"ft/s^2", g))

function period(θ0)
    integral, _ = quadgk(φ -> 1 / sqrt(1 - sin(θ0 / 2)^2 * sin(φ)^2), 0, π / 2)
    return 4 * sqrt(L / g) * integral
end

for θ0 in [10u"°", 45u"°", 90u"°"]
    @printf("amplitude %d° period %.5g s\n", ustrip(u"°", θ0), ustrip(u"s", period(θ0)))
end
```

**Python**

```python
# Measure g with a pendulum, then find the exact period of big swings
import numpy as np
from scipy.integrate import quad

L = 1.20   # m
T = 2.21   # s
g = 4 * np.pi**2 * L / T**2
print(f"g = {g:.4g} m/s²")
print(f"g = {g / 0.3048:.4g} ft/s²")


def period(theta0):
    k2 = np.sin(theta0 / 2) ** 2
    integral, _ = quad(lambda phi: 1 / np.sqrt(1 - k2 * np.sin(phi) ** 2), 0, np.pi / 2)
    return 4 * np.sqrt(L / g) * integral


for deg in [10, 45, 90]:
    print(f"amplitude {deg}° period {period(np.radians(deg)):.5g} s")
```

**Output of all three versions:**

```
g = 9.700 m/s²
g = 31.82 ft/s²
amplitude 10° period 2.2142 s
amplitude 45° period 2.2983 s
amplitude 90° period 2.6086 s
```

---

## 2. Escape velocity

A function applied to several bodies, and an integral of the gravitational force out to infinity. The longer version is [`examples/05_escape_velocity.fm`](../examples/05_escape_velocity.fm).

**Fermium**

```fermium
# Escape velocity, and the work needed to escape as an integral to infinity
v_esc(M, R) = √(2 G M / R)

print "Earth:", v_esc(M_earth, R_earth) in km/s to 4 digits
print "Moon:", v_esc(7.342e22 kg, 1737.4 km) in km/s to 4 digits
print "Sun:", v_esc(M_sun, R_sun) in km/s to 4 digits

F(r) = G M_earth (1 kg) / r²
W_esc = ∫ F(r) dr from R_earth to ∞
print "work per kg:", W_esc in MJ to 4 digits
print "Schwarzschild radius of the Sun:", 2 G M_sun / c² in km to 4 digits
```

**Julia**

```julia
# Escape velocity, and the work needed to escape as an integral to infinity
using Unitful, QuadGK, Printf
using Unitful: G, c

M_earth, R_earth = 5.9722e24u"kg", 6.3781e6u"m"
M_sun, R_sun = 1.98841e30u"kg", 6.957e8u"m"

v_esc(M, R) = sqrt(2G * M / R)

@printf("Earth: %.4g km/s\n", ustrip(u"km/s", v_esc(M_earth, R_earth)))
@printf("Moon: %.4g km/s\n", ustrip(u"km/s", v_esc(7.342e22u"kg", 1737.4u"km")))
@printf("Sun: %.4g km/s\n", ustrip(u"km/s", v_esc(M_sun, R_sun)))

F(r) = G * M_earth * 1u"kg" / r^2
# quadgk can't take a unitful infinite limit, so strip units for the integral
W, _ = quadgk(r -> ustrip(u"N", F(r * u"m")), ustrip(u"m", R_earth), Inf)
W_esc = W * u"J"
@printf("work per kg: %.4g MJ\n", ustrip(u"MJ", W_esc))
@printf("Schwarzschild radius of the Sun: %.4g km\n", ustrip(u"km", 2G * M_sun / c^2))
```

**Python**

```python
# Escape velocity, and the work needed to escape as an integral to infinity
import numpy as np
from scipy.constants import G, c
from scipy.integrate import quad

M_earth, R_earth = 5.9722e24, 6.3781e6   # kg, m (IAU nominal)
M_sun, R_sun = 1.98841e30, 6.957e8       # kg, m


def v_esc(M, R):
    return np.sqrt(2 * G * M / R)


print(f"Earth: {v_esc(M_earth, R_earth) / 1e3:.4g} km/s")
print(f"Moon: {v_esc(7.342e22, 1737.4e3) / 1e3:.4g} km/s")
print(f"Sun: {v_esc(M_sun, R_sun) / 1e3:.4g} km/s")

W_esc, _ = quad(lambda r: G * M_earth * 1.0 / r**2, R_earth, np.inf)   # for 1 kg
print(f"work per kg: {W_esc / 1e6:.4g} MJ")
print(f"Schwarzschild radius of the Sun: {2 * G * M_sun / c**2 / 1e3:.4g} km")
```

**Output of all three versions:**

```
Earth: 11.18 km/s
Moon: 2.375 km/s
Sun: 617.7 km/s
work per kg: 62.50 MJ
Schwarzschild radius of the Sun: 2.953 km
```

---

## 3. Blackbody radiation: Wien and Stefan–Boltzmann

Find the peak of Planck's law where dB/dλ = 0, then integrate the spectrum and compare with σT⁴. The longer version is [`examples/06_blackbody.fm`](../examples/06_blackbody.fm).

**Fermium**

```fermium
# Wien's law from dB/dλ = 0, and Stefan–Boltzmann from integrating Planck's law
T = 5778 K
B(λ) = 2 h c² / λ⁵ / (exp(h c / (λ k_B T)) - 1)
dB = d/dλ B

lo = 100 nm
hi = 2000 nm
for i from 1 to 60
    mid = (lo + hi) / 2
    if dB(mid) > 0 W/m⁴
        lo = mid
    else
        hi = mid
print "peak:", (lo + hi) / 2 in nm to 5 digits
print "Wien b/T:", b_W / T in nm to 5 digits

flux = π * ∫ B(λ) dλ from 0 nm to ∞
print "π ∫ B dλ =", flux in MW/m² to 6 digits
print "σ T⁴ =", σ T⁴ in MW/m² to 6 digits
```

**Julia**

```julia
# Wien's law from dB/dλ = 0, and Stefan–Boltzmann from integrating Planck's law
using QuadGK, Printf

const h, c, k_B = 6.62607015e-34, 299792458.0, 1.380649e-23   # SI units
const σ, b_W = 5.670374419e-8, 2.897771955e-3
T = 5778.0   # K

B(λ) = 2h * c^2 / λ^5 / expm1(h * c / (λ * k_B * T))
dB(λ) = (B(λ * (1 + 1e-6)) - B(λ * (1 - 1e-6))) / (2e-6 * λ)   # central difference

function bisect(f, lo, hi; iters=60)
    for _ in 1:iters
        mid = (lo + hi) / 2
        f(mid) > 0 ? (lo = mid) : (hi = mid)
    end
    return (lo + hi) / 2
end

@printf("peak: %.5g nm\n", bisect(dB, 100e-9, 2000e-9) * 1e9)
@printf("Wien b/T: %.5g nm\n", b_W / T * 1e9)

integral, _ = quadgk(B, 0, Inf)
@printf("π ∫ B dλ = %.6g MW/m²\n", π * integral / 1e6)
@printf("σ T⁴ = %.6g MW/m²\n", σ * T^4 / 1e6)
```

**Python**

```python
# Wien's law from dB/dλ = 0, and Stefan–Boltzmann from integrating Planck's law
import numpy as np
from scipy.constants import h, c, k, sigma, Wien
from scipy.integrate import quad
from scipy.optimize import minimize_scalar

T = 5778.0  # K


def B(lam):
    with np.errstate(over="ignore"):   # exp overflows to inf at tiny λ, giving B = 0: fine
        return 2 * h * c**2 / lam**5 / np.expm1(h * c / (lam * k * T))


peak = minimize_scalar(lambda lam: -B(lam), bounds=(100e-9, 2000e-9), method="bounded",
                       options={"xatol": 1e-15})
print(f"peak: {peak.x * 1e9:.5g} nm")
print(f"Wien b/T: {Wien / T * 1e9:.5g} nm")

# quad needs numbers of order 1: integrate over λ in μm, not in m
integral, _ = quad(lambda x: B(x * 1e-6) * 1e-6, 0, np.inf)
print(f"π ∫ B dλ = {np.pi * integral / 1e6:.6g} MW/m²")
print(f"σ T⁴ = {sigma * T**4 / 1e6:.6g} MW/m²")
```

**Output of all three versions:**

```
peak: 501.52 nm
Wien b/T: 501.52 nm
π ∫ B dλ = 63.2007 MW/m²
σ T⁴ = 63.2007 MW/m²
```

---

## 4. Damped spring (an ODE)

Solve m x'' = −k x − b x' and read off the position, the velocity and the energy. The longer version is [`examples/03_damped_spring.fm`](../examples/03_damped_spring.fm).

**Fermium**

```fermium
# A damped mass on a spring: solve m x'' = -k x - b x'
k = 50 N/m
m = 0.5 kg
b = 0.2 kg/s

solve m x'' = -k x - b x'
  with x(0) = 10.0 cm, x'(0) = 0 cm/s
  for t from 0 s to 10 s

print "x(5 s) =", x(5 s) in cm to 5 digits
print "v(1 s) =", x'(1 s) in cm/s to 5 digits
E(t) = ½ k x(t)² + ½ m x'(t)²
print "energy left after 10 s:", E(10 s) / E(0 s) to 4 digits
```

**Julia**

```julia
# A damped mass on a spring: solve m x'' = -k x - b x'
# (Normally you'd use DifferentialEquations.jl; a small RK4 keeps this dependency-free.)
using Printf

const k, m, b = 50.0, 0.5, 0.2   # N/m, kg, kg/s

rhs(t, y) = [y[2], (-k * y[1] - b * y[2]) / m]

function rk4(f, y0, t0, t1; dt=1e-4)
    n = round(Int, (t1 - t0) / dt)
    h = (t1 - t0) / n
    y, t = copy(y0), t0
    for _ in 1:n
        k1 = f(t, y)
        k2 = f(t + h / 2, y .+ h / 2 .* k1)
        k3 = f(t + h / 2, y .+ h / 2 .* k2)
        k4 = f(t + h, y .+ h .* k3)
        y = y .+ h / 6 .* (k1 .+ 2k2 .+ 2k3 .+ k4)
        t += h
    end
    return y
end

y0 = [0.10, 0.0]
@printf("x(5 s) = %.5g cm\n", rk4(rhs, y0, 0.0, 5.0)[1] * 100)
@printf("v(1 s) = %.5g cm/s\n", rk4(rhs, y0, 0.0, 1.0)[2] * 100)

E(y) = 0.5k * y[1]^2 + 0.5m * y[2]^2
@printf("energy left after 10 s: %.4g\n", E(rk4(rhs, y0, 0.0, 10.0)) / E(y0))
```

**Python**

```python
# A damped mass on a spring: solve m x'' = -k x - b x'
from scipy.integrate import solve_ivp

k = 50.0   # N/m
m = 0.5    # kg
b = 0.2    # kg/s


def rhs(t, y):
    x, v = y
    return [v, (-k * x - b * v) / m]


sol = solve_ivp(rhs, (0, 10), [0.10, 0.0], method="DOP853", rtol=1e-10, atol=1e-12,
                dense_output=True)

x5, _ = sol.sol(5.0)
_, v1 = sol.sol(1.0)
print(f"x(5 s) = {x5 * 100:.5g} cm")
print(f"v(1 s) = {v1 * 100:.5g} cm/s")


def E(t):
    x, v = sol.sol(t)
    return 0.5 * k * x**2 + 0.5 * m * v**2


print(f"energy left after 10 s: {E(10.0) / E(0.0):.4g}")
```

**Output of all three versions:**

```
x(5 s) = 3.5201 cm
v(1 s) = 44.412 cm/s
energy left after 10 s: 0.01799
```

---

## 5. Decay chain Mo-99 → Tc-99m → Tc-99 (a system of ODEs)

Three coupled decay equations: the Bateman equations. The longer version is [`examples/08_bateman_chain.fm`](../examples/08_bateman_chain.fm).

**Fermium**

```fermium
# Decay chain Mo-99 -> Tc-99m -> Tc-99: a system of three ODEs
λ_Mo = ln(2) / (65.94 hr)
λ_Tc = ln(2) / (6.0067 hr)

solve N_Mo' = -λ_Mo N_Mo,
      N_Tc' = λ_Mo N_Mo - λ_Tc N_Tc,
      N_99' = λ_Tc N_Tc
  with N_Mo(0) = 1.0, N_Tc(0) = 0, N_99(0) = 0
  for t from 0 hr to 240 hr

print "N_Tc(24 h) / N0 =", N_Tc(24 hr) to 5 digits
print "N_99(240 h) / N0 =", N_99(240 hr) to 5 digits
print "Tc-99m peaks at", ln(λ_Tc/λ_Mo) / (λ_Tc - λ_Mo) in hr to 5 digits
print "activity ratio at 200 h:", λ_Tc N_Tc(200 hr) / (λ_Mo N_Mo(200 hr)) to 5 digits
```

**Julia**

```julia
# Decay chain Mo-99 -> Tc-99m -> Tc-99: a system of three ODEs
# (Normally you'd use DifferentialEquations.jl; a small RK4 keeps this dependency-free.)
using Printf

const λ_Mo = log(2) / 65.94    # 1/h
const λ_Tc = log(2) / 6.0067   # 1/h

rhs(t, N) = [-λ_Mo * N[1], λ_Mo * N[1] - λ_Tc * N[2], λ_Tc * N[2]]

function rk4(f, y0, t0, t1; dt=1e-3)
    n = round(Int, (t1 - t0) / dt)
    h = (t1 - t0) / n
    y, t = copy(y0), t0
    for _ in 1:n
        k1 = f(t, y)
        k2 = f(t + h / 2, y .+ h / 2 .* k1)
        k3 = f(t + h / 2, y .+ h / 2 .* k2)
        k4 = f(t + h, y .+ h .* k3)
        y = y .+ h / 6 .* (k1 .+ 2k2 .+ 2k3 .+ k4)
        t += h
    end
    return y
end

N0 = [1.0, 0.0, 0.0]
@printf("N_Tc(24 h) / N0 = %.5g\n", rk4(rhs, N0, 0.0, 24.0)[2])
@printf("N_99(240 h) / N0 = %.5g\n", rk4(rhs, N0, 0.0, 240.0)[3])
@printf("Tc-99m peaks at %.5g hr\n", log(λ_Tc / λ_Mo) / (λ_Tc - λ_Mo))
N200 = rk4(rhs, N0, 0.0, 200.0)
@printf("activity ratio at 200 h: %.5g\n", λ_Tc * N200[2] / (λ_Mo * N200[1]))
```

**Python**

```python
# Decay chain Mo-99 -> Tc-99m -> Tc-99: a system of three ODEs
import numpy as np
from scipy.integrate import solve_ivp

lam_Mo = np.log(2) / 65.94    # 1/h
lam_Tc = np.log(2) / 6.0067   # 1/h


def rhs(t, N):
    N_Mo, N_Tc, N_99 = N
    return [-lam_Mo * N_Mo, lam_Mo * N_Mo - lam_Tc * N_Tc, lam_Tc * N_Tc]


sol = solve_ivp(rhs, (0, 240), [1.0, 0.0, 0.0], rtol=1e-10, atol=1e-14, dense_output=True)

print(f"N_Tc(24 h) / N0 = {sol.sol(24)[1]:.5g}")
print(f"N_99(240 h) / N0 = {sol.sol(240)[2]:.5g}")
print(f"Tc-99m peaks at {np.log(lam_Tc / lam_Mo) / (lam_Tc - lam_Mo):.5g} hr")
N200 = sol.sol(200)
print(f"activity ratio at 200 h: {lam_Tc * N200[1] / (lam_Mo * N200[0]):.5g}")
```

**Output of all three versions:**

```
N_Tc(24 h) / N0 = 0.071592
N_99(240 h) / N0 = 0.91173
Tc-99m peaks at 22.843 hr
activity ratio at 200 h: 1.1002
```

---

## 6. Binding energy per nucleon (semi-empirical mass formula)

A formula with a pairing term decided by `if`, and a loop over A to find the peak of the curve of binding energy. The longer version is [`examples/09_binding_energy.fm`](../examples/09_binding_energy.fm).

**Fermium**

```fermium
# Semi-empirical mass formula: binding energy per nucleon, peak of the curve
a_V = 15.75 MeV
a_S = 17.8 MeV
a_C = 0.711 MeV
a_A = 23.7 MeV
a_P = 11.18 MeV

pairing(Z, A) = if mod(A, 2) == 1 then 0 MeV else if mod(Z, 2) == 0 then a_P / √A else -a_P / √A
B(Z, A) = a_V A - a_S A^(2/3) - a_C Z (Z - 1) / A^(1/3) - a_A (A - 2Z)² / A + pairing(Z, A)
Z_stable(A) = round(A / (2 + a_C / (2 a_A) * A^(2/3)))

print "Fe-56: B/A =", B(26, 56) / 56 in MeV to 5 digits
print "U-238: B/A =", B(92, 238) / 238 in MeV to 5 digits

best_A = 0
best = 0 MeV
for A from 10 to 250
    b = B(Z_stable(A), A) / A
    if b > best
        best = b
        best_A = A
print "peak at A =", best_A, "with B/A =", best in MeV to 5 digits
```

**Julia**

```julia
# Semi-empirical mass formula: binding energy per nucleon, peak of the curve
using Printf

const a_V, a_S, a_C, a_A, a_P = 15.75, 17.8, 0.711, 23.7, 11.18   # MeV

pairing(Z, A) = isodd(A) ? 0.0 : (iseven(Z) ? a_P / √A : -a_P / √A)
B(Z, A) = a_V * A - a_S * A^(2 / 3) - a_C * Z * (Z - 1) / A^(1 / 3) - a_A * (A - 2Z)^2 / A + pairing(Z, A)
Z_stable(A) = round(Int, A / (2 + a_C / (2a_A) * A^(2 / 3)))

@printf("Fe-56: B/A = %.5g MeV\n", B(26, 56) / 56)
@printf("U-238: B/A = %.5g MeV\n", B(92, 238) / 238)

best, i = findmax(A -> B(Z_stable(A), A) / A, 10:250)
@printf("peak at A = %d with B/A = %.5g MeV\n", (10:250)[i], best)
```

**Python**

```python
# Semi-empirical mass formula: binding energy per nucleon, peak of the curve
import numpy as np

a_V, a_S, a_C, a_A, a_P = 15.75, 17.8, 0.711, 23.7, 11.18   # MeV


def pairing(Z, A):
    if A % 2 == 1:
        return 0.0
    return a_P / np.sqrt(A) if Z % 2 == 0 else -a_P / np.sqrt(A)


def B(Z, A):
    return (a_V * A - a_S * A ** (2 / 3) - a_C * Z * (Z - 1) / A ** (1 / 3)
            - a_A * (A - 2 * Z) ** 2 / A + pairing(Z, A))


def Z_stable(A):
    return round(A / (2 + a_C / (2 * a_A) * A ** (2 / 3)))


print(f"Fe-56: B/A = {B(26, 56) / 56:.5g} MeV")
print(f"U-238: B/A = {B(92, 238) / 238:.5g} MeV")

As = np.arange(10, 251)
BperA = np.array([B(Z_stable(A), A) / A for A in As])
i = np.argmax(BperA)
print(f"peak at A = {As[i]} with B/A = {BperA[i]:.5g} MeV")
```

**Output of all three versions:**

```
Fe-56: B/A = 8.8461 MeV
U-238: B/A = 7.6249 MeV
peak at A = 58 with B/A = 8.8648 MeV
```

---

## 7. Q-values of nuclear reactions

Masses in atomic mass units u; Q = (Σ m_before − Σ m_after) c², printed in MeV. The longer version is [`examples/11_q_value.fm`](../examples/11_q_value.fm).

**Fermium**

```fermium
# Q-values of nuclear reactions from atomic masses (in u), results in MeV
n   = 1.00866491595 u
H1  = 1.00782503223 u
H2  = 2.01410177812 u
H3  = 3.01604928199 u
He4 = 4.00260325413 u
Li7 = 7.0160034366 u
U238 = 238.0507884 u
Th234 = 234.0436014 u

Q(before, after) = (sum(before) - sum(after)) c²

print "D + T -> He-4 + n:", Q([H2, H3], [He4, n]) in MeV to 5 digits
print "p + Li-7 -> 2 He-4:", Q([H1, Li7], [He4, He4]) in MeV to 5 digits
print "U-238 -> Th-234 + He-4:", Q([U238], [Th234, He4]) in MeV to 5 digits
print "4 H -> He-4:", Q([H1, H1, H1, H1], [He4]) in MeV to 5 digits
```

**Julia**

```julia
# Q-values of nuclear reactions from atomic masses (in u), results in MeV
using Unitful, Printf
using Unitful: c

n = 1.00866491595u"u"
H1 = 1.00782503223u"u"
H2 = 2.01410177812u"u"
H3 = 3.01604928199u"u"
He4 = 4.00260325413u"u"
Li7 = 7.0160034366u"u"
U238 = 238.0507884u"u"
Th234 = 234.0436014u"u"

Q(before, after) = uconvert(u"MeV", (sum(before) - sum(after)) * c^2)

@printf("D + T -> He-4 + n: %.5g MeV\n", ustrip(Q([H2, H3], [He4, n])))
@printf("p + Li-7 -> 2 He-4: %.5g MeV\n", ustrip(Q([H1, Li7], [He4, He4])))
@printf("U-238 -> Th-234 + He-4: %.5g MeV\n", ustrip(Q([U238], [Th234, He4])))
@printf("4 H -> He-4: %.5g MeV\n", ustrip(Q([H1, H1, H1, H1], [He4])))
```

**Python**

```python
# Q-values of nuclear reactions from atomic masses (in u), results in MeV
from scipy.constants import physical_constants

uc2 = physical_constants["atomic mass constant energy equivalent in MeV"][0]   # 931.494 MeV

n = 1.00866491595   # masses in u
H1 = 1.00782503223
H2 = 2.01410177812
H3 = 3.01604928199
He4 = 4.00260325413
Li7 = 7.0160034366
U238 = 238.0507884
Th234 = 234.0436014


def Q(before, after):
    return (sum(before) - sum(after)) * uc2


print(f"D + T -> He-4 + n: {Q([H2, H3], [He4, n]):.5g} MeV")
print(f"p + Li-7 -> 2 He-4: {Q([H1, Li7], [He4, He4]):.5g} MeV")
print(f"U-238 -> Th-234 + He-4: {Q([U238], [Th234, He4]):.5g} MeV")
print(f"4 H -> He-4: {Q([H1, H1, H1, H1], [He4]):.5g} MeV")
```

**Output of all three versions:**

```
D + T -> He-4 + n: 17.589 MeV
p + Li-7 -> 2 He-4: 17.346 MeV
U-238 -> Th-234 + He-4: 4.2697 MeV
4 H -> He-4: 26.731 MeV
```

