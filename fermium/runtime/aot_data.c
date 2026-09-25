/* `load`, `fit` and `plot` for standalone executables (included by aot_rt.c).
 *
 * These are C versions of Runtime.load / Runtime.fit / Runtime.make_plot in runtime/core.py
 * and of runtime/fitting.py.  Column units, fit parameters' display units and plot labels are
 * worked out at build time and generated into fm_tables.c (see fermium/aot.py).
 *
 * - load reads the CSV when the program runs, relative to the current working directory.
 * - fit is Levenberg-Marquardt with a numeric Jacobian, started like fitting.py does.
 * - plot writes an SVG file (a .png name is changed to .svg).
 */

/* ------------------------------------------------------------------ tables (fm_tables.c) */
typedef struct { const char *header; double factor, offset; } fm_colinfo;
typedef struct { const char *path; int ncols; const fm_colinfo *cols; } fm_loadinfo;

typedef void (*fm_model_fn)(double *, double **, int64_t, double *);
typedef struct { const char *name; double factor; const char *unit; } fm_paraminfo;
typedef struct {
    const char *text, *path;
    int nparams; const fm_paraminfo *params;
    int ncols; const int *cols;
    double yfactor; const char *yunit;
    fm_model_fn model;
} fm_fitinfo;

typedef struct {
    const char *legend, *ylabel, *xlabel;      /* XML-escaped */
    double yfactor, yoffset, xfactor, xoffset;
    int points;
} fm_seriesinfo;
typedef struct {
    const char *svg, *shown;       /* file to write; name to print */
    int renamed;                   /* asked for another format: say it was written as SVG */
    const char *title;             /* XML-escaped, "" = none */
    int logx, logy, equal, nseries;
    const fm_seriesinfo *series;
    int hasx, hasy;                /* with x from a to b / y from a to b (D161), in display units */
    double xlo, xhi, ylo, yhi;
    int revx, revy;                /* with reversed x / reversed y */
} fm_plotinfo;

extern const fm_loadinfo fm_loads[];
extern const fm_fitinfo fm_fits[];
extern const fm_plotinfo fm_plots[];

/* ------------------------------------------------------------------ helpers */
static void *xmalloc(size_t n) {
    void *p = malloc(n ? n : 1);
    if (!p) { fputs("out of memory\n", stderr); exit(1); }
    return p;
}

/* Python's repr() of a string, near enough for error messages */
static void py_repr(const char *s, char *out, size_t cap) {
    char q = (strchr(s, '\'') && !strchr(s, '"')) ? '"' : '\'';
    size_t k = 0;
    if (cap < 8) { if (cap) out[0] = 0; return; }
    out[k++] = q;
    for (; *s && k + 6 < cap; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '\\' || c == (unsigned char)q) { out[k++] = '\\'; out[k++] = (char)c; }
        else if (c == '\n') { out[k++] = '\\'; out[k++] = 'n'; }
        else if (c == '\r') { out[k++] = '\\'; out[k++] = 'r'; }
        else if (c == '\t') { out[k++] = '\\'; out[k++] = 't'; }
        else if (c < 32 || c == 127) k += (size_t)snprintf(out + k, cap - k, "\\x%02x", c);
        else out[k++] = (char)c;
    }
    out[k++] = q;
    out[k] = 0;
}

/* ------------------------------------------------------------------ CSV (excel dialect, like Python's csv) */
typedef struct { char **cells; int n, cap; } csv_row;

static void row_push(csv_row *r, const char *s, size_t len) {
    if (r->n == r->cap) {
        r->cap = r->cap ? 2 * r->cap : 8;
        char **nc = xmalloc(sizeof(char *) * (size_t)r->cap);
        if (r->n) memcpy(nc, r->cells, sizeof(char *) * (size_t)r->n);
        free(r->cells);
        r->cells = nc;
    }
    char *c = xmalloc(len + 1);
    memcpy(c, s, len);
    c[len] = 0;
    r->cells[r->n++] = c;
}

static void row_free(csv_row *r) {
    for (int i = 0; i < r->n; i++) free(r->cells[i]);
    free(r->cells);
    r->cells = NULL;
    r->n = r->cap = 0;
}

/* Read one record starting at *pos; returns 0 at the end of the text. */
static int csv_next(const char *txt, size_t len, size_t *pos, csv_row *row) {
    size_t i = *pos;
    if (i >= len) return 0;
    row_free(row);
    char *buf = xmalloc(len - i + 1);
    size_t b = 0;
    int quoted = 0, any = 0, field_start = 1;
    while (i < len) {
        char c = txt[i];
        if (quoted) {
            if (c == '"') {
                if (i + 1 < len && txt[i + 1] == '"') { buf[b++] = '"'; i += 2; continue; }
                quoted = 0; i++; continue;
            }
            buf[b++] = c; i++; continue;
        }
        if (c == '"' && field_start) { quoted = 1; field_start = 0; any = 1; i++; continue; }
        if (c == ',') { row_push(row, buf, b); b = 0; field_start = 1; any = 1; i++; continue; }
        if (c == '\r' || c == '\n') {
            if (c == '\r' && i + 1 < len && txt[i + 1] == '\n') i++;
            i++;
            break;
        }
        buf[b++] = c; field_start = 0; any = 1; i++;
    }
    if (any || b) row_push(row, buf, b);          /* a blank line is an empty record, like csv.reader */
    free(buf);
    *pos = i;
    return 1;
}

static int is_space(unsigned char c) { return c == ' ' || (c >= 9 && c <= 13); }

static void strip_into(const char *s, char *out, size_t cap) {
    while (*s && is_space((unsigned char)*s)) s++;
    size_t n = strlen(s);
    while (n && is_space((unsigned char)s[n - 1])) n--;
    if (n >= cap) n = cap - 1;
    memcpy(out, s, n);
    out[n] = 0;
}

/* float() as Python does it: surrounding spaces, underscores between digits, inf/nan; no hex */
static int py_float(const char *s, double *out) {
    char t[256], u[256];
    strip_into(s, t, sizeof t);
    if (!t[0] || strlen(t) >= sizeof t - 1) return 0;
    size_t k = 0;
    for (size_t i = 0; t[i]; i++) {
        char c = t[i];
        if (c == '_') {
            if (i == 0 || !t[i + 1] || !(t[i - 1] >= '0' && t[i - 1] <= '9') || !(t[i + 1] >= '0' && t[i + 1] <= '9')) return 0;
            continue;
        }
        if (c == 'x' || c == 'X' || c == 'p' || c == 'P' || c == '(') return 0;
        u[k++] = c;
    }
    u[k] = 0;
    char *end;
    errno = 0;
    double v = strtod(u, &end);
    if (end == u || *end) return 0;
    *out = v;
    return 1;
}

/* ------------------------------------------------------------------ load */
typedef struct { int ncols; int64_t n; double **cols; } fm_dataset;
static fm_dataset *datasets = NULL;
static int ndatasets = 0;

static char *read_file(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    size_t cap = 1 << 16, n = 0;
    char *buf = xmalloc(cap);
    size_t got;
    while ((got = fread(buf + n, 1, cap - n, f)) > 0) {
        n += got;
        if (n == cap) {
            char *nb = xmalloc(cap * 2);
            memcpy(nb, buf, n);
            free(buf);
            buf = nb;
            cap *= 2;
        }
    }
    fclose(f);
    *len = n;
    return buf;
}

static const char *base_name(const char *p) {
    const char *s = strrchr(p, '/');
    return s ? s + 1 : p;
}

int64_t fm_load(int64_t id) {
    const fm_loadinfo *L = &fm_loads[id];
    size_t len;
    char *txt = read_file(L->path, &len);
    if (!txt) {
        char cwd[1024] = ".", dir[1400];
        if (!getcwd(cwd, sizeof cwd)) strcpy(cwd, ".");
        const char *slash = strrchr(L->path, '/');
        if (L->path[0] == '/') snprintf(dir, sizeof dir, "%.*s", slash ? (int)(slash - L->path) : 0, L->path);
        else if (slash) snprintf(dir, sizeof dir, "%s/%.*s", cwd, (int)(slash - L->path), L->path);
        else snprintf(dir, sizeof dir, "%s", cwd);
        snprintf(err_msg, sizeof err_msg, "can't find the file '%s' (looked in %s; data files are read from the "
                 "folder you run the program in)", L->path, dir[0] ? dir : "/");
        return 0;
    }
    size_t pos = 0;
    if (len >= 3 && (unsigned char)txt[0] == 0xEF && (unsigned char)txt[1] == 0xBB && (unsigned char)txt[2] == 0xBF) pos = 3;
    csv_row row = {0};
    if (!csv_next(txt, len, &pos, &row)) {
        snprintf(err_msg, sizeof err_msg, "the file %s is empty", base_name(L->path));
        free(txt);
        return 0;
    }
    /* the header must be the one the program was built with: the units were compiled in */
    int same = row.n == L->ncols;
    char cell[512];
    for (int k = 0; same && k < row.n; k++) {
        strip_into(row.cells[k], cell, sizeof cell);
        same = !strcmp(cell, L->cols[k].header);
    }
    if (!same) {
        char want[600] = "", got[600] = "";
        for (int k = 0; k < L->ncols; k++) {
            if (k) strncat(want, ", ", sizeof want - strlen(want) - 1);
            strncat(want, L->cols[k].header, sizeof want - strlen(want) - 1);
        }
        for (int k = 0; k < row.n; k++) {
            strip_into(row.cells[k], cell, sizeof cell);
            if (k) strncat(got, ", ", sizeof got - strlen(got) - 1);
            strncat(got, cell, sizeof got - strlen(got) - 1);
        }
        snprintf(err_msg, sizeof err_msg, "%s: the header is '%s' but the program was built for '%s' "
                 "(the columns' units are compiled in; build it again with  fermium build)", L->path, got, want);
        row_free(&row);
        free(txt);
        return 0;
    }
    int nc = L->ncols;
    int64_t n = 0, cap = 64;
    double **cols = xmalloc(sizeof(double *) * (size_t)(nc ? nc : 1));
    for (int k = 0; k < nc; k++) cols[k] = xmalloc(sizeof(double) * (size_t)cap);
    int ln = 1;
    while (csv_next(txt, len, &pos, &row)) {
        ln++;
        int blank = 1;
        for (int k = 0; k < row.n && blank; k++) { strip_into(row.cells[k], cell, sizeof cell); blank = !cell[0]; }
        if (blank) continue;
        if (row.n != nc) {
            snprintf(err_msg, sizeof err_msg, "%s, line %d: expected %d values but found %d", L->path, ln, nc, row.n);
            goto fail;
        }
        if (n == cap) {
            cap *= 2;
            for (int k = 0; k < nc; k++) {
                double *d = xmalloc(sizeof(double) * (size_t)cap);
                memcpy(d, cols[k], sizeof(double) * (size_t)n);
                free(cols[k]);
                cols[k] = d;
            }
        }
        for (int k = 0; k < nc; k++) {
            double v;
            if (!py_float(row.cells[k], &v)) {
                char r[400] = "[", q[200];
                for (int j = 0; j < row.n; j++) {
                    py_repr(row.cells[j], q, sizeof q);
                    if (j) strncat(r, ", ", sizeof r - strlen(r) - 1);
                    strncat(r, q, sizeof r - strlen(r) - 1);
                }
                strncat(r, "]", sizeof r - strlen(r) - 1);
                snprintf(err_msg, sizeof err_msg, "%s, line %d: not a number: %s", L->path, ln, r);
                goto fail;
            }
            cols[k][n] = v * L->cols[k].factor + L->cols[k].offset;
        }
        n++;
    }
    row_free(&row);
    free(txt);
    {
        fm_dataset *nd = xmalloc(sizeof(fm_dataset) * (size_t)(ndatasets + 1));
        if (ndatasets) memcpy(nd, datasets, sizeof(fm_dataset) * (size_t)ndatasets);
        free(datasets);
        datasets = nd;
        datasets[ndatasets].ncols = nc;
        datasets[ndatasets].n = n;
        datasets[ndatasets].cols = cols;
        ndatasets++;
    }
    return ndatasets;
fail:
    for (int k = 0; k < nc; k++) free(cols[k]);
    free(cols);
    row_free(&row);
    free(txt);
    return 0;
}

/* table(x = xs, y = ys) (D193): copies the lists (already SI) into a new data set (Runtime.table) */
int64_t fm_table(int64_t nc, double **ptrs, int64_t *lens) {
    int64_t n = nc ? lens[0] : 0;
    for (int64_t k = 1; k < nc; k++) {
        if (lens[k] != n) {
            snprintf(err_msg, sizeof err_msg, "the columns of this table have different lengths (%lld and %lld)",
                     (long long)n, (long long)lens[k]);
            return 0;
        }
    }
    double **cols = xmalloc(sizeof(double *) * (size_t)(nc ? nc : 1));
    for (int64_t k = 0; k < nc; k++) {
        cols[k] = xmalloc(sizeof(double) * (size_t)(n ? n : 1));
        if (n) memcpy(cols[k], ptrs[k], sizeof(double) * (size_t)n);
    }
    fm_dataset *nd = xmalloc(sizeof(fm_dataset) * (size_t)(ndatasets + 1));
    if (ndatasets) memcpy(nd, datasets, sizeof(fm_dataset) * (size_t)ndatasets);
    free(datasets);
    datasets = nd;
    datasets[ndatasets].ncols = (int)nc;
    datasets[ndatasets].n = n;
    datasets[ndatasets].cols = cols;
    ndatasets++;
    return ndatasets;
}

int64_t fm_column(int64_t h, int64_t col, double **out) {
    fm_dataset *d = &datasets[h - 1];
    *out = d->cols[col];
    return d->n;
}

/* ------------------------------------------------------------------ fit (runtime/fitting.py) */
typedef struct {
    fm_model_fn model;
    double **cols;
    int64_t n;
    int k;
    double *tmp;
    long nfev;
} fit_ctx;

/* sum of squares of the residuals, inf if any is not finite (fitting._sse) */
static double sse(fit_ctx *c, const double *p) {
    c->model((double *)p, c->cols, c->n, c->tmp);
    c->nfev++;
    double s = 0;
    for (int64_t i = 0; i < c->n; i++) s += c->tmp[i] * c->tmp[i];
    return isfinite(s) ? s : INFINITY;
}

/* the residuals least_squares sees: non-finite values become 1e300 */
static void resid(fit_ctx *c, const double *p, double *r) {
    c->model((double *)p, c->cols, c->n, r);
    c->nfev++;
    for (int64_t i = 0; i < c->n; i++) if (!isfinite(r[i])) r[i] = 1e300;
}

static long double cost_of(const double *r, int64_t n) {
    long double s = 0;
    for (int64_t i = 0; i < n; i++) s += (long double)r[i] * r[i];
    return 0.5L * s;
}

/* fitting._initial_guess: fill in missing guesses by scanning a grid of values */
static void initial_guess(fit_ctx *c, const double *guess, int pos125, double *p) {
    int k = c->k;
    for (int i = 0; i < k; i++) p[i] = isfinite(guess[i]) ? guess[i] : 1.0;
    double *q = xmalloc(sizeof(double) * (size_t)k);
    for (int pass = 0; pass < 2; pass++) {
        for (int i = 0; i < k; i++) {
            if (isfinite(guess[i])) continue;
            double best = p[i], bestv = sse(c, p);
            for (int e = -35; e <= 35; e++) {
                for (int m = 0; m < (pos125 ? 3 : 2); m++) {
                    double s = pos125 ? (m == 0 ? 1.0 : m == 1 ? 2.0 : 5.0) * pow(10.0, e)
                                      : (m == 0 ? 1.0 : -1.0) * pow(10.0, e);
                    memcpy(q, p, sizeof(double) * (size_t)k);
                    q[i] = s;
                    double v = sse(c, q);
                    if (v < bestv) { best = s; bestv = v; }
                }
            }
            p[i] = best;
        }
    }
    free(q);
}

/* solve the k x k system A x = b in place (partial pivoting); 0 if singular */
static int solve_lin(double *A, double *b, int k) {
    for (int col = 0; col < k; col++) {
        int piv = col;
        for (int r = col + 1; r < k; r++) if (fabs(A[r * k + col]) > fabs(A[piv * k + col])) piv = r;
        if (A[piv * k + col] == 0 || !isfinite(A[piv * k + col])) return 0;
        if (piv != col) {
            for (int j = 0; j < k; j++) { double t = A[col * k + j]; A[col * k + j] = A[piv * k + j]; A[piv * k + j] = t; }
            double t = b[col]; b[col] = b[piv]; b[piv] = t;
        }
        for (int r = col + 1; r < k; r++) {
            double f = A[r * k + col] / A[col * k + col];
            for (int j = col; j < k; j++) A[r * k + j] -= f * A[col * k + j];
            b[r] -= f * b[col];
        }
    }
    for (int r = k - 1; r >= 0; r--) {
        double s = b[r];
        for (int j = r + 1; j < k; j++) s -= A[r * k + j] * b[j];
        b[r] = s / A[r * k + r];
    }
    return 1;
}

static void jacobian(fit_ctx *c, const double *p, const double *r, double *J, int scipy_step) {
    int k = c->k;
    int64_t n = c->n;
    double *q = xmalloc(sizeof(double) * (size_t)k), *rq = xmalloc(sizeof(double) * (size_t)n);
    const double EPS = 1.4901161193847656e-8;           /* sqrt(machine epsilon) */
    for (int j = 0; j < k; j++) {
        double h;
        if (scipy_step) {            /* scipy's approx_derivative '2-point', used for the reported errors */
            h = EPS * (p[j] >= 0 ? 1.0 : -1.0) * fmax(1.0, fabs(p[j]));
        } else {
            h = EPS * fabs(p[j]);
            if (h == 0) h = EPS;
        }
        memcpy(q, p, sizeof(double) * (size_t)k);
        q[j] = p[j] + h;
        h = q[j] - p[j];
        resid(c, q, rq);
        for (int64_t i = 0; i < n; i++) J[i * k + j] = (rq[i] - r[i]) / h;
    }
    free(q);
    free(rq);
}

/* Levenberg-Marquardt with Marquardt's diagonal scaling; returns the final cost */
static long double levmar(fit_ctx *c, double *p, double *r) {
    int k = c->k;
    int64_t n = c->n;
    double *J = xmalloc(sizeof(double) * (size_t)(n * k));
    double *A = xmalloc(sizeof(double) * (size_t)(k * k)), *M = xmalloc(sizeof(double) * (size_t)(k * k));
    double *g = xmalloc(sizeof(double) * (size_t)k), *d = xmalloc(sizeof(double) * (size_t)k);
    double *D = xmalloc(sizeof(double) * (size_t)k), *pn = xmalloc(sizeof(double) * (size_t)k);
    double *rn = xmalloc(sizeof(double) * (size_t)n);
    resid(c, p, r);
    long double cost = cost_of(r, n);
    double lam = 1e-3;
    long start = c->nfev;
    while (c->nfev - start < 20000) {
        jacobian(c, p, r, J, 0);
        for (int a = 0; a < k; a++) {
            double s = 0;
            for (int64_t i = 0; i < n; i++) s += J[i * k + a] * r[i];
            g[a] = s;
            for (int b = 0; b <= a; b++) {
                double t = 0;
                for (int64_t i = 0; i < n; i++) t += J[i * k + a] * J[i * k + b];
                A[a * k + b] = A[b * k + a] = t;
            }
        }
        for (int a = 0; a < k; a++) D[a] = A[a * k + a] > 0 && isfinite(A[a * k + a]) ? sqrt(A[a * k + a]) : 1.0;
        double gmax = 0;
        for (int a = 0; a < k; a++) gmax = fmax(gmax, fabs(g[a] / D[a]));
        if (!(gmax > 1e-14 * sqrt((double)(2 * cost))) || cost == 0) break;     /* gradient ~ 0: at the minimum */
        int accepted = 0, done = 0;
        while (c->nfev - start < 20000) {
            for (int a = 0; a < k; a++) {
                for (int b = 0; b < k; b++) M[a * k + b] = A[a * k + b] / (D[a] * D[b]);
                M[a * k + a] += lam;
                d[a] = -g[a] / D[a];
            }
            if (!solve_lin(M, d, k)) { lam *= 10; if (lam > 1e20) { done = 1; break; } continue; }
            double stepmax = 0;
            for (int a = 0; a < k; a++) {
                d[a] /= D[a];
                pn[a] = p[a] + d[a];
                stepmax = fmax(stepmax, fabs(d[a]) / (fabs(p[a]) + 1e-300));
            }
            resid(c, pn, rn);
            long double cn = cost_of(rn, n);
            if (cn < cost) {
                long double dec = cost - cn;
                memcpy(p, pn, sizeof(double) * (size_t)k);
                memcpy(r, rn, sizeof(double) * (size_t)n);
                cost = cn;
                lam = fmax(lam / 10, 1e-12);
                accepted = 1;
                if (dec <= 1e-15L * cost || stepmax <= 1e-15) done = 1;
                break;
            }
            lam *= 10;
            if (lam > 1e20 || stepmax <= 1e-16) { done = 1; break; }
        }
        if (done || !accepted) break;
    }
    free(J); free(A); free(M); free(g); free(d); free(D); free(pn); free(rn);
    return cost;
}

static void quantity(double x, int sig, const char *unit, char *out, size_t cap) {
    char b[128];
    fmt_num(x, sig, 0, b, sizeof b);
    if (!unit[0] || !strcmp(unit, "1")) snprintf(out, cap, "%s", b);
    else if (attached(unit)) snprintf(out, cap, "%s%s", b, unit);
    else snprintf(out, cap, "%s %s", b, unit);
}

int64_t fm_fit(int64_t fid, int64_t h, double *p) {
    const fm_fitinfo *F = &fm_fits[fid];
    fm_dataset *ds = &datasets[h - 1];
    int k = F->nparams;
    int64_t n = ds->ncols ? ds->n : 0;
    if (n < k) {
        snprintf(err_msg, sizeof err_msg, "can't fit %d parameters to only %lld data points", k, (long long)n);
        return 1;
    }
    fit_ctx c;
    c.model = F->model;
    c.cols = xmalloc(sizeof(double *) * (size_t)(F->ncols ? F->ncols : 1));
    for (int j = 0; j < F->ncols; j++) c.cols[j] = ds->cols[F->cols[j]];
    c.n = n;
    c.k = k;
    c.tmp = xmalloc(sizeof(double) * (size_t)(n ? n : 1));
    c.nfev = 0;
    double *guess = xmalloc(sizeof(double) * (size_t)k);
    int missing = 0;
    for (int i = 0; i < k; i++) {
        guess[i] = isfinite(p[i]) ? p[i] : NAN;
        if (!isfinite(p[i])) missing = 1;
    }
    double *best = xmalloc(sizeof(double) * (size_t)k), *cand = xmalloc(sizeof(double) * (size_t)k);
    double *r = xmalloc(sizeof(double) * (size_t)(n ? n : 1)), *rbest = xmalloc(sizeof(double) * (size_t)(n ? n : 1));
    long double bestcost = 0;
    int have = 0;
    for (int s = 0; s < (missing ? 2 : 1); s++) {    /* several starts when a guess is missing; keep the best */
        initial_guess(&c, guess, s == 1, cand);
        long double cc = levmar(&c, cand, r);
        if (!have || cc < bestcost) {
            memcpy(best, cand, sizeof(double) * (size_t)k);
            memcpy(rbest, r, sizeof(double) * (size_t)n);
            bestcost = cc;
            have = 1;
        }
    }
    /* standard errors: covariance = (JᵀJ)⁻¹ rss/dof with the Jacobian at the best fit (fitting._finish) */
    double rss = 0;
    for (int64_t i = 0; i < n; i++) rss += rbest[i] * rbest[i];
    int dof = (int)(n - k) > 1 ? (int)(n - k) : 1;
    double *J = xmalloc(sizeof(double) * (size_t)(n * k + 1));
    double *A = xmalloc(sizeof(double) * (size_t)(k * k)), *col = xmalloc(sizeof(double) * (size_t)k);
    double *errs = xmalloc(sizeof(double) * (size_t)k);
    int *ok = xmalloc(sizeof(int) * (size_t)k);
    jacobian(&c, best, rbest, J, 1);
    int singular = 0;
    for (int j = 0; j < k && !singular; j++) {        /* column j of the inverse */
        for (int a = 0; a < k; a++)
            for (int b = 0; b < k; b++) {
                double t = 0;
                for (int64_t i = 0; i < n; i++) t += J[i * k + a] * J[i * k + b];
                A[a * k + b] = t;
            }
        for (int a = 0; a < k; a++) col[a] = a == j ? 1.0 : 0.0;
        if (!solve_lin(A, col, k)) singular = 1;
        else {
            double v = col[j] * (rss / dof);
            ok[j] = v >= 0 && isfinite(v);
            errs[j] = ok[j] ? sqrt(v) : NAN;
        }
    }
    for (int i = 0; i < k; i++) {
        p[i] = best[i];
        if (singular) { ok[i] = 0; errs[i] = NAN; }
        p[k + i] = (ok[i] && isfinite(errs[i])) ? errs[i] : NAN;     /* for err(x) */
    }
    printf("fit %s   (%lld data points from %s)\n", F->text, (long long)n, F->path);
    char val[200], se[100];
    for (int i = 0; i < k; i++) {
        const fm_paraminfo *P = &F->params[i];
        int sig = 4;          /* fit_sigfigs in runtime/core.py */
        if (ok[i] && isfinite(errs[i]) && errs[i] > 0 && isfinite(best[i]) && best[i] != 0) {
            sig = (int)floor(log10(fabs(best[i]))) - (int)floor(log10(errs[i])) + 2;
            if (sig < 4) sig = 4;
            if (sig > 12) sig = 12;
        }
        quantity(best[i] / P->factor, sig, P->unit, val, sizeof val);
        if (ok[i] && isfinite(errs[i])) {
            fmt_num(errs[i] / P->factor, 2, 0, se, sizeof se);
            int has_unit = P->unit[0] && strcmp(P->unit, "1");
            printf("  %s = %s   (standard error %s%s%s)\n", P->name, val, se, has_unit ? " " : "", has_unit ? P->unit : "");
        } else {
            printf("  %s = %s   (standard error could not be estimated)\n", P->name, val);
        }
    }
    int yu = F->yunit[0] && strcmp(F->yunit, "1");
    fmt_num(sqrt(rss / (double)n) / F->yfactor, 3, 0, se, sizeof se);
    printf("  rms residual = %s%s%s\n", se, yu ? " " : "", yu ? F->yunit : "");
    for (int i = 0; i < k; i++) {
        if (!(ok[i] && isfinite(errs[i]))) {
            printf("  warning: the fit may not have converged; give a starting guess, like  fit ... with %s = ...\n",
                   F->params[i].name);
            break;
        }
    }
    free(c.cols); free(c.tmp); free(guess); free(best); free(cand); free(r); free(rbest);
    free(J); free(A); free(col); free(errs); free(ok);
    return 0;
}

/* ------------------------------------------------------------------ plot (SVG) */
typedef struct { int idx; int64_t n; double *x, *y; } plot_series;
typedef struct { int n, cap; plot_series *s; } plot_acc;
static plot_acc *plot_accs = NULL;
static int nplot_accs = 0;

static plot_acc *acc_for(int64_t pid) {
    if (pid >= nplot_accs) {
        int nn = (int)pid + 8;
        plot_acc *na = xmalloc(sizeof(plot_acc) * (size_t)nn);
        memset(na, 0, sizeof(plot_acc) * (size_t)nn);
        if (nplot_accs) memcpy(na, plot_accs, sizeof(plot_acc) * (size_t)nplot_accs);
        free(plot_accs);
        plot_accs = na;
        nplot_accs = nn;
    }
    return &plot_accs[pid];
}

static void acc_clear(plot_acc *a) {
    for (int i = 0; i < a->n; i++) { free(a->s[i].x); free(a->s[i].y); }
    free(a->s);
    a->s = NULL;
    a->n = a->cap = 0;
}

static void acc_add(int64_t pid, int64_t idx, double *x, double *y, int64_t n) {
    plot_acc *a = acc_for(pid);
    if (a->n == a->cap) {
        a->cap = a->cap ? a->cap * 2 : 4;
        plot_series *ns = xmalloc(sizeof(plot_series) * (size_t)a->cap);
        if (a->n) memcpy(ns, a->s, sizeof(plot_series) * (size_t)a->n);
        free(a->s);
        a->s = ns;
    }
    plot_series *s = &a->s[a->n++];
    s->idx = (int)idx;
    s->n = n;
    s->x = x;
    s->y = y;
}

int64_t fm_plot_series(int64_t pid, int64_t idx, double *xp, int64_t nx, double *yp, int64_t ny) {
    if (nx != ny) {
        snprintf(err_msg, sizeof err_msg, "plot: the two lists have different lengths (%lld and %lld values)",
                 (long long)ny, (long long)nx);
        acc_clear(acc_for(pid));
        return 1;
    }
    double *x = xmalloc(sizeof(double) * (size_t)(nx ? nx : 1)), *y = xmalloc(sizeof(double) * (size_t)(nx ? nx : 1));
    memcpy(x, xp, sizeof(double) * (size_t)nx);
    memcpy(y, yp, sizeof(double) * (size_t)nx);
    acc_add(pid, idx, x, y, nx);
    return 0;
}

typedef struct { int64_t n, dim, cap; double *t, *y, *dy; } fm_sol;

/* dense samples of one solution component: cubic Hermite between steps (core.sample_solution) */
static void sample_solution(const fm_sol *s, int64_t comp, int use_dy, double **tt, double **yy, int64_t *m) {
    int64_t n = s->n, dim = s->dim;
    const int64_t npts = 600;
    if (n >= npts || n < 2) {
        *m = n;
        *tt = xmalloc(sizeof(double) * (size_t)(n ? n : 1));
        *yy = xmalloc(sizeof(double) * (size_t)(n ? n : 1));
        for (int64_t i = 0; i < n; i++) {
            (*tt)[i] = s->t[i];
            (*yy)[i] = use_dy ? s->dy[i * dim + comp] : s->y[i * dim + comp];
        }
        return;
    }
    *m = npts;
    *tt = xmalloc(sizeof(double) * (size_t)npts);
    *yy = xmalloc(sizeof(double) * (size_t)npts);
    double t0 = s->t[0], t1 = s->t[n - 1];
    double sg = t1 >= t0 ? 1.0 : -1.0;     /* times decrease for a solve towards smaller t (D39) */
    int64_t i = 0;
    for (int64_t j = 0; j < npts; j++) {
        double t = t0 + (t1 - t0) * (double)j / (double)(npts - 1);
        if (j == npts - 1) t = t1;
        while (i < n - 2 && sg * s->t[i + 1] <= sg * t) i++;
        double h = s->t[i + 1] - s->t[i], u = (t - s->t[i]) / h;
        double y0 = s->y[i * dim + comp], y1 = s->y[(i + 1) * dim + comp];
        double d0 = s->dy[i * dim + comp], d1 = s->dy[(i + 1) * dim + comp];
        double v;
        if (use_dy) {
            double a00 = 6 * u * u - 6 * u, a10 = 3 * u * u - 4 * u + 1, a01 = 6 * u - 6 * u * u, a11 = 3 * u * u - 2 * u;
            v = (a00 * y0 + a10 * h * d0 + a01 * y1 + a11 * h * d1) / h;
        } else {
            double h00 = 2 * u * u * u - 3 * u * u + 1, h10 = u * u * u - 2 * u * u + u;
            double h01 = -2 * u * u * u + 3 * u * u, h11 = u * u * u - u * u;
            v = h00 * y0 + h10 * h * d0 + h01 * y1 + h11 * h * d1;
        }
        (*tt)[j] = t;
        (*yy)[j] = v;
    }
}

void fm_plot_sol(int64_t pid, int64_t idx, void *solp, int64_t comp, int64_t dy, int64_t comp2, int64_t dy2) {
    const fm_sol *s = (const fm_sol *)solp;
    double *ts, *ys, *xs;
    int64_t m, m2;
    sample_solution(s, comp, (int)dy, &ts, &ys, &m);
    if (comp2 >= 0) {
        double *t2;
        sample_solution(s, comp2, (int)dy2, &t2, &xs, &m2);
        free(t2);
        free(ts);
    } else {
        xs = ts;
    }
    acc_add(pid, idx, xs, ys, m);
}

static const char *COLORS[] = {"#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd",
                               "#8c564b", "#e377c2", "#7f7f7f", "#bcbd22", "#17becf"};

typedef struct { double lo, hi; int log; } axis;

static double ax_t(const axis *a, double v) { return a->log ? log10(v) : v; }

static void mkdirs_for(const char *path) {
    char buf[1024];
    snprintf(buf, sizeof buf, "%s", path);
    for (char *p = buf + 1; *p; p++) {
        if (*p == '/') {
            *p = 0;
            mkdir(buf, 0777);
            *p = '/';
        }
    }
}

static void tick_label(double v, int log, char *out, size_t cap) {
    if (log) {
        int e = (int)lround(log10(v));
        if (fabs(log10(v) - e) < 1e-9) {
            if (e >= 0 && e <= 3) { snprintf(out, cap, "%g", pow(10, e)); return; }
            char s[32], sup[96] = "";
            snprintf(s, sizeof s, "%d", e);
            for (char *p = s; *p; p++) strcat(sup, *p == '-' ? "⁻" : SUP[*p - '0']);
            snprintf(out, cap, "10%s", sup);
            return;
        }
    }
    fmt_num(v, 6, 1, out, cap);
}

/* axis ticks in data units; returns the count */
static int make_ticks(const axis *a, double *t, int maxn) {
    int n = 0;
    if (a->log) {
        int e0 = (int)ceil(a->lo - 1e-9), e1 = (int)floor(a->hi + 1e-9);
        int stride = 1;
        while ((e1 - e0) / stride > 8) stride++;
        for (int e = e0; e <= e1 && n < maxn; e += stride) t[n++] = pow(10, e);
        if (e1 - e0 < 1) {           /* less than a decade: add 2× and 5× ticks */
            for (int e = e0 - 1; e <= e1; e++)
                for (int m = 2; m <= 5; m += 3) {
                    double v = m * pow(10, e);
                    if (log10(v) >= a->lo && log10(v) <= a->hi && n < maxn) t[n++] = v;
                }
        }
        return n;
    }
    double span = a->hi - a->lo;
    double raw = span / 6, mag = pow(10, floor(log10(raw))), f = raw / mag;
    double step = (f < 1.5 ? 1 : f < 2.25 ? 2 : f < 3.5 ? 2.5 : f < 7.5 ? 5 : 10) * mag;
    for (double k = ceil(a->lo / step - 1e-9); k * step <= a->hi + step * 1e-9 && n < maxn; k++) {
        double v = k * step;
        if (fabs(v) < step * 1e-9) v = 0;
        t[n++] = v;
    }
    return n;
}

static void range_of(double lo, double hi, int log, axis *a) {
    a->log = log;
    if (!(lo <= hi)) { lo = log ? 0 : 0; hi = log ? 1 : 1; }      /* no data */
    if (lo == hi) {
        if (log) { lo -= 1; hi += 1; }
        else if (lo == 0) { lo = -1; hi = 1; }
        else { double d = fabs(lo) * 0.05; lo -= d; hi += d; }
    }
    double m = (hi - lo) * 0.05;       /* matplotlib's default margins */
    a->lo = lo - m;
    a->hi = hi + m;
}

void fm_plot_done(int64_t pid) {
    const fm_plotinfo *P = &fm_plots[pid];
    plot_acc *acc = acc_for(pid);
    /* series in the order they were written */
    for (int i = 1; i < acc->n; i++)
        for (int j = i; j > 0 && acc->s[j - 1].idx > acc->s[j].idx; j--) {
            plot_series t = acc->s[j]; acc->s[j] = acc->s[j - 1]; acc->s[j - 1] = t;
        }
    /* convert to display units, find the ranges */
    double xlo = INFINITY, xhi = -INFINITY, ylo = INFINITY, yhi = -INFINITY;
    for (int i = 0; i < acc->n; i++) {
        plot_series *s = &acc->s[i];
        const fm_seriesinfo *si = &P->series[s->idx];
        for (int64_t j = 0; j < s->n; j++) {
            s->x[j] = (s->x[j] - si->xoffset) / si->xfactor;
            s->y[j] = (s->y[j] - si->yoffset) / si->yfactor;
            double x = s->x[j], y = s->y[j];
            if (!isfinite(x) || !isfinite(y) || (P->logx && x <= 0) || (P->logy && y <= 0)) continue;
            double tx = P->logx ? log10(x) : x, ty = P->logy ? log10(y) : y;
            xlo = fmin(xlo, tx); xhi = fmax(xhi, tx);
            ylo = fmin(ylo, ty); yhi = fmax(yhi, ty);
        }
    }
    axis ax, ay;
    range_of(xlo, xhi, P->logx, &ax);
    range_of(ylo, yhi, P->logy, &ay);
    if (P->hasx) { ax.lo = P->logx ? log10(P->xlo) : P->xlo; ax.hi = P->logx ? log10(P->xhi) : P->xhi; }
    if (P->hasy) { ay.lo = P->logy ? log10(P->ylo) : P->ylo; ay.hi = P->logy ? log10(P->yhi) : P->yhi; }
    const double W = 770, H = 495;
    double L = 90, R = W - 25, T = P->title[0] ? 45 : 25, B = H - 62;
    if (P->equal && !P->logx && !P->logy) {       /* orbits look round: same scale on both axes */
        double sx = (ax.hi - ax.lo) / (R - L), sy = (ay.hi - ay.lo) / (B - T);
        if (sx > sy) { double c = (ay.lo + ay.hi) / 2, h = sx * (B - T) / 2; ay.lo = c - h; ay.hi = c + h; }
        else { double c = (ax.lo + ax.hi) / 2, h = sy * (R - L) / 2; ax.lo = c - h; ax.hi = c + h; }
    }
#define FX(v) ((ax_t(&ax, (v)) - ax.lo) / (ax.hi - ax.lo))
#define FY(v) ((ax_t(&ay, (v)) - ay.lo) / (ay.hi - ay.lo))
#define PX(v) (P->revx ? R - FX(v) * (R - L) : L + FX(v) * (R - L))
#define PY(v) (P->revy ? T + FY(v) * (B - T) : B - FY(v) * (B - T))
    mkdirs_for(P->svg);
    FILE *f = fopen(P->svg, "w");
    if (!f) {
        printf("(plot not saved: can't write %s: %s)\n", P->svg, strerror(errno));
        acc_clear(acc);
        return;
    }
    fprintf(f, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    fprintf(f, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"%.0f\" height=\"%.0f\" viewBox=\"0 0 %.0f %.0f\" "
               "font-family=\"DejaVu Sans, Helvetica, Arial, sans-serif\">\n", W, H, W, H);
    fprintf(f, "<rect width=\"100%%\" height=\"100%%\" fill=\"white\"/>\n");
    fprintf(f, "<defs><clipPath id=\"area\"><rect x=\"%.2f\" y=\"%.2f\" width=\"%.2f\" height=\"%.2f\"/></clipPath></defs>\n",
            L, T, R - L, B - T);
    /* grid and ticks */
    double ticks[64];
    char lab[128];
    int nt = make_ticks(&ax, ticks, 64);
    fprintf(f, "<g font-size=\"11\" fill=\"#222\">\n");
    for (int i = 0; i < nt; i++) {
        double x = PX(ticks[i]);
        if (x < L - 0.5 || x > R + 0.5) continue;
        tick_label(ticks[i], ax.log, lab, sizeof lab);
        fprintf(f, "<line x1=\"%.2f\" y1=\"%.2f\" x2=\"%.2f\" y2=\"%.2f\" stroke=\"#000\" stroke-opacity=\"0.12\"/>\n", x, T, x, B);
        fprintf(f, "<line x1=\"%.2f\" y1=\"%.2f\" x2=\"%.2f\" y2=\"%.2f\" stroke=\"#000\"/>\n", x, B, x, B + 5);
        fprintf(f, "<text x=\"%.2f\" y=\"%.2f\" text-anchor=\"middle\">%s</text>\n", x, B + 18, lab);
    }
    nt = make_ticks(&ay, ticks, 64);
    for (int i = 0; i < nt; i++) {
        double y = PY(ticks[i]);
        if (y < T - 0.5 || y > B + 0.5) continue;
        tick_label(ticks[i], ay.log, lab, sizeof lab);
        fprintf(f, "<line x1=\"%.2f\" y1=\"%.2f\" x2=\"%.2f\" y2=\"%.2f\" stroke=\"#000\" stroke-opacity=\"0.12\"/>\n", L, y, R, y);
        fprintf(f, "<line x1=\"%.2f\" y1=\"%.2f\" x2=\"%.2f\" y2=\"%.2f\" stroke=\"#000\"/>\n", L - 5, y, L, y);
        fprintf(f, "<text x=\"%.2f\" y=\"%.2f\" text-anchor=\"end\">%s</text>\n", L - 8, y + 4, lab);
    }
    fprintf(f, "</g>\n");
    /* the data */
    fprintf(f, "<g clip-path=\"url(#area)\" fill=\"none\">\n");
    for (int i = 0; i < acc->n; i++) {
        plot_series *s = &acc->s[i];
        const fm_seriesinfo *si = &P->series[s->idx];
        const char *col = COLORS[i % 10];
        if (si->points) {
            fprintf(f, "<path class=\"series\" fill=\"%s\" stroke=\"none\" d=\"", col);
            for (int64_t j = 0; j < s->n; j++) {
                double x = s->x[j], y = s->y[j];
                if (!isfinite(x) || !isfinite(y) || (ax.log && x <= 0) || (ay.log && y <= 0)) continue;
                fprintf(f, "M%.2f %.2fm-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0", PX(x), PY(y));
            }
            fprintf(f, "\"/>\n");
        } else {
            int open = 0;
            for (int64_t j = 0; j < s->n; j++) {
                double x = s->x[j], y = s->y[j];
                int good = isfinite(x) && isfinite(y) && !(ax.log && x <= 0) && !(ay.log && y <= 0);
                if (!good) { if (open) { fprintf(f, "\"/>\n"); open = 0; } continue; }
                if (!open) {
                    fprintf(f, "<polyline class=\"series\" stroke=\"%s\" stroke-width=\"1.8\" stroke-linejoin=\"round\" points=\"", col);
                    open = 1;
                }
                fprintf(f, "%.2f,%.2f ", PX(x), PY(y));
            }
            if (open) fprintf(f, "\"/>\n");
        }
    }
    fprintf(f, "</g>\n");
    /* frame, labels, title */
    fprintf(f, "<rect x=\"%.2f\" y=\"%.2f\" width=\"%.2f\" height=\"%.2f\" fill=\"none\" stroke=\"#000\"/>\n", L, T, R - L, B - T);
    /* x label: the first series'; y label: the distinct y labels */
    const char *xl = acc->n ? P->series[acc->s[0].idx].xlabel : (P->nseries ? P->series[0].xlabel : "");
    fprintf(f, "<text x=\"%.2f\" y=\"%.2f\" font-size=\"13\" text-anchor=\"middle\">%s</text>\n", (L + R) / 2, H - 18, xl);
    fprintf(f, "<text transform=\"translate(%.2f %.2f) rotate(-90)\" font-size=\"13\" text-anchor=\"middle\">", 20.0, (T + B) / 2);
    for (int i = 0, shown = 0; i < acc->n; i++) {
        const char *yl = P->series[acc->s[i].idx].ylabel;
        int dup = 0;
        for (int j = 0; j < i; j++) if (!strcmp(P->series[acc->s[j].idx].ylabel, yl)) dup = 1;
        if (dup) continue;
        fprintf(f, "%s%s", shown++ ? ", " : "", yl);
    }
    fprintf(f, "</text>\n");
    if (P->title[0])
        fprintf(f, "<text x=\"%.2f\" y=\"%.2f\" font-size=\"15\" text-anchor=\"middle\">%s</text>\n", (L + R) / 2, 28.0, P->title);
    if (acc->n > 1) {
        double lw = 0;
        for (int i = 0; i < acc->n; i++) lw = fmax(lw, (double)strlen(P->series[acc->s[i].idx].legend));
        lw = 48 + lw * 7;
        double lx = R - lw - 10, ly = T + 10;
        fprintf(f, "<g class=\"legend\" font-size=\"11\">\n<rect x=\"%.2f\" y=\"%.2f\" width=\"%.2f\" height=\"%.2f\" "
                   "fill=\"white\" fill-opacity=\"0.85\" stroke=\"#ccc\" rx=\"3\"/>\n", lx, ly, lw, 8 + 18.0 * acc->n);
        for (int i = 0; i < acc->n; i++) {
            const fm_seriesinfo *si = &P->series[acc->s[i].idx];
            double y = ly + 16 + 18.0 * i;
            if (si->points)
                fprintf(f, "<circle cx=\"%.2f\" cy=\"%.2f\" r=\"3.5\" fill=\"%s\"/>\n", lx + 18, y - 4, COLORS[i % 10]);
            else
                fprintf(f, "<line x1=\"%.2f\" y1=\"%.2f\" x2=\"%.2f\" y2=\"%.2f\" stroke=\"%s\" stroke-width=\"1.8\"/>\n",
                        lx + 8, y - 4, lx + 28, y - 4, COLORS[i % 10]);
            fprintf(f, "<text x=\"%.2f\" y=\"%.2f\">%s</text>\n", lx + 34, y, si->legend);
        }
        fprintf(f, "</g>\n");
    }
    fprintf(f, "</svg>\n");
    int bad = ferror(f);
    if (fclose(f) != 0 || bad) {
        printf("(plot not saved: can't write %s)\n", P->svg);
    } else {
        printf("plot saved to %s%s\n", P->shown, P->renamed ? " (standalone programs write SVG)" : "");
    }
    fflush(stdout);
    acc_clear(acc);
#undef PX
#undef PY
#undef FX
#undef FY
}
