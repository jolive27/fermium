# Research reproductions

Published physics results reproduced in Fermium. Each folder has the code, the plots, a README with the physics,
and a comparison with the published numbers; `legacy/tests/test_research.py` checks every program against an independent
computation, and the conformance suite (`make check`) runs every program with the `fermium` binary.

Reproductions 12 onwards are the **research track of Fermium 2.5 (spec C8)**: nuclear physics and astrophysics, each with
its data **downloaded from a cited source** (the raw file or the lines used are in the folder, with a `SOURCE.md` giving the
URL, date, citation and license, and a `prepare.py` that downloads and converts it), numbers compared with published values
in its README (agreements and honest disagreements), and a "Friction" section on what was awkward in Fermium.
`rust/crates/fermium-cli/tests/research_c8.rs` runs each of them with the `fermium` binary and compares the output with its
`expected_output.txt`. (They are not yet in the harvested conformance suite, which is frozen for v1 behaviour.)

| # | Reproduction | Published comparison | Status |
|---|---|---|---|
| 1 | [Semi-empirical mass formula vs AME2020](semf_ame2020/) | coefficients vs Rohlf/Krane; magic numbers 28, 50, 82, 126 | done |
| 2 | [Neutron-star mass–radius curve from the TOV equation (ideal neutron gas)](neutron_star_tov/) | M_max = 0.7102 M☉ vs 0.71 M☉ (Oppenheimer & Volkoff 1939); R = 9.16 km vs 9.6 km | done |
| 3 | [Lane–Emden polytropes and the Chandrasekhar mass](lane_emden_chandrasekhar/) | ξ₁, −ξ₁²θ′(ξ₁) for n = 1, 1.5, 3 vs Chandrasekhar (1939) table (all digits); M_Ch = 5.825/μ_e² M☉ vs 5.83 | done |
| 4 | [U-238 decay series: Bateman equations, secular equilibrium](u238_chain/) | 15 members (radau) vs the closed-form Bateman solution, all digits; Rn-222 99 % in-growth 25.40 d | done |
| 5 | [Hydrogen levels from the radial Schrödinger equation (shooting, reduced mass)](hydrogen_levels/) | n = 1..4, all l: Bohr × μ/m_e to 9×10⁻⁹; Lyman α 121.5684 nm vs 121.567 nm (NIST) | done |
| 6 | [Age of the universe for Planck 2018 ΛCDM (Friedmann equation)](friedmann_planck2018/) | t₀ = 13.791 Gyr vs 13.787 ± 0.020 Gyr (Planck 2018 VI); z_eq 3419 vs 3387 ± 21; D_C(z = 1100) = 13 866 Mpc | done |
| 7 | [Rutherford scattering: Monte Carlo vs dσ/dΩ ∝ 1/sin⁴(θ/2) and Geiger–Marsden 1913](rutherford_mc/) | 2×10⁷ α: χ² = 38.2 for 35 bins vs exact Rutherford; N sin⁴(θ/2) flat like Geiger–Marsden (±20 %; their Table II checked against the paper) | done |
| 8 | [pp chain vs CNO cycle: crossover temperature](pp_cno_crossover/) | 17.79 MK (Carroll & Ostlie rates), 18.06 MK (Kippenhahn & Weigert) vs ≈ 17–18 MK; ν = 3.9 and 19.9 vs T⁴, T^19.9 | done |
| 9 | [Big Bang nucleosynthesis: a stiff 12-reaction network from 10 MeV to 10⁴ s (radau)](bbn_network/) | Y_p = 0.2423 vs 0.2471 (Fields 2020; Born weak rates, −1.9 %); D/H 2.60×10⁻⁵ vs 2.51×10⁻⁵; ³He/H 1.03×10⁻⁵; ⁷Li/H 4.36×10⁻¹⁰ vs 4.7–5.6 (rates checked against NUC123; one coefficient fixed); SciPy agrees to 3×10⁻⁶ | done |
| 10 | [Nuclear shell model: Woods–Saxon + spin–orbit levels and the magic numbers](shell_model_magic_numbers/) | 7 largest shell gaps at N = 2, 8, 20, 28, 50, 82, 126 (without l·s: 2, 8, 20, 40, 70); ²⁰⁸Pb N = 126 gap 3.54 vs 3.43 MeV; 13 levels near the Fermi surface vs AME2020 + ENSDF, rms 0.478 MeV (Bohr & Mottelson parameters) | done |
| 11 | [Hydrogen recombination: Saha vs Peebles' three-level atom, visibility function (radau)](recombination_history/) | τ = 1 at z_* = 1089.61 vs 1089.92 ± 0.25 (Planck 2018); visibility peak z = 1089.1 (conformal time), 1078.8 (per unit z); x_e(200) = 3.85×10⁻⁴ vs ≈ 2×10⁻⁴ (RECFAST/HyRec residual); SciPy Radau agrees to 10⁻⁵ | done |
| 12 | [The CMB blackbody from the COBE/FIRAS spectrum](cmb_firas/) | weighted fit T = 2.725015 K ± 7.9 μK (file built on 2.725 K; Fixsen & Mather 2002: 2.725 ± 0.001); μ = (−1.1 ± 3.6)×10⁻⁵ vs (−1 ± 4)×10⁻⁵ (Fixsen 1996); rms residual 49 ppm of peak vs < 50 ppm; n_γ = 410.5 cm⁻³, Ω_γh² = 2.471×10⁻⁵ (PDG) | done |
| 13 | [Nuclear charge radii against the Angeli–Marinova table](charge_radii/) | 834 nuclei: r₀ = 0.9487 fm, √(5/3) r₀ = 1.225 fm vs R₀ ≈ 1.2 fm; ⁴⁸Ca − ⁴⁰Ca = −0.0005 fm (A^⅓: +0.20); Pb kink at N = 126, slope ratio 1.86; textbook Fermi model 3 % low for ²⁰⁸Pb | done |
| 14 | [The Gamow window of key stellar reactions vs JINA REACLIB](gamow_window/) | constant-S Gamow integral / REACLIB at 15.7 MK: pp 0.917, ³He+³He 1.02, ³He+α 0.995, ⁷Be+p 1.04, ¹⁴N+p 0.930, ¹²C+p 0.70; ¹²C(α,γ) at 0.2 GK 1.01 (E₀ = 315 keV); Gaussian closed form low by 1 + 5/(12τ) | done |
| 15 | [Alpha decay: Gamow tunnelling and Geiger–Nuttall vs NUBASE2020](alpha_decay/) | 103 even–even emitters over 24.6 decades: Gamow model offset 0.99 decades (preformation ≈ 0.1), scatter 0.35; Viola–Seaborg with Sobiczewski 1989 coefficients: rms 0.20 for N > 126 | done |
| 16 | [Pulsar spin-down: dipole fields and ages (ATNF catalogue)](pulsar_spindown/) | 2052 pulsars: B = 3.200×10¹⁹ G √(PṖ) from μ₀, c, I, R; Crab Ė = 4.46×10³⁸ erg/s, braking index 2.342 (catalogue ν̈) vs 2.509 (Lyne 1993), birth period 19.3 ms vs ≈ 19 ms | done |
| 17 | [Main-sequence mass–luminosity relation from eclipsing binaries (DEBCat)](mass_luminosity/) | L = 4πR²σT⁴ for 576 main-sequence stars; slopes vs Eker et al. 2018 in six mass ranges: 4 agree within 1σ, 0.72–1.05 M☉ (3.9 vs 5.7) and 1.05–2.4 M☉ (4.0 vs 4.3) do not; one power law α = 3.79 | done |
| 18 | [The accelerating universe from the Pantheon+ supernovae](supernova_hubble/) | 1590 light curves, diagonal errors: flat ΛCDM Ωm = 0.350 ± 0.012 vs 0.334 ± 0.018 (Brout 2022, full covariance); Einstein–de Sitter Δχ² = +661; ΩΛ > 0 at 9.2σ | done |
| 19 | [White dwarf cooling ages: Mestel's theory vs the Gaia 100 pc white dwarfs](white_dwarf_cooling/) | 16 281 white dwarfs: Mestel's L(T_c) derived with units, t ∝ L^(−5/7) exactly; luminosity-function slope −0.61 ± 0.04 vs −0.71; cut-off at log L/L☉ = −4.50 (Winget 1987: ≈ −4.5), but Mestel's age there is 3.8 Gyr vs 9.3 ± 2 Gyr (no crystallisation or convective coupling) | done |
| 20 | [Neutron star cooling vs 48 thermally emitting neutron stars (Ioffe table)](neutron_star_cooling/) | one-zone model from constants (C₉ = 1.3×10³⁹ erg/K, modified Urca N₉ = 7.5×10⁴⁰ erg/s, GPE envelope): 34 of 48 stars within ×3, median ratio 0.65; 3C 58 20 × too cold (enhanced cooling, Slane 2002); Cas A consistent | done |
