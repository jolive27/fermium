"""Convert the AME2020 mass table (mass_1.mas20.txt, downloaded from
https://www-nds.iaea.org/amdc/ame2020/mass_1.mas20.txt) into ame2020_binding.csv for Fermium.

Keeps measured values only (entries marked # are extrapolated), A >= 16 (the liquid-drop formula isn't
meant for the lightest nuclei).  Columns: Z, N, A, P (pairing parity: +1 even-even, -1 odd-odd, 0 odd A),
B [MeV] (total binding energy = binding energy per nucleon × A).
Run:  python3 prepare.py   (downloads the table if it isn't here)
"""
import os
import urllib.request

URL = "https://www-nds.iaea.org/amdc/ame2020/mass_1.mas20.txt"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "mass_1.mas20.txt")


def rows():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    lines = open(RAW, encoding="latin-1").read().splitlines()
    start = next(i for i, ln in enumerate(lines) if ln.startswith("1N-Z")) + 2   # header of the data block
    for ln in lines[start:]:
        if len(ln) < 70:
            continue
        # fixed-width format (see the file header): N at 5:10, Z at 10:15, A at 15:20,
        # binding energy per nucleon (keV) at 54:67
        n, z, a = int(ln[5:10]), int(ln[10:15]), int(ln[15:20])
        be = ln[54:67].strip()
        if "#" in be or not be or "*" in be:
            continue
        yield z, n, a, float(be) * a / 1000.0


def main():
    out = os.path.join(HERE, "ame2020_binding.csv")
    count = 0
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("Z, N, A, P, B [MeV]\n")
        for z, n, a, b in rows():
            if a < 16:
                continue
            p = 1 if z % 2 == 0 and n % 2 == 0 else (-1 if z % 2 == 1 and n % 2 == 1 else 0)
            fh.write(f"{z}, {n}, {a}, {p}, {b:.6f}\n")
            count += 1
    print(f"wrote {count} nuclei to {out}")


if __name__ == "__main__":
    main()
