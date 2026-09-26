"""M3 part 4: 1-D partial differential equations (DECISIONS D83), checked against analytic solutions:
heat decay of a sine mode exp(-D k² t), d'Alembert's solution of the wave equation, and a free Gaussian
wave packet in the time-dependent Schrödinger equation (Crank–Nicolson: its norm is conserved)."""
import io
import math

import pytest

from conftest import run
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
from numparse import num

HBAR = 6.62607015e-34 / (2 * math.pi)
ME = 9.1093837139e-31


def interp(src, base=None):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=base)
    return out.getvalue().strip()


def both(src, base=None):
    a = run(src, base_dir=base)
    assert interp(src, base) == a
    return a


def nums(out):
    return [num(x) for x in out.replace(",", " ").split("\n")]


HEAT = """L = 1 m
D = 0.01 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 2 K * sin(π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 10 s{step}
    grid {grid}{method}
print u(0.5 m, 10 s) / (1 K) to 12 digits
print u(0.25 m, 4 s) / (1 K) to 12 digits
"""


def heat_exact(x, t, D=0.01, L=1.0):
    return 2 * math.exp(-D * (math.pi / L) ** 2 * t) * math.sin(math.pi * x / L)


def test_heat_sine_mode_decays_like_exp_minus_D_k2_t():
    out = both(HEAT.format(step="", grid=200, method="")).splitlines()
    assert num(out[0]) == pytest.approx(heat_exact(0.5, 10), rel=3e-5)
    assert num(out[1]) == pytest.approx(heat_exact(0.25, 4), rel=3e-5)


def test_heat_is_second_order_in_space():
    errs = []
    for grid in (50, 100, 200):
        out = run(HEAT.format(step="", grid=grid, method="")).splitlines()
        errs.append(abs(num(out[0]) - heat_exact(0.5, 10)))
    assert 3.5 < errs[0] / errs[1] < 4.5
    assert 3.5 < errs[1] / errs[2] < 4.5


@pytest.mark.parametrize("method,step,tol", [(" using implicit", " step 0.01 s", 1e-3),
                                             (" using explicit", "", 1e-4),
                                             (" using crank_nicolson", " step 0.5 s", 3e-4)])
def test_heat_methods(method, step, tol):
    out = run(HEAT.format(step=step, grid=200, method=method)).splitlines()
    assert num(out[0]) == pytest.approx(heat_exact(0.5, 10), rel=tol)


def test_heat_explicit_refuses_an_unstable_step():
    with pytest.raises(FermiumError, match="explicit method is unstable"):
        run(HEAT.format(step=" step 1 s", grid=200, method=" using explicit"))


def test_heat_insulated_ends_conserve_heat_and_decay_a_cosine_mode():
    src = """L = 2 m
D = 0.05 m²/s
solve ∂T/∂t = D * ∂²T/∂x²
    with T(x, 0 s) = 300 K + 10 K * cos(π x / L), ∂T/∂x(0 m, t) = 0 K/m, ∂T/∂x(L, t) = 0 K/m
    for x from 0 m to L, t from 0 s to 20 s
    grid 400
print (T(0 m, 20 s) - 300 K) / (10 K) to 12 digits
print ∫ T(x, 20 s) dx from 0 m to L / L in K to 12 digits
"""
    out = both(src).splitlines()
    assert num(out[0]) == pytest.approx(math.exp(-0.05 * (math.pi / 2) ** 2 * 20), rel=1e-4)
    assert num(out[1]) == pytest.approx(300, rel=1e-9)


def test_heat_with_source_and_moving_boundary_reaches_steady_state():
    # ∂u/∂t = D u_xx + S with u(0) = 0, u(L) = 1 K: steady state u = x/L · 1 K + S x (L - x) / (2D)
    src = """L = 1 m
D = 0.1 m²/s
S = 0.4 K/s
solve ∂u/∂t = D * ∂²u/∂x² + S
    with u(x, 0 s) = 0 K, u(0 m, t) = 0 K, u(L, t) = min(t / (1 s), 1) * 1 K
    for x from 0 m to L, t from 0 s to 60 s
    grid 200
print u(0.3 m, 60 s) / (1 K) to 10 digits
"""
    out = both(src).splitlines()
    want = 0.3 + 0.4 * 0.3 * 0.7 / (2 * 0.1)
    assert num(out[0]) == pytest.approx(want, rel=1e-5)


WAVE = """c = 2 m/s
f(x) = 1 cm * exp(-((x - 0.5 m) / 0.05 m)^2)
solve ∂²u/∂t² = c² ∂²u/∂x²
    with u(x, 0 s) = f(x), ∂u/∂t(x, 0 s) = 0 m/s, u(0 m, t) = 0 m, u(1 m, t) = 0 m
    for x from 0 m to 1 m, t from 0 s to 0.1 s{step}
    grid 1000
print u(0.7 m, 0.1 s) / (1 cm) to 12 digits
print u(0.63 m, 0.07 s) / (1 cm) to 12 digits
print ∂u/∂t(0.62 m, 0.05 s) / (1 cm/s) to 12 digits
"""


def dalembert(x, t, c=2.0):
    f = lambda z: math.exp(-((z - 0.5) / 0.05) ** 2)      # noqa: E731
    return 0.5 * (f(x - c * t) + f(x + c * t))


def test_wave_dalembert_at_courant_one():
    out = nums(both(WAVE.format(step="")))
    assert out[0] == pytest.approx(dalembert(0.7, 0.1), abs=1e-9)
    assert out[1] == pytest.approx(dalembert(0.63, 0.07), abs=1e-6)
    # ∂u/∂t = c (f'(x + ct) - f'(x - ct)) / 2
    fp = lambda z: -2 * (z - 0.5) / 0.05 ** 2 * math.exp(-((z - 0.5) / 0.05) ** 2)   # noqa: E731
    want = 2.0 * (fp(0.62 + 0.1) - fp(0.62 - 0.1)) / 2
    assert out[2] == pytest.approx(want, rel=1e-3)


def test_wave_with_a_smaller_step_is_second_order_accurate():
    out = nums(run(WAVE.format(step=" step 0.2 ms")))
    assert out[0] == pytest.approx(dalembert(0.7, 0.1), abs=3e-3)


def test_wave_reflects_with_inverted_sign_at_a_fixed_end():
    src = WAVE.format(step="").replace("t from 0 s to 0.1 s", "t from 0 s to 0.35 s") \
        .replace("print u(0.7 m, 0.1 s)", "print u(0.2 m, 0.35 s)").replace("u(0.63 m, 0.07 s)", "u(0.8 m, 0.35 s)")

    def F(z):          # the odd, period-2 extension that fixed ends at 0 and 1 give (method of images)
        z = (z + 1) % 2 - 1
        return math.copysign(1, z) * math.exp(-((abs(z) - 0.5) / 0.05) ** 2)
    out = nums(run(src))
    for got, x in zip(out[:2], (0.2, 0.8)):
        assert got == pytest.approx(0.5 * (F(x - 0.7) + F(x + 0.7)), abs=1e-6)
    assert out[0] < -0.4 and out[1] < -0.4        # the pulses came back upside down


def test_wave_refuses_a_step_beyond_the_courant_limit():
    with pytest.raises(FermiumError, match="Courant"):
        run(WAVE.format(step=" step 1 ms"))


TDSE = """m = m_e
σ = 1 nm
k0 = 2 / (1 nm)
τ = 2*m σ² / ħ
solve i ħ ∂ψ/∂t = -ħ²/(2*m) * ∂²ψ/∂x²
    with ψ(x, 0 s) = (2π σ²)^(-1/4) exp(-x² / (4σ²)) exp(i k0 x), ψ(-40 nm, t) = 0 nm^(-1/2), ψ(40 nm, t) = 0 nm^(-1/2)
    for x from -40 nm to 40 nm, t from 0 s to 2 τ
    grid {grid}
T = 2 τ
print ∫ |ψ(x, 0 s)|^2 dx from -40 nm to 40 nm to 14 digits
print ∫ |ψ(x, T)|^2 dx from -40 nm to 40 nm to 14 digits
xm = ∫ x |ψ(x, T)|^2 dx from -40 nm to 40 nm
print xm / (ħ k0 T / m) to 10 digits
print sqrt(∫ (x - xm)^2 |ψ(x, T)|^2 dx from -40 nm to 40 nm) / (σ sqrt(1 + (T / τ)^2)) to 10 digits
"""


def test_schrodinger_free_packet_moves_and_spreads_like_the_analytic_solution():
    out = nums(both(TDSE.format(grid=4000)))
    assert abs(out[1] - out[0]) < 1e-10              # Crank–Nicolson is unitary
    assert out[2] == pytest.approx(1, rel=1e-3)      # the centre moves at ħk₀/m
    assert out[3] == pytest.approx(1, rel=1.5e-3)    # the width grows as σ √(1 + (t/τ)²)


def test_schrodinger_spreading_converges_with_the_grid():
    a = nums(run(TDSE.format(grid=2000)))
    b = nums(run(TDSE.format(grid=4000)))
    assert abs(b[3] - 1) < abs(a[3] - 1) / 3
    assert abs(b[2] - 1) < abs(a[2] - 1) / 3


def test_solver_conserves_the_discrete_norm_to_rounding():
    """Directly on runtime/pde.py: Σ|ψ_j|² h is conserved by Crank–Nicolson to 1e-12 over 1000 steps."""
    np = pytest.importorskip("numpy")
    from fermium.runtime.pde import pde_solve
    sigma, k0, L = 1e-9, 3e9, 40e-9

    def probe(x, y):
        u, ux, uxx, t, ut, i = y
        rhs = (-HBAR ** 2 / (2 * ME) * uxx + 1e-20 * (x / 1e-9) ** 2 * u) / (i * HBAR) if i else 0.0  # V ∝ x²
        amp = (2 * math.pi * sigma ** 2) ** -0.25 * math.exp(-x * x / (4 * sigma ** 2))
        return [rhs, amp, k0 * x, 0.0, 0.0, 0.0]
    ts, ys, dys, ncomp, m = pde_solve(probe, -L, L, 0.0, 5e-14, grid=1600, is_complex=True)
    Y = np.array(ys).reshape(len(ts), 2 * (m + 1))
    h = 2 * L / m
    norms = [float(np.sum(r[: m + 1] ** 2 + r[m + 1:] ** 2) * h) for r in Y]
    assert max(norms) - min(norms) < 1e-12
    assert norms[0] == pytest.approx(1, abs=1e-6)


def test_crank_nicolson_matches_scipy_expm_of_the_semidiscrete_system():
    """The heat solver's time stepping against the exact solution of du/dt = T u (scipy.linalg.expm)."""
    np = pytest.importorskip("numpy")
    from scipy.linalg import expm
    from fermium.runtime.pde import pde_solve
    D, M = 0.3, 40

    def probe(x, y):
        u, ux, uxx, t, ut, i = y
        return [D * uxx - 0.5 * u, math.sin(math.pi * x) + 0.3 * math.sin(3 * math.pi * x), 0.0, 0.0, 0.0, 0.0]
    ts, ys, dys, ncomp, m = pde_solve(probe, 0.0, 1.0, 0.0, 0.5, grid=M, step=0.5 / 2000)
    h = 1.0 / M
    n = M - 1
    T = (np.diag(-2 * np.ones(n)) + np.diag(np.ones(n - 1), 1) + np.diag(np.ones(n - 1), -1)) * D / h ** 2 \
        - 0.5 * np.eye(n)
    x = np.linspace(0, 1, M + 1)[1:-1]
    u0 = np.sin(np.pi * x) + 0.3 * np.sin(3 * np.pi * x)
    exact = expm(0.5 * T) @ u0
    got = np.array(ys).reshape(len(ts), M + 1)[-1][1:-1]
    assert np.max(np.abs(got - exact)) < 1e-7


def test_animation_and_snapshot_plot(tmp_path):
    src = HEAT.format(step="", grid=100, method="") + \
        'plot u vs x animate over t frames 12 to "heat.gif"\nplot u vs x to "heat.png"\n'
    out = run(src, base_dir=str(tmp_path))
    assert f"animation saved to {tmp_path / 'heat.gif'} (12 frames)" in out
    assert f"plot saved to {tmp_path / 'heat.png'}" in out
    from PIL import Image
    with Image.open(tmp_path / "heat.gif") as im:
        assert im.n_frames == 12
    assert (tmp_path / "heat.png").stat().st_size > 1000


def test_animation_of_a_wave_packet_shows_the_density(tmp_path):
    src = TDSE.format(grid=800).split("T = 2 τ")[0] + 'plot ψ vs x animate over t frames 5 to "packet.gif"\n'
    out = run(src, base_dir=str(tmp_path))
    assert f"animation saved to {tmp_path / 'packet.gif'} (5 frames)" in out


# ---------------------------------------------------------------- errors
BASE = """L = 1 m
D = 0.01 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with {conds}
    for x from 0 m to L, t from 0 s to 1 s
"""


@pytest.mark.parametrize("conds,match", [
    ("u(0 m, t) = 0 K, u(L, t) = 0 K", "missing the initial condition"),
    ("u(x, 0 s) = 1 K, u(0 m, t) = 0 K", "missing a boundary condition at x = L"),
    ("u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 m", "should be temperature"),
    ("u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(0.5 m, t) = 0 K", "at the ends of the range"),
    ("u(x, 1 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K", "start of the t range"),
])
def test_condition_errors(conds, match):
    with pytest.raises(FermiumError, match=match):
        run(BASE.format(conds=conds))


def test_equation_errors():
    with pytest.raises(FermiumError, match="don't match|same units"):
        run(BASE.replace("D * ∂²u/∂x²", "D * ∂u/∂x").format(conds="u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K"))
    with pytest.raises(FermiumError, match="linear"):
        run(BASE.replace("D * ∂²u/∂x²", "D * ∂²u/∂x² - u^2 / (1 K s)").format(
            conds="u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K"))
    with pytest.raises(FermiumError, match="initial velocity"):
        run("solve ∂²u/∂t² = (1 m/s)^2 * ∂²u/∂x² with u(x, 0 s) = 0 m, u(0 m, t) = 0 m, u(1 m, t) = 0 m "
            "for x from 0 m to 1 m, t from 0 s to 1 s")


def test_using_the_solution():
    src = BASE.format(conds="u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K")
    with pytest.raises(FermiumError, match="write u\\(x, t\\)"):
        run(src + "print u")
    with pytest.raises(FermiumError, match="outside the range"):
        run(src + "print u(2 m, 0.5 s)")
    with pytest.raises(FermiumError, match="second argument is t"):
        run(src + "print u(0.5 m, 2 m)")


def test_build_refuses_pdes(tmp_path):
    from fermium.aot import build
    with pytest.raises(FermiumError, match="fermium build can't compile a PDE"):
        build(HEAT.format(step="", grid=50, method=""), str(tmp_path / "h.fm"), str(tmp_path / "h"))


def test_barrier_walls_between_grid_points_converge():
    """A barrier whose walls fall between grid points is cell-averaged (D83), so the transmitted probability
    converges smoothly with the grid instead of jumping around by 20 %."""
    np = pytest.importorskip("numpy")
    from fermium.runtime.pde import pde_solve
    ev = 1.602176634e-19
    s, k0 = 2e-9, math.sqrt(2 * ME * 0.2 * ev) / HBAR

    def probe(x, y):
        u, ux, uxx, t, ut, i = y
        V = 0.3 * ev if abs(x) < 0.5e-9 else 0.0
        rhs = (-HBAR ** 2 / (2 * ME) * uxx + V * u) / (i * HBAR) if i else 0.0
        return [rhs, (2 * math.pi * s * s) ** -0.25 * math.exp(-(x + 15e-9) ** 2 / (4 * s * s)), k0 * x, 0, 0, 0]
    through = []
    for grid in (1500, 3000, 6000):
        ts, ys, dys, nc, m = pde_solve(probe, -60e-9, 60e-9, 0, 100e-15, grid=grid, is_complex=True)
        r = np.array(ys).reshape(len(ts), 2 * (m + 1))[-1]
        x = np.linspace(-60e-9, 60e-9, m + 1)
        p = r[: m + 1] ** 2 + r[m + 1:] ** 2
        through.append(float(p[x > 0.5e-9].sum() * (120e-9 / m)))
    assert abs(through[1] - through[2]) < 0.003 * through[2]
    assert abs(through[0] - through[2]) < 0.01 * through[2]


def test_animation_to_a_png_name_writes_png_frames(tmp_path):
    # (matplotlib itself needs pillow, so the GIF writer is always there with matplotlib; frames are the
    # fallback for a name that isn't .gif)
    src = HEAT.format(step="", grid=50, method="") + 'plot u vs x animate over t frames 4 to "heat.png"\n'
    out = run(src, base_dir=str(tmp_path))
    assert f"animation saved as 4 PNG frames in {tmp_path / 'heat_frames'}/" in out
    assert sorted(p.name for p in (tmp_path / "heat_frames").iterdir()) == [f"frame_000{i}.png" for i in range(4)]
