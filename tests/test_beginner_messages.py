"""Beginner mistakes get messages that say what to write instead (Phase 4 beginner pass)."""
import pytest

from conftest import error_of, warnings_of


@pytest.mark.parametrize("src,hint", [
    ("x = 5 meters", "Fermium writes units as symbols: m"),
    ("t = 3 seconds", "Fermium writes units as symbols: s"),
    ("L = 2 m\nprint L in feet", "Fermium writes units as symbols: ft"),
    ("for i in range(10)\n    print i", "for i from 1 to 10"),
    ("x = 3\nx++", "Fermium has no ++; write  x += 1"),
    ("def f(x): return x", "f(x) = 2 x"),
    ("v = 5 m/s\nprint v.x", "a vector is written <3, 4> m/s"),
])
def test_hints(src, hint):
    e = error_of(src)
    assert hint in (e.hint or "") or hint in str(e)


def test_zero_index_mentions_counting_from_one():
    assert "Fermium counts from 1" in str(error_of("xs = [1, 2, 3]\nprint xs[0]"))


def test_sin_of_degrees_written_as_a_number_warns():
    assert any("30 radians" in w for w in warnings_of("print sin(30)"))
    assert not warnings_of("print sin(30°)")
    assert not warnings_of("print sin(2)")
