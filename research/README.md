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
