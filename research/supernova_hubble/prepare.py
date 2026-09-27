"""Convert the Pantheon+ distance table (Pantheon+SH0ES.dat, downloaded from the Pantheon+ data release,
https://github.com/PantheonPlusSH0ES/DataRelease, file Pantheon+_Data/4_DISTANCES_AND_COVAR/Pantheon+SH0ES.dat) into
sne.csv for Fermium.

Keeps the light curves with zHD > 0.01 (Brout et al. 2022 use this cut for the cosmology fits; below it peculiar
velocities dominate). Columns (unchanged numbers): z (zHD, the Hubble-diagram redshift), zhel (heliocentric redshift),
mb (m_b_corr, the corrected B-band peak magnitude), dmb (m_b_corr_err_DIAG, the diagonal error).
Run:  python3 prepare.py   (downloads the file if it isn't here)
"""
import os
import urllib.request

URL = ("https://raw.githubusercontent.com/PantheonPlusSH0ES/DataRelease/main/"
       "Pantheon%2B_Data/4_DISTANCES_AND_COVAR/Pantheon%2BSH0ES.dat")
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "Pantheon+SH0ES.dat")


def main():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    lines = open(RAW, encoding="ascii").read().splitlines()
    head = lines[0].split()
    col = {name: i for i, name in enumerate(head)}
    rows = []
    for ln in lines[1:]:
        f = ln.split()
        if float(f[col["zHD"]]) <= 0.01:
            continue
        rows.append(", ".join(f[col[k]] for k in ("zHD", "zHEL", "m_b_corr", "m_b_corr_err_DIAG")))
    with open(os.path.join(HERE, "sne.csv"), "w", encoding="utf-8") as out:
        out.write("z, zhel, mb, dmb\n")
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} light curves to sne.csv")


if __name__ == "__main__":
    main()
