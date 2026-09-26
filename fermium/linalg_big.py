"""Linear algebra for matrices larger than 4×4, up to 16×16 (DECISIONS D195).

The same algorithms as fermium/linalg.py, written with loops over arrays (ops.loop, ops.array, ops.ld/st
and whole-number index ops) instead of Python loops that unroll: the code generator then emits a few
LLVM loops instead of O(n³) straight-line instructions (O(n³ · sweeps) for Jacobi), while the reference
interpreter runs the very same floating-point operations in the same order with FloatOps, so the two back
ends still agree bit for bit.  Up to 4×4 the unrolled routines of fermium/linalg.py are kept (det by
cofactors is exact for whole numbers).

ops needs, besides fermium.linalg's scalar ops: array(n), iarray(n), ld(arr, i), st(arr, i, v),
loop(lo, hi, body) (body(i) for i from lo to hi - 1; lo and hi may be index values), iadd, isub, imul,
ieq, iselect, and_, not_.
"""
from .linalg import SYMMETRY_TOL

MAX_DIM = 16             # the largest matrix is 16×16, the longest vector 16 components
SWEEPS = 16              # cyclic Jacobi converges quadratically; a 16×16 matrix needs ~8–10 sweeps


def is_big(*sizes):
    return max(sizes) > 4


def _to_array(ops, vals):
    arr = ops.array(len(vals))
    for k, v in enumerate(vals):
        ops.st(arr, k, v)
    return arr


def _from_array(ops, arr, n):
    return [ops.ld(arr, k) for k in range(n)]


def _ix(ops, i, n, j):
    """The flat index i·n + j."""
    return ops.iadd(ops.imul(i, n), j)


def matmul(ops, a, r, k, b, c):
    """(r×k) · (k×c), with the operation order of linalg.matmul."""
    A, B = _to_array(ops, a), _to_array(ops, b)
    out = ops.array(r * c)
    acc = ops.array(1)

    def row(i):
        def col(j):
            ops.st(acc, 0, ops.mul(ops.ld(A, ops.imul(i, k)), ops.ld(B, j)))

            def term(m):
                ops.st(acc, 0, ops.add(ops.ld(acc, 0), ops.mul(ops.ld(A, _ix(ops, i, k, m)),
                                                              ops.ld(B, _ix(ops, m, c, j)))))
            ops.loop(1, k, term)
            ops.st(out, _ix(ops, i, c, j), ops.ld(acc, 0))
        ops.loop(0, c, col)
    ops.loop(0, r, row)
    return _from_array(ops, out, r * c)


def solve(ops, a, n, b, m):
    """A X = B (A n×n, B n×m) by Gaussian elimination with partial pivoting (the row with the largest
    |a_ik|, the first one on a tie, swapped once).  Returns (X flat n×m, [the smallest |pivot|] for the
    caller's zero check, det A)."""
    w = n + m
    W = ops.array(n * w)
    for i in range(n):
        for j in range(n):
            ops.st(W, i * w + j, a[i * n + j])
        for j in range(m):
            ops.st(W, i * w + n + j, b[i * m + j])
    best = ops.iarray(1)
    sign = ops.array(1)
    ops.st(sign, 0, ops.const(1.0))
    piv = ops.array(n)

    def column(k):
        ops.st(best, 0, k)

        def scan(i):
            bi = ops.ld(best, 0)
            c = ops.gt_abs(ops.ld(W, _ix(ops, i, w, k)), ops.ld(W, _ix(ops, bi, w, k)))
            ops.st(best, 0, ops.iselect(c, i, bi))
        ops.loop(ops.iadd(k, 1), n, scan)
        p = ops.ld(best, 0)

        def swap(j):
            x, y = ops.ld(W, _ix(ops, k, w, j)), ops.ld(W, _ix(ops, p, w, j))
            ops.st(W, _ix(ops, k, w, j), y)
            ops.st(W, _ix(ops, p, w, j), x)
        ops.loop(k, w, swap)
        s = ops.ld(sign, 0)
        ops.st(sign, 0, ops.select(ops.ieq(p, k), s, ops.neg(s)))
        pk = ops.ld(W, _ix(ops, k, w, k))
        ops.st(piv, k, pk)

        def eliminate(i):
            f = ops.div(ops.ld(W, _ix(ops, i, w, k)), pk)

            def upd(j):
                ij = _ix(ops, i, w, j)
                ops.st(W, ij, ops.sub(ops.ld(W, ij), ops.mul(f, ops.ld(W, _ix(ops, k, w, j)))))
            ops.loop(ops.iadd(k, 1), w, upd)
        ops.loop(ops.iadd(k, 1), n, eliminate)
    ops.loop(0, n, column)

    X = ops.array(n * m)
    acc = ops.array(1)

    def back(t):
        i = ops.isub(n - 1, t)

        def rhs(j):
            ops.st(acc, 0, ops.ld(W, _ix(ops, i, w, ops.iadd(n, j))))

            def term(q):
                ops.st(acc, 0, ops.sub(ops.ld(acc, 0), ops.mul(ops.ld(W, _ix(ops, i, w, q)),
                                                              ops.ld(X, _ix(ops, q, m, j)))))
            ops.loop(ops.iadd(i, 1), n, term)
            ops.st(X, _ix(ops, i, m, j), ops.div(ops.ld(acc, 0), ops.ld(W, _ix(ops, i, w, i))))
        ops.loop(0, m, rhs)
    ops.loop(0, n, back)

    # the determinant (the swaps' sign times the pivots) and the smallest |pivot|
    ops.st(acc, 0, ops.ld(sign, 0))
    small = ops.array(1)
    ops.st(small, 0, ops.abs(ops.ld(piv, 0)))

    def prod(k):
        pk = ops.ld(piv, k)
        ops.st(acc, 0, ops.mul(ops.ld(acc, 0), pk))
        sm = ops.ld(small, 0)
        ops.st(small, 0, ops.select(ops.lt(ops.abs(pk), sm), ops.abs(pk), sm))
    ops.loop(0, n, prod)
    return _from_array(ops, X, n * m), [ops.ld(small, 0)], ops.ld(acc, 0)


def det(ops, a, n):
    return solve(ops, a, n, [ops.const(0.0)] * n, 1)[2]


def inverse(ops, a, n, one, zero):
    x, piv, _ = solve(ops, a, n, [one if i == j else zero for i in range(n) for j in range(n)], n)
    return x, piv


def asymmetry(ops, a, n):
    """[tol − max |a_ij − a_ji|]: negative exactly when a isn't symmetric (as linalg.asymmetry)."""
    A = _to_array(ops, a)
    scale = ops.array(1)
    ops.st(scale, 0, ops.abs(ops.ld(A, 0)))

    def sc(k):
        v, s = ops.ld(A, k), ops.ld(scale, 0)
        ops.st(scale, 0, ops.select(ops.gt_abs(v, s), ops.abs(v), s))
    ops.loop(1, n * n, sc)
    worst = ops.array(1)
    ops.st(worst, 0, ops.const(0.0))

    def row(i):
        def col(j):
            d = ops.abs(ops.sub(ops.ld(A, _ix(ops, i, n, j)), ops.ld(A, _ix(ops, j, n, i))))
            wv = ops.ld(worst, 0)
            ops.st(worst, 0, ops.select(ops.lt(wv, d), d, wv))
        ops.loop(ops.iadd(i, 1), n, col)
    ops.loop(0, n, row)
    return [ops.sub(ops.mul(ops.ld(scale, 0), ops.const(SYMMETRY_TOL)), ops.ld(worst, 0))]


def _jacobi(ops, A, n):
    """Cyclic Jacobi rotations on the symmetric array A (overwritten: its diagonal ends up holding the
    eigenvalues); returns V, whose columns are the eigenvectors.  The rotation is linalg.jacobi_eigen's."""
    zero, one = ops.const(0.0), ops.const(1.0)
    V = ops.array(n * n)
    for i in range(n):
        for j in range(n):
            ops.st(V, i * n + j, one if i == j else zero)

    def rotate(p, q):
        pq, qp, pp, qq = _ix(ops, p, n, q), _ix(ops, q, n, p), _ix(ops, p, n, p), _ix(ops, q, n, q)
        apq, app, aqq = ops.ld(A, pq), ops.ld(A, pp), ops.ld(A, qq)
        theta = ops.div(ops.sub(aqq, app), ops.add(apq, apq))
        sgn = ops.select(ops.lt(theta, zero), ops.neg(one), one)
        t = ops.div(sgn, ops.add(ops.abs(theta), ops.sqrt(ops.add(ops.mul(theta, theta), one))))
        t = ops.select(ops.eq(apq, zero), zero, t)          # already diagonal here: no rotation
        c = ops.div(one, ops.sqrt(ops.add(ops.mul(t, t), one)))
        s = ops.mul(t, c)
        ops.st(A, pp, ops.sub(app, ops.mul(t, apq)))
        ops.st(A, qq, ops.add(aqq, ops.mul(t, apq)))
        ops.st(A, pq, zero)
        ops.st(A, qp, zero)

        def each(r):
            other = ops.and_(ops.not_(ops.ieq(r, p)), ops.not_(ops.ieq(r, q)))
            rp, rq = _ix(ops, r, n, p), _ix(ops, r, n, q)
            arp, arq = ops.ld(A, rp), ops.ld(A, rq)
            nrp = ops.select(other, ops.sub(ops.mul(c, arp), ops.mul(s, arq)), arp)
            nrq = ops.select(other, ops.add(ops.mul(s, arp), ops.mul(c, arq)), arq)
            ops.st(A, rp, nrp)
            ops.st(A, _ix(ops, p, n, r), nrp)
            ops.st(A, rq, nrq)
            ops.st(A, _ix(ops, q, n, r), nrq)
            vrp, vrq = ops.ld(V, rp), ops.ld(V, rq)
            ops.st(V, rp, ops.sub(ops.mul(c, vrp), ops.mul(s, vrq)))
            ops.st(V, rq, ops.add(ops.mul(s, vrp), ops.mul(c, vrq)))
        ops.loop(0, n, each)

    def sweep(_):
        ops.loop(0, n, lambda p: ops.loop(ops.iadd(p, 1), n, lambda q: rotate(p, q)))
    ops.loop(0, SWEEPS, sweep)
    return V


def _sort_and_fix_signs(ops, vals, V, n):
    """Ascending eigenvalues (a bubble-sort network that swaps V's columns along), then each column's
    largest-magnitude entry made positive (as linalg._sort_and_fix_signs)."""
    zero = ops.const(0.0)

    def outer(i):
        def inner(j):
            j1 = ops.iadd(j, 1)
            a, b = ops.ld(vals, j), ops.ld(vals, j1)
            swap = ops.lt(b, a)
            ops.st(vals, j, ops.select(swap, b, a))
            ops.st(vals, j1, ops.select(swap, a, b))

            def col(r):
                x, y = ops.ld(V, _ix(ops, r, n, j)), ops.ld(V, _ix(ops, r, n, j1))
                ops.st(V, _ix(ops, r, n, j), ops.select(swap, y, x))
                ops.st(V, _ix(ops, r, n, j1), ops.select(swap, x, y))
            ops.loop(0, n, col)
        ops.loop(0, ops.isub(n - 1, i), inner)
    ops.loop(0, n, outer)
    big = ops.array(1)

    def fix(j):
        ops.st(big, 0, ops.ld(V, j))

        def scan(r):
            v, bv = ops.ld(V, _ix(ops, r, n, j)), ops.ld(big, 0)
            ops.st(big, 0, ops.select(ops.gt_abs(v, bv), v, bv))
        ops.loop(1, n, scan)
        flip = ops.lt(ops.ld(big, 0), zero)

        def neg(r):
            k = _ix(ops, r, n, j)
            v = ops.ld(V, k)
            ops.st(V, k, ops.select(flip, ops.neg(v), v))
        ops.loop(0, n, neg)
    ops.loop(0, n, fix)
    return _from_array(ops, vals, n), _from_array(ops, V, n * n)


def _diagonal(ops, A, n):
    vals = ops.array(n)
    ops.loop(0, n, lambda i: ops.st(vals, i, ops.ld(A, _ix(ops, i, n, i))))
    return vals


def jacobi_eigen(ops, a, n):
    """Eigenvalues (ascending) and unit eigenvectors (columns) of the symmetrised a, as linalg.jacobi_eigen."""
    half = ops.const(0.5)
    A = ops.array(n * n)
    for i in range(n):
        for j in range(n):
            ops.st(A, i * n + j, a[i * n + i] if i == j else ops.mul(half, ops.add(a[i * n + j], a[j * n + i])))
    V = _jacobi(ops, A, n)
    return _sort_and_fix_signs(ops, _diagonal(ops, A, n), V, n)


def generalized_eigen(ops, k, m, n):
    """K v = λ M v (as linalg.generalized_eigen): Cholesky M = L Lᵀ, Jacobi on A = L⁻¹ K L⁻ᵀ, v = L⁻ᵀ y
    scaled to unit length.  Returns (values, vectors, [the smallest Cholesky pivot, > 0 when M is positive
    definite])."""
    zero, half = ops.const(0.0), ops.const(0.5)
    M, K = _to_array(ops, m), _to_array(ops, k)
    L = ops.array(n * n)
    for i in range(n * n):
        ops.st(L, i, zero)
    acc = ops.array(1)
    small = ops.array(1)
    ops.st(small, 0, ops.ld(M, 0))

    def chol(j):
        jj = _ix(ops, j, n, j)
        ops.st(acc, 0, ops.ld(M, jj))

        def sub_sq(q):
            ljq = ops.ld(L, _ix(ops, j, n, q))
            ops.st(acc, 0, ops.sub(ops.ld(acc, 0), ops.mul(ljq, ljq)))
        ops.loop(0, j, sub_sq)
        d = ops.ld(acc, 0)
        sm = ops.ld(small, 0)
        ops.st(small, 0, ops.select(ops.lt(d, sm), d, sm))
        ops.st(L, jj, ops.sqrt(d))

        def below(i):
            ops.st(acc, 0, ops.mul(half, ops.add(ops.ld(M, _ix(ops, i, n, j)), ops.ld(M, _ix(ops, j, n, i)))))

            def sub_prod(q):
                ops.st(acc, 0, ops.sub(ops.ld(acc, 0), ops.mul(ops.ld(L, _ix(ops, i, n, q)),
                                                              ops.ld(L, _ix(ops, j, n, q)))))
            ops.loop(0, j, sub_prod)
            ops.st(L, _ix(ops, i, n, j), ops.div(ops.ld(acc, 0), ops.ld(L, jj)))
        ops.loop(ops.iadd(j, 1), n, below)
    ops.loop(0, n, chol)

    def forward(src, dst, transposed):
        """Column j of dst = L⁻¹ (column j of src, or row j when transposed)."""
        def col(j):
            def row(i):
                ops.st(acc, 0, ops.ld(src, _ix(ops, j, n, i) if transposed else _ix(ops, i, n, j)))

                def term(q):
                    ops.st(acc, 0, ops.sub(ops.ld(acc, 0), ops.mul(ops.ld(L, _ix(ops, i, n, q)),
                                                                  ops.ld(dst, _ix(ops, q, n, j)))))
                ops.loop(0, i, term)
                ops.st(dst, _ix(ops, i, n, j), ops.div(ops.ld(acc, 0), ops.ld(L, _ix(ops, i, n, i))))
            ops.loop(0, n, row)
        ops.loop(0, n, col)
    Y = ops.array(n * n)
    forward(K, Y, False)            # Y = L⁻¹ K
    A = ops.array(n * n)
    forward(Y, A, True)             # A = L⁻¹ Yᵀ = L⁻¹ K L⁻ᵀ (K symmetric)
    V = _jacobi(ops, A, n)
    vals = _diagonal(ops, A, n)
    X = ops.array(n * n)

    def back(j):
        def row(t):
            i = ops.isub(n - 1, t)
            ops.st(acc, 0, ops.ld(V, _ix(ops, i, n, j)))

            def term(q):
                ops.st(acc, 0, ops.sub(ops.ld(acc, 0), ops.mul(ops.ld(L, _ix(ops, q, n, i)),
                                                              ops.ld(X, _ix(ops, q, n, j)))))
            ops.loop(ops.iadd(i, 1), n, term)
            ops.st(X, _ix(ops, i, n, j), ops.div(ops.ld(acc, 0), ops.ld(L, _ix(ops, i, n, i))))
        ops.loop(0, n, row)
        x0 = ops.ld(X, j)
        ops.st(acc, 0, ops.mul(x0, x0))

        def sq(r):
            x = ops.ld(X, _ix(ops, r, n, j))
            ops.st(acc, 0, ops.add(ops.ld(acc, 0), ops.mul(x, x)))
        ops.loop(1, n, sq)
        norm = ops.sqrt(ops.ld(acc, 0))

        def scale(r):
            kk = _ix(ops, r, n, j)
            ops.st(X, kk, ops.div(ops.ld(X, kk), norm))
        ops.loop(0, n, scale)
    ops.loop(0, n, back)
    vals_out, vecs = _sort_and_fix_signs(ops, vals, X, n)
    return vals_out, vecs, [ops.ld(small, 0)]
