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
    int direct;   /* 1: value written directly as a literal (3: a written list with exact whole items); 2: `to N digits`;
                     4/5: a loop variable over a written list, printed as the list prints it (5: exact whole items, D242) */
    const char *unit;
} fm_fmt;

extern const fm_fmt fm_fmts[];
extern const char *fm_texts0[];
extern const long long fm_ntexts0;
/* the text table: the program's texts, then those made while it runs ("3p" + "1/2", str(x)) (D216) */
static const char **fm_textv = NULL;
static int64_t fm_ntext = 0, fm_captext = 0;
static const char **fm_texts_(void) {
    if (!fm_textv) {
        fm_captext = fm_ntexts0 + 64;
        fm_textv = malloc(sizeof(char *) * fm_captext);
        memcpy(fm_textv, fm_texts0, sizeof(char *) * fm_ntexts0);
        fm_ntext = fm_ntexts0;
    }
    return fm_textv;
}
#define fm_texts (fm_texts_())
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

#define FM_DEFAULT_SF 3   /* output precision when the inputs don't say (DECISIONS D11) */

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
    if (ex >= -4 && ex < 6 && (trim || ex <= sig)) {   /* units.format_number: 3.33×10⁵, not 333000 */
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

/* a value whose precision the program doesn't give (D11): whole numbers below 10⁷ exactly, else
   FM_DEFAULT_SF significant figures with trailing zeros kept; mirrors units.format_default */
static int is_whole(double x) {     /* units._whole: allows for rounding in the last bits */
    return x == 0 || (x == x && fabs(x) < 1e7 && round(x) != 0 && fabs(x - round(x)) <= 1e-13 * fabs(x));
}

static void fmt_default(double x, int whole_ok, char *out, size_t cap) {
    if (whole_ok && is_whole(x)) { fmt_num(round(x), 17, 1, out, cap); return; }
    fmt_num(x, FM_DEFAULT_SF, 0, out, cap);
}

/* an element of a list written out in the program (core.format_written): with the list's significant
   figures if that shows it exactly, else as written */
static void fmt_written(double x, int sf, int exact_items, char *out, size_t cap) {
    char t[64];
    snprintf(t, sizeof t, "%.*e", sf > 1 ? sf - 1 : 0, x);
    if (exact_items && isfinite(x) && fabs(x) < 1e7 && x == trunc(x)) { snprintf(out, cap, "%lld", (long long)x); return; }
    if (isfinite(x) && x != 0 && fabs(strtod(t, NULL) - x) > 1e-13 * fabs(x)) fmt_num(x, 15, 1, out, cap);
    else fmt_num(x, sf, 0, out, cap);
}

/* one number style for a whole list/vector/matrix (units.format_default_seq): whole numbers exactly only
   if every printed element is one; with skip, an n > 12 list prints elements 0-4 and n-3..n-1 */
static int all_whole(const fm_fmt *f, const double *p, int64_t n, int skip) {
    for (int64_t i = 0; i < n; i++) {
        if (skip && n > 12 && i == 5) i = n - 3;
        double x = (p[i] - f->offset) / f->factor;
        if (isfinite(x) && !is_whole(x)) return 0;      /* NaN and ∞ don't decide the style */
    }
    return 1;
}

static int attached(const char *u) {
    return !strcmp(u, "°") || !strcmp(u, "%") || !strcmp(u, "′") || !strcmp(u, "″");
}

static void fmt_value_w(const fm_fmt *f, double v, int sig_default, int whole_ok, char *out, size_t cap);

static void fmt_value(const fm_fmt *f, double v, int sig_default, char *out, size_t cap) {
    fmt_value_w(f, v, sig_default, 1, out, cap);
}

static void fmt_value_w(const fm_fmt *f, double v, int sig_default, int whole_ok, char *out, size_t cap) {
    double x = (v - f->offset) / f->factor;
    if (f->sf < 0) {
        if (f->direct && isfinite(x) && fabs(x) < 1e15 && x == trunc(x)) snprintf(out, cap, "%lld", (long long)x);
        else if (f->direct) fmt_num(x, sig_default, 1, out, cap);
        else fmt_default(x, whole_ok, out, cap);
    }
    else if (f->direct == 4 || f->direct == 5) fmt_written(x, f->sf, f->direct == 5, out, cap);   /* D242 */
    else fmt_num(x, f->direct ? f->sf : (f->sf > 2 ? f->sf : 2), 0, out, cap);
}

static void num_text(int64_t fid, double v, char *all, size_t cap) {
    const fm_fmt *f = &fm_fmts[fid];
    char buf[128];
    fmt_value(f, v, 15, buf, sizeof buf);
    if (!f->unit[0] || !strcmp(f->unit, "1")) snprintf(all, cap, "%s", buf);
    else if (attached(f->unit)) snprintf(all, cap, "%s%s", buf, f->unit);
    else snprintf(all, cap, "%s %s", buf, f->unit);
}

void fm_print_num(int64_t fid, double v) {
    char all[256];
    num_text(fid, v, all, sizeof all);
    emit(all);
}

/* a text made while the program runs; the same text keeps one id (D216) */
static int64_t text_add(const char *s) {
    const char **t = fm_texts_();
    for (int64_t i = fm_ntexts0; i < fm_ntext; i++)
        if (!strcmp(t[i], s)) return i;
    if (fm_ntext == fm_captext) {
        fm_captext *= 2;
        fm_textv = realloc(fm_textv, sizeof(char *) * fm_captext);
    }
    char *c = malloc(strlen(s) + 1);
    strcpy(c, s);
    fm_textv[fm_ntext] = c;
    return fm_ntext++;
}

int64_t fm_text_concat(int64_t a, int64_t b) {
    const char *x = fm_texts[a], *y = fm_texts[b];
    size_t n = strlen(x), m = strlen(y);
    char *s = malloc(n + m + 1);
    memcpy(s, x, n);
    memcpy(s + n, y, m + 1);
    int64_t r = text_add(s);
    free(s);
    return r;
}

int64_t fm_text_num(int64_t fid, double v) {
    char all[256];
    num_text(fid, v, all, sizeof all);
    return text_add(all);
}

/* a complex number, 3 + 4i or (3 + 4i) Ω; mirrors fermium.cplx.format_complex (D94) */
static void fmt_part(const fm_fmt *f, double v, int whole, char *out, size_t cap) {
    if (f->sf < 0) {
        if (f->direct) fmt_num(v, 15, 1, out, cap);
        else if (whole) snprintf(out, cap, "%lld", (long long)llround(v));
        else fmt_num(v, 3, 0, out, cap);        /* 3 significant figures per part, zeros kept (D94) */
    }
    else fmt_num(v, f->direct ? f->sf : (f->sf > 2 ? f->sf : 2), 0, out, cap);
}

void fm_print_cplx(int64_t fid, double re, double im) {
    const fm_fmt *f = &fm_fmts[fid];
    double x = re / f->factor, y = im / f->factor, size = hypot(x, y);
    char bx[128], by[128], body[300], all[400];
    if (isfinite(size) && size > 0) {       /* a part below 1e-14 |z| is rounding noise */
        if (fabs(x) < 1e-14 * size) x = 0;
        if (fabs(y) < 1e-14 * size) y = 0;
    }
    int whole = is_whole(x) && is_whole(y);
    fmt_part(f, x, whole, bx, sizeof bx);
    fmt_part(f, fabs(y), whole, by, sizeof by);
    snprintf(body, sizeof body, "%s %s %si", bx, y < 0 ? "-" : "+", by);
    if (!f->unit[0] || !strcmp(f->unit, "1")) snprintf(all, sizeof all, "%s", body);
    else if (attached(f->unit)) snprintf(all, sizeof all, "(%s)%s", body, f->unit);
    else snprintf(all, sizeof all, "(%s) %s", body, f->unit);
    emit(all);
}

/* a list of complex numbers, (re, im) interleaved: [3 + 0i, -1 + 1i] V; mirrors fermium.clist.format_clist
   (D243): per element noise as in fm_print_cplx, one number style for the shown elements, 5 … 3 above 12 */
void fm_print_clist(int64_t fid, double *p, int64_t n) {
    const fm_fmt *f = &fm_fmts[fid];
    static char out[1 << 15];
    char bx[128], by[128], body[300];
    int64_t idx[16], m = 0;
    for (int64_t i = 0; i < n; i++) {
        if (n > 12 && i == 5) i = n - 3;
        idx[m++] = i;
    }
    double xs[16], ys[16];
    int whole = 1;
    for (int64_t j = 0; j < m; j++) {
        double x = p[2 * idx[j]] / f->factor, y = p[2 * idx[j] + 1] / f->factor, size = hypot(x, y);
        if (isfinite(size) && size > 0) {
            if (fabs(x) < 1e-14 * size) x = 0;
            if (fabs(y) < 1e-14 * size) y = 0;
        }
        xs[j] = x; ys[j] = y;
        if (!is_whole(x) || !is_whole(y)) whole = 0;
    }
    strcpy(out, "[");
    for (int64_t j = 0; j < m; j++) {
        if (n > 12 && j == 5) strcat(out, "…, ");
        fmt_part(f, xs[j], whole, bx, sizeof bx);
        fmt_part(f, fabs(ys[j]), whole, by, sizeof by);
        snprintf(body, sizeof body, "%s %s %si", bx, ys[j] < 0 ? "-" : "+", ys[j] == ys[j] ? by : "NaN");
        strcat(out, body);
        if (j + 1 < m) strcat(out, ", ");
    }
    strcat(out, "]");
    if (f->unit[0] && strcmp(f->unit, "1")) { strcat(out, " "); strcat(out, f->unit); }
    if (n > 12) { char c[48]; snprintf(c, sizeof c, "  (%lld values)", (long long)n); strcat(out, c); }
    emit(out);
}

/* a computed vector's or matrix's entries below 1e-14 of its largest are rounding noise: printed as 0
   (core.denoise, D197); q receives the n values */
static void denoise(const fm_fmt *f, const double *p, int64_t n, double *q) {
    double big = 0;
    for (int64_t i = 0; i < n; i++) {
        q[i] = p[i];
        if (isfinite(p[i]) && fabs(p[i]) > big) big = fabs(p[i]);
    }
    (void)f;
    return;     /* disabled, as in core.denoise (red team round 6 #1, #2: a small entry isn't always noise) */
    for (int64_t i = 0; i < n; i++)
        if (isfinite(q[i]) && fabs(q[i]) < 1e-14 * big) q[i] = 0.0;
}

static void print_seq(int64_t fid, const double *p, int64_t n, const char *open, const char *close) {
    const fm_fmt *f = &fm_fmts[fid];
    static char out[1 << 15];
    char buf[128];
    int sf = f->sf;
    int list = !strcmp(open, "[");          /* only lists are shortened: vectors have at most 16 components */
    if (sf >= 0 && !f->direct && sf < 2) sf = 2;
    int whole = sf < 0 && all_whole(f, p, n, list);
    strcpy(out, open);
    for (int64_t i = 0; i < n; i++) {
        if (list && n > 12 && i == 5) { strcat(out, "…, "); i = n - 3; }
        double x = (p[i] - f->offset) / f->factor;
        if (sf < 0) fmt_num(whole ? round(x) : x, whole ? 17 : FM_DEFAULT_SF, whole, buf, sizeof buf);
        else if (f->direct == 1 || f->direct == 3) fmt_written(x, sf, f->direct == 3, buf, sizeof buf);
        else fmt_num(x, sf, 0, buf, sizeof buf);
        strcat(out, buf);
        if (i + 1 < n) strcat(out, ", ");
    }
    strcat(out, close);
    if (f->unit[0] && strcmp(f->unit, "1")) { strcat(out, " "); strcat(out, f->unit); }
    if (n > 12 && list) { char c[48]; snprintf(c, sizeof c, "  (%lld values)", (long long)n); strcat(out, c); }
    emit(out);
}

void fm_print_list(int64_t fid, double *p, int64_t n) { print_seq(fid, p, n, "[", "]"); }
void fm_print_vec(int64_t fid, double *p, int64_t n) {
    double q[256];
    denoise(&fm_fmts[fid], p, n, q);
    print_seq(fid, q, n, "<", ">");
}

/* a vector with a unit per component, <1 m, 2 m/s>: formats fid, fid+1, ... (D29) */
void fm_print_mvec(int64_t fid, double *p, int64_t n) {
    static char out[4096];
    char buf[128];
    int whole = 1;              /* one number style for all components (core.print_mvec, D11) */
    for (int64_t i = 0; i < n; i++) {
        const fm_fmt *f = &fm_fmts[fid + i];
        double x = (p[i] - f->offset) / f->factor;
        if (f->sf < 0 && !f->direct && isfinite(x) && !is_whole(x)) whole = 0;
    }
    strcpy(out, "<");
    for (int64_t i = 0; i < n; i++) {
        const fm_fmt *f = &fm_fmts[fid + i];
        fmt_value_w(f, p[i], 15, whole, buf, sizeof buf);
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
void fm_print_mat(int64_t fid, double *p0, int64_t r, int64_t c) {
    const fm_fmt *f = &fm_fmts[fid];
    static char out[1 << 15];               /* up to 16×16 entries (D195) */
    double p[256];
    denoise(f, p0, r * c, p);
    char buf[128];
    int sf = f->sf;
    if (sf >= 0 && !f->direct && sf < 2) sf = 2;
    int whole = sf < 0 && all_whole(f, p, r * c, 0);
    strcpy(out, "[");
    for (int64_t i = 0; i < r; i++) {
        strcat(out, "[");
        for (int64_t j = 0; j < c; j++) {
            double x = (p[i * c + j] - f->offset) / f->factor;
            if (sf < 0) fmt_num(whole ? round(x) : x, whole ? 17 : FM_DEFAULT_SF, whole, buf, sizeof buf);
            else if (f->direct == 1 || f->direct == 3) fmt_written(x, sf, f->direct == 3, buf, sizeof buf);
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
    int with_units = kind == 2 || kind == 3 || kind == 8 || kind == 13 || kind == 14 || (kind >= 16 && !(kind >= 24 && kind <= 26));
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
    if (kind >= 1000000 && kind < 2000000) {
        /* too many steps: a = the place reached, b = the start, the variable's name in the kind; enough digits
           to tell the two apart (2499.4 s, not 2500 s next to a start of 2500 s) (D214) */
        const char *nm = tname((double)(kind - 1000001));
        for (int sig = 3; sig <= 15; sig++) {
            char bx[128], by[128];
            if (fmt >= 0) {
                const fm_fmt *f = &fm_fmts[fmt];
                const char *sep = (f->unit[0] && strcmp(f->unit, "1")) ? " " : "";
                fmt_num((a - f->offset) / f->factor, sig, 1, bx, sizeof bx);
                fmt_num((b - f->offset) / f->factor, sig, 1, by, sizeof by);
                snprintf(x, sizeof x, "%s%s%s", bx, sep, f->unit);
                snprintf(y, sizeof y, "%s%s%s", by, sep, f->unit);
            } else {
                fmt_num(a, sig, 1, bx, sizeof bx);
                fmt_num(b, sig, 1, by, sizeof by);
                snprintf(x, sizeof x, "%s (SI units)", bx);
                snprintf(y, sizeof y, "%s (SI units)", by);
            }
            if (strcmp(x, y) != 0) break;
        }
        snprintf(err_msg, sizeof err_msg, "the ODE solver needed too many steps (20 million: it got from %s = %s only to %s = %s); if the equation is stiff (time scales far apart, like a 164 \xce\xbcs half-life in a chain followed for hours), add  using radau  after the range; otherwise the solution may blow up", nm, y, nm, x);
        return;
    }
    switch (kind) {
    case 1:
        if (a != a)
            snprintf(err_msg, sizeof err_msg, "a list index must be a whole number (1, 2, 3, ...), not NaN");
        else if ((long long)b < 0 && !isinf(a) && a != floor(a))
            snprintf(err_msg, sizeof err_msg, "an index must be a whole number (1, 2, 3, ...), not %s", x);
        else if ((long long)b < 0)
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: valid indexes here are 1 to %lld", x, -(long long)b);
        else if (!isinf(a) && a != floor(a))
            snprintf(err_msg, sizeof err_msg, "a list index must be a whole number (1, 2, 3, ...), not %s", x);
        else if ((long long)b == 0)
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list is empty", x);
        else
            snprintf(err_msg, sizeof err_msg, "index %s is out of range: the list has %lld element%s (valid indexes are 1 to %lld)",
                     x, (long long)b, (long long)b == 1 ? "" : "s", (long long)b);
        break;
    case 2: snprintf(err_msg, sizeof err_msg, "asked for the solution at %s%s, outside the range it was solved for (it ends at %s)", x, fmt >= 0 ? "" : " (SI units)", y); break;
    case 3: snprintf(err_msg, sizeof err_msg, "the ODE solver needed too many steps (reached %s = %s%s); if the equation is stiff (time scales far apart, like a 164 \xce\xbcs half-life in a chain followed for hours), add  using radau  after the range; otherwise the solution may blow up", tname(b), x, fmt >= 0 ? "" : " (SI units)"); break;
    case 4: snprintf(err_msg, sizeof err_msg, "%s", fm_texts[(int64_t)a]); break;
    case 5: snprintf(err_msg, sizeof err_msg, "these two lists have different lengths (%s and %s)", x, y); break;
    case 6: snprintf(err_msg, sizeof err_msg, "this list is empty"); break;
    case 25: snprintf(err_msg, sizeof err_msg, "the number of samples must be a whole number, 0 or more, not %s", x); break;
    case 26: snprintf(err_msg, sizeof err_msg, "randn(\xce\xbc, \xcf\x83): \xcf\x83 is a standard deviation, so it can't be negative"); break;
    case 24: snprintf(err_msg, sizeof err_msg, "std needs at least 2 values: it is the sample standard deviation, which divides by N \xe2\x88\x92 1, so one value says nothing about the spread (quote the instrument's uncertainty for a single measurement)"); break;
    case 7: snprintf(err_msg, sizeof err_msg, "the step must be a non-zero number that goes from the start towards the end"); break;
    case 8: snprintf(err_msg, sizeof err_msg, "the ODE solver's step became too small near %s = %s%s; the solution may blow up there", tname(b), x, fmt >= 0 ? "" : " (SI units)"); break;
    case 34: snprintf(err_msg, sizeof err_msg, "the ODE solver's step became too small near %s = %s%s; no unknown has grown there, so this is probably not a blow-up: the error control asks for relative accuracy on values that are tiny or rounding noise (like an abundance of 10\xe2\x81\xbb\xc2\xb2\xc2\xb3); add an absolute tolerance after the range, e.g.  tolerance 1e-9 absolute 1e-16  (in the units of the unknowns)", tname(b), x, fmt >= 0 ? "" : " (SI units)"); break;
    case 16: snprintf(err_msg, sizeof err_msg, "the right side of the equation is NaN or infinite at %s = %s (0/0? 1/0?); if the equation is singular there, start slightly away from %s", tname(b), x, x); break;
    case 17: snprintf(err_msg, sizeof err_msg, "the range of %s is empty: it starts and ends at %s", tname(b), x); break;
    case 33: snprintf(err_msg, sizeof err_msg, "%s%s: the matrix of their coefficients is singular (a zero mass or length?)", fm_texts[(int64_t)b], x); break;
    case 18: snprintf(err_msg, sizeof err_msg, "%s%s; make the range longer", fm_texts[(int64_t)b], x); break;
    case 31: snprintf(err_msg, sizeof err_msg, "the integrand is NaN at %s = %s (0/0? ∞/∞? an overflow like exp(710)?), so this integral can't be computed; rewrite the integrand so it stays finite there, e.g. exp(x) / (exp(x) - 1)² as exp(-x) / (1 - exp(-x))², or 1 - cos(x) as 2 sin(x/2)²", (b == b && b >= 0) ? fm_texts[(int64_t)b] : "x", x); break;
    case 32: snprintf(err_msg, sizeof err_msg, "couldn't compute this integral: the integrand is infinite at %s = %s (1/0? an overflow like exp(710)?), so it may blow up there (like 1/x at 0); if it shouldn't, rewrite it so it stays finite, e.g. 1 - cos(x) as 2 sin(x/2)²", (b == b && b >= 0) ? fm_texts[(int64_t)b] : "x", x); break;
    case 9: snprintf(err_msg, sizeof err_msg, "couldn't compute this integral numerically: it may diverge (like 1/x at 0) or oscillate without decaying (like sin(x)/x up to ∞), or the integrand is NaN or ∞ somewhere"); break;
    case 10: snprintf(err_msg, sizeof err_msg, "%s called itself too many times (the program ran out of stack) -- is a base case missing?", a >= 0 ? fm_texts[(int64_t)a] : "a function"); break;
    case 11:
        if (a != a) snprintf(err_msg, sizeof err_msg, "the length of a list must be a number, not NaN");
        else snprintf(err_msg, sizeof err_msg, "not enough memory for a list of %s numbers (the most is 10⁹)", x);
        break;
    case 14: snprintf(err_msg, sizeof err_msg, "the two sides of this equation jump past each other near %s (like tan at 90 degrees) instead of crossing: that's not a solution; narrow the range", x); break;
    case 13: snprintf(err_msg, sizeof err_msg, "this equation has no solution between %s and %s: the two sides never cross there (checked at 200 points)", x, y); break;
    case 40: snprintf(err_msg, sizeof err_msg, "%s are the same list (one was set from the other), so the iterations of this parallel for would write and read the same numbers at the same time; make a copy first, e.g.  ys = xs * 1", fm_texts[(int64_t)a]); break;
    case 12: snprintf(err_msg, sizeof err_msg, "this for loop has no definite number of steps: it goes from %s to %s (NaN in the start, end or step)", x, y); break;
    case 15: snprintf(err_msg, sizeof err_msg, "this matrix is singular (its determinant is 0), so it has no inverse and M x = b has no unique solution"); break;
    case 21: snprintf(err_msg, sizeof err_msg, "eigenvalues and eigenvectors need a symmetric matrix (M[i, j] = M[j, i]), like a stiffness or mass matrix; for K v = ω² M v write eigenvalues(K, M) rather than eigenvalues(inverse(M) K)"); break;
    case 22: snprintf(err_msg, sizeof err_msg, "in eigenvalues(K, M) the second matrix M must be positive definite, like a mass matrix (positive masses on the diagonal)"); break;
    default: snprintf(err_msg, sizeof err_msg, "runtime error"); break;
    }
}

/* a warning found while the program runs (#36) */
/* A run-time line code (errors.py, D185): code from a module carries (k << 20) | line and the calling program
   line << 32, with k - 1 the text id of the module's file name.  Sets *line to the program line (0: none) and
   suf to " (in stats.fm, line 6)" or "". */
static void fm_where(int64_t code, int64_t *line, char *suf, size_t n) {
    suf[0] = 0;
    if (code <= 0xFFFFF) { *line = code; return; }
    int64_t low = code & 0xFFFFFFFFLL, k = low >> 20, ml = low & 0xFFFFF;
    *line = code >> 32;
    snprintf(suf, n, " (in %s, line %lld)", k > 0 ? fm_texts[k - 1] : "a module", (long long)ml);
}

/* A warning is printed once per text, as the JIT's Runtime.warn does (it keeps the texts it has shown): the
   zero-integral warning of a function called in a loop comes once, not once per call (red team round 3 #12). */
static int fm_warn_seen(const char *text) {
    static uint64_t seen[1024];
    static int nseen = 0;
    uint64_t h = 1469598103934665603ULL;
    for (const unsigned char *p = (const unsigned char *)text; *p; p++) { h ^= *p; h *= 1099511628211ULL; }
    for (int i = 0; i < nseen; i++)
        if (seen[i] == h) return 1;
    if (nseen < 1024) seen[nseen++] = h;
    return 0;
}

void fm_warn(int64_t kind, double a, int64_t ln, int64_t fmt) {
    char x[160], msg[800], where[200], suf[200];
    int64_t pl;
    fm_where(ln, &pl, suf, sizeof suf);
    if (pl > 0) snprintf(where, sizeof where, "line %lld: ", (long long)pl); else where[0] = 0;
    if (fmt >= 0) {
        const fm_fmt *f = &fm_fmts[fmt];
        char bx[128];
        fmt_num((a - f->offset) / f->factor, 6, 1, bx, sizeof bx);
        snprintf(x, sizeof x, "%s%s%s", bx, (f->unit[0] && strcmp(f->unit, "1")) ? " " : "", f->unit);
    } else {
        fmt_num(a, 6, 1, x, sizeof x);
    }
    msg[0] = 0;
    if (kind == 1)
        snprintf(msg, sizeof msg, "warning: %sthe two sides of this equation agree only to rounding error near %s, so the solution found there may be meaningless (large terms cancelling?); rewrite the equation so they cancel on paper%s\n", where, x, suf);
    else if (kind == 2)
        snprintf(msg, sizeof msg, "warning: %sthis equation looks stiff: rk45 has taken %lld steps, held small by stability rather than accuracy (time scales far apart); add  using radau  after the range for an implicit solver made for this%s\n", where, (long long)a, suf);
    else if (kind == 3)
        snprintf(msg, sizeof msg, "warning: %sthis integral came out as exactly 0 because the integrand was 0 at every point where it was sampled; if it is non-zero somewhere narrow (a peak in a wide range), integrate over a range that fits it%s\n", where, suf);
    else if (kind == 7) {
        static int64_t warned_line = -1;       /* once per solve, not once per loop pass */
        char pc[64];
        if (ln == warned_line) return;
        warned_line = ln;
        fmt_num(a * 100, 2, 1, pc, sizeof pc);
        fprintf(stderr, "warning: %sthe step is too coarse for this equation: the estimated error is %s%% of the solution's size (fixed-step RK4, checked by step doubling); use a smaller step, or drop  step  to use the adaptive solver%s\n", where, pc, suf);
        return;
    }
    if (msg[0] && !fm_warn_seen(msg))
        fputs(msg, stderr);
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
        int64_t pl;
        char suf[200];
        fm_where(err_line, &pl, suf, sizeof suf);
        if (pl > 0) fprintf(stderr, "line %lld: %s%s\n", (long long)pl, err_msg, suf);
        else fprintf(stderr, "%s%s\n", err_msg, suf);
        return 1;
    }
    return 0;
}

/* ---- Fourier transforms (DECISIONS D81): the same results as runtime/spectral.py (NumPy) up to rounding.
 * Radix-2 for powers of two, Bluestein's chirp-z (through a power-of-two FFT) for other lengths. */
static void fft_pow2(double *re, double *im, int64_t n, int inverse) {
    for (int64_t i = 1, j = 0; i < n; i++) {
        int64_t bit = n >> 1;
        for (; j & bit; bit >>= 1) j ^= bit;
        j ^= bit;
        if (i < j) {
            double t = re[i]; re[i] = re[j]; re[j] = t;
            t = im[i]; im[i] = im[j]; im[j] = t;
        }
    }
    for (int64_t len = 2; len <= n; len <<= 1) {
        double ang = 2 * M_PI / (double)len * (inverse ? 1 : -1);
        for (int64_t i = 0; i < n; i += len) {
            for (int64_t k = 0; k < len / 2; k++) {
                double wr = cos(ang * (double)k), wi = sin(ang * (double)k);
                double ur = re[i + k], ui = im[i + k];
                double xr = re[i + k + len / 2], xi = im[i + k + len / 2];
                double vr = xr * wr - xi * wi, vi = xr * wi + xi * wr;
                re[i + k] = ur + vr; im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr; im[i + k + len / 2] = ui - vi;
            }
        }
    }
}

static void fft_any(double *re, double *im, int64_t n, int inverse) {
    if ((n & (n - 1)) == 0) { fft_pow2(re, im, n, inverse); return; }
    int64_t m = 1;
    while (m < 2 * n - 1) m <<= 1;
    double *ar = calloc((size_t)m, 8), *ai = calloc((size_t)m, 8), *br = calloc((size_t)m, 8),
           *bi = calloc((size_t)m, 8), *wr = malloc((size_t)n * 8), *wi = malloc((size_t)n * 8);
    double sg = inverse ? 1.0 : -1.0;
    for (int64_t k = 0; k < n; k++) {
        double a = M_PI * (double)((k * k) % (2 * n)) / (double)n;   /* e^{sg·iπk²/n}, k² reduced mod 2n */
        wr[k] = cos(a); wi[k] = sg * sin(a);
        ar[k] = re[k] * wr[k] - im[k] * wi[k];
        ai[k] = re[k] * wi[k] + im[k] * wr[k];
    }
    br[0] = wr[0]; bi[0] = -wi[0];
    for (int64_t k = 1; k < n; k++) {
        br[k] = br[m - k] = wr[k];
        bi[k] = bi[m - k] = -wi[k];
    }
    fft_pow2(ar, ai, m, 0);
    fft_pow2(br, bi, m, 0);
    for (int64_t k = 0; k < m; k++) {
        double r = ar[k] * br[k] - ai[k] * bi[k], i = ar[k] * bi[k] + ai[k] * br[k];
        ar[k] = r; ai[k] = i;
    }
    fft_pow2(ar, ai, m, 1);
    for (int64_t k = 0; k < n; k++) {
        double r = ar[k] / (double)m, i = ai[k] / (double)m;
        re[k] = r * wr[k] - i * wi[k];
        im[k] = r * wi[k] + i * wr[k];
    }
    free(ar); free(ai); free(br); free(bi); free(wr); free(wi);
}

int64_t fm_fft(int64_t kind, double *a, double *b, int64_t n, double dt, double *out) {
    double *re = malloc((size_t)n * 8), *im = malloc((size_t)n * 8);
    if (kind >= 5) {    /* fft/ifft with complex results, (re, im) interleaved (spectral.py, D243) */
        int cin = kind == 6 || kind == 7, inv = kind == 6 || kind == 8;
        for (int64_t i = 0; i < n; i++) { re[i] = cin ? a[2 * i] : a[i]; im[i] = cin ? a[2 * i + 1] : 0.0; }
        fft_any(re, im, n, inv);
        for (int64_t i = 0; i < n; i++) {
            out[2 * i] = inv ? re[i] / (double)n : re[i];
            out[2 * i + 1] = inv ? im[i] / (double)n : im[i];
        }
        free(re); free(im);
        return 0;
    }
    for (int64_t i = 0; i < n; i++) { re[i] = a[i]; im[i] = (kind == 4 && b) ? b[i] : 0.0; }
    fft_any(re, im, n, kind == 4);
    if (kind == 0 || kind == 1) {
        for (int64_t i = 0; i < n; i++) out[i] = kind == 0 ? re[i] : im[i];
    } else if (kind == 4) {
        for (int64_t i = 0; i < n; i++) out[i] = re[i] / (double)n;
    } else {
        for (int64_t k = 0; k <= n / 2; k++) {
            double w = (k == 0 || (n % 2 == 0 && k == n / 2)) ? 1.0 : 2.0;
            double mag2 = re[k] * re[k] + im[k] * im[k];
            out[k] = kind == 2 ? w * sqrt(mag2) / (double)n : w * mag2 * dt / (double)n;
        }
    }
    free(re); free(im);
    return 0;
}
