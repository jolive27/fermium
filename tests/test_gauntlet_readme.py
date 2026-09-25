"""The README's Gauntlet table matches the files in gauntlet/."""
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
