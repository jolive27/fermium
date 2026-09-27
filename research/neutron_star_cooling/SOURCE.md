# Data source

- **File:** `cooldat.html` (raw page, unchanged; the table is preformatted text in it), converted to `ns.csv` by `prepare.py`.
- **URL:** http://www.ioffe.ru/astro/NSG/thermal/cooldat.html (Ioffe Institute, Neutron Star Group, "Thermal emission of
  neutron stars: cooling neutron stars"; page last updated 03.08.2026 according to its header).
- **Downloaded:** 2026-09-27 (curl, HTTP 200).
- **Citation:** A. Y. Potekhin, D. A. Zyuzin, D. G. Yakovlev, M. V. Beznogov and Yu. A. Shibanov, "Thermal luminosities of
  cooling neutron stars", MNRAS **496**, 5052 (2020); each entry's own references are linked on the page.
- **License:** a public research compilation; cite the MNRAS paper (and the original measurements).
- **Physics inputs (typed from the papers, not downloaded):** modified-Urca emissivity 8.1 × 10²¹ (n/n₀)^(2/3) T₉⁸ erg cm⁻³ s⁻¹
  (B. L. Friman & O. V. Maxwell, ApJ **232**, 541 (1979), in the form of D. G. Yakovlev et al., Phys. Rep. **354**, 1 (2001));
  envelope relation T_s = 0.87 × 10⁶ K g₁₄^(1/4) (T_b/10⁸ K)^0.55 (E. H. Gudmundsson, C. J. Pethick & R. I. Epstein,
  ApJ **272**, 286 (1983)); 3C 58 too cold for standard cooling (P. O. Slane, D. J. Helfand & S. S. Murray, ApJ **571**, L45 (2002)).
