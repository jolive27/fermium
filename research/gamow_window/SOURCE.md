# Data sources

## JINA REACLIB reaction-rate fits
- **Files:** `reaclib_excerpt.txt` (the 14 fit sets used, three lines each, verbatim) and `reaclib.csv` (their coefficients a0..a6),
  written by `prepare.py`.
- **Origin:** the JINA REACLIB database, https://reaclib.jinaweb.org/ ("default2" library, snapshot of 2025-03-30).
  The site answers scripted requests with a bot-protection page (Incapsula), so the snapshot was taken from the
  copy that pynucastro 3.1.0 ships (`pynucastro/data/reaclib_default2_20250330`, downloaded by pynucastro's authors from
  reaclib.jinaweb.org), fetched from PyPI:
  https://files.pythonhosted.org/packages/63/43/55cab9d3829d9c0ec9a032092929edd8fa3e8e5566b3f89c75ce50a35708/pynucastro-3.1.0-py3-none-any.whl
- **Downloaded:** 2026-09-27.
- **Citation:** R. H. Cyburt et al., "The JINA REACLIB Database: Its Recent Updates and Impact on Type-I X-ray Bursts",
  ApJS **189**, 240 (2010). The sets used: p(p,e⁺ν)d `bet+`; ³He(³He,2p)⁴He `nacr` (NACRE, Angulo et al. 1999);
  ³He(α,γ)⁷Be `cd08` (Cyburt & Davids 2008); ⁷Be(p,γ)⁸B `nacr`; ¹²C(p,γ)¹³N `ls09`; ¹⁴N(p,γ)¹⁵O `im05` (Imbriani et al. 2005);
  ¹²C(α,γ)¹⁶O `nac2` (NACRE II, Xu et al. 2013).
- **License:** REACLIB is freely available for research; pynucastro is BSD-3-Clause.

## AME2020 masses
- **Files:** `ame2020_excerpt.txt` (the six lines used, verbatim) and `masses.csv` (nuclear masses: atomic mass − Z mₑ).
- **URL:** https://www-nds.iaea.org/amdc/ame2020/mass_1.mas20.txt
- **Downloaded:** 2026-09-27.
- **Citation:** M. Wang, W. J. Huang, F. G. Kondev, G. Audi, S. Naimi, Chinese Physics C **45**, 030003 (2021).
- **License:** freely available (IAEA AMDC); cite the paper.

## S-factors (typed from the papers, not downloaded)
- S(0) for p+p (4.01 × 10⁻²⁵ MeV b), ³He+³He (5.21 MeV b), ³He+α (0.56 keV b), ⁷Be+p (20.8 eV b), ¹²C+p (1.34 keV b) and
  ¹⁴N+p (1.66 keV b): E. G. Adelberger et al., "Solar fusion cross sections II", Rev. Mod. Phys. **83**, 195 (2011), the
  recommended values of its summary table.
- S(300 keV) = 140 keV b for ¹²C(α,γ)¹⁶O: R. J. deBoer et al., Rev. Mod. Phys. **89**, 035007 (2017).
