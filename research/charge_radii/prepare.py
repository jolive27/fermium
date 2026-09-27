"""Convert the IAEA table of nuclear charge radii (charge_radii.csv, downloaded from
https://www-nds.iaea.org/radii/charge_radii.csv; Angeli & Marinova, ADNDT 99, 69 (2013)) into radii.csv.

Keeps the evaluated 2013 values (column radius_val) for Z >= 1; the "preliminary" post-2013 column and the neutron
(Z = 0, a mean-square radius, negative) are left out. Columns: Z, N, A, R [fm] (rms charge radius), dR [fm].
Run:  python3 prepare.py   (downloads the table if it isn't here)
"""
import csv
import os
import urllib.request

URL = "https://www-nds.iaea.org/radii/charge_radii.csv"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "charge_radii.csv")


def main():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    n = 0
    with open(os.path.join(HERE, "radii.csv"), "w", encoding="utf-8") as out:
        out.write("Z, N, A, R [fm], dR [fm]\n")
        for r in csv.DictReader(open(RAW, encoding="utf-8")):
            if int(r["z"]) < 1 or not r["radius_val"]:
                continue
            out.write(f"{r['z']}, {r['n']}, {r['a']}, {r['radius_val']}, {r['radius_unc']}\n")
            n += 1
    print(f"wrote {n} nuclei to radii.csv")


if __name__ == "__main__":
    main()
