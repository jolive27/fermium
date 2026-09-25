"""Gauntlet friction fixes in the checker, units and constants (gauntlet/FRICTION.md #4, #5, #6, #16, #27, #34)."""
from conftest import run, warnings_of


def test_4_variable_set_again_in_a_second_loop():
    src = "n = 0\nwhile n < 3\n    mid = n\n    n = mid + 1\nwhile n < 6\n    mid = n\n    n = mid + 1\nprint n"
    assert run(src) == "6"


def test_4_still_an_error_when_really_unset():
    from conftest import error_of
    assert "might not have a value" in str(error_of("n = 0\nwhile n < 3\n    mid = n\n    n = mid + 1\nprint mid"))


def test_5_omega_in_hz_warns():
    src = "k = 50 N/m\nm = 0.125 kg\nω0 = √(k/m)\nprint ω0 in Hz"
    assert any("not the frequency ω0/2π" in w for w in warnings_of(src))
    assert not warnings_of("k = 50 N/m\nm = 0.125 kg\nω0 = √(k/m)\nprint ω0/(2π) in Hz")
    assert not warnings_of("f = 3 Hz\nprint f in kHz")


def test_6_temperature_difference_in_celsius_has_no_offset():
    src = "T0 = 90 °C\nd = T0 - 20 °C\nprint d in °C\nprint (T0 - 20 °C) in °F\nprint T0 in °F"
    assert run(src).split("\n") == ["70 °C", "126 °F", "194 °F"]
    assert any("difference of two temperatures" in w for w in warnings_of(src))


def test_6_scaling_an_absolute_temperature_warns():
    assert any("scales an absolute temperature" in w for w in warnings_of("T0 = 90 °C\nprint 2 T0"))


def test_16_milli_and_micro_arcseconds():
    assert run("θ = 1 arcsec\nprint θ in mas\nprint θ in μas\nprint 5 marcsec in mas").split("\n") == \
        ["1000 mas", "1000000 μas", "5 mas"]


def test_27_gm_constants():
    assert run("print GM_sun / G in kg\nprint GM☉").split("\n") == ["1.98841×10³⁰ kg", "1.32712×10²⁰ m³/s²"]
    assert run("print √(GM_earth / R_earth)") == "7905.39 m/s"


def test_34_redefining_a_constant_that_was_used():
    assert any("from here on, h means your value" in w for w in warnings_of("E = h * 1 Hz\nh = 3 m\nprint h"))
    assert not warnings_of("h = 10 m\nprint h")          # h for height, never used as Planck's constant


def test_26_fit_standard_errors_as_values(tmp_path):
    import shutil
    import os
    shutil.copy(os.path.join(os.path.dirname(__file__), "..", "examples", "data", "pendulum.csv"), tmp_path)
    src = 'data = load "pendulum.csv"\nfit T = 2π √(L/g) to data\nprint g, err(g)\nprint err(g)/g'
    out = run(src, base_dir=str(tmp_path)).splitlines()
    assert out[-2:] == ["9.82 m/s² 0.021 m/s²", "0.0021"]


def test_26_err_of_something_else_is_an_error():
    from conftest import error_of
    assert "standard error of a parameter found by fit" in str(error_of("x = 3\nprint err(x)"))


def test_21_celsius_in_compound_units_is_a_temperature_step():
    src = ("r = 2 °C/min\nprint r\nprint r in K/s\nc = 4.18 J/(g °C)\nprint c in J/(kg K)\n"
           "T = 90 °C\nprint T + r * 5 min\nprint 0.5 K/s in °C/min\nprint 1 °F/s in K/s")
    assert run(src).split("\n") == ["2 °C/min", "0.0333333 K/s", "4180 J/(kg K)", "100 °C", "30 °C/min",
                                    "0.555556 K/s"]
