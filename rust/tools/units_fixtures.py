#!/usr/bin/env python3
"""Generate the parity fixtures for the Rust crate fermium-units FROM Fermium 1.5 (the Python oracle).

    python3 rust/tools/units_fixtures.py        # writes rust/crates/fermium-units/tests/fixtures/units.json

Every case records inputs and the Python outputs; `cargo test -p fermium-units` must reproduce them all.
Floats are stored as repr() strings (exact round trip), dimensions as 7 Fraction strings in Dim.e order,
units as [name, dim, factor, offset]. The generator is deterministic (seeded).
"""
import io
import json
import math
import os
import random
import sys
from fractions import Fraction
from types import SimpleNamespace

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "legacy"))   # Fermium 1.5 (the oracle) is in legacy/ since v2.0 (D269)

from fermium import units as U                                   # noqa: E402
from fermium import constants as K                               # noqa: E402
from fermium import natural as NAT                               # noqa: E402
from fermium.runtime import core as CORE                         # noqa: E402
from fermium.uncertain import format_pm, format_uncertain, UFloat  # noqa: E402
from fermium.units_selfhosted import FACTORS                        # noqa: E402

OUT = os.path.join(ROOT, "rust", "crates", "fermium-units", "tests", "fixtures", "units.json")
rng = random.Random(20260926)


def F(x):
    return repr(float(x))


def D(d):
    return [str(Fraction(e)) for e in d.e]


def UN(u):
    return None if u is None else [u.name, D(u.dim), F(u.factor), F(u.offset)]


# ------------------------------------------------------------------ values
def interesting_values():
    vals = [0.0, -0.0, 1.0, -1.0, 0.5, 1.5, 2.5, 3.5, -2.5, 0.125, 0.375, 0.0625, 1.0625, 12.5, 125.0, 1250.0,
            0.25, 0.75, 2.675, 1.005, 0.045, 1.25e-5, 1e-300, -1e-300, 1e300, -1e300, 5e-324, 2.2250738585072014e-308,
            1.7976931348623157e308, float("nan"), float("inf"), float("-inf"), 1e7, -1e7, 9999999.0, 9999999.5,
            9999999.4, 1e7 - 1e-6, 123456789.0, 333333.0, 9549.0, 9550.0, 99999.5, 999999.0, 999999.5, 1e6, 1e5,
            1e-4, 1e-5, 0.0001234, 0.00012345, 0.000099995, 3.0000000000000004, 0.9999999999999999,
            2.9999999999999996, 1e15, 1e15 + 1, 1e16, 12345678901234567.0, 6.02214076e23, 1.602176634e-19,
            299792458.0, 9.80665, math.pi, math.e, 1 / 3, 2 / 3, 0.1 + 0.2, 5 + 5e-13, 5 + 4e-13, 5 * (1 + 1e-13),
            1e6 + 1e-7, 1e6 + 1e-6, 100 * (1 + 2e-13), 20.0, 1000000.0, 1048576.0, -1048576.0, 0.1, 0.01, 0.001,
            100.0, 1e-3 * 1.2, 1.2000000000000002, 0.30000000000000004, 1.98841e30, 1.9884098706980512e30,
            0.99999999999999989, 12.345, 99.95, 99.949999, 0.00999, 9.995, 0.995, 999.5, 9995.0, 99995.0,
            4.35e-7, 7.2973525643e-3, 1e-13, 1e-14, 123.456, -0.000123456, 1e21, 1e22, 1e23, 2 ** 53, 2 ** 63,
            -(2 ** 53) + 1.0, 1.5e-10, 6.62607015e-34, 1.0000000000000002, 17.0, 0.2, 314159.26535]
    vals += [k + 0.5 for k in range(-5, 12)]
    vals += [10.0 ** k for k in range(-12, 16)]
    vals += [10.0 ** k * (1 - 2 ** -52) for k in range(-6, 9)]
    vals += [5 * 10.0 ** k for k in range(-8, 8)]
    for _ in range(260):
        e = rng.uniform(-320, 308)
        vals.append((-1) ** rng.randint(0, 1) * 10 ** e)
    for _ in range(80):
        vals.append(rng.uniform(-1e4, 1e4))
    for _ in range(60):
        vals.append(round(rng.uniform(-1000, 1000), rng.randint(0, 5)))
    for _ in range(40):
        vals.append(float(rng.randint(-10 ** 8, 10 ** 8)))
    return vals


VALS = interesting_values()

# ------------------------------------------------------------------ dims and hints
PREF_DIMS = [U.parse_unit_string(s).dim for s in U._PREFERRED_SPECS]
EXTRA_SPECS = ["1/s", "kg/s³", "V/m²", "T m", "W/(m² K)", "J/kg", "kg m²/s", "m^(1/2)", "kg^(1/2) m^(3/2)/s",
               "A s/kg", "mol/m³", "cd/m²", "K/W", "1", "J/(mol K²)", "N/C", "C/s", "m⁴", "s⁻²", "kg² m",
               "W/(m K)", "m/s³", "kg/(m s)", "A²", "K⁻¹", "mol/s", "J/m²", "Pa/K", "Ω m", "S", "H/m²"]
EXTRA_DIMS = [U.parse_unit_string(s).dim for s in EXTRA_SPECS]


def random_dim(max_e=3, half=False):
    e = [rng.randint(-max_e, max_e) if rng.random() < 0.5 else 0 for _ in range(7)]
    if half and rng.random() < 0.3:
        e[rng.randint(0, 2)] = Fraction(rng.choice([-3, -1, 1, 3]), 2)
    return U.Dim(e)


RAND_DIMS = [random_dim(3, True) for _ in range(120)] + [random_dim(2) for _ in range(80)]
ALL_DIMS = list(dict.fromkeys(PREF_DIMS + EXTRA_DIMS + RAND_DIMS))

HINT_SPECS = ["km", "cm", "MeV", "eV", "MeV/c²", "MeV/c", "GeV/c²", "c", "km/s", "°C", "°F", "K", "%", "°",
              "deg", "rad", "mm", "μs", "ns", "g/cm³", "kPa", "atm", "AU", "ly", "M☉", "yr", "kW hr", "J s",
              "eV s", "fm", "b", "mb", "L", "N m", "keV", "u", "kg m/s",
              "m/s²", "Hz", "kHz", "1", "c²", "km/hr", "mi/hr", "ft", "lbf", "psi", "Å", "nm", "T", "gauss"]
HINTS = [U.parse_unit_string(s) for s in HINT_SPECS]


def rand_si_value():
    r = rng.random()
    if r < 0.15:
        return float(rng.randint(-2000, 2000))
    if r < 0.2:
        return rng.choice([0.0, float("nan"), float("inf"), -float("inf"), 1e-300, 1e300])
    if r < 0.5:
        return round(rng.uniform(-100, 100), rng.randint(0, 4))
    return (-1) ** rng.randint(0, 1) * 10 ** rng.uniform(-35, 35)


# ------------------------------------------------------------------ sections
def sec_format_number():
    out = []
    sigs = list(range(0, 18)) + [18, 20]
    for x in VALS:
        out.append([F(x), [U.format_number(x, s, False) for s in sigs], [U.format_number(x, s, True) for s in sigs],
                    U.format_number(x)])
    return {"sigs": sigs, "cases": out}


def sec_format_default():
    return [[F(x), U.format_default(x), U.format_default(x, 3, False), U.format_default(x, 5), U._whole(x)]
            for x in VALS]


def sec_format_written():
    out = []
    for x in VALS[:250]:
        for sf in (1, 2, 3, 4, 6, 17):
            out.append([F(x), sf, CORE.format_written(x, sf, False), CORE.format_written(x, sf, True)])
    return out


def rand_seq(n):
    kind = rng.random()
    xs = []
    for _ in range(n):
        if kind < 0.35:
            xs.append(float(rng.randint(-50, 50)) * (1 + rng.choice([0, 0, 1e-14, 3e-13])))
        elif kind < 0.5:
            xs.append(rng.choice([0.5, 1.0, 1.5, 2.0, float("nan"), float("inf"), 3.0]))
        else:
            xs.append(rand_si_value())
    return xs


def sec_format_default_seq():
    return [[[F(x) for x in xs], U.format_default_seq(xs)] for xs in (rand_seq(rng.randint(0, 8)) for _ in range(300))]


def sec_lookup_unit():
    names = list(U._UNITS) + list(U._AFFINE)
    for p in U.PREFIXES:
        for base in ("m", "g", "s", "eV", "Pa", "J", "W", "Hz", "yr", "pc", "ly", "b", "L", "cd", "ft", "min", "Gs",
                     "arcsec", "rad", "Da", "cal", "Wh", "bar", "Ci", "T", "Ω", "ohm", "mol", "K", "A", "sr", "au"):
            names.append(p + base)
    names += ["µm", "µs", "µeV", "µ", "μ", "Rg", "Qg", "qg", "Qm", "rg", "rm", "Rm", "qm", "rs", "RC", "qV",
              "dam", "nmi", "ms_", "hPa", "kPa", "dag", "das", "xyz", "", "m²", "M☉", "Msun", "kM☉", "h", "t",
              "mas", "μas", "uas", "kmas", "Mpc", "Gyr", "kyr", "TeV", "fm", "mbarn", "degC", "kdegC", "k°C",
              "u", "ku", "amu", "kamu", "percent", "%", "k%", "hp", "khp"]
    names = list(dict.fromkeys(names))
    return [[n, UN(U.lookup_unit(n))] for n in names]


def parse_case(text):
    try:
        u = U.parse_unit_string(text)
        return ["ok"] + UN(u)
    except U.UnitSyntaxError as ex:
        return ["err", str(ex)]
    except Exception as ex:          # noqa: BLE001 - Fermium 1.5 lets these escape; the port reports them
        return ["exc", type(ex).__name__]


def sec_parse():
    texts = list(U._PREFERRED_SPECS) + EXTRA_SPECS + HINT_SPECS + [
        "kg m/s²", "J/(mol K)", "m^(1/2)", "m^(-1/2)", "m^-2", "s^2", "m^(3)", "m^(1/0)", "m^", "m^(", "m^(1/",
        "°C", "degC", "°F", "°C/s", "J/(g °C)", "(°C)", "°C^1", "  km  ", "km/", "/s", "(m", "m)", "m s)", "3 m",
        "1/s", "1", "1/(m s)", "m·s", "m*s", "m * s / kg", "m/s/s", "m s⁻¹", "m²s", "kg·m²·s⁻²", "N m²/C²",
        "m^2^3", "m^2³", "m$", "m + s", "xyz", "", "   ", "m^x", "m^(a/b)", "AU³/(M☉ yr²)", "M☉/yr", "µm", "µs",
        "Å", "Ω m", "ohm m", "W/(m² K⁴)", "(kg m)/(s² A)", "((m))", "m^(2/4)", "s^(-3/6)", "m^-1/2",
        "1/1", "1 m", "m 1", "MeV/c^2", "MeV fm", "eV/c", "J/T", "C²/(N m²)", "m³/(kg s²)", "%/s", "°/s",
        "rev/min", "rpm", "Bq", "kBq", "Ci", "mCi", "L/min", "mL", "km/hr", "mi/hr", "ft/s^2", "ft s^-2",
        "lbf/inch²", "in_", "inch", "R_E", "M_E", "R☉", "L☉/M☉", "g/cm³", "kg/m^3", "mol/L", "mmol/L",
        "m⁻¹", "s⁻¹", "m⁻²", "Hz^(1/2)", "m/(s)", "m/(s kg)", "m//s", "m**2", "m^^2", "m ^ 2", "m ^2", "m^ 2",
        "( m / s )", "1/m²", "kg^-1", "°", "deg/s", "arcsec", "mas/yr", "μas", "uas", "-m", "m-", "m^-", "m^(-)",
        "m^(-1)", "m^(1/-2)", "m^(1/2", "m^(12345)", "°C m", "°F/hr"]
    for _ in range(300):
        parts = []
        for _ in range(rng.randint(1, 4)):
            nm = rng.choice(["m", "kg", "s", "A", "K", "mol", "N", "J", "W", "eV", "MeV", "km", "g", "Pa", "C",
                             "V", "T", "Ω", "Hz", "L", "cm", "mm", "μs", "yr", "AU", "M☉", "fm", "c"])
            ex = rng.choice(["", "", "²", "³", "⁻¹", "⁻²", "^2", "^-1", "^(1/2)", "^(3/2)"])
            parts.append(nm + ex)
        sep = rng.choice([" ", " ", "/", "·", "*"])
        t = sep.join(parts)
        if rng.random() < 0.2 and len(parts) > 1:
            t = parts[0] + "/(" + " ".join(parts[1:]) + ")"
        texts.append(t)
    texts = list(dict.fromkeys(texts))
    return [[t, parse_case(t)] for t in texts]


def sec_dims():
    out = []
    for d in ALL_DIMS:
        su = U.suggest_units(d)
        out.append([D(d), U.format_dim(d), U.format_dim(d, pretty=False), U.dim_name(d), UN(U.preferred_unit(d)),
                    None if su is None else [su[0], list(su[1])]])
    return out


def fmt_entry(dim, hint, sf, direct, echo=True):
    return {"rdim": dim, "hint": hint, "sf": sf, "direct": direct, "echo": echo}


def FMT(f):
    return [D(f["rdim"]), UN(f["hint"]), f["sf"], int(f["direct"]), f["echo"]]


def rand_fmt(dim=None, hint_prob=0.4):
    if dim is None:
        dim = rng.choice(ALL_DIMS)
    hint = None
    if rng.random() < hint_prob:
        h = rng.choice(HINTS)
        if rng.random() < 0.8:
            dim = h.dim
        hint = h
    sf = rng.choice([None, None, None, 0, 1, 2, 3, 4, 5, 6, 8, 12, 15, 17])
    direct = rng.choice([0, 0, 1, 1, 3, 4, 5])
    echo = rng.random() < 0.85
    return fmt_entry(dim, hint, sf, direct, echo)


def sec_quantity():
    out = []
    for _ in range(2500):
        f = rand_fmt()
        v = rand_si_value()
        whole_ok = rng.random() < 0.8
        r = CORE.format_quantity(v, f["rdim"], f["hint"], f["sf"], f["direct"], f["echo"], whole_ok)
        out.append([F(v), FMT(f), whole_ok, r])
    # every preferred dimension, plain computed values
    for d in ALL_DIMS:
        for v in (1.0, 2.5e-7, 123456.0, 6.02e23):
            f = fmt_entry(d, None, None, 0)
            out.append([F(v), FMT(f), True, CORE.format_quantity(v, d, None, None, 0)])
    # literals as written: whole numbers exactly below 10^15 (10000000, not 1×10⁷)
    for v in (1e7, 1e14, 123456789012345.0, 999999999999999.0, 1e15, 1e16, -5e14, 2.5e14, 1e14 + 0.5, 0.1, 1e-20):
        for direct in (1, 3):
            f = fmt_entry(U.DIMLESS, None, None, direct)
            out.append([F(v), FMT(f), True, CORE.format_quantity(v, U.DIMLESS, None, None, direct)])
    # `in` conversions: affine units, c-units echo
    for spec in ("°C", "°F", "MeV/c²", "MeV/c", "GeV/c²", "c", "c²", "km/s", "%", "°", "keV", "eV/c^2"):
        h = U.parse_unit_string(spec)
        for v in (0.0, 273.15, 300.0, 1.602176634e-13, 1e-27, 299792458.0, 0.5, 1e-30, -40.0 + 273.15):
            for sf, direct in ((None, 0), (None, 1), (3, 1), (3, 0), (5, 1), (0, 1)):
                f = fmt_entry(h.dim, h, sf, direct)
                out.append([F(v), FMT(f), True, CORE.format_quantity(v, h.dim, h, sf, direct)])
    return out


class FakeTables:
    def __init__(self, fmts):
        self.fmts = fmts
        self.texts = []


RT = CORE.Runtime(out=io.StringIO())


def call(name, fmts, *args):
    RT.tables = FakeTables(fmts)
    RT.line = []
    RT.py[name](0, *args)
    (s,) = RT.line
    return s


def sec_seq():
    lists, vecs, mats, mvecs = [], [], [], []
    for _ in range(500):
        n = rng.choice([0, 1, 2, 3, 5, 8, 12, 13, 20])
        xs = rand_seq(n)
        f = rand_fmt()
        lists.append([[F(x) for x in xs], FMT(f), call("print_list", [f], xs, n)])
    for _ in range(350):
        n = rng.choice([2, 3, 4])
        xs = rand_seq(n)
        f = rand_fmt()
        f["hint"] = f["hint"] if (f["hint"] is None or not f["hint"].affine) else None
        vecs.append([[F(x) for x in xs], FMT(f), call("print_vec", [f], xs, n)])
    for _ in range(300):
        r, c = rng.randint(1, 4), rng.randint(1, 4)
        xs = rand_seq(r * c)
        f = rand_fmt()
        f["hint"] = f["hint"] if (f["hint"] is None or not f["hint"].affine) else None
        mats.append([[F(x) for x in xs], r, c, FMT(f), call("print_mat", [f], xs, r, c)])
    for _ in range(300):
        n = rng.choice([2, 3])
        fs = [rand_fmt() for _ in range(n)]
        xs = [rand_si_value() for _ in range(n)]
        if rng.random() < 0.5:
            xs = [float(rng.randint(-9, 9)) for _ in range(n)]
        mvecs.append([[F(x) for x in xs], [FMT(f) for f in fs], call("print_mvec", fs, xs, n)])
    return {"list": lists, "vec": vecs, "mat": mats, "mvec": mvecs}


def sec_complex():
    cplx, clist = [], []
    for _ in range(600):
        f = rand_fmt()
        f["hint"] = f["hint"] if (f["hint"] is None or not f["hint"].affine) else None
        if rng.random() < 0.4:
            re_, im_ = float(rng.randint(-9, 9)), float(rng.randint(-9, 9))
        else:
            re_, im_ = rand_si_value(), rand_si_value()
            if rng.random() < 0.2:
                im_ = re_ * 1e-16
        cplx.append([F(re_), F(im_), FMT(f), call("print_cplx", [f], re_, im_)])
    for _ in range(250):
        f = rand_fmt()
        f["hint"] = f["hint"] if (f["hint"] is None or not f["hint"].affine) else None
        n = rng.choice([1, 2, 3, 13, 15])
        whole = rng.random() < 0.4
        p = []
        for _ in range(n):
            if whole:
                p += [float(rng.randint(-9, 9)), float(rng.randint(-9, 9))]
            else:
                p += [rand_si_value(), rand_si_value()]
        clist.append([[F(x) for x in p], FMT(f), call("print_clist", [f], p, n)])
    return {"cplx": cplx, "clist": clist}


def sec_uncertain():
    pm, unc, ulist = [], [], []
    for _ in range(1500):
        x = rand_si_value() if rng.random() < 0.5 else round(rng.uniform(-100, 100), rng.randint(0, 4))
        s = rng.choice([0.0, abs(x) * 10 ** rng.uniform(-8, 1), 10 ** rng.uniform(-30, 30), 0.0996, 0.095, 0.25,
                        0.05, 1.0, 0.012, float("inf")])
        t, sci = format_pm(x, s)
        pm.append([F(x), F(s), t, sci])
    for _ in range(600):
        f = rand_fmt()
        v = rand_si_value()
        s = abs(v) * 10 ** rng.uniform(-6, 0) if v == v and abs(v) < math.inf and v != 0 else rng.uniform(0, 1)
        u = UFloat.measured(v, s)
        unc.append([F(v), F(u.s), D(f["rdim"]), UN(f["hint"]),
                    format_uncertain(u, f["rdim"], f["hint"], CORE.display_unit)])
    from fermium.uncertain import format_uncertain_list
    for _ in range(200):
        h = rng.choice(HINTS)
        items = []
        vals = []
        for _ in range(rng.randint(0, 5)):
            v = rand_si_value()
            if rng.random() < 0.7 and v == v and abs(v) < math.inf:
                s = abs(v) * 0.01 + 1e-9
                u = UFloat.measured(v, s)
                vals.append(u)
                items.append([F(v), F(u.s)])
            else:
                vals.append(v)
                items.append([F(v), None])
        ulist.append([items, UN(h), format_uncertain_list(vals, h)])
    return {"pm": pm, "uncertain": unc, "list": ulist}


def sec_constants():
    return [[name, F(val), UN(u), desc] for name, (val, u, desc) in K.all_constants().items()]


def sec_natural():
    systems = [("natural", None), ("natural", ["ħ", "c"]), ("nuclear", None), ("nuclear", ["c", "ħ"]),
               ("astro", None), ("SI", None), ("natural", ["ħ", "c", "k_B"]), ("natural", ["G", "c"]),
               ("natural", ["ħ", "c", "G"]), ("natural", ["ħ", "c", "k_B", "G", "ε_0"]),
               ("natural", ["c"]), ("natural", ["ħ"]), ("natural", ["k_B"]), ("natural", ["ε_0", "ħ", "c"]),
               ("natural", ["c", "c"]), ("natural", ["foo"]), ("nuclear", ["G"]), ("astro", ["c"]),
               ("planck", None), ("natural", ["G", "c", "k_B"])]
    units = ["MeV", "fm", "kg", "s", "GeV/c²", "°C", "km/s", "J s", "m", "K", "eV", "J/K", "A", "C", "N"]
    out = []
    for name, consts in systems:
        try:
            sysm = NAT.make_system(name, consts)
        except ValueError as ex:
            out.append([name, consts, ["err", str(ex)]])
            continue
        dims = []
        for d in ALL_DIMS[:90]:
            du = sysm.display_unit(d) if (sysm.natural or sysm.display == "astro") else None
            if sysm.natural:
                cd = sysm.canon_dim(d)
                cdu = sysm.display_unit(cd)
                row = [D(d), D(cd), F(sysm.factor(d)), sysm.invariant(d), UN(cdu), sysm.describe(cd)]
            else:
                row = [D(d), D(sysm.canon_dim(d)), F(sysm.factor(d)), sysm.invariant(d), UN(du), sysm.describe(d)]
            dims.append(row)
        cu = []
        for s in units:
            u = U.parse_unit_string(s)
            cu.append([s, UN(sysm.canon_unit(u))])
        cv = []
        for cname, (val, u, _) in K.all_constants().items():
            cv.append([cname, F(sysm.const_value(cname, val, u.dim))])
        out.append([name, consts, ["ok", sysm.label(), sysm.display, list(sysm.consts), dims, cu, cv]])
    return out


def sec_tables():
    return {"spelled": U.SPELLED_UNITS, "long": U.UNIT_NAMES_LONG, "pretty": U.UNIT_PRETTY,
            "prefixes": {k: F(v) for k, v in U.PREFIXES.items()},
            "self_hosted": {k: F(v) for k, v in FACTORS.items()}}


def main():
    data = {
        "generator": "rust/tools/units_fixtures.py (Fermium 1.5 oracle)",
        "format_number": sec_format_number(),
        "format_default": sec_format_default(),
        "format_written": sec_format_written(),
        "format_default_seq": sec_format_default_seq(),
        "lookup_unit": sec_lookup_unit(),
        "parse_unit_string": sec_parse(),
        "dims": sec_dims(),
        "format_quantity": sec_quantity(),
        "seq": sec_seq(),
        "complex": sec_complex(),
        "uncertain": sec_uncertain(),
        "constants": sec_constants(),
        "natural": sec_natural(),
        "tables": sec_tables(),
    }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(text)
    print(f"wrote {OUT} ({len(text.encode()) / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
