"""M8 self-hosting: the unit database is a Fermium program (fermium/selfhost/units_db.fm), compiled by Fermium,
and the compiler uses its output (fermium/units_selfhosted.py)."""
import subprocess
import sys
import os

import pytest

from fermium.selfhost.__main__ import render, run_db
from fermium.units import _UNITS
from fermium.units_selfhosted import FACTORS

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# the values the Python table had before self-hosting (all exact definitions or published values)
REFERENCE = {
    "min": 60, "hr": 3600, "day": 86400, "yr": 365.25 * 86400, "inch": 0.0254, "ft": 0.3048, "yd": 0.9144,
    "mi": 1609.344, "Å": 1e-10, "au": 149597870700, "ly": 9460730472580800, "pc": 3.0856775814913673e16,
    "b": 1e-28, "ha": 1e4, "L": 1e-3, "u": 1.66053906892e-27, "lb": 0.45359237, "M☉": 1.3271244e20 / 6.67430e-11,
    "lbf": 4.4482216152605, "dyn": 1e-5, "bar": 1e5, "atm": 101325, "Torr": 101325 / 760, "mmHg": 133.322387415,
    "psi": 6894.757293168361, "eV": 1.602176634e-19, "erg": 1e-7, "cal": 4.184, "hp": 745.69987158227022,
    "gauss": 1e-4, "c": 299792458, "kph": 1000 / 3600, "mph": 0.44704, "Ci": 3.7e10,
}


def test_generated_table_is_up_to_date():
    assert render(run_db()) == open(os.path.join(ROOT, "fermium", "units_selfhosted.py"), encoding="utf-8").read()


@pytest.mark.parametrize("name,value", sorted(REFERENCE.items()))
def test_fermium_derived_factors_match_the_definitions(name, value):
    assert FACTORS[name] == pytest.approx(value, rel=3e-16)


def test_compiler_uses_the_self_hosted_factors():
    for name, value in FACTORS.items():
        assert _UNITS[name][0] == value, name


def test_a_wrong_definition_is_rejected():
    # the unit checker carries the dimension through: a "light year" that is really a speed leaves 1/s behind
    with pytest.raises(ValueError, match="ly has the wrong dimension"):
        run_db("c0 = 299792458 m/s\nprint \"ly\", c0 / (1 m) to 17 digits")


def test_regenerate_command_check_mode():
    r = subprocess.run([sys.executable, "-m", "fermium.selfhost", "--check"], cwd=ROOT, capture_output=True, text=True)
    assert r.returncode == 0 and "up to date" in r.stdout
