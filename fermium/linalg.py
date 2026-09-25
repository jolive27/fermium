"""Small dense linear algebra for matrices of up to 4×4 (D29).

Every routine works on flat row-major lists of scalars through an `ops` object, so the LLVM
backend (ops build instructions) and the reference interpreter (ops on Python floats) run the
very same sequence of operations and agree to the last bit.  Sizes are known at compile time,
so the loops below unroll into straight-line code.

ops must provide: add, sub, mul, div, neg, gt_abs(a, b) -> "|a| > |b|", select(cond, a, b).
"""
import math


def matmul(ops, a, r, k, b, c):
    """(r×k) · (k×c) -> r×c."""
    out = []
    for i in range(r):
        for j in range(c):
            acc = ops.mul(a[i * k], b[j])
            for m in range(1, k):
                acc = ops.add(acc, ops.mul(a[i * k + m], b[m * c + j]))
            out.append(acc)
    return out


def transpose_index(r, c):
    """Flat indices that turn an r×c matrix into its c×r transpose."""
    return [i * c + j for j in range(c) for i in range(r)]


def det(ops, a, n):
    """Determinant by cofactor expansion along the first row (exact for whole numbers; n <= 4)."""
    if n == 1:
        return a[0]
    if n == 2:
        return ops.sub(ops.mul(a[0], a[3]), ops.mul(a[1], a[2]))
    acc = None
    for j in range(n):
        minor = [a[i * n + m] for i in range(1, n) for m in range(n) if m != j]
        term = ops.mul(a[j], det(ops, minor, n - 1))
        if acc is None:
            acc = term
        elif j % 2:
            acc = ops.sub(acc, term)
        else:
            acc = ops.add(acc, term)
    return acc


def solve(ops, a, n, b, m):
    """Solve A X = B (A n×n, B n×m) by Gaussian elimination with partial pivoting.

    Returns (X as a flat n×m list, pivots).  A zero pivot means A is singular; the caller checks.
    Row swaps are branch-free (select), so the same code works as straight-line LLVM IR.
    """
    rows = [[a[i * n + j] for j in range(n)] + [b[i * m + j] for j in range(m)] for i in range(n)]
    w = n + m
    pivots = []
    for k in range(n):
        for i in range(k + 1, n):          # bring the largest |a_ik| (i >= k) up to row k
            swap = ops.gt_abs(rows[i][k], rows[k][k])
            for j in range(k, w):
                x, y = rows[k][j], rows[i][j]
                rows[k][j] = ops.select(swap, y, x)
                rows[i][j] = ops.select(swap, x, y)
        p = rows[k][k]
        pivots.append(p)
        for i in range(k + 1, n):
            f = ops.div(rows[i][k], p)
            for j in range(k + 1, w):
                rows[i][j] = ops.sub(rows[i][j], ops.mul(f, rows[k][j]))
    x = [[None] * m for _ in range(n)]
    for i in range(n - 1, -1, -1):
        for j in range(m):
            acc = rows[i][n + j]
            for q in range(i + 1, n):
                acc = ops.sub(acc, ops.mul(rows[i][q], x[q][j]))
            x[i][j] = ops.div(acc, rows[i][i])
    return [x[i][j] for i in range(n) for j in range(m)], pivots


def inverse(ops, a, n, one, zero):
    ident = [one if i == j else zero for i in range(n) for j in range(n)]
    return solve(ops, a, n, ident, n)


class FloatOps:
    """ops on Python floats, with IEEE division (x/0 -> ±inf or NaN) like the compiled code."""

    @staticmethod
    def add(x, y):
        return x + y

    @staticmethod
    def sub(x, y):
        return x - y

    @staticmethod
    def mul(x, y):
        return x * y

    @staticmethod
    def div(x, y):
        try:
            return x / y
        except ZeroDivisionError:
            if x == 0 or x != x:
                return float("nan")
            return math.copysign(math.inf, x) * math.copysign(1.0, y)

    @staticmethod
    def neg(x):
        return -x

    @staticmethod
    def gt_abs(x, y):
        return abs(x) > abs(y)

    @staticmethod
    def select(c, x, y):
        return x if c else y
