"""Convert the RIPL-3 s-wave neutron resonance parameters (resonances0.dat, downloaded from
https://www-nds.iaea.org/RIPL-3/resonances/resonances0.dat) into d0.csv for Fermium.

Keeps every target with a measured mean s-wave resonance spacing D0. Columns (the file's numbers, unchanged): Z, A (of the
target), I0 (target spin), Bn [MeV] (neutron binding energy of the compound nucleus A+1), D0 [keV], dD [keV].
Run:  python3 prepare.py   (downloads the file if it isn't here)
"""
import os
import urllib.request

URL = "https://www-nds.iaea.org/RIPL-3/resonances/resonances0.dat"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "resonances0.dat")


def main():
    if not os.path.exists(RAW):
        req = urllib.request.Request(URL, headers={"User-Agent": "curl/8"})
        open(RAW, "wb").write(urllib.request.urlopen(req).read())
    rows = []
    for ln in open(RAW, encoding="latin-1"):
        if ln.startswith("#") or not ln.strip():
            continue
        f = ln.split()
        z, a, i0, bn, d0, dd = f[0], f[2], f[3], f[4], f[5], f[6]
        try:
            if float(d0) <= 0:
                continue
        except ValueError:
            continue
        rows.append(f"{z}, {a}, {float(i0)}, {bn}, {d0}, {dd}")
    with open(os.path.join(HERE, "d0.csv"), "w", encoding="utf-8") as out:
        out.write("Z, A, I0, Bn [MeV], D0 [keV], dD [keV]\n")
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} targets to d0.csv")


if __name__ == "__main__":
    main()
