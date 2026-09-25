"""The README's Gauntlet table matches the files in gauntlet/."""
import glob
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_readme_gauntlet_table_is_current():
    table = subprocess.run([sys.executable, os.path.join(ROOT, "gauntlet", "summary.py")], capture_output=True,
                           text=True, check=True).stdout.strip()
    readme = open(os.path.join(ROOT, "README.md"), encoding="utf-8").read()
    got = readme.split("<!-- gauntlet-table -->\n", 1)[1].split("\n<!-- /gauntlet-table -->", 1)[0]
    assert got == table, "run: python3 gauntlet/summary.py and paste the table into README.md"


def test_every_gauntlet_problem_is_counted():
    """Each problem file's first digit names its pass (0, 2 or 3), so the table counts every file."""
    sys.path.insert(0, os.path.join(ROOT, "gauntlet"))
    import summary
    files = [f for t in summary.TOPICS for f in glob.glob(os.path.join(ROOT, "gauntlet", t, "*.fm"))]
    assert files and all(os.path.basename(f)[0] in summary.PASSES for f in files)
    assert sum(sum(r[1:]) for r in summary.counts()) == len(files)
