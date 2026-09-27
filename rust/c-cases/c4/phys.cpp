// The definitions for phys.hpp (Fermium's  import cpp  tests, spec C4).
#include "phys.hpp"

#include <cmath>

namespace phys {

static const double C = 299792458.0;        // m/s (exact)
static const double H = 6.62607015e-34;     // J s (exact)
static const double WIEN_B = 2.897771955e-3; // m K (CODATA 2018)

double kinetic_energy(double m, double v) { return 0.5 * m * v * v; }
double energy(double m) { return m * C * C; }
double energy(double m, double v) { return m * C * C / std::sqrt(1 - v * v / (C * C)); }
float energy(float m) { return -1.0f; }
int twice(int n) { return 2 * n; }
double sum_sq(const double *x, int n) {
    double s = 0;
    for (int i = 0; i < n; i++) s += x[i] * x[i];
    return s;
}
double scale_sum(double *x, int n, double f) {
    double s = 0;
    for (int i = 0; i < n; i++) {
        x[i] *= f;
        s += x[i];
    }
    return s;
}
double checked_sqrt(double x) {
    if (x < 0) throw std::domain_error("checked_sqrt of a negative number");
    return std::sqrt(x);
}
double throws_int(double x) {
    if (x > 1) throw 42;
    return x;
}
double lambda_max(double T) { return WIEN_B / T; }
double Particle::compton_wavelength(double m) { return H / (m * C); }
namespace nested { namespace deeper { double third(double x) { return x / 3; } } }

} // namespace phys
