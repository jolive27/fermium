"""Dimensional analysis: `analyze pendulum: T depends on L, m, g` (Buckingham Π theorem, DECISIONS D70).

Each classic case is checked against its textbook answer: the exponents of the formula for the target,
the number of groups (n − rank) and what drops out.  The groups are also checked to be dimensionless
and independent, and (when SymPy is installed) the rank is cross-checked against sympy's."""
import io
import math
from fractions import Fraction as F

import pytest

from conftest import error_of, run
from fermium.dimanalysis import AnalysisError, analyze, nullspace, product_text, rank
from fermium.interp import run_interpreted
from fermium.units import DIMLESS, parse_unit_string


def D(u):
    return parse_unit_string(u).dim if u != "1" else DIMLESS


def an(target, tunit, *inputs):
    return analyze(target, D(tunit), [(n, D(u)) for n, u in inputs])


def interp(src):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    return o.getvalue().strip()


def both(src):
    out = run(src)
    assert interp(src) == out
    return out


def check_groups(a):
    """Every group is dimensionless, the groups are independent, and there are n − rank of them."""
    for g in a.groups:
        d = DIMLESS
        for name, e in g.items():
            d = d * a.dims[name] ** e
        assert d == DIMLESS, g
    names = [a.target] + a.names
    vecs = [[g.get(n, F(0)) for n in names] for g in a.groups]
    assert rank(vecs) == len(a.groups)
    cols = [[F(x) for x in a.dims[n].e] for n in names]
    assert len(a.groups) == len(names) - rank(cols) == len(nullspace(cols))
    # the target is in exactly one group, with exponent 1
    assert [g.get(a.target, 0) for g in a.groups] == [1] + [0] * (len(a.groups) - 1)
    try:
        import sympy
    except ImportError:
        return
    m = sympy.Matrix([[sympy.Rational(x.numerator, x.denominator) for x in col] for col in cols]).T
    assert len(m.nullspace()) == len(a.groups)


# ---------------------------------------------------------------- classic cases (the algorithm)
def test_pendulum_mass_drops_out():
    a = an("T", "s", ("L", "m"), ("m", "kg"), ("g", "m/s^2"))
    check_groups(a)
    assert a.prefactor == {"L": F(1, 2), "g": F(-1, 2)}
    assert len(a.groups) == 1 and a.rank == 3
    assert a.dropped == [("m", "nothing else has mass")]
    assert product_text(a.groups[0], ["T", "L", "m", "g"]) == "T √(g/L)"


def test_drag_force_depends_on_reynolds_number():
    a = an("F", "N", ("ρ", "kg/m^3"), ("v", "m/s"), ("A", "m^2"), ("μ", "Pa s"))
    check_groups(a)
    assert a.prefactor == {"ρ": 1, "v": 2, "A": 1}
    assert len(a.groups) == 2
    re_ = a.groups[1]            # Re = ρ v L/μ with L = √A
    assert re_ == {"ρ": 1, "v": 1, "A": F(1, 2), "μ": -1}
    assert product_text(re_, ["F", "ρ", "v", "A", "μ"]) == "ρ v √A/μ"


def test_taylor_blast_radius():
    a = an("R", "m", ("E", "J"), ("ρ", "kg/m^3"), ("t", "s"))
    check_groups(a)
    assert a.prefactor == {"E": F(1, 5), "ρ": F(-1, 5), "t": F(2, 5)}
    assert product_text(a.prefactor, ["R", "E", "ρ", "t"]) == "(E t²/ρ)^(1/5)"


def test_water_wave_speed():
    a = an("v", "m/s", ("g", "m/s^2"), ("λ", "m"))
    check_groups(a)
    assert a.prefactor == {"g": F(1, 2), "λ": F(1, 2)}
    assert product_text(a.groups[0], ["v", "g", "λ"]) == "v/√(g λ)"


def test_planck_length():
    a = an("ℓ", "m", ("G", "m^3/(kg s^2)"), ("ħ", "J s"), ("c", "m/s"))
    check_groups(a)
    assert a.prefactor == {"G": F(1, 2), "ħ": F(1, 2), "c": F(-3, 2)}


def test_kepler_period():
    a = an("T", "s", ("a", "m"), ("G", "m^3/(kg s^2)"), ("M", "kg"))
    check_groups(a)
    assert a.prefactor == {"a": F(3, 2), "G": F(-1, 2), "M": F(-1, 2)}
    assert product_text(a.prefactor, ["T", "a", "G", "M"]) == "√(a³/(G M))"


def test_hydrogen_energy():
    a = an("E", "J", ("m_e", "kg"), ("e", "C"), ("ε₀", "F/m"), ("ħ", "J s"))
    check_groups(a)
    assert a.prefactor == {"m_e": 1, "e": 4, "ε₀": -2, "ħ": -2}
    assert a.rank == 4


def test_two_groups_pendulum_with_amplitude():
    a = an("T", "s", ("L", "m"), ("g", "m/s^2"), ("θ", "1"))
    check_groups(a)
    assert len(a.groups) == 2 and a.groups[1] == {"θ": 1}
    assert a.prefactor == {"L": F(1, 2), "g": F(-1, 2)}


def test_repeating_variables_follow_the_written_order():
    # the same physics, other inputs first: F/(μ v √A) and the inverse Reynolds number
    a = an("F", "N", ("μ", "Pa s"), ("v", "m/s"), ("A", "m^2"), ("ρ", "kg/m^3"))
    check_groups(a)
    assert a.prefactor == {"μ": 1, "v": 1, "A": F(1, 2)}
    order = ["F", "μ", "v", "A", "ρ"]
    assert product_text(a.groups[0], order) == "F/(μ v √A)"
    assert product_text(a.groups[1], order) == "v ρ √A/μ"


def test_no_dimensionless_group():
    with pytest.raises(AnalysisError) as ei:
        an("v", "m/s", ("m", "kg"), ("t", "s"))
    msg = ei.value.message
    assert "v can't be made from m, t" in msg and "has length" in msg
    assert "no dimensionless group at all (3 quantities, 3 independent dimensions)" in msg


def test_target_not_expressible():
    # L²/A is a group, but nothing has the mass or time a force needs
    with pytest.raises(AnalysisError) as ei:
        an("F", "N", ("L", "m"), ("A", "m^2"))
    assert ei.value.message == "F can't be made from L, A: F has mass and time, but nothing it depends on has " \
                               "mass or time"
    assert ei.value.index == 0


def test_target_not_expressible_without_a_lonely_dimension():
    # every base dimension of E appears somewhere, but no power product gives J from kg and m/s²... and s
    with pytest.raises(AnalysisError) as ei:
        an("E", "J", ("p", "kg m/s"), ("F", "N"))
    assert "can't be made from any powers of p, F" in ei.value.message


def test_duplicate_and_self_dependence():
    with pytest.raises(AnalysisError, match="listed twice"):
        an("T", "s", ("L", "m"), ("L", "m"))
    with pytest.raises(AnalysisError, match="can't depend on itself"):
        an("T", "s", ("T", "s"))


# ---------------------------------------------------------------- the statement
PENDULUM = "analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]\n"


def test_pendulum_statement_output():
    out = both(PENDULUM).split("\n")
    assert out == [
        "dimensional analysis of pendulum: T depends on L, m, g",
        "  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group",
        "  Π₁ = T √(g/L)",
        "  so T ∝ √(L/g)   (T = C √(L/g), with C a pure number)",
        "  m drops out: nothing else has mass",
        "  defined pendulum(L, g) = √(L/g), so T = C pendulum(L, g)",
    ]


def test_defined_function_is_usable():
    out = both(PENDULUM + "print 2π pendulum(1 m, 9.81 m/s²) to 4 digits\n").split("\n")[-1]
    assert out == "2.006 s"


def test_defined_function_checks_units():
    e = error_of(PENDULUM + "print pendulum(1 s, 9.81 m/s²)\n")
    assert "length" in e.message and "time" in e.message


def test_fit_with_the_analysis_result(tmp_path):
    p = tmp_path / "pendulum.csv"
    rows = ["L [m], T [s]"] + [f"{L}, {2 * math.pi * math.sqrt(L / 9.81):.6f}" for L in (0.2, 0.4, 0.6, 0.8, 1.0)]
    p.write_text("\n".join(rows) + "\n")
    out = run(f'data = load "{p}"\n' + PENDULUM + "fit T = C pendulum(L, 9.81 m/s²) to data\nprint C to 6 digits\n")
    assert abs(float(out.split("\n")[-1]) - 2 * math.pi) < 1e-3


def test_only_constants_defines_a_value():
    out = both("analyze planck: ℓ [m] depends on G, ħ, c\nprint planck to 6 digits\n").split("\n")
    assert out[3] == "  so ℓ ∝ √(G ħ/c³)   (ℓ = C √(G ħ/c³), with C a pure number)"
    assert out[4].startswith("  defined planck = √(G ħ/c³) = 1.62×10⁻³⁵ m")
    assert out[5] == "1.61626×10⁻³⁵ m"


def test_hydrogen_statement_gives_the_rydberg_energy():
    out = both("analyze hydrogen: E [J] depends on m_e, e, ε₀, ħ\nprint hydrogen/(32 π²) in eV to 6 digits\n").split("\n")
    assert "  so E ∝ m_e e⁴/(ε₀² ħ²)   (E = C m_e e⁴/(ε₀² ħ²), with C a pure number)" in out
    assert out[-1] == "13.6057 eV"


def test_kepler_statement_constant_is_not_a_parameter():
    out = both("analyze kepler: T [s] depends on a [m], G, M [kg]\n"
               "print 2π kepler(AU, M_sun) in day to 6 digits\n").split("\n")
    assert "  defined kepler(a, M) = √(a³/(G M)), so T = C kepler(a, M)" in out
    assert out[-1].startswith("365.2")


def test_taylor_statement_and_trinity():
    out = both("analyze blast: R [m] depends on E [J], ρ [kg/m³], t [s]\n"
               "E = 1.0e14 J\nprint blast(E, 1.2 kg/m³, 25 ms)\n").split("\n")
    assert "  so R ∝ (E t²/ρ)^(1/5)   (R = C (E t²/ρ)^(1/5), with C a pure number)" in out
    assert out[-1].endswith(" m")


def test_drag_statement_two_groups():
    out = both("analyze drag: F [N] depends on ρ [kg/m³], v [m/s], A [m²], μ [Pa s]\n").split("\n")
    assert out[1].endswith("→ 5 − 3 = 2 dimensionless groups")
    assert out[2:5] == ["  Π₁ = F/(ρ v² A)", "  Π₂ = ρ v √A/μ",
                        "  so F = ρ v² A · f(Π₂)   (f is a function dimensional analysis can't give)"]
    assert out[5] == "  defined drag(ρ, v, A) = ρ v² A, so F = drag(ρ, v, A) · f(Π₂)"


def test_untitled_with_known_target():
    out = both("L = 2 m\ng = 9.81 m/s²\nv = 3 m/s\nanalyze v depends on g, L\n").split("\n")
    assert out[0] == "dimensional analysis: v depends on g, L"
    assert out[3] == "  so v ∝ √(g L)   (v = C √(g L), with C a pure number)"
    assert not any("defined" in x for x in out)


def test_statement_errors():
    e = error_of("analyze x [m/s] depends on m [kg], t [s]\n")
    assert "no dimensionless group" in e.message and e.col == 9
    e = error_of("analyze F [N] depends on L [m], A [m²]\n")
    assert "F can't be made from L, A" in e.message
    e = error_of("analyze T [s] depends on L, g [m/s²]\n")
    assert "L has no units yet" in e.message and "L [m]" in e.hint
    e = error_of("analyze T: T [s] depends on L [m], g [m/s²]\n")
    assert "can't be called T" in e.message
    e = error_of("analyze p: T [s] depends L [m]\n")
    assert "expected 'on'" in e.message
    e = error_of("f(x) = x\nanalyze T [s] depends on f\n")
    assert "isn't a number with units" in e.message
    e = error_of("if true\n    analyze T [s] depends on L [m], g [m/s²]\n")
    assert "top level" in e.message


def test_analyze_is_still_an_ordinary_name():
    assert run("analyze = 3\nprint analyze + 1\n") == "4"
