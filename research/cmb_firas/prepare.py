"""Convert the COBE/FIRAS monopole spectrum (firas_monopole_spec_v1.txt, downloaded from NASA LAMBDA,
https://lambda.gsfc.nasa.gov/data/cobe/firas/monopole_spec/firas_monopole_spec_v1.txt) into firas.csv.

The numbers are copied unchanged; only the header is written in Fermium's form. Columns: wavenumber
sigma [1/cm], the monopole intensity I_MJy (MJy/sr), the residual from a 2.725 K blackbody r_kJy, the 1-sigma
uncertainty s_kJy and the modelled Galactic spectrum at the poles g_kJy (all kJy/sr). Fermium has no jansky
unit, so the intensities are plain numbers here and the program multiplies them by MJy = 1e-20 W/(m² Hz).
Run:  python3 prepare.py   (downloads the file if it isn't here)
"""
import os
import urllib.request

URL = "https://lambda.gsfc.nasa.gov/data/cobe/firas/monopole_spec/firas_monopole_spec_v1.txt"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "firas_monopole_spec_v1.txt")


def main():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    rows = [ln.split() for ln in open(RAW, encoding="ascii") if ln.strip() and not ln.startswith("#")]
    with open(os.path.join(HERE, "firas.csv"), "w", encoding="utf-8") as fh:
        fh.write("sigma [1/cm], I_MJy, r_kJy, s_kJy, g_kJy\n")
        for r in rows:
            fh.write(", ".join(r) + "\n")
    print(f"wrote {len(rows)} frequencies to firas.csv")


if __name__ == "__main__":
    main()
