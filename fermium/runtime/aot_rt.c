/* Fermium runtime for standalone executables built with `fermium build`.
 *
 * The compiled program calls these functions for printing and error reporting,
 * exactly like the JIT'd program calls the Python callbacks in runtime/core.py.
 * Number formatting follows fermium.units.format_number.
 * The format and text tables are generated per program (fm_tables.c).
 * `load`, `fit` and `plot` live in aot_data.c (included below).
 */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <errno.h>
#include <sys/stat.h>
#include <unistd.h>

typedef struct {
    double factor, offset;
    int sf;       /* significant figures, -1 = exact */
    int direct;   /* value written directly as a literal */
    const char *unit;
} fm_fmt;

extern const fm_fmt fm_fmts[];
extern const char *fm_texts[];
extern int fm_run(void);

static char line[1 << 16];
static size_t line_len = 0;
static char err_msg[512];
static int64_t err_line = 0;

static void emit(const char *s) {
    size_t n = strlen(s);
    if (line_len && line_len + 1 < sizeof line) line[line_len++] = ' ';
    if (line_len + n >= sizeof line) n = sizeof line - line_len - 1;
    memcpy(line + line_len, s, n);
    line_len += n;
    line[line_len] = 0;
}

static const char *SUP[] = {"⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"};

static void trim_zeros(char *s) {
    if (!strchr(s, '.')) return;
    size_t n = strlen(s);
    while (n && s[n - 1] == '0') s[--n] = 0;
    if (n && s[n - 1] == '.') s[--n] = 0;
}

static void fmt_num(double x, int sig, int trim, char *out, size_t cap) {
    if (isnan(x)) { snprintf(out, cap, "NaN"); return; }
    if (isinf(x)) { snprintf(out, cap, x > 0 ? "∞" : "-∞"); return; }
    if (x == 0) { snprintf(out, cap, "0"); return; }
    if (trim && x == (double)(long long)x && fabs(x) < 1e7 && sig >= 6) {
        snprintf(out, cap, "%lld", (long long)x);
        return;
    }
    if (sig < 1) sig = 1;
    if (sig > 17) sig = 17;
    char tmp[64];
    snprintf(tmp, sizeof tmp, "%.*e", sig - 1, x);
    double r = strtod(tmp, NULL);
    char *epos = strchr(tmp, 'e');
    int ex = atoi(epos + 1);
    if (ex >= -4 && ex < 6) {
        int dec = sig - 1 - ex;
        if (dec < 0) dec = 0;
        snprintf(out, cap, "%.*f", dec, r);
        if (trim) trim_zeros(out);
        return;
    }
    *epos = 0;                      /* mantissa text, already rounded */
    if (trim) trim_zeros(tmp);
    char e[32], sup[96] = "";
    snprintf(e, sizeof e, "%d", ex);
    for (char *p = e; *p; p++) strcat(sup, *p == '-' ? "⁻" : SUP[*p - '0']);
    snprintf(out, cap, "%s×10%s", tmp, sup);
}

static int attached(const char *u) {
    return !strcmp(u, "°") || !strcmp(u, "%") || !strcmp(u, "′") || !strcmp(u, "″");
}

static void fmt_value(const fm_fmt *f, double v, int sig_default, char *out, size_t cap) {
    double x = (v - f->offset) / f->factor;
    if (f->sf < 0) fmt_num(x, sig_default, 1, out, cap);
    else fmt_num(x, f->direct ? f->sf : (f->sf > 2 ? f->sf : 2), 0, out, cap);
}

void fm_print_num(int64_t fid, double v) {
    const fm_fmt *f = &fm_fmts[fid];
    char buf[128], all[256];
    fmt_value(f, v, 6, buf, sizeof buf);
    if (!f->unit[0] || !strcmp(f->unit, "1")) snprintf(all, sizeof all, "%s", buf);
    else if (attached(f->unit)) snprintf(all, sizeof all, "%s%s", buf, f->unit);
    else snprintf(all, sizeof all, "%s %s", buf, f->unit);
    emit(all);
}

static void print_seq(int64_t fid, const double *p, int64_t n, const char *open, const char *close) {
    const fm_fmt *f = &fm_fmts[fid];
    static char out[1 << 15];
    char buf[128];
    int sf = f->sf;
    if (sf >= 0 && !f->direct && sf < 2) sf = 2;
    strcpy(out, open);
    for (int64_t i = 0; i < n; i++) {
        if (n > 12 && i == 5) { strcat(out, "…, "); i = n - 3; }
        double x = (p[i] - f->offset) / f->factor;
        if (sf < 0) fmt_num(x, n > 12 ? 4 : 6, 1, buf, sizeof buf);
        else fmt_num(x, sf, 0, buf, sizeof buf);
        strcat(out, buf);
        if (i + 1 < n) strcat(out, ", ");
    }
    strcat(out, close);
    if (f->unit[0] && strcmp(f->unit, "1")) { strcat(out, " "); strcat(out, f->unit); }
    if (n > 12 && !strcmp(open, "[")) { char c[48]; snprintf(c, sizeof c, "  (%lld values)", (long long)n); strcat(out, c); }
    emit(out);
}

void fm_print_list(int64_t fid, double *p, int64_t n) { print_seq(fid, p, n, "[", "]"); }
void fm_print_vec(int64_t fid, double *p, int64_t n) { print_seq(fid, p, n, "<", ">"); }

/* a vector with a unit per component, <1 m, 2 m/s>: formats fid, fid+1, ... (D29) */
void fm_print_mvec(int64_t fid, double *p, int64_t n) {
    static char out[4096];
    char buf[128];
    strcpy(out, "<");
    for (int64_t i = 0; i < n; i++) {
        const fm_fmt *f = &fm_fmts[fid + i];
        fmt_value(f, p[i], 6, buf, sizeof buf);
        strcat(out, buf);
        if (f->unit[0] && strcmp(f->unit, "1")) {
            if (!attached(f->unit)) strcat(out, " ");
            strcat(out, f->unit);
        }
        if (i + 1 < n) strcat(out, ", ");
    }
    strcat(out, ">");
    emit(out);
}

/* a matrix, rows on one line: [[1, 2], [3, 4]] N/m */
void fm_print_mat(int64_t fid, double *p, int64_t r, int64_t c) {
    const fm_fmt *f = &fm_fmts[fid];
    static char out[4096];
    char buf[128];
    int sf = f->sf;
    if (sf >= 0 && !f->direct && sf < 2) sf = 2;
    strcpy(out, "[");
    for (int64_t i = 0; i < r; i++) {
        strcat(out, "[");
        for (int64_t j = 0; j < c; j++) {
            double x = (p[i * c + j] - f->offset) / f->factor;
            if (sf < 0) fmt_num(x, 6, 1, buf, sizeof buf);
            else fmt_num(x, sf, 0, buf, sizeof buf);
            strcat(out, buf);
            if (j + 1 < c) strcat(out, ", ");
        }
        strcat(out, i + 1 < r ? "], " : "]");
    }
    strcat(out, "]");
    if (f->unit[0] && strcmp(f->unit, "1")) { strcat(out, " "); strcat(out, f->unit); }
    emit(out);
}
void fm_print_textlist(double *p, int64_t n) {
    static char out[1 << 15];
    strcpy(out, "[");
    for (int64_t i = 0; i < n; i++) {
        strcat(out, fm_texts[(int64_t)p[i]]);
        if (i + 1 < n) strcat(out, ", ");
    }
    strcat(out, "]");
    emit(out);
}

void fm_print_bool(int64_t b) { emit(b ? "true" : "false"); }
void fm_print_text(int64_t i) { emit(fm_texts[i]); }

void fm_print_end(void) {
    fputs(line, stdout);
    fputc('\n', stdout);
    line_len = 0;
    line[0] = 0;
}

/* the name of a solve's independent variable (a text id), for ODE errors */
static const char *tname(double b) { return b >= 0 && b == b ? fm_texts[(int64_t)b] : "t"; }

void fm_error(int64_t kind, double a, double b, int64_t ln, int64_t fmt) {
    char x[160], y[160];
    int with_units = kind == 2 || kind == 3 || kind == 8 || kind == 13 || kind == 14 || kind >= 16;
    if (with_units && fmt >= 0) {       /* values with their units, like the Python runtime */
        const fm_fmt *f = &fm_fmts[fmt];
        char bx[128], by[128];
        fmt_num((a - f->offset) / f->factor, 6, 1, bx, sizeof bx);
        fmt_num((b - f->offset) / f->factor, 6, 1, by, sizeof by);
        const char *sep = (f->unit[0] && strcmp(f->unit, "1")) ? " " : "";
        snprintf(x, sizeof x, "%s%s%s", bx, sep, f->unit);
        snprintf(y, sizeof y, "%s%s%s", by, sep, f->unit);
    } else {
        fmt_num(a, 6, 1, x, sizeof x);
        fmt_num(b, 6, 1, y, sizeof y);
    }
    err_line = ln;
    if (kind == -1) return;            /* ERR_PENDING: load/fit/plot already wrote err_msg */
    switch (kind) {
    case 1:
        if (a != a)
            snprintf(err_msg, sizeof err_msg, "a list index must be a whole number (1, 2, 3, ...), not NaN");
        else if (!isinf(a) && a != floor(a))
            snprintf(err_msg, sizeof err_msg, "a list index must be a whole number (1, 2, 3, ...), not %s", x);
        else if ((long long)b == 0)
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list is empty", x);
        else
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list has %lld element%s (valid indexes are 1 to %lld)",
                     x, (long long)b, (long long)b == 1 ? "" : "s", (long long)b);
        break;
    case 2: snprintf(err_msg, sizeof err_msg, "asked for the solution at %s%s, outside the range it was solved for (it ends at %s)", x, fmt >= 0 ? "" : " (SI units)", y); break;
    case 3: snprintf(err_msg, sizeof err_msg, "the ODE solver needed too many steps (reached %s = %s%s); the equation may be stiff or blow up", tname(b), x, fmt >= 0 ? "" : " (SI units)"); break;
    case 4: snprintf(err_msg, sizeof err_msg, "%s", fm_texts[(int64_t)a]); break;
    case 5: snprintf(err_msg, sizeof err_msg, "these two lists have different lengths (%s and %s)", x, y); break;
    case 6: snprintf(err_msg, sizeof err_msg, "this list is empty"); break;
    case 7: snprintf(err_msg, sizeof err_msg, "the step must be a non-zero number that goes from the start towards the end"); break;
    case 8: snprintf(err_msg, sizeof err_msg, "the ODE solver's step became too small near %s = %s%s; the solution may blow up there", tname(b), x, fmt >= 0 ? "" : " (SI units)"); break;
    case 16: snprintf(err_msg, sizeof err_msg, "the right side of the equation is NaN or infinite at %s = %s (0/0? 1/0?); if the equation is singular there, start slightly away from %s", tname(b), x, x); break;
    case 17: snprintf(err_msg, sizeof err_msg, "the range of %s is empty: it starts and ends at %s", tname(b), x); break;
    case 18: snprintf(err_msg, sizeof err_msg, "%s%s; make the range longer", fm_texts[(int64_t)b], x); break;
    case 9: snprintf(err_msg, sizeof err_msg, "this integral doesn't converge: the integrand may blow up (like 1/x at 0) or keep oscillating (like sin(x) up to ∞)"); break;
    case 10: snprintf(err_msg, sizeof err_msg, "%s called itself too many times (the program ran out of stack) -- is a base case missing?", a >= 0 ? fm_texts[(int64_t)a] : "a function"); break;
    case 11:
        if (a != a) snprintf(err_msg, sizeof err_msg, "the length of a list must be a number, not NaN");
        else snprintf(err_msg, sizeof err_msg, "not enough memory for a list of %s numbers (the most is 10⁹)", x);
        break;
    case 14: snprintf(err_msg, sizeof err_msg, "the two sides of this equation jump past each other near %s (like tan at 90 degrees) instead of crossing: that's not a solution; narrow the range", x); break;
    case 13: snprintf(err_msg, sizeof err_msg, "this equation has no solution between %s and %s: the two sides never cross there (checked at 200 points)", x, y); break;
    case 12: snprintf(err_msg, sizeof err_msg, "this for loop has no definite number of steps: it goes from %s to %s (NaN in the start, end or step)", x, y); break;
    case 15: snprintf(err_msg, sizeof err_msg, "this matrix is singular (its determinant is 0), so it has no inverse and M x = b has no unique solution"); break;
    default: snprintf(err_msg, sizeof err_msg, "runtime error"); break;
    }
}

/* a warning found while the program runs (#36) */
void fm_warn(int64_t kind, double a, int64_t ln, int64_t fmt) {
    char x[160];
    if (fmt >= 0) {
        const fm_fmt *f = &fm_fmts[fmt];
        char bx[128];
        fmt_num((a - f->offset) / f->factor, 6, 1, bx, sizeof bx);
        snprintf(x, sizeof x, "%s%s%s", bx, (f->unit[0] && strcmp(f->unit, "1")) ? " " : "", f->unit);
    } else {
        fmt_num(a, 6, 1, x, sizeof x);
    }
    if (kind == 1)
        fprintf(stderr, "warning: line %lld: the two sides of this equation agree only to rounding error near %s, so the solution found there may be meaningless (large terms cancelling?); rewrite the equation so they cancel on paper\n", (long long)ln, x);
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    if (x != x || y != y) return (x != x) - (y != y);     /* NaN last */
    return (x > y) - (x < y);
}

void fm_sort(double *p, int64_t n) { qsort(p, (size_t)n, sizeof(double), cmp_double); }

double fm_clock(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec * 1e-9;
}

#include "aot_data.c"

#include <pthread.h>
static int run_result;
static void *runner(void *arg) { (void)arg; run_result = fm_run(); return NULL; }

int main(void) {
    /* run on a thread with a big stack, like `fermium run` (deep recursion is caught before 400 MB) */
    pthread_attr_t attr;
    pthread_t th;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, (size_t)512 << 20);
    int r;
    if (pthread_create(&th, &attr, runner, NULL) == 0) { pthread_join(th, NULL); r = run_result; }
    else r = fm_run();
    if (line_len) fm_print_end();
    fflush(stdout);
    if (r != 0) {
        if (err_line > 0) fprintf(stderr, "line %lld: %s\n", (long long)err_line, err_msg);
        else fprintf(stderr, "%s\n", err_msg);
        return 1;
    }
    return 0;
}
