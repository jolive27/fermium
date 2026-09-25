"""Research reproductions (research/<name>/): each program's numbers against an independent computation."""
import os
import re

import numpy as np
import pytest

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RES = os.path.join(ROOT, "research")


def run_prog(name, prog):
    d = os.path.join(RES, name)
    return run(open(os.path.join(d, prog), encoding="utf-8").read(), base_dir=d)


def num(text, label):
    m = re.search(re.escape(label) + r"\s*=?\s*(-?[\d.]+(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+)?)", text)
    s = m.group(1)
    if "×10" in s:
        mant, ex = s.split("×10")
        ex = ex.translate(str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789"))
        return float(mant) * 10 ** int(ex)
    return float(s)


def test_semf_fit_matches_numpy_least_squares():
    out = run_prog("semf_ame2020", "semf.fm")
    data = np.genfromtxt(os.path.join(RES, "semf_ame2020", "ame2020_binding.csv"), delimiter=",", skip_header=1)
    Z, N, A, P, B = data.T
    X = np.column_stack([A, -A ** (2 / 3), -Z * (Z - 1) / A ** (1 / 3), -(A - 2 * Z) ** 2 / A, P / np.sqrt(A)])
    coef, *_ = np.linalg.lstsq(X, B, rcond=None)     # the model is linear in the coefficients
    for name, c in zip(["a_V", "a_S", "a_C", "a_A", "a_P"], coef):
        assert num(out, f"  {name} =") == pytest.approx(c, rel=2e-4), name
    rms = np.sqrt(np.mean((B - X @ coef) ** 2))
    assert num(out, "rms residual:") == pytest.approx(np.std(B - X @ coef, ddof=1), rel=1e-4)   # sample std
    assert rms < 3.5
    assert "at Z = 50 N = 82" in out          # doubly magic ¹³²Sn has the largest shell correction
    for n0 in (50, 82, 126):
        line = next(ln for ln in out.splitlines() if ln.startswith(f"N = {n0} "))
        vals = [float(v) for v in re.findall(r"(-?[\d.]+) MeV", line)]
        assert vals[0] > vals[1] and vals[0] > vals[2]     # magic isotones are more bound than N ± 8
