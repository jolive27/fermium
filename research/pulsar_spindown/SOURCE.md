# Data source

- **File:** `atnf_vizier_B_psr.tsv` (raw VizieR output, unchanged), converted to `pulsars.csv` by `prepare.py`.
- **URL:** https://vizier.cds.unistra.fr/viz-bin/asu-tsv?-source=B/psr/psr&-out.max=unlimited&-out=PSRJ&-out=Name&-out=P0&-out=P1&-out=F0&-out=F1&-out=F2&-out=PEpoch&-out=Dist&-out=Assoc&-out=Binary&-out=Type&-out=Age&-out=Bsurf&-out=Edot
  (VizieR catalogue B/psr, published in VizieR 2017-07-18). The ATNF site itself
  (https://www.atnf.csiro.au/research/pulsar/psrcat/) reset the connection from this machine, so the VizieR copy was used;
  it is an older version of the catalogue than the live one (2584 lines, 2052 pulsars with Ṗ > 0).
- **Downloaded:** 2026-09-27.
- **Citation:** R. N. Manchester, G. B. Hobbs, A. Teoh and M. Hobbs, "The Australia Telescope National Facility Pulsar
  Catalogue", AJ **129**, 1993 (2005); https://www.atnf.csiro.au/research/pulsar/psrcat/.
- **License:** VizieR/CDS terms (free for research, cite the catalogue and CDS): https://cds.unistra.fr/vizier-org/licences_vizier.html
- **Published values compared (typed from the paper):** the Crab pulsar's braking index n = 2.509 ± 0.001 and birth period
  ≈ 19 ms, A. G. Lyne, R. S. Pritchard and F. Graham Smith, MNRAS **265**, 1003 (1993).
