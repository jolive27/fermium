"""Small dense linear algebra for matrices of up to 4×4 (D29).

Every routine works on flat row-major lists of scalars through an `ops` object, so the LLVM
backend (ops build instructions) and the reference interpreter (ops on Python floats) run the
very same sequence of operations and agree to the last bit.  Sizes are known at compile time,
so the loops below unroll into straight-line code.

ops must provide: add, sub, mul, div, neg, gt_abs(a, b) -> "|a| > |b|", select(cond, a, b);
the eigenvalue routines also use sqrt, abs, lt(a, b) -> "a < b" and eq(a, b) -> "a == b".
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


JACOBI_SWEEPS = 10      # cyclic sweeps; n <= 4 converges to machine precision in about 5
SYMMETRY_TOL = 1e-10    # |a_ij - a_ji| may be at most this times the largest |entry|


def asymmetry(ops, a, n):
    """Values that are negative exactly when a is not symmetric (within SYMMETRY_TOL): the caller
    reports an error if any of them is < 0."""
    scale = ops.abs(a[0])
    for k in range(1, n * n):
        scale = ops.select(ops.gt_abs(a[k], scale), ops.abs(a[k]), scale)
    tol = ops.mul(scale, ops.const(SYMMETRY_TOL))
    return [ops.sub(tol, ops.abs(ops.sub(a[i * n + j], a[j * n + i])))
            for i in range(n) for j in range(i + 1, n)]


def jacobi_eigen(ops, a, n):
    """Eigenvalues and eigenvectors of a symmetric n×n matrix by cyclic Jacobi rotations (D38).

    Returns (values sorted ascending, vectors as a flat row-major n×n list whose column j is the unit
    eigenvector of values[j], with its largest-magnitude entry positive).  The matrix is symmetrised
    first ((a + aᵀ)/2).  A fixed number of sweeps and branch-free rotations (select) make this
    straight-line code for LLVM, exactly as `solve` is.
    """
    zero, one, half = ops.const(0.0), ops.const(1.0), ops.const(0.5)
    A = [[ops.mul(half, ops.add(a[i * n + j], a[j * n + i])) if i != j else a[i * n + i] for j in range(n)]
         for i in range(n)]
    V = [[one if i == j else zero for j in range(n)] for i in range(n)]
    for _ in range(JACOBI_SWEEPS):
        for p in range(n):
            for q in range(p + 1, n):
                apq, app, aqq = A[p][q], A[p][p], A[q][q]
                theta = ops.div(ops.sub(aqq, app), ops.add(apq, apq))
                sgn = ops.select(ops.lt(theta, zero), ops.neg(one), one)
                t = ops.div(sgn, ops.add(ops.abs(theta), ops.sqrt(ops.add(ops.mul(theta, theta), one))))
                t = ops.select(ops.eq(apq, zero), zero, t)          # already diagonal here: no rotation
                c = ops.div(one, ops.sqrt(ops.add(ops.mul(t, t), one)))
                s = ops.mul(t, c)
                A[p][p] = ops.sub(app, ops.mul(t, apq))
                A[q][q] = ops.add(aqq, ops.mul(t, apq))
                A[p][q] = A[q][p] = zero
                for r in range(n):
                    if r != p and r != q:
                        arp, arq = A[r][p], A[r][q]
                        A[r][p] = A[p][r] = ops.sub(ops.mul(c, arp), ops.mul(s, arq))
                        A[r][q] = A[q][r] = ops.add(ops.mul(s, arp), ops.mul(c, arq))
                    vrp, vrq = V[r][p], V[r][q]
                    V[r][p] = ops.sub(ops.mul(c, vrp), ops.mul(s, vrq))
                    V[r][q] = ops.add(ops.mul(s, vrp), ops.mul(c, vrq))
    vals = [A[i][i] for i in range(n)]
    cols = [[V[r][j] for r in range(n)] for j in range(n)]
    return _sort_and_fix_signs(ops, vals, cols, n)


def _sort_and_fix_signs(ops, vals, cols, n):
    zero = ops.const(0.0)
    for i in range(n):                          # bubble-sort network: ascending eigenvalues
        for j in range(n - 1 - i):
            swap = ops.lt(vals[j + 1], vals[j])
            vals[j], vals[j + 1] = ops.select(swap, vals[j + 1], vals[j]), ops.select(swap, vals[j], vals[j + 1])
            cj, ck = cols[j], cols[j + 1]
            cols[j] = [ops.select(swap, y, x) for x, y in zip(cj, ck)]
            cols[j + 1] = [ops.select(swap, x, y) for x, y in zip(cj, ck)]
    for j in range(n):                          # the largest-magnitude entry of each vector is positive
        big = cols[j][0]
        for r in range(1, n):
            big = ops.select(ops.gt_abs(cols[j][r], big), cols[j][r], big)
        flip = ops.lt(big, zero)
        cols[j] = [ops.select(flip, ops.neg(x), x) for x in cols[j]]
    return vals, [cols[j][r] for r in range(n) for j in range(n)]


def cholesky_pivots_and_factor(ops, m, n):
    """M = L Lᵀ.  Returns (L as rows, the pivots d_k before their square roots); M is positive
    definite exactly when every pivot is > 0, which the caller checks."""
    L = [[ops.const(0.0)] * n for _ in range(n)]
    piv = []
    for j in range(n):
        d = m[j * n + j]
        for k in range(j):
            d = ops.sub(d, ops.mul(L[j][k], L[j][k]))
        piv.append(d)
        L[j][j] = ops.sqrt(d)
        for i in range(j + 1, n):
            x = ops.mul(ops.const(0.5), ops.add(m[i * n + j], m[j * n + i]))
            for k in range(j):
                x = ops.sub(x, ops.mul(L[i][k], L[j][k]))
            L[i][j] = ops.div(x, L[j][j])
    return L, piv


def _forward(ops, L, b, n):
    """L y = b for lower-triangular L."""
    y = []
    for i in range(n):
        acc = b[i]
        for k in range(i):
            acc = ops.sub(acc, ops.mul(L[i][k], y[k]))
        y.append(ops.div(acc, L[i][i]))
    return y


def _backward_t(ops, L, b, n):
    """Lᵀ x = b for lower-triangular L."""
    x = [None] * n
    for i in range(n - 1, -1, -1):
        acc = b[i]
        for k in range(i + 1, n):
            acc = ops.sub(acc, ops.mul(L[k][i], x[k]))
        x[i] = ops.div(acc, L[i][i])
    return x


def generalized_eigen(ops, k, m, n):
    """K v = λ M v for symmetric K and symmetric positive-definite M (normal modes: λ = ω²).

    Reduces to the symmetric problem A y = λ y with A = L⁻¹ K L⁻ᵀ (M = L Lᵀ), then v = L⁻ᵀ y,
    scaled to unit length.  Returns (values ascending, vectors as columns, Cholesky pivots)."""
    L, piv = cholesky_pivots_and_factor(ops, m, n)
    cols = [_forward(ops, L, [k[i * n + j] for i in range(n)], n) for j in range(n)]     # Y = L⁻¹ K
    # A = L⁻¹ Yᵀ (= L⁻¹ K L⁻ᵀ since K is symmetric); row i of Yᵀ is column i of Y
    acols = [_forward(ops, L, [cols[i][j] for i in range(n)], n) for j in range(n)]
    a = [acols[j][i] for i in range(n) for j in range(n)]
    vals, y = jacobi_eigen(ops, a, n)
    out = []
    for j in range(n):
        v = _backward_t(ops, L, [y[i * n + j] for i in range(n)], n)
        s = ops.mul(v[0], v[0])
        for x in v[1:]:
            s = ops.add(s, ops.mul(x, x))
        norm = ops.sqrt(s)
        out.append([ops.div(x, norm) for x in v])
    vals, vecs = _sort_and_fix_signs(ops, vals, out, n)
    return vals, vecs, piv


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

    @staticmethod
    def const(v):
        return float(v)

    @staticmethod
    def sqrt(x):
        return math.sqrt(x) if x >= 0 else (x if x != x else math.nan)

    @staticmethod
    def abs(x):
        return abs(x)

    @staticmethod
    def lt(x, y):
        return x < y

    @staticmethod
    def eq(x, y):
        return x == y

    # ---- loops, arrays and whole-number indexes, for matrices larger than 4×4 (linalg_big, D195)
    @staticmethod
    def array(n):
        return [0.0] * n

    @staticmethod
    def iarray(n):
        return [0] * n

    @staticmethod
    def ld(arr, i):
        return arr[i]

    @staticmethod
    def st(arr, i, v):
        arr[i] = v

    @staticmethod
    def loop(lo, hi, body):
        for i in range(lo, hi):
            body(i)

    @staticmethod
    def iadd(a, b):
        return a + b

    @staticmethod
    def isub(a, b):
        return a - b

    @staticmethod
    def imul(a, b):
        return a * b

    @staticmethod
    def ieq(a, b):
        return a == b

    @staticmethod
    def iselect(c, a, b):
        return a if c else b

    @staticmethod
    def and_(a, b):
        return a and b

    @staticmethod
    def not_(a):
        return not a
