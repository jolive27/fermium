"""Dimension checking: programs that must be rejected (with a readable one-line message
pointing at the right line), and programs that must be accepted with the right result."""
import pytest

from conftest import run, run_lines, error_of, check_only
from fermium.errors import FermiumError

XT = "x = 3 m\nt = 2 s\n"
SPRING = "m = 1 kg\nk = 5 [N/m]\n"
CSV = "L [m], T [s]\n0.5, 1.42\n1.0, 2.01\n1.5, 2.46\n2.0, 2.84\n"

# (id, program, phrases that must all appear in the message, line of the error)
REJECT = [
    # + and - with mismatched units
    ("add_m_s", XT + "y = x + t", ["can't add length [m] to time [s]"], 3),
    ("sub_m_s", XT + "print x - t", ["can't subtract time [s] from length [m]"], 3),
    ("add_m_plain", "print 3 m + 2", ["can't add", "length [m]", "plain number"], 1),
    ("add_kg_J", "print 3 kg + 2 J", ["can't add", "mass [kg]", "energy [J]"], 1),
    ("add_m_m2", "print 3 m + 2 m²", ["can't add", "length [m]", "area [m²]"], 1),
    ("add_eV_kg", "print 1 eV + 1 kg", ["can't add", "energy", "mass [kg]"], 1),
    ("add_atm_J", "print 1 atm + 1 J", ["can't add", "pressure", "energy"], 1),
    ("add_N_momentum", "print 1 N + 1 kg m/s", ["can't add", "force [N]", "momentum"], 1),
    ("add_Hz_speed", "print 5 Hz + 1 m/s", ["can't add", "speed [m/s]"], 1),
    ("add_ly_s", "print 1 ly + 1 s", ["can't add", "length", "time [s]"], 1),
    ("add_sqrt_m", "print √(4 m²) + 1 s", ["can't add", "length [m]", "time [s]"], 1),
    ("add_cbrt", "print (8 m³)^(1/3) + 1 s", ["can't add", "length [m]", "time [s]"], 1),
    ("add_lists", "xs = [1 m, 2 m]\nys = [1 s, 2 s]\nprint xs + ys", ["can't add", "length [m]", "time [s]"], 3),
    ("add_multiline", "a = 1 m\n\n# comment\nb = 2 kg\nc = a + b", ["can't add length [m] to mass [kg]"], 5),
    # comparisons
    ("cmp_m_s", "print 3 m < 2 s", ["can't compare length [m] with time [s]"], 1),
    ("cmp_eq", "print 3 m == 2 kg", ["can't compare", "length [m]", "mass [kg]"], 1),
    ("cmp_plain", "if 3 m > 2\n    print 1", ["can't compare", "length [m]", "plain number"], 1),
    ("cmp_while", "t = 0 s\nwhile t < 5\n    t += 1 s", ["can't compare", "time [s]"], 2),
    ("cmp_assert", "assert 1 m == 2 s", ["can't compare length [m] with time [s]"], 1),
    ("cmp_approx", "print 1 m ≈ 1 s", ["can't compare"], 1),
    ("cmp_ifexpr", "y = if 3 m > 2 then 1 else 2", ["can't compare", "length [m]"], 1),
    # function arguments
    ("arg_annot", "f(x [m]) = 2 x\nprint f(3 s)", ["f expects x in m", "length [m]", "time [s]"], 2),
    ("arg_annot_plain", "f(x [m]) = x\nprint f(3)", ["f expects x in m"], 2),
    ("arg_atan2", "print atan2(1 m, 1 s)", ["atan2", "same units"], 1),
    ("arg_hypot", "print hypot(1 m, 2 kg)", ["hypot", "same units"], 1),
    ("arg_min", "print min(1 m, 2 s)", ["min", "same units"], 1),
    ("arg_max", "print max(1 m, 2 s, 3 m)", ["max", "same units"], 1),
    ("arg_clamp", "print clamp(1 m, 0 s, 2 s)", ["clamp", "same units"], 1),
    ("arg_mod", "print mod(5 m, 2 s)", ["mod", "same units"], 1),
    ("arg_linspace", "print linspace(0 s, 1 m, 5)", ["linspace", "same units"], 1),
    ("arg_two_params", "g(a, b) = a + b\nprint g(1 m, 1 s)", ["can't add"], None),
    # transcendental functions need plain numbers
    ("sin_m", "print sin(3 m)", ["sin needs a plain number, but got length [m]"], 1),
    ("cos_s", "print cos(2 s)", ["cos needs a plain number", "time [s]"], 1),
    ("tan_kg", "print tan(1 kg)", ["tan needs a plain number", "mass [kg]"], 1),
    ("exp_s", "print exp(2 s)", ["exp needs a plain number", "time [s]"], 1),
    ("exp_neg_t", "t = 3 s\nprint exp(-t)", ["exp needs a plain number", "time [s]"], 2),
    ("log_kg", "print log(2 kg)", ["log needs a plain number", "mass [kg]"], 1),
    ("ln_m", "print ln(3 m)", ["ln needs a plain number", "length [m]"], 1),
    ("sin_omega", "ω = 10 rad/s\nprint sin(ω)", ["sin needs a plain number"], 2),
    ("sinh_J", "print sinh(1 J)", ["sinh needs a plain number"], 1),
    # exponents
    ("exp_units", "x = 2 m\nprint 2^x", ["exponent must be a plain number", "length [m]"], 2),
    ("exp_variable", "n = 2\nx = 3 m\nprint x^n", ["can't raise length [m]", "fixed number"], 3),
    ("e_caret", "x = 2\nprint e^x", ["elementary charge"], 2),
    # list elements
    ("list_mixed", "xs = [1 m, 2 s]", ["all elements of a list need the same units", "time [s]", "length [m]"], 1),
    ("list_plain", "xs = [1 m, 2]", ["same units"], 1),
    ("list_set", "xs = [1 s, 2 s]\nxs[1] = 3 m", ["xs is a list of time [s]", "length [m]"], 2),
    ("list_push", "xs = [1 s]\npush(xs, 3 m)", ["time [s]", "length [m]"], 2),
    # reassigning a variable with different units
    ("reassign", "x = 3 m\nx = 2 s", ["x is length [m]; it can't now hold time [s]"], 2),
    ("reassign_plus", "x = 3 m\nx += 2 s", ["can't add"], 2),
    ("reassign_times", "x = 3 m\nx *= 2 m", ["x is length [m]", "area [m²]"], 2),
    ("reassign_in_loop", "x = 1 m\nfor i from 1 to 3\n    x = i", ["x is length [m]"], 3),
    ("infer_zero", "E = 0\nE += 3 J\nprint E\nE += 2 m", ["can't add energy [J] to length [m]"], 4),
    ("infer_zero_where", "E = 0\nE += ½ m v² where m = 2 kg, v = 3 m/s\nE += 1 N", ["can't add", "energy", "force"], 3),
    # integrals
    ("int_limits", "print ∫ x dx from 0 m to 1 s", ["limits of this integral", "length [m]", "time [s]"], 1),
    ("int_result", "k = 5 N/m\nF(x) = k x\nW = ∫ F(x) dx from 0 m to 1 m\nprint W + 1 N", ["can't add", "energy [J]", "force [N]"], 4),
    ("int_limit_vs_param", "f(x) = sin(x)\nprint ∫ f(x) dx from 0 m to 1 m", ["sin needs a plain number"], None),
    # ODEs
    ("ode_sides", SPRING + "solve m x'' = -k\n  with x(0) = 0.1 [m], x'(0) = 0 m/s\n  for t from 0 s to 1 s",
     ["two sides of this equation don't match", "force [N]", "spring constant [N/m]"], 3),
    ("ode_ic_units", SPRING + "solve m x'' = -k x\n  with x(0) = 0.1 s, x'(0) = 0 m/s\n  for t from 0 s to 1 s",
     ["x'"], 4),
    ("ode_ic_rate", "a = 2 s⁻¹\nsolve x' = -a x with x(0) = 1 kg, x'(0) = 3 kg for t from 0 s to 1 s", ["x'"], 2),
    ("ode_range", "solve x' = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 m", ["the range goes from time [s] to length [m]"], 1),
    ("ode_eval", "solve x' = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 s\nprint x(0.5 m)",
     ["x is a function of t", "time [s]", "length [m]"], 2),
    ("ode_rate_units", "solve x' = -x with x(0) = 1 kg for t from 0 s to 1 s", ["don't match"], 1),
    # fit
    ("fit_model", 'data = load "pend.csv"\nfit T = a L + 1 m to data', ["the model gives length [m] but T is time [s]"], 2),
    ("fit_guess", 'data = load "pend.csv"\nfit T = 2π √(L / g) to data with g = 5 m', ["starting guess for g", "length [m]", "acceleration [m/s²]"], 2),
    ("fit_plain_model", 'data = load "pend.csv"\nfit T = a sin(L) to data', ["sin needs a plain number"], 2),
    # in conversions
    ("in_m_s", "print 3 m in s", ["can't show length [m] in s", "time [s]"], 1),
    ("in_acc_m", "g = 9.81 m/s²\nprint g in m", ["can't show acceleration [m/s²] in m"], 2),
    ("in_J_W", "print 1 J in W", ["can't show energy [J] in W"], 1),
    ("in_fm_eV", "print 1 fm in eV", ["can't show length"], 1),
    ("in_u_MeV", "print 1 u in MeV", ["can't show mass"], 1),
    ("in_kmhr_m", "print 100 km/hr in m", ["can't show speed"], 1),
    ("in_barn_m", "print 1 barn in m", ["can't show area"], 1),
    ("in_C_m", "print 20 °C in m", ["can't show"], 1),
    ("to_func", "print to(3 m, s)", ["can't show length [m] in s"], 1),
    # °C
    ("degC_squared", "c = 4.2 J/(°C m)\nd = c + 1 J/m", ["can't add"], 2),
    ("degC_plus_m", "print 20 °C + 5 m", ["can't add"], 1),
    ("degC_plus_degC", "print 20 °C + 10 °C", ["absolute temperatures"], 1),
    # loops and branches
    ("for_step_units", "for t from 0 s to 1 s step 1 m\n    print t", ["the step is length [m] but the range is time [s]"], 1),
    ("for_needs_step", "for t from 0 s to 1 s\n    print t", ["needs a step with units"], 1),
    ("for_range", "for t from 0 s to 1 m step 0.1 s\n    print t", ["time [s]", "length [m]"], 1),
    ("if_branches", "y = if 3 m > 2 m then 1 m else 2 s", ["the two branches give length [m] and time [s]"], 1),
    # derivatives
    ("deriv_add", "A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nv = d/dt x\nprint v(1 s) + 1 m", ["can't add speed [m/s] to length [m]"], 5),
    ("prime_add", "x(t) = 3 m/s * t\nprint x'(1 s) + 1 m", ["can't add", "speed [m/s]", "length [m]"], 2),
    # functions: inferred parameter units
    ("inferred_param", "x(t) = 0.1 m cos(10 t / 1 s)\nprint x(3 m)", ["cos needs a plain number"], None),
]


@pytest.mark.parametrize("src,phrases,line", [pytest.param(s, p, ln, id=i) for i, s, p, ln in REJECT])
def test_rejected(src, phrases, line, tmp_path):
    (tmp_path / "pend.csv").write_text(CSV)
    e = error_of(src, base_dir=str(tmp_path))
    msg = e.message
    for ph in phrases:
        assert ph in msg, f"{ph!r} not in {msg!r}"
    if line is not None:
        assert e.line == line, f"error on line {e.line}, expected {line}: {msg}"
    # one line, readable, no Python internals
    assert "\n" not in msg
    assert "Traceback" not in msg and "Error:" not in msg
    assert len(msg) < 200


def test_reject_count():
    assert len(REJECT) >= 60


@pytest.mark.parametrize("src,phrases,line", [pytest.param(s, p, ln, id=i) for i, s, p, ln in REJECT
                                              if "load" not in s])
def test_rejected_by_check_alone(src, phrases, line):
    """Unit errors are found before the program runs (fermium check)."""
    with pytest.raises(FermiumError) as ei:
        check_only(src)
    assert phrases[0] in ei.value.message


def test_error_format_has_caret_and_hint():
    e = error_of(XT + "y = x + t")
    text = e.format(XT + "y = x + t")
    lines = text.split("\n")
    assert lines[0] == "line 3: can't add length [m] to time [s]"
    assert lines[1] == "    y = x + t"
    assert lines[2].strip().startswith("^")
    assert lines[2].index("^") == lines[1].index("x")
    assert lines[3].startswith("  hint:")


def test_bad_call_points_at_call_site():
    e = error_of("f(x) = x + 1 m\nprint f(2 s)")
    assert "can't add" in e.message
    assert e.line == 2 or "line 2" in str(e)


def test_bad_call_points_at_call_site_inferred():
    e = error_of("A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nprint x(3 m)")
    assert e.line == 4 or "line 4" in str(e)


# ---------------------------------------------------------------------------------- accepted
ACCEPT = [
    # the spec's pendulum
    ("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g", "9.70 m/s²"),
    ("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g in ft/s²", "31.8 ft/s²"),
    ("k = 50 N/m\nm = 0.5 kg\nprint √(k/m)", "10 1/s"),
    ("k = 50 N/m\nm = 0.5 kg\nprint √(k/m) in rad/s", "10 rad/s"),
    # constants
    ("print ħ c in MeV fm to 6 digits", "197.327 MeV fm"),
    ("print hbar c in MeV fm to 6 digits", "197.327 MeV fm"),
    ("T = 300 K\nprint k_B T in eV to 5 digits", "0.025852 eV"),
    ("print m_e c² in MeV to 6 digits", "0.510999 MeV"),
    ("print m_p c^2 in MeV to 6 digits", "938.272 MeV"),
    ("print 1 u c² in MeV to 6 digits", "931.494 MeV"),
    ("print 1/α to 6 digits", "137.036"),
    ("print c in km/s to 6 digits", "299792 km/s"),
    ("print G M_sun / AU^2 to 6 digits", "0.00593008 m/s²"),
    ("print √(2 G M_earth / R_earth) in km/s to 6 digits", "11.1799 km/s"),
    ("print a_0 in Å to 6 digits", "0.529177 Å"),
    # rational exponents
    ("A = 27\nprint 1.2 [fm] A^(1/3)", "3.6 fm"),
    ("print (8 m³)^(1/3)", "2 m"),
    ("x = 4 m²\nprint x^(1/2)", "2 m"),
    ("print (1 m^3)^(2/3)", "1 m²"),
    ("print √(9 m²), ∛(27 m³)", "3 m 3 m"),
    # inference through 0
    ("E = 0\nE += 3 J\nE += 2 J\nprint E", "5 J"),
    ("m = 2 kg\nv = 3 m/s\nE = 0\nE += ½ m v²\nprint E", "9 J"),
    ("total = 0\nfor i from 1 to 4\n    total += 1 s\nprint total", "4 s"),
    # inferred function parameter units (monomorphised per call)
    ("f(x) = 2 x\nprint f(1 m), f(2 s)", "2 m 4 s"),
    ("A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nprint x", "x(t) = A cos(ω t)   [m, for t in s]"),
    ("speed(h) =\n    g = 9.81 m/s²\n    return √(2*g*h)\nprint speed(10 m)", "14.0 m/s"),
    ("f(x [m]) = 2 x\nprint f(3 cm) in cm", "6 cm"),
    ("KE(m, v) = ½ m v²\nprint KE(2 kg, 3 m/s)", "9 J"),
    # mixed units of the same kind
    ("print 3 m + 2 cm", "3.02 m"),
    ("print 1 km > 999 m", "true"),
    ("print 1 m ≈ 100 cm", "true"),
    ("print 1 hr + 30 min in min", "90 min"),
    ("print 5 N m in J", "5 J"),
    ("print atan2(1 m, 1 m) to 6 digits", "0.785398"),
    ("print hypot(3 m, 4 m)", "5 m"),
    # plain-number functions of dimensionless ratios
    ("print sin(30°)", "0.500"),
    ("print cos(π)", "-1"),
    ("t = 2 s\nτ = 1 s\nprint exp(-t/τ) to 6 digits", "0.135335"),
    ("p = 2 atm\nprint ln(p / (1 atm)) to 6 digits", "0.693147"),
    ("print sin(90 deg)", "1"),
    # unit conversions
    ("print 1 eV in J to 6 digits", "1.60218×10⁻¹⁹ J"),
    ("print 1 MeV in J to 6 digits", "1.60218×10⁻¹³ J"),
    ("print 1 GeV in MeV", "1000 MeV"),
    ("print 1 J in eV to 6 digits", "6.24151×10¹⁸ eV"),
    ("print 1 fm in m", "1.00×10⁻¹⁵ m"),
    ("print 1 Å in m", "1.00×10⁻¹⁰ m"),
    ("print 1 angstrom in m", "1.00×10⁻¹⁰ m"),
    ("print 1 u in kg to 6 digits", "1.66054×10⁻²⁷ kg"),
    ("print 1 amu in kg to 6 digits", "1.66054×10⁻²⁷ kg"),
    ("print 1 barn in m²", "1.00×10⁻²⁸ m²"),
    ("print 1 b in fm²", "100 fm²"),
    ("print 1 AU in m to 6 digits", "1.49598×10¹¹ m"),
    ("print 1 ly in m to 6 digits", "9.46073×10¹⁵ m"),
    ("print 1 pc in ly to 6 digits", "3.26156 ly"),
    ("print 1 kpc in ly to 6 digits", "3261.56 ly"),
    ("print 1 M☉ in kg to 6 digits", "1.98841×10³⁰ kg"),
    ("print 1 Msun in kg to 6 digits", "1.98841×10³⁰ kg"),
    ("print 1 erg in J", "1.00×10⁻⁷ J"),
    ("print 1 atm in Pa", "101325 Pa"),
    ("print 1 bar in Pa", "100000 Pa"),
    ("print 760 Torr in atm", "1 atm"),
    ("print 760 mmHg in atm", "1.00 atm"),
    ("print 180 ° in rad to 6 digits", "3.14159 rad"),
    ("print 1 rad in ° to 6 digits", "57.2958°"),
    ("print 100 km/hr in m/s to 6 digits", "27.7778 m/s"),
    ("print 1 mi in km to 6 digits", "1.60934 km"),
    ("print 1 lb in kg to 6 digits", "0.453592 kg"),
    ("print 1 cal in J to 4 digits", "4.184 J"),
    ("print 1 L in m³", "0.00100 m³"),
    ("print 1 day in hr", "24 hr"),
    ("print 1 hr in s", "3600 s"),
    ("print 1 um in m", "1.00×10⁻⁶ m"),
    ("print 1 µm in m", "1.00×10⁻⁶ m"),
    ("print 1 dyn in N", "1.00×10⁻⁵ N"),
    ("print 1 gauss in T", "0.000100 T"),
    ("print 1 N in kg m/s²", "1 kg m/s²"),
    ("print 1 Pa in N/m²", "1 N/m²"),
    # temperatures
    ("T = 20 °C\nprint T in K to 5 digits", "293.15 K"),
    ("T = 20 °C\nprint T in °F", "68 °F"),
    ("print 300 K in °C to 4 digits", "26.85 °C"),
    ("print 212 °F in °C", "100 °C"),
    ("print 25 degC in K to 5 digits", "298.15 K"),
    ("T = 20 °C\nprint T + 5 K", "25 °C"),
    ("print 20 °C - 10 °C", "10 K"),
    ("print 20 °C > 10 °C", "true"),
    # lists keep units
    ("xs = [1 m, 2 m, 3 m]\nprint sum(xs), mean(xs), max(xs), min(xs)", "6 m 2 m 3 m 1 m"),
    ("xs = [1 m, 2 m, 3 m]\nprint xs in cm", "[100, 200, 300] cm"),
    ("xs = [1 m, 200 cm]\nprint xs[2] in m", "2 m"),
    # calculus units
    ("k = 50 N/m\nF(x) = k x\nW = ∫ F(x) dx from 0 m to 0.2 m\nprint W", "1.0 J"),
    ("k = 5 N/m\nF(x) = k x\nprint ∫ F(x) dx from 0 s to 1 s", "2.50 kg"),
    ("x(t) = 3 m/s² * t^2\nprint x'(2 s), x''(2 s)", "12 m/s 6 m/s²"),
    ("a = 2 s⁻¹\nsolve x' = -a x with x(0) = 1 kg for t from 0 s to 1 s\nprint x(1 s) to 6 digits", "0.135335 kg"),
    ("print to(9.81 m/s², ft/s²)", "32.2 ft/s²"),
    ("where_E = ½ m v² where m = 2 kg, v = 3 m/s\nprint where_E in eV to 6 digits", "5.61736×10¹⁹ eV"),
]


@pytest.mark.parametrize("src,expected", ACCEPT, ids=[f"a{i:02d}" for i in range(len(ACCEPT))])
def test_accepted(src, expected):
    assert run(src) == expected


def test_accept_count():
    assert len(ACCEPT) >= 60


def test_accepted_programs_pass_check():
    for src, _ in ACCEPT:
        check_only(src)


def test_fit_parameter_units_inferred(tmp_path):
    (tmp_path / "pend.csv").write_text(CSV)
    out = run('data = load "pend.csv"\nfit T = a L to data with a = 1 s/m\nprint a', base_dir=str(tmp_path))
    assert out.split("\n")[-1] == "1.61 s/m"


def test_fit_parameter_inferred_through_sqrt(tmp_path):
    (tmp_path / "pend.csv").write_text(CSV)
    out = run('data = load "pend.csv"\nfit T = 2π √(L / g) to data\nprint g in m/s²', base_dir=str(tmp_path))
    assert out.split("\n")[-1].endswith("m/s²")


@pytest.mark.parametrize("a,b", [("1 um", "1 μm"), ("1 angstrom", "1 Å"), ("1 Msun", "1 M☉"),
                                 ("20 degC", "20 °C"), ("30 deg", "30°"), ("1 AU", "1 au")])
def test_ascii_unit_spellings_equal(a, b):
    assert run(f"print {a} == {b}") == "true"


def test_percent_unit():
    assert run("x = 0.5\nprint x in %") in ("50 %", "50%")
    assert run("print 50 % == 0.5") == "true"


def test_constants_units():
    lines = run_lines("print c\nprint h to 6 digits\nprint e in C to 6 digits\nprint N_A\nprint g_n to 6 digits")
    assert lines[0].endswith("m/s")
    assert lines[1] == "6.62607×10⁻³⁴ J s"
    assert lines[2] == "1.60218×10⁻¹⁹ C"
    assert lines[3].endswith("1/mol")
    assert lines[4] == "9.80665 m/s²"


def test_units_erased_same_result():
    """Units cost nothing: a formula with units gives the same number as the plain version."""
    a = run("m = 2 kg\nv = 3 m/s\nprint ½ m v² in J")
    b = run("m = 2\nv = 3\nprint ½ m v²")
    assert a.split()[0] == b
