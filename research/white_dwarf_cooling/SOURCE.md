# Data source

- **File:** `gaia_edr3_wd_100pc.csv` (raw TAP result, unchanged: 16 676 white dwarfs), converted to `wd.csv` by `prepare.py`
  (the 16 281 with a pure-hydrogen atmosphere fit).
- **Query:** VizieR TAP service, catalogue J/MNRAS/508/3877 (table `maincat`):
  `SELECT WDJname, Plx, e_Plx, Pwd, "GMAG", "BP-RP", TeffH, e_TeffH, loggH, e_loggH, MassH, e_MassH FROM "J/MNRAS/508/3877/maincat" WHERE Plx > 10 AND Pwd > 0.75`
  sent to https://tapvizier.cds.unistra.fr/TAPVizieR/tap/sync (REQUEST=doQuery, LANG=ADQL, FORMAT=csv); the full URL is in `prepare.py`.
- **Downloaded:** 2026-09-27.
- **Citation:** N. P. Gentile Fusillo et al., "A catalogue of white dwarfs in Gaia EDR3", MNRAS **508**, 3877 (2021);
  Gaia Collaboration, "Gaia Early Data Release 3: Summary of the contents and survey properties", A&A **649**, A1 (2021).
- **License:** VizieR/CDS terms (free for research with citation); Gaia data under the ESA Gaia archive terms (CC BY-SA 3.0 IGO).
- **Published values compared (typed from the papers):** D. E. Winget et al., "An independent method for determining the age
  of the universe", ApJ **315**, L77 (1987): the disk age 9.3 ± 2.0 Gyr from the cut-off of the white dwarf luminosity
  function; L. Mestel, MNRAS **112**, 583 (1952): t_cool ∝ L^(−5/7).
