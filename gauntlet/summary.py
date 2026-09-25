"""Print the README's Gauntlet table from the files in gauntlet/ (tests/test_gauntlet_readme.py checks it)."""
import glob
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
TOPICS = ["mechanics", "oscillations", "gravitation", "thermodynamics", "electromagnetism", "optics_waves",
          "special_relativity", "quantum", "nuclear", "astrophysics"]


def counts():
    rows = []
    for t in TOPICS:
        progs = sorted(glob.glob(os.path.join(HERE, t, "*.fm")))
        first = [p for p in progs if not os.path.basename(p).startswith("2")]
        second = [p for p in progs if os.path.basename(p).startswith("2")]
        rows.append((t, len(first), len(second)))
    return rows


def friction():
    """(total, fixed) rows of the table in gauntlet/FRICTION.md."""
    rows = [ln for ln in open(os.path.join(HERE, "FRICTION.md"), encoding="utf-8")
            if re.match(r"\|\s*\d+\s*\|", ln)]
    fixed = [ln for ln in rows if re.match(r"\s*\**(Fixed|Resolved)", ln.rsplit("|", 2)[-2])]
    return len(rows), len(fixed)


def table():
    rows = counts()
    out = ["| Topic | Pass 1 | Pass 2 |", "|---|---|---|"]
    for t, a, b in rows:
        out.append(f"| {t.replace('_', ' ')} | {a} | {b} |")
    out.append(f"| **total** | **{sum(r[1] for r in rows)}** | **{sum(r[2] for r in rows)}** |")
    total, fixed = friction()
    out.append("")
    out.append(f"Friction items logged: {total}; fixed in the language: {fixed}.")
    return "\n".join(out)


if __name__ == "__main__":
    print(table())
