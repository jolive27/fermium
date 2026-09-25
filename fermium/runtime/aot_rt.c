/* Fermium runtime for standalone executables built with `fermium build`.
 *
 * The compiled program calls these functions for printing and error reporting,
 * exactly like the JIT'd program calls the Python callbacks in runtime/core.py.
 * Number formatting follows fermium.units.format_number.
 * The format and text tables are generated per program (fm_tables.c).
 */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

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

void fm_error(int64_t kind, double a, double b, int64_t ln, int64_t fmt) {
    char x[160], y[160];
    if (kind == 2 && fmt >= 0) {       /* values with their units, like the Python runtime */
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
    switch (kind) {
    case 1:
        if (a == a && a != floor(a))
            snprintf(err_msg, sizeof err_msg, "a list index must be a whole number (1, 2, 3, ...), not %s", x);
        else if ((long long)b == 0)
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list is empty", x);
        else
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list has %lld elements (valid indexes are 1 to %lld)",
                     x, (long long)b, (long long)b);
        break;
    case 2: snprintf(err_msg, sizeof err_msg, "asked for the solution at %s%s, outside the range it was solved for (it ends at %s)", x, fmt >= 0 ? "" : " (SI units)", y); break;
    case 3: snprintf(err_msg, sizeof err_msg, "the ODE solver needed too many steps (reached t = %s in SI units)", x); break;
    case 4: snprintf(err_msg, sizeof err_msg, "%s", fm_texts[(int64_t)a]); break;
    case 5: snprintf(err_msg, sizeof err_msg, "these two lists have different lengths (%s and %s)", x, y); break;
    case 6: snprintf(err_msg, sizeof err_msg, "this list is empty"); break;
    case 7: snprintf(err_msg, sizeof err_msg, "the step must be a non-zero number that goes from the start towards the end"); break;
    case 8: snprintf(err_msg, sizeof err_msg, "the ODE solver's step became too small near t = %s (SI units)", x); break;
    case 9: snprintf(err_msg, sizeof err_msg, "this integral doesn't converge: the integrand may blow up (like 1/x at 0) or keep oscillating (like sin(x) up to ∞)"); break;
    default: snprintf(err_msg, sizeof err_msg, "runtime error"); break;
    }
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

void fm_sort(double *p, int64_t n) { qsort(p, (size_t)n, sizeof(double), cmp_double); }

double fm_clock(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec * 1e-9;
}

int main(void) {
    int r = fm_run();
    if (line_len) fm_print_end();
    fflush(stdout);
    if (r != 0) {
        if (err_line > 0) fprintf(stderr, "line %lld: %s\n", (long long)err_line, err_msg);
        else fprintf(stderr, "%s\n", err_msg);
        return 1;
    }
    return 0;
}
