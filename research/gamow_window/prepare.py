"""Extract the JINA REACLIB fits and AME2020 masses the Gamow-window program needs.

REACLIB: the JINA REACLIB web site (https://reaclib.jinaweb.org/) blocks scripted downloads (a bot-protection page),
so the snapshot is taken from pynucastro 3.1.0 (BSD-3), which ships the REACLIB "default2" library downloaded from
reaclib.jinaweb.org on 2025-03-30 (file pynucastro/data/reaclib_default2_20250330), fetched from PyPI:
  https://files.pythonhosted.org/packages/63/43/55cab9d3829d9c0ec9a032092929edd8fa3e8e5566b3f89c75ce50a35708/pynucastro-3.1.0-py3-none-any.whl
The raw entries used (three lines each, REACLIB 2 format) are copied verbatim to reaclib_excerpt.txt, and their seven
coefficients a0..a6 to reaclib.csv (one row per fit set; a reaction's rate is the sum of its sets):
  N_A<σv> = Σ exp(a0 + a1/T9 + a2 T9^(-1/3) + a3 T9^(1/3) + a4 T9 + a5 T9^(5/3) + a6 ln T9)  cm³ mol⁻¹ s⁻¹.

Masses: AME2020 (https://www-nds.iaea.org/amdc/ame2020/mass_1.mas20.txt), atomic masses in micro-u; the nuclear
masses subtract Z electron masses (electron binding energies, < 0.1 keV here, are neglected). The raw lines used are
copied to ame2020_excerpt.txt, the masses to masses.csv.
Run:  python3 prepare.py   (downloads both, about 4 MB and 0.5 MB)
"""
import io
import os
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
WHEEL = ("https://files.pythonhosted.org/packages/63/43/55cab9d3829d9c0ec9a032092929edd8fa3e8e5566b3f89c75ce50a35708/"
         "pynucastro-3.1.0-py3-none-any.whl")
AME = "https://www-nds.iaea.org/amdc/ame2020/mass_1.mas20.txt"

# reaction id -> (REACLIB nuclides as written in the set header, labels to keep (None: all), chapter)
REACTIONS = {
    "pp": (["p", "p", "d"], ["bet+"]),          # p(p, e+ ν)d; the "ecw" set is the pep reaction, left out
    "he3he3": (["he3", "he3", "p", "p", "he4"], None),
    "he3he4": (["he4", "he3", "be7"], None),
    "be7p": (["p", "be7", "b8"], None),
    "c12p": (["p", "c12", "n13"], None),
    "n14p": (["p", "n14", "o15"], None),
    "c12a": (["he4", "c12", "o16"], None),
}
NUCLEI = {"H1": (1, 1), "He3": (2, 3), "He4": (2, 4), "Be7": (4, 7), "C12": (6, 12), "N14": (7, 14)}
M_E_MICRO_U = 548.579909065     # electron mass in micro-u (CODATA 2018, as AME2020 uses)


def reaclib_text():
    path = os.path.join(HERE, "pynucastro-3.1.0-py3-none-any.whl")
    data = open(path, "rb").read() if os.path.exists(path) else urllib.request.urlopen(WHEEL).read()
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        return z.read("pynucastro/data/reaclib_default2_20250330").decode("ascii")


def sets(text):
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        ln = lines[i]
        if ln.strip() in [str(k) for k in range(1, 12)]:
            i += 1
            continue
        if i + 2 < len(lines):
            head = ln[5:35].split()
            label = ln[43:47].strip()
            coef = lines[i + 1][:52] + lines[i + 2][:39]
            a = [float(coef[13 * k:13 * k + 13]) for k in range(7)]
            yield head, label, ln[47:48], [ln, lines[i + 1], lines[i + 2]], a
        i += 3


def main():
    text = reaclib_text()
    with open(os.path.join(HERE, "reaclib.csv"), "w", encoding="utf-8") as csv, \
            open(os.path.join(HERE, "reaclib_excerpt.txt"), "w", encoding="ascii") as raw:
        csv.write("reaction, a0, a1, a2, a3, a4, a5, a6\n")
        raw.write("# JINA REACLIB default2 snapshot of 2025-03-30 (via pynucastro 3.1.0); entries used by gamow.fm\n")
        for k, (nuc, labels) in enumerate(REACTIONS.items(), start=1):
            nuclides, keep = labels
            for head, label, flag, rawlines, a in sets(text):
                if head == nuclides and (keep is None or label in keep):
                    csv.write(f"{k}, " + ", ".join(f"{x:.6e}" for x in a) + "\n")
                    raw.write("\n".join(rawlines) + "\n")
    amelines = urllib.request.urlopen(urllib.request.Request(AME, headers={"User-Agent": "curl/8"})).read().decode("latin-1").splitlines()
    with open(os.path.join(HERE, "masses.csv"), "w", encoding="utf-8") as out, \
            open(os.path.join(HERE, "ame2020_excerpt.txt"), "w", encoding="latin-1") as raw:
        out.write("Z, A, M [u]\n")
        raw.write("# AME2020 mass_1.mas20.txt, the lines used by gamow.fm (atomic mass in micro-u at columns 107-123)\n")
        for name, (z, a) in NUCLEI.items():
            ln = next(x for x in amelines if len(x) > 120 and x[10:15].strip() == str(z) and x[15:20].strip() == str(a)
                      and x[0] in " 0")
            raw.write(ln + "\n")
            micro = float(ln[106:109]) * 1e6 + float(ln[110:123].replace("#", ""))
            out.write(f"{z}, {a}, {(micro - z * M_E_MICRO_U) / 1e6:.12f}\n")
    print("wrote reaclib.csv, masses.csv and the raw excerpts")


if __name__ == "__main__":
    main()
