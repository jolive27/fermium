"""A fit whose parameters appear only together (`A B`) is degenerate on every platform (D262, macOS CI)."""
import numpy as np

from fermium.runtime.fitting import degenerate


def test_exactly_and_nearly_singular_are_degenerate():
    J = np.array([[1.0, 2.0], [2.0, 4.0], [3.0, 6.0]])          # columns proportional: A and B only as A·B
    assert degenerate(J.T @ J)
    Jn = J.copy()
    Jn[:, 1] *= 1 + 1e-15 * np.array([1, -1, 1])             # what rounding does on another platform
    assert degenerate(Jn.T @ Jn)


def test_badly_scaled_but_healthy_fit_is_not_degenerate():
    t = np.linspace(0, 5e-3, 11)
    J = np.column_stack([1e20 * np.exp(-t / 3e-3), 1e-3 * t])  # parameters of wildly different sizes
    assert not degenerate(J.T @ J)


def test_zero_column_is_degenerate():
    J = np.array([[1.0, 0.0], [2.0, 0.0]])
    assert degenerate(J.T @ J)
