"""Convert DEBCat (John Southworth's catalogue of detached eclipsing binaries; debs.dat, downloaded from
https://www.astro.keele.ac.uk/jkt/debcat/debs.dat) into stars.csv for Fermium: one row per component star.

Columns (the catalogue's own numbers, unchanged): logM (M in solar masses), logM_e, logR (R in solar radii), logg (cgs),
logT (Teff in K), logL (log L/L☉ as given by the catalogue, -9.99 where missing). Stars with a missing mass, radius or
temperature (-9.99) are left out.
Run:  python3 prepare.py   (downloads the file if it isn't here)
"""
import os
import urllib.request

URL = "https://www.astro.keele.ac.uk/jkt/debcat/debs.dat"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "debs.dat")


def main():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    rows = []
    for ln in open(RAW, encoding="latin-1"):
        if ln.startswith("#") or not ln.strip():
            continue
        f = ln.split()
        # System SpT1 SpT2 Pday Vmag BmV logM1 logM1e logM2 logM2e logR1 logR1e logR2 logR2e logg1 logg1e logg2 logg2e
        # logT1 logT1e logT2 logT2e logL1 logL1e logL2 logL2e MoH MoHe
        v = f[6:]
        for k in (0, 1):
            logm, logme = v[0 + 2 * k], v[1 + 2 * k]
            logr, logg = v[4 + 2 * k], v[8 + 2 * k]
            logt, logl = v[12 + 2 * k], v[16 + 2 * k]
            if min(float(logm), float(logr), float(logt)) < -9:
                continue
            rows.append(f"{logm}, {logme}, {logr}, {logg}, {logt}, {logl}")
    with open(os.path.join(HERE, "stars.csv"), "w", encoding="utf-8") as out:
        out.write("logM, logM_e, logR, logg, logT, logL\n")
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} stars to stars.csv")


if __name__ == "__main__":
    main()
