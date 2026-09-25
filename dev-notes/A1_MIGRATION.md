# A1 migration log (D235)

Every edit `fermium fmt --fix` made when the unit-name rule changed. Each keeps what Fermium 1 did.

## benchmarks/fermium/spring_adaptive.fm
- line 8: `k = 100 N/m` → `k = 100 [N/m]`
- line 15: `print "x_100s", x(100 s) / (1 m) to 10 digits` → `print "x_100s", x(100 s) / (1 [m]) to 10 digits`

## benchmarks/fermium/spring_rk4.fm
- line 3: `k = 100 N/m` → `k = 100 [N/m]`
- line 11: `print "x_10s", x(10 s) / (1 m) to 10 digits` → `print "x_10s", x(10 s) / (1 [m]) to 10 digits`

## examples/41_pde_heat_waves_tunnelling.fm
- line 16: `exact = 80 K * exp(-α_Cu π² 200 s / L²)` → `exact = 80 K * exp(-α_Cu π² 200 [s] / L²)`

## gauntlet/oscillations/01_driven_resonance.fm
- line 17: `k = 80.0 N/m` → `k = 80.0 [N/m]`

## gauntlet/oscillations/22_parametric_resonance.fm
- line 26: `k0 = 10.0 N/m` → `k0 = 10.0 [N/m]`
- line 44: `with x2(0) = 0 m, x2'(0) = 1 m/s` → `with x2(0) = 0 [m], x2'(0) = 1 m/s`
- line 46: `return x1(T) / (1 m) + x2'(T) / (1 m/s)` → `return x1(T) / (1 [m]) + x2'(T) / (1 m/s)`

## gauntlet/special_relativity/02_relativistic_rocket.fm
- line 37: `"  exact ship time", 2 c / g * acosh(1 + g d / (2 c²)) in yr to 10 digits` → `"  exact ship time", 2 [c] / g * acosh(1 + g d / (2 c²)) in yr to 10 digits`

## gauntlet/special_relativity/22_twin_paradox.fm
- line 40: `τ_exact = 4 c / g * asinh(g t1 / c) + 2 tc / γ_max` → `τ_exact = 4 [c] / g * asinh(g t1 / c) + 2 tc / γ_max`
- line 48: `"  exact", 2 c² / g * (γ_max - 1) + v(t1) tc in ly to 10 digits` → `"  exact", 2 [c²] / g * (γ_max - 1) + v(t1) tc in ly to 10 digits`
- line 66: `solve 4 c / g * asinh(g t_a / c) = τ_goal for t_a from 0 yr to 1e6 yr` → `solve 4 [c] / g * asinh(g t_a / c) = τ_goal for t_a from 0 yr to 1e6 yr`
- line 68: `print "    farthest point", 2 c² / g * (√(1 + (g t_a / c)²) - 1) in ly to 8 digits` → `print "    farthest point", 2 [c²] / g * (√(1 + (g t_a / c)²) - 1) in ly to 8 digits`

## research/shell_model_magic_numbers/shell.fm
- line 16: `ħω(A) = 41 MeV / A^(1/3)           # the oscillator shell spacing, the natural unit for a shell gap` → `ħω(A) = 41 [MeV] / A^(1/3)           # the oscillator shell spacing, the natural unit for a shell gap`

## tests/programs/spec_31.fm
- line 22: `W = ∫ F(x) dx from 0 m to 0.2 m     # 1.0 J` → `W = ∫ F(x) dx from 0 [m] to 0.2 [m]     # 1.0 J`
- line 25: `with x(0) = 0.1 m, x'(0) = 0 m/s` → `with x(0) = 0.1 [m], x'(0) = 0 m/s`

## SHOWCASE.md
- line 7: `k = 50 N/m` → `k = 50 [N/m]`

## bootcamp/lesson01_numbers_units.md
- line 2: `print 20 m/s / g` → `print 20 [m/s] / g`
- line 3: `print 20 m/s/g` → `print 20 [m/s/g]`

## bootcamp/solutions/lesson03.md
- line 1: `peak(T) = 2.898e-3 m K / T` → `peak(T) = 2.898e-3 [m K] / T`

## bootcamp/solutions/lesson06.md
- line 2: `fit x = M * 9.81 m/s^2 / k to s` → `fit x = M * 9.81 [m/s^2] / k to s`

## docs/reference.md
- line 2: `k = 50 N/m` → `k = 50 [N/m]`
- line 6: `print u(0.5 m, 10 s), "  exact:", 2 K exp(-D π² 10 s / L²)` → `print u(0.5 m, 10 s), "  exact:", 2 K exp(-D π² 10 [s] / L²)`
- line 9: `print "centre:", ∫ x |ψ(x, 30 fs)|^2 dx from -40 nm to 40 nm, "  (ħ k0 t / m =", ħ k0 30 fs / m in nm, ")"` → `print "centre:", ∫ x |ψ(x, 30 fs)|^2 dx from -40 nm to 40 nm, "  (ħ k0 t / m =", ħ k0 30 [fs] / m in nm, ")"`

