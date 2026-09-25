# Fermium standard library

Modules shipped with Fermium. Import one with `import mechanics` (then `mechanics.kinetic_energy(…)`),
`import astro as a`, or `from nuclear import semf_binding, Q_value`; see [the reference, Modules](reference.md#modules).

Each function checks the units of its arguments (the units in brackets; any unit of the same kind works, like `km/hr` for `[m/s]`), and its result has the units shown after the arrow. Parameters without brackets take any units. Every function is tested against a closed form or SciPy in `tests/test_stdlib.py`.

This page is generated from the modules' source (`fermium/stdlib/*.fm`) by `python3 -m fermium.stdlib_doc > docs/stdlib.md`.

Modules: [astro](#astro), [em](#em), [mechanics](#mechanics), [nuclear](#nuclear), [quantum](#quantum), [stats](#stats).

## astro

Gravity, stars and cosmology. Masses in kg or M☉, distances in m, au, pc; the Hubble constant like 70 km/s/Mpc.

| Constant | Value | Meaning |
|---|---|---|
| `σ_T` | `8π/3 * r_e²` | Thomson scattering cross-section of the electron: (8π/3) r_e² |

- `schwarzschild_radius(M [kg])` → length [m], shown in km  
  Schwarzschild radius of a mass M: 2 G M / c²
- `eddington_luminosity(M [kg])` → power [W]  
  Eddington luminosity of a mass M (ionised hydrogen): 4π G M m_p c / σ_T
- `kepler_period(a [m], M [kg])` → time [s]  
  Kepler's third law: period of an orbit with semi-major axis a around total mass M: 2π √(a³ / (G M))
- `orbital_velocity(M [kg], r [m])` → speed [m/s]  
  Circular orbital speed at distance r from a mass M: √(G M / r)
- `escape_velocity(M [kg], r [m])` → speed [m/s]  
  Escape velocity from distance r of a mass M: √(2 G M / r)
- `wien_peak(T [K])` → length [m], shown in nm  
  Wien's law: wavelength where a blackbody at temperature T is brightest: b / T
- `stellar_luminosity(R [m], T [K])` → power [W]  
  Luminosity of a spherical blackbody star with radius R and surface temperature T: 4π R² σ T⁴
- `distance_modulus(d [m])` → a plain number (no units)  
  Distance modulus m − M = 5 log10(d / 10 pc) of an object at distance d
- `hubble_distance(H0 [1/s])` → length [m], shown in Mpc  
  Hubble distance c / H0
- `comoving_distance(z [1], H0 [1/s], Ω_m [1])` → length [m], shown in Mpc  
  Comoving distance to redshift z in a flat ΛCDM universe (radiation neglected): c/H0 ∫₀^z dz' / E(z')
- `luminosity_distance(z [1], H0 [1/s], Ω_m [1])` → length [m]  
  Luminosity distance to redshift z in a flat ΛCDM universe: (1 + z) times the comoving distance

## em

Electrostatics, circuits and charged particles in fields. Charges in C (or e), fields in V/m and T.  Frequencies ending in _frequency are in cycles per second (Hz); the _angular_ ones are ω in rad/s.

- `coulomb_field(q [C], r [m])` → electric field [V/m]  
  Electric field of a point charge q at distance r: k_e q / r²
- `coulomb_force(q1 [C], q2 [C], r [m])` → force [N]  
  Force between two point charges at distance r: k_e q1 q2 / r² (positive means they repel)
- `coulomb_potential(q [C], r [m])` → voltage [V]  
  Electric potential of a point charge q at distance r: k_e q / r
- `parallel_plate_capacitance(area [m²], d [m])` → capacitance [F]  
  Capacitance of a parallel-plate capacitor, plate area A and gap d, vacuum: ε₀ A / d
- `capacitor_energy(cap [F], V [V])` → energy [J]  
  Energy stored in a capacitor C charged to voltage V: ½ C V²
- `rc_time_constant(R [Ω], cap [F])` → time [s]  
  Time constant of a resistor R and capacitor C: τ = R C
- `rc_charging_voltage(V0 [V], τ [s], t [s])` → voltage [V]  
  Voltage on a charging capacitor after time t, supply V0, time constant τ: V0 (1 − e^(−t/τ))
- `cyclotron_angular_frequency(q [C], B [T], mass [kg])` → frequency [1/s], shown in rad/s  
  Cyclotron angular frequency of a charge q, mass m in a field B: ω = |q| B / m
- `cyclotron_frequency(q [C], B [T], mass [kg])` → frequency [1/s], shown in Hz  
  Cyclotron frequency in turns per second: f = |q| B / (2π m)
- `larmor_radius(mass [kg], v_perp [m/s], q [C], B [T])` → length [m]  
  Larmor (gyro) radius of a charge q, mass m with speed v_perp across a field B: r = m v_perp / (|q| B)
- `skin_depth(ρ [Ω m], f [Hz], μ_r [1])` → length [m]  
  Skin depth of a conductor with resistivity ρ at frequency f (Hz), relative permeability μ_r: √(ρ / (π f μ_r μ₀))
- `wire_field(I [A], r [m])` → magnetic field [T]  
  Magnetic field at distance r from a long straight wire carrying current I: μ₀ I / (2π r)
- `solenoid_field(n [1/m], I [A])` → magnetic field [T]  
  Magnetic field inside a long solenoid with n turns per metre and current I: μ₀ n I

## mechanics

Projectiles, pendulums, energy and rockets. Angles are plain numbers: write 30° or 0.5 (radians).  Pass the local gravity as g, e.g. g_n.

- `projectile_range(v [m/s], θ [rad], g [m/s²])` → length [m]  
  Horizontal range of a projectile launched at speed v and angle θ above level ground, no air: v² sin 2θ / g
- `projectile_apex(v [m/s], θ [rad], g [m/s²])` → length [m]  
  Greatest height above the launch point: (v sin θ)² / 2g
- `projectile_flight_time(v [m/s], θ [rad], g [m/s²])` → time [s]  
  Time until the projectile lands back at launch height: 2 v sin θ / g
- `pendulum_period(L [m], g [m/s²])` → time [s]  
  Small-amplitude period of a simple pendulum of length L: 2π √(L/g)
- `pendulum_period_large(L [m], g [m/s²], θ0 [rad])` → time [s]  
  Exact period for amplitude θ0 (any angle below π): 4 √(L/g) K(sin²(θ0/2)), K the complete elliptic integral
- `spring_period(mass [kg], k [N/m])` → time [s]  
  Period of a mass on a spring with constant k: 2π √(mass/k)
- `kinetic_energy(mass [kg], v [m/s])` → energy [J]  
  Kinetic energy ½ m v²
- `relativistic_kinetic_energy(mass [kg], v [m/s])` → energy [J]  
  Relativistic kinetic energy (γ − 1) m c², written so it stays accurate for small v
- `potential_energy(mass [kg], g [m/s²], height [m])` → energy [J]  
  Gravitational potential energy m g h near a planet's surface
- `rocket_equation(v_e [m/s], m0 [kg], m_f [kg])` → speed [m/s]  
  Tsiolkovsky rocket equation, the speed gained: Δv = v_e ln(m0 / m_f), exhaust speed v_e, start and final masses
- `rocket_mass_ratio(Δv [m/s], v_e [m/s])` → a plain number (no units)  
  The mass ratio m0 / m_f a rocket needs to gain Δv: exp(Δv / v_e)

## nuclear

Binding energies, Q-values, radioactive decay and tunneling. A (mass number) and Z (proton number) are plain numbers.  Energies come out in MeV.

| Constant | Value | Meaning |
|---|---|---|
| `a_V` | `15.75 MeV` | semi-empirical mass formula coefficients (least-squares fit quoted by Rohlf, "Modern Physics", 1994) |
| `a_S` | `17.8 MeV` | — |
| `a_C` | `0.711 MeV` | — |
| `a_A` | `23.7 MeV` | — |
| `a_P` | `11.18 MeV` | — |

- `semf_pairing(A [1], Z [1])` → energy [J], shown in MeV  
  Pairing term of the SEMF: +a_P/√A for even-even nuclei, −a_P/√A for odd-odd, 0 for odd A
- `semf_binding(A [1], Z [1])` → energy [J], shown in MeV  
  Binding energy from the semi-empirical (Bethe–Weizsäcker) mass formula
- `semf_binding_per_nucleon(A [1], Z [1])` → energy [J]  
  Binding energy per nucleon B/A from the SEMF
- `Q_value(m_initial [kg], m_final [kg])` → energy [J], shown in MeV  
  Q-value of a reaction or decay from the total mass before and after: (m_initial − m_final) c²
- `nuclear_radius(A [1])` → length [m], shown in fm  
  Nuclear radius R = r0 A^(1/3) with r0 = 1.2 fm
- `decay_constant(t_half [s])` → frequency [1/s]  
  Decay constant λ = ln 2 / t½
- `mean_lifetime(t_half [s])` → time [s]  
  Mean lifetime τ = t½ / ln 2
- `activity(N [1], t_half [s])` → frequency [1/s], shown in Bq  
  Activity of N nuclei with half-life t½: A = λ N = N ln 2 / t½
- `remaining(N0, t_half [s], t [s])`  
  How much of N0 (a number, an activity or a mass) is left after time t: N0 2^(−t/t½)
- `bateman_daughter(N0, t_half_parent [s], t_half_daughter [s], t [s])`  
  Bateman two-member chain P → D (D starts at zero): the number of daughter nuclei at time t
- `sommerfeld_parameter(Z1 [1], Z2 [1], v [m/s])` → a plain number (no units)  
  Sommerfeld parameter η = Z1 Z2 α c / v of two nuclei with relative speed v
- `gamow_energy(Z1 [1], Z2 [1], μ [kg])` → energy [J], shown in MeV  
  Gamow energy E_G = 2 μ c² (π α Z1 Z2)² for reduced mass μ
- `gamow_factor(Z1 [1], Z2 [1], E [J], μ [kg])` → a plain number (no units)  
  Gamow (tunneling) factor exp(−2π η) = exp(−√(E_G/E)) at centre-of-mass energy E, reduced mass μ

## quantum

Textbook energy levels, wavelengths and tunneling. Quantum numbers n are plain numbers.  Energies of atomic scale come out in eV.

- `box_energy(n [1], mass [kg], L [m])` → energy [J], shown in eV  
  Energy of level n (1, 2, 3, …) of a particle of mass m in a 1-D infinite well of width L: n² π² ħ² / (2 m L²)
- `oscillator_energy(n [1], ω [rad/s])` → energy [J], shown in eV  
  Energy of level n (0, 1, 2, …) of a harmonic oscillator with angular frequency ω: (n + ½) ħ ω
- `de_broglie_wavelength(p [kg m/s])` → length [m], shown in nm  
  De Broglie wavelength of a particle with momentum p: h / p
- `de_broglie_wavelength_from_energy(mass [kg], E [J])` → length [m], shown in nm  
  De Broglie wavelength of a (non-relativistic) particle of mass m with kinetic energy E: h / √(2 m E)
- `photon_energy(λ [m])` → energy [J], shown in eV  
  Energy of a photon with wavelength λ: h c / λ
- `reduced_mass(m1 [kg], m2 [kg])` → mass [kg]  
  Reduced mass m1 m2 / (m1 + m2)
- `hydrogenlike_level(n [1], Z [1], M [kg])` → energy [J], shown in eV  
  Level n of a hydrogen-like atom, nuclear charge Z and nuclear mass M, with the reduced mass: −μ c² (Z α)² / (2 n²)
- `hydrogen_level(n [1])` → energy [J], shown in eV  
  Level n of hydrogen (−13.598 eV / n², including the proton's recoil through the reduced mass)
- `barrier_transmission(E [J], V0 [J], a [m], mass [kg])` → a plain number (no units)  
  Transmission probability through a rectangular barrier of height V0 and width a, for mass m at energy E (exact)

## stats

Weighted means, χ² and straight-line fits for lab data. xs, ys are lists with units; σs lists hold each value's uncertainty, in the same units. (mean, std, sum and len are built in; std is the sample standard deviation.)

- `standard_error(xs)`  
  Standard error of the mean of the measurements xs: std / √N
- `weighted_mean(xs, σs)`  
  Inverse-variance weighted mean Σ(x/σ²) / Σ(1/σ²) of values xs with uncertainties σs
- `weighted_mean_error(σs)`  
  Uncertainty of the weighted mean: 1 / √(Σ 1/σ²)
- `chi_squared(ys, model, σs)`  
  Χ² of measurements ys (uncertainties σs) against model values model: Σ ((y − model)/σ)²
- `reduced_chi_squared(ys, model, σs, n_params)`  
  Reduced χ²: χ² divided by the degrees of freedom N − n_params
- `linear_slope(xs, ys)`  
  Slope of the least-squares straight line y = intercept + slope x through the points (xs, ys)
- `linear_intercept(xs, ys)`  
  Intercept of the least-squares straight line through the points (xs, ys)
- `linear_slope_error(xs, ys)`  
  Standard error of the fitted slope, from the scatter of the points about the line (N − 2 degrees of freedom)
- `linear_intercept_error(xs, ys)`  
  Standard error of the fitted intercept
- `correlation(xs, ys)`  
  Pearson correlation coefficient r of the points (xs, ys), from −1 to 1
