"""Small numeric helpers shared by the LLVM backend and the reference interpreter (fermium/interp.py);
they work on LLVM values or Python floats through an `ops` object."""
import math


class PyOps:
    @staticmethod
    def sub(x, y):
        return x - y

    @staticmethod
    def div(x, y):
        return x / y if y else math.nan


def quintic_hermite(ops, t, y, d):
    """Newton coefficients of the quintic with values y and slopes d at t0 < t1 < t2 (nodes
    t0, t0, t1, t1, t2, t2).  Shared by fm_sol_ext and the reference interpreter."""
    z = [t[0], t[0], t[1], t[1], t[2], t[2]]
    col = [d[0], ops.div(ops.sub(y[1], y[0]), ops.sub(t[1], t[0])), d[1],
           ops.div(ops.sub(y[2], y[1]), ops.sub(t[2], t[1])), d[2]]
    coef = [y[0], col[0]]
    for order in range(2, 6):
        col = [ops.div(ops.sub(col[i + 1], col[i]), ops.sub(z[i + order], z[i])) for i in range(len(col) - 1)]
        coef.append(col[0])
    return coef


def odd_root_numerator(p):
    """For an exponent p = n/q with q odd and > 1 (1/3, 2/3, -2/3, 1/5, ...) return n, else None.

    Such powers have a real value for negative bases: (-8)^(2/3) = 4, (-8)^(1/3) = -2."""
    if not math.isfinite(p) or p == int(p):
        return None
    from fractions import Fraction
    f = Fraction(p).limit_denominator(99)
    if f.denominator % 2 == 0 or abs(float(f) - p) > 1e-12 * max(1.0, abs(p)):
        return None
    return f.numerator


# Gauss–Kronrod 7-15 nodes/weights (from QUADPACK qk15)
XGK = [0.991455371120812639206854697526329, 0.949107912342758524526189684047851,
       0.864864423359769072789712788640926, 0.741531185599394439863864773280788,
       0.586087235467691130294144845693013, 0.405845151377397166906606412076961,
       0.207784955007898467600689403773245, 0.000000000000000000000000000000000]
WGK = [0.022935322010529224963732008058970, 0.063092092629978553290700663189204,
       0.104790010322250183839876322541518, 0.140653259715525918745189590510238,
       0.169004726639267902826583426598550, 0.190350578064785409913256402421014,
       0.204432940075298892414161999234649, 0.209482141084727828012999174891714]
WG = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780,
      0.381830050505118944950369775488975, 0.417959183673469387755102040816327]
