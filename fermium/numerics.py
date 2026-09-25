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
