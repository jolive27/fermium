# Research reproductions

Published physics results reproduced in Fermium. Each folder has the code, the plots, a README with the physics,
and a comparison with the published numbers; `tests/test_research.py` checks every program against an independent
computation.

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
