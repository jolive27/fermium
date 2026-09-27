"""Convert the ATNF Pulsar Catalogue as served by VizieR (catalogue B/psr, Manchester et al. 2005, AJ 129, 1993) into
pulsars.csv for Fermium. The raw file atnf_vizier_B_psr.tsv was downloaded with
  https://vizier.cds.unistra.fr/viz-bin/asu-tsv?-source=B/psr/psr&-out.max=unlimited&-out=PSRJ&-out=Name&-out=P0
  &-out=P1&-out=F0&-out=F1&-out=F2&-out=PEpoch&-out=Dist&-out=Assoc&-out=Binary&-out=Type&-out=Age&-out=Bsurf&-out=Edot
(one line; the ATNF site itself, https://www.atnf.csiro.au/research/pulsar/psrcat/, reset the connection).

pulsars.csv: every pulsar with a measured P and a positive Pdot. Columns: P [s], Pdot (s/s), binary (1 if it has a binary
model), age [yr], B [gauss], Edot [erg/s] (the catalogue's derived values, for comparison), crab (1 for B0531+21),
vela (1 for B0833-45), F0 [1/s], F1 [1/s²], F2 [1/s³] (0 where the catalogue has none).
Run:  python3 prepare.py
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    rows = []
    for ln in open(os.path.join(HERE, "atnf_vizier_B_psr.tsv"), encoding="utf-8"):
        if ln.startswith("#") or not ln.strip() or ln.startswith("PSRJ") or ln.startswith("-"):
            continue
        f = [x.strip() for x in ln.rstrip("\n").split("\t")]
        if len(f) < 15 or not f[0] or not f[2] or not f[3]:
            continue
        psrj, name, p0, p1, f0, f1, f2, _ep, _d, _assoc, binary, _typ, age, bs, edot = f[:15]
        try:
            P, Pd = float(p0), float(p1)
        except ValueError:
            continue
        if Pd <= 0:
            continue
        num = lambda s: s if s else "0"    # noqa: E731
        rows.append(f"{P:.14g}, {Pd:.10g}, {1 if binary else 0}, {num(age)}, {num(bs)}, {num(edot)}, "
                    f"{1 if name == 'B0531+21' else 0}, {1 if name == 'B0833-45' else 0}, {num(f0)}, {num(f1)}, {num(f2)}")
    with open(os.path.join(HERE, "pulsars.csv"), "w", encoding="utf-8") as out:
        out.write("P [s], Pdot, binary, age [yr], B [gauss], Edot [erg/s], crab, vela, F0 [1/s], F1 [1/s²], F2 [1/s³]\n")
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} pulsars to pulsars.csv")


if __name__ == "__main__":
    main()
