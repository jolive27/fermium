"""Print the README's Gauntlet table from the files in gauntlet/ (tests/test_gauntlet_readme.py checks it)."""
import glob
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
TOPICS = ["mechanics", "oscillations", "gravitation", "thermodynamics", "electromagnetism", "optics_waves",
          "special_relativity", "quantum", "nuclear", "astrophysics"]


# the pass a problem belongs to is the first digit of its file name: 0 (pass 1, 01_… to 09_…),
# 2 (pass 2, 21_…) and 3 (pass 3, graduate level, 31_…)
PASSES = ["0", "2", "3"]


def counts():
    rows = []
    for t in TOPICS:
        names = [os.path.basename(p) for p in glob.glob(os.path.join(HERE, t, "*.fm"))]
        rows.append((t, *[sum(1 for n in names if n.startswith(d)) for d in PASSES]))
    return rows


def friction():
    """(total, fixed) rows of the table in gauntlet/FRICTION.md."""
    rows = [ln for ln in open(os.path.join(HERE, "FRICTION.md"), encoding="utf-8")
            if re.match(r"\|\s*\d+\s*\|", ln)]
    fixed = [ln for ln in rows if re.match(r"\s*\**(Fixed|Resolved)", ln.rsplit("|", 2)[-2])]
    return len(rows), len(fixed)


def table():
    rows = counts()
    out = ["| Topic | Pass 1 | Pass 2 | Pass 3 (graduate) |", "|---|---|---|---|"]
    for t, *n in rows:
        out.append(f"| {t.replace('_', ' ')} | " + " | ".join(str(k) for k in n) + " |")
    out.append("| **total** | " + " | ".join(f"**{sum(r[i] for r in rows)}**" for i in range(1, len(PASSES) + 1)) + " |")
    total, fixed = friction()
    out.append("")
    out.append(f"Friction items logged: {total}; fixed in the language: {fixed}.")
    return "\n".join(out)


if __name__ == "__main__":
    print(table())
