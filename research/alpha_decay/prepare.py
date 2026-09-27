"""Select the even-even alpha emitters from NUBASE2020 (nubase_4.mas20.txt, downloaded from
https://www-nds.iaea.org/amdc/ame2020/nubase_4.mas20.txt) and write alpha.csv for Fermium.

For every ground state with even Z and N, 84 <= Z <= 104, a measured half-life and mass excess (no '#'), and a
measured alpha branch ("A=..." or "A~..." in the decay-mode field), the alpha Q value is computed from the NUBASE
mass excesses, Q = Δ(parent) − Δ(daughter) − Δ(⁴He), and the partial alpha half-life is T½ / b_α.
Columns: Z, N, A, Q [MeV], T [s] (total half-life), b (alpha branch, a fraction), logT (log10 of the partial alpha
half-life in seconds). The NUBASE lines used (parents, daughters, ⁴He) are copied verbatim to nubase_excerpt.txt.
Run:  python3 prepare.py   (downloads the table if it isn't here; 0.8 MB)
"""
import math
import os
import re
import urllib.request

URL = "https://www-nds.iaea.org/amdc/ame2020/nubase_4.mas20.txt"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "nubase_4.mas20.txt")
UNITS = {"ys": 1e-24, "zs": 1e-21, "as": 1e-18, "fs": 1e-15, "ps": 1e-12, "ns": 1e-9, "us": 1e-6, "ms": 1e-3, "s": 1.0,
         "m": 60.0, "h": 3600.0, "d": 86400.0, "y": 31556952.0}   # NUBASE2020 uses 1 y = 365.2422 d
for p, f in (("k", 1e3), ("M", 1e6), ("G", 1e9), ("T", 1e12), ("P", 1e15), ("E", 1e18), ("Z", 1e21), ("Y", 1e24)):
    UNITS[p + "y"] = f * 31556952.0


def lines():
    if not os.path.exists(RAW):
        req = urllib.request.Request(URL, headers={"User-Agent": "curl/8"})
        open(RAW, "wb").write(urllib.request.urlopen(req).read())
    return [ln for ln in open(RAW, encoding="latin-1") if not ln.startswith("#")]


def main():
    table = {}
    for ln in lines():
        a, zi = int(ln[0:3]), ln[4:8]
        if zi[3] != "0":
            continue            # ground states only
        table[(int(zi[:3]), a)] = ln.rstrip("\n")
    he4 = table[(2, 4)]
    me = lambda ln: float(ln[18:31])        # noqa: E731
    rows, used = [], [he4]
    for (z, a), ln in sorted(table.items()):
        n = a - z
        if z % 2 or n % 2 or not 84 <= z <= 104 or (z - 2, a - 4) not in table:
            continue
        d = table[(z - 2, a - 4)]
        if "#" in ln[18:42] or "#" in d[18:42] or "#" in ln[69:81]:
            continue
        m = re.search(r"\bA[=~]\s*([\d.]+)", ln[119:])
        t, unit = ln[69:78].strip(), ln[78:80].strip()
        if not m or unit not in UNITS or not re.fullmatch(r"[\d.]+", t):
            continue
        b = float(m.group(1)) / 100.0
        if b <= 0:
            continue
        T = float(t) * UNITS[unit]
        Q = (me(ln) - me(d) - me(he4)) / 1000.0
        rows.append((z, n, a, Q, T, b, math.log10(T / b)))
        used += [ln, d]
    with open(os.path.join(HERE, "alpha.csv"), "w", encoding="utf-8") as out:
        out.write("Z, N, A, Q [MeV], T [s], b, logT\n")
        for z, n, a, Q, T, b, lt in rows:
            out.write(f"{z}, {n}, {a}, {Q:.6f}, {T:.6e}, {b:.6g}, {lt:.6f}\n")
    with open(os.path.join(HERE, "nubase_excerpt.txt"), "w", encoding="latin-1") as raw:
        raw.write("# NUBASE2020 (nubase_4.mas20.txt), the ground-state lines used by prepare.py (parents, daughters, 4He)\n")
        for ln in dict.fromkeys(used):
            raw.write(ln + "\n")
    print(f"wrote {len(rows)} alpha emitters to alpha.csv")


if __name__ == "__main__":
    main()
