"""Runtime glue for the M3 solvers that run in Python (DECISIONS D82, D83): compiled code calls these
through ctypes callbacks registered in runtime/core.py, passing the address of a compiled right-hand
side; the result goes back as a malloc'ed SolStruct, like the stiff solvers' (D42)."""
from __future__ import annotations

import ctypes

from .core import SolStruct, _libc, DPTR

c_double, c_void_p = ctypes.c_double, ctypes.c_void_p


class _Inner(Exception):
    """The compiled right-hand side stopped with its own run-time error (message and line already set)."""


def compiled_rhs(guard, fn, env, n):
    """A Python f(t, y) -> dy calling a compiled ODE_FN lambda through fm_ode_guard."""
    call = ctypes.CFUNCTYPE(ctypes.c_int32, c_void_p, DPTR, c_double, DPTR, DPTR)(guard)
    yb = (c_double * n)()
    ob = (c_double * n)()

    def f(t, y):
        for j in range(n):
            yb[j] = y[j]
        if call(fn, env, t, yb, ob):
            raise _Inner()
        return ob[:n]
    f._keep = call
    return f


def solstruct(ts, ys, dys, dim):
    """A malloc'ed SolStruct holding copies of the arrays (the compiled program owns it)."""
    libc = _libc()
    m = len(ts)
    sp = ctypes.cast(libc.malloc(ctypes.sizeof(SolStruct)), ctypes.POINTER(SolStruct))
    s = sp.contents
    s.n, s.dim, s.cap = m, dim, m
    for name, vals in (("t", ts), ("y", ys), ("dy", dys)):
        buf = libc.malloc(max(1, len(vals)) * 8)
        ctypes.memmove(buf, (c_double * len(vals))(*vals), len(vals) * 8)
        setattr(s, name, ctypes.cast(buf, DPTR))
    return ctypes.cast(sp, c_void_p).value


def eigen_cb(rt, guard, fn, env, a, b, nstates, grid, method, out):
    """fm_eigen: 0 = ok, 1 = solver error (rt.error set), 2 = the equation stopped with its own error."""
    from .eigen import EigenFail, eigen_solve
    try:
        f = compiled_rhs(guard, fn, env, 3)
        xs, ys, dys, _ = eigen_solve(f, a, b, nstates, grid, "shooting" if method == 1 else "matrix")
    except _Inner:
        return 2
    except EigenFail as ex:
        rt.error = ex.message
        return 1
    except BaseException as ex:          # nothing may escape into the compiled code
        rt.error = f"the eigenvalue solver failed: {ex}"
        return 1
    out[0] = solstruct(xs, ys, dys, 3 * int(nstates))
    return 0


PDE_METHOD_NAMES = {0: "crank_nicolson", 1: "implicit", 2: "explicit"}


def pde_cb(rt, guard, fn, env, xa, xb, t0, t1, step, grid, order, method, bcl, bcr, cx, tdep, out):
    """fm_pde: 0 = ok, 1 = solver error (rt.error set), 2 = the equation stopped with its own error."""
    from .pde import PdeFail, pde_solve
    try:
        f = compiled_rhs(guard, fn, env, 6)
        ts, ys, dys, ncomp, m = pde_solve(f, xa, xb, t0, t1, grid=grid, order=order,
                                          method=PDE_METHOD_NAMES[method], step=None if step != step else step,
                                          bc=(bcl, bcr), is_complex=bool(cx), tdep=bool(tdep))
    except _Inner:
        return 2
    except PdeFail as ex:
        rt.error = ex.message
        return 1
    except BaseException as ex:          # nothing may escape into the compiled code
        rt.error = f"the PDE solver failed: {ex}"
        return 1
    out[0] = solstruct(ts, ys, dys, ncomp * (m + 1))
    return 0


def pde_weights(r, h, which):
    """Cubic Lagrange weights through the grid points j-1, j, j+1, j+2 at x = x_j + r h (or their x-derivatives)."""
    if which == 1:
        w = [-(3 * r * r - 6 * r + 2) / 6, (3 * r * r - 4 * r - 1) / 2, -(3 * r * r - 2 * r - 2) / 2,
             (3 * r * r - 1) / 6]
        return [c / h for c in w]
    return [-r * (r - 1) * (r - 2) / 6, (r + 1) * (r - 1) * (r - 2) / 2, -(r + 1) * r * (r - 2) / 2,
            (r + 1) * r * (r - 1) / 6]


def pde_eval_py(sol_eval, xa, xb, m, comp0, x, t, which):
    """u(x, t) from the snapshots: cubic Lagrange interpolation in x through 4 grid points (mirrors
    codegen_m3.pde_eval).  sol_eval(comp, t, use_dy) evaluates one grid point's time series."""
    import math
    h = (xb - xa) / m
    s = (x - xa) / h
    j = int(math.floor(s))
    j = min(max(j, 1), m - 2)
    r = s - j
    w = pde_weights(r, h, which)
    acc = 0.0
    for k in range(4):
        acc = acc + w[k] * sol_eval(comp0 + j - 1 + k, t, which == 2)
    return acc


def animate(rt, aid, ts, Y, xa, xb):
    """Draw `plot u vs x animate over t` (a GIF with pillow, else PNG frames in a folder) or, without
    animate, one PNG with the solution at 6 times.  Y: array (snapshots, ncomp·(m+1))."""
    import os
    import numpy as np
    from .core import display_unit
    from ..units import format_number
    info = rt.tables.m3_anims[aid]
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        rt.out.write("(animation skipped: matplotlib is not installed -- run: pip install matplotlib)\n")
        return
    m, ncomp = info["m"], info["ncomp"]
    ts = np.asarray(ts, dtype=float)
    Y = np.asarray(Y, dtype=float).reshape(len(ts), ncomp * (m + 1))
    xu = display_unit(info["rxdim"], info["xhint"])
    tu = display_unit(info["rtdim"], info["thint"])
    xs = (np.linspace(xa, xb, m + 1) - xu.offset) / xu.factor
    name = info["name"]
    if ncomp == 2:                      # a complex solution: show the probability density |ψ|²
        vals = Y[:, : m + 1] ** 2 + Y[:, m + 1:] ** 2
        uu = display_unit(info["rudim"] ** 2, None)
        try:                             # a density per nm when x is in nm
            from ..units import parse_unit_string
            per = parse_unit_string(f"1/{xu.name}")
            if per.dim == info["rudim"] ** 2:
                uu = per
        except Exception:
            pass
        ylabel = f"|{name}|²"
    else:
        uu = display_unit(info["rudim"], info["uhint"])
        vals = Y
        ylabel = name
    vals = (vals - uu.offset) / uu.factor
    ylabel += f" [{uu.name}]" if uu.name not in ("", "1") else ""
    xlabel = info["xname"] + (f" [{xu.name}]" if xu.name not in ("", "1") else "")

    def tlabel(t):
        v = (t - tu.offset) / tu.factor
        return f"{info['tname']} = {format_number(v, 4, trim=True)}" + (f" {tu.name}" if tu.name not in ("", "1")
                                                                         else "")
    full, out = info["full"], info["out"]
    d = os.path.dirname(full)
    if d:
        os.makedirs(d, exist_ok=True)
    fig, ax = plt.subplots(figsize=(7, 4.5), dpi=100)
    lo, hi = float(np.min(vals)), float(np.max(vals))
    pad = 0.05 * (hi - lo if hi > lo else 1.0)
    ax.set_xlim(xs[0], xs[-1])
    ax.set_ylim(lo - pad, hi + pad)
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.grid(True, alpha=0.3)
    if info.get("title"):
        ax.set_title(info["title"])
    if not info["animate"]:
        for k in np.linspace(0, len(ts) - 1, min(6, len(ts))).round().astype(int):
            ax.plot(xs, vals[k], linewidth=1.8, label=tlabel(ts[k]))
        ax.legend()
        fig.tight_layout()
        fig.savefig(full)
        plt.close(fig)
        rt.plots_saved.append(full)
        rt.out.write(f"plot saved to {out}\n")
        rt.out.flush()
        return
    frames = min(info["frames"], len(ts))
    idx = np.linspace(0, len(ts) - 1, frames).round().astype(int)
    line, = ax.plot(xs, vals[idx[0]], linewidth=1.8)
    label = ax.text(0.02, 0.95, tlabel(ts[idx[0]]), transform=ax.transAxes, va="top")
    fig.tight_layout()
    try:
        import PIL  # noqa: F401
        have_pillow = True
    except ImportError:
        have_pillow = False
    if have_pillow and full.lower().endswith(".gif"):
        from matplotlib.animation import FuncAnimation, PillowWriter

        def draw(k):
            line.set_ydata(vals[idx[k]])
            label.set_text(tlabel(ts[idx[k]]))
            return line, label
        anim = FuncAnimation(fig, draw, frames=len(idx), blit=False)
        anim.save(full, writer=PillowWriter(fps=15))
        rt.plots_saved.append(full)
        rt.out.write(f"animation saved to {out} ({len(idx)} frames)\n")
    else:
        folder = os.path.splitext(full)[0] + "_frames"
        os.makedirs(folder, exist_ok=True)
        for n, k in enumerate(idx):
            line.set_ydata(vals[k])
            label.set_text(tlabel(ts[k]))
            fig.savefig(os.path.join(folder, f"frame_{n:04d}.png"))
        rt.plots_saved.append(folder)
        why = "" if have_pillow else " (install pillow for a GIF)"
        rt.out.write(f"animation saved as {len(idx)} PNG frames in {os.path.splitext(out)[0]}_frames/{why}\n")
    plt.close(fig)
    rt.out.flush()


def animate_cb(rt, aid, solp, xa, xb):
    """fm_animate: 0 = ok, 1 = failed (rt.error set)."""
    import numpy as np
    try:
        s = ctypes.cast(solp, ctypes.POINTER(SolStruct)).contents
        ts = np.ctypeslib.as_array(s.t, shape=(s.n,)).copy()
        Y = np.ctypeslib.as_array(s.y, shape=(s.n * s.dim,)).reshape(s.n, s.dim).copy()
        animate(rt, aid, ts, Y, xa, xb)
        return 0
    except BaseException as ex:
        rt.error = f"the animation failed: {ex}"
        return 1
