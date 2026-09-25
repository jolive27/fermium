"""D7 (revised): a single unit right after a number that is also one of your variables."""
import pytest

from conftest import run, error_of, warnings_of


@pytest.mark.parametrize("src,shown", [
    ("g = 9.81 m/s²\nh = 2 m\nprint √(2 g h)", "2 g"),
    ("m = 2 kg\nv = 3 m/s\nprint 0.5 m v^2", "0.5 m"),
    ("m = 2 kg\nv = 3 m/s\nprint 2 m v", "2 m"),
    ("g = 9.81 m/s²\nh = 10 m\nprint 2 g * h", "2 g"),
    ("g = 9.81 m/s²\nh = 10 m\nprint h * 2 g", "2 g"),
    ("m = 2 kg\nt = 3 s\nprint 2 m / t", "2 m"),
    ("V = 2 m³\nx = 3 V V", "3 V"),
])
def test_combined_with_other_factors_is_an_error(src, shown):
    e = error_of(src)
    assert f"'{shown}' is ambiguous" in str(e)
    assert "[" in e.hint and "*" in e.hint


def test_error_names_the_unit_in_words():
    assert "g is a unit (grams)" in str(error_of("g = 9.81 m/s²\nh = 2 m\nprint 2 g h"))


@pytest.mark.parametrize("src,out", [
    ("m = 0.5 kg\nk = 50 N/m\nF(x) = k x\nprint ∫ F(x) dx from 0 m to 0.2 m", "1.0 J"),   # the spec's own example
    ("m = 2 kg\nx = 3 m\nprint x", "3 m"),
])
def test_standing_alone_it_is_the_unit_with_a_warning(src, out):
    assert run(src) == out
    assert any("is the unit m, not your variable m" in w for w in warnings_of(src))


@pytest.mark.parametrize("src,out", [
    ("g = 9.81 m/s²\nh = 10 m\nprint 2*g*h", "196 J/kg"),
    ("g = 9.81 m/s²\nprint 2 [g]", "2 g"),
    ("m = 0.5 kg\ng = 9.81 m/s²\nprint m g", "4.9 N"),
    ("m = 0.5 kg\nprint 9.81 m/s²", "9.81 m/s²"),                # compound units are never ambiguous
    ("m = 2 kg\nv = 3 m/s\nprint ½ m v²", "9 J"),                  # ½ isn't a digit literal
])
def test_unambiguous_forms(src, out):
    assert run(src) == out
