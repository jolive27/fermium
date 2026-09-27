"""Convert the Ioffe table of thermally emitting neutron stars (cooldat.html, downloaded from
http://www.ioffe.ru/astro/NSG/thermal/cooldat.html; Potekhin et al. 2020, MNRAS 496, 5052, updated online) into
ns.csv for Fermium.

The table is preformatted text inside the HTML. Kept: the four groups with measured thermal luminosities (weakly
magnetized CCO-like stars, ordinary pulsars, high-B pulsars, the "Magnificent Seven"); the upper limits and the
small hot spots are left out. For each star: the age t (the independent age t* when known: its "mid" value, or the
middle of its min–max interval; otherwise the characteristic age tc, as the table's authors do), and the redshifted
thermal luminosity L (the "mid" value, or the geometric mean of min and max when only those are given) with its
min and max. Columns: group (1 CCO, 2 pulsar, 3 high-B, 4 XINS), agetype (1 = t*, 0 = tc), t [yr], L [erg/s],
Lmin [erg/s], Lmax [erg/s].
Run:  python3 prepare.py   (downloads the page if it isn't here)
"""
import math
import os
import re
import urllib.request

URL = "http://www.ioffe.ru/astro/NSG/thermal/cooldat.html"
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "cooldat.html")
GROUPS = {"Weakly magnetized": 1, "Ordinary pulsars": 2, "High-B pulsars": 3, "XINSs": 4}


def val(s):
    return None if s.startswith("---") else float(s)


def main():
    if not os.path.exists(RAW):
        urllib.request.urlretrieve(URL, RAW)
    text = re.sub(r"<[^>]*>", "", open(RAW, encoding="latin-1").read())
    group, rows = None, []
    for ln in text.splitlines():
        head = ln.strip()
        if head.endswith(":") and not head.startswith("Reference"):
            group = next((g for k, g in GROUPS.items() if head.startswith(k)), None)
            continue
        if group is None or len(ln) < 60 or ln.startswith(("-", "=")):
            continue
        tok = ln[46:].split()
        if len(tok) < 16:
            continue
        v = [val(t) for t in tok[:16]]
        _dmin, _dmid, _dmax, _p, _pd, _b, tc, tmin, tmid, tmax, lmin, lmid, lmax = v[:13]
        if tmid is not None:
            t, kind = tmid, 1
        elif tmin is not None and tmax is not None:
            t, kind = (tmin + tmax) / 2, 1
        elif tc is not None:
            t, kind = tc, 0
        else:
            continue
        if lmid is None:
            if lmin is None or lmax is None:
                continue
            lmid = math.sqrt(lmin * lmax)
        lo = lmin if lmin is not None else lmid
        hi = lmax if lmax is not None else lmid
        rows.append(f"{group}, {kind}, {t:.4g}, {lmid * 1e33:.4g}, {lo * 1e33:.4g}, {hi * 1e33:.4g}")
    with open(os.path.join(HERE, "ns.csv"), "w", encoding="utf-8") as out:
        out.write("group, agetype, t [yr], L [erg/s], Lmin [erg/s], Lmax [erg/s]\n")
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} neutron stars to ns.csv")


if __name__ == "__main__":
    main()
