// kinematics.cpp - the definitions for kinematics.hpp (Fermium's C++ interop example, spec C4).
// Build:  c++ -std=c++17 -O2 -shared -fPIC -o libkinematics.so kinematics.cpp
#include "kinematics.hpp"

#include <cmath>

namespace kin {

static const double C = 299792458.0; // m/s (exact)

// Källén's triangle function
static double lambda(double a, double b, double c) { return a * a + b * b + c * c - 2 * (a * b + b * c + c * a); }

double TwoBody::momentum(double M, double m1, double m2) {
    if (M < m1 + m2) throw std::domain_error("the decay is kinematically forbidden (M < m1 + m2)");
    return std::sqrt(lambda(M * M, m1 * m1, m2 * m2)) / (2 * M);
}

double TwoBody::energy(double M, double m1, double m2) { return (M * M + m1 * m1 - m2 * m2) / (2 * M); }

double gamma(double p, double m) { return std::sqrt(1 + (p / m) * (p / m)); }

double decay_length(double p, double m, double tau) { return p / m * C * tau; }

double invariant_mass(double E, double p) { return std::sqrt(E * E - p * p); }

double invariant_mass(double E1, double p1, double E2, double p2, double cos_theta) {
    double s = (E1 + E2) * (E1 + E2) - (p1 * p1 + p2 * p2 + 2 * p1 * p2 * cos_theta);
    return std::sqrt(s);
}

double invariant_mass(const double *E, const double *px, const double *py, const double *pz, int n) {
    double e = 0, x = 0, y = 0, z = 0;
    for (int i = 0; i < n; i++) {
        e += E[i];
        x += px[i];
        y += py[i];
        z += pz[i];
    }
    return std::sqrt(e * e - x * x - y * y - z * z);
}

} // namespace kin
