"""Convert the Gaia EDR3 white-dwarf catalogue query (gaia_edr3_wd_100pc.csv) into wd.csv for Fermium.

The raw file is the result of this ADQL query to the VizieR TAP service (catalogue J/MNRAS/508/3877, Gentile Fusillo
et al. 2021), downloaded with curl:
  https://tapvizier.cds.unistra.fr/TAPVizieR/tap/sync?REQUEST=doQuery&LANG=ADQL&FORMAT=csv&QUERY=
  SELECT WDJname, Plx, e_Plx, Pwd, "GMAG", "BP-RP", TeffH, e_TeffH, loggH, e_loggH, MassH, e_MassH
  FROM "J/MNRAS/508/3877/maincat" WHERE Plx > 10 AND Pwd > 0.75
i.e. every high-probability white dwarf (P_WD > 0.75) within 100 pc. wd.csv keeps those with a pure-hydrogen
atmosphere fit (TeffH, loggH, MassH present). Columns: T [K] (TeffH), logg (log10 of g in cm/s²), M [Msun] (MassH),
GMAG (absolute Gaia G magnitude), plx [mas].
Run:  python3 prepare.py
"""
import csv
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    n = 0
    with open(os.path.join(HERE, "wd.csv"), "w", encoding="utf-8") as out:
        out.write("T [K], logg, M [Msun], GMAG, plx [mas]\n")
        for r in csv.DictReader(open(os.path.join(HERE, "gaia_edr3_wd_100pc.csv"), encoding="utf-8")):
            if not (r["TeffH"] and r["loggH"] and r["MassH"] and r["GMAG"]):
                continue
            out.write(f"{r['TeffH']}, {r['loggH']}, {r['MassH']}, {r['GMAG']}, {r['Plx']}\n")
            n += 1
    print(f"wrote {n} white dwarfs to wd.csv")


if __name__ == "__main__":
    main()
