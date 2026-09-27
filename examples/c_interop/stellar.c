/* stellar.c - the Gamow peak of a thermonuclear reaction, in C, for Fermium's C/Fortran interop example
 * (examples/c_interop/c_interop.fm).
 *
 * Two nuclei with charges Z1, Z2 and reduced mass mu meet in a plasma at temperature T. The reaction rate
 * integrand is the Maxwell-Boltzmann factor times the Coulomb-barrier tunnelling probability:
 *
 *     f(E) = exp(-E / kT - sqrt(E_G / E)),        E_G = 2 mu c^2 (pi alpha Z1 Z2)^2  (the Gamow energy)
 *
 * It peaks at the Gamow peak E0 = (E_G (kT)^2 / 4)^(1/3), with a 1/e width Delta = 4 sqrt(E0 kT / 3), and
 * the area under it is close to sqrt(pi) Delta / 2 exp(-3 E0 / kT) (the Gaussian approximation).
 * References: D. D. Clayton, "Principles of Stellar Evolution and Nucleosynthesis" (McGraw-Hill, 1968),
 * ch. 4 (thermonuclear reaction rates); C. Iliadis, "Nuclear Physics of Stars", 2nd ed. (Wiley-VCH, 2015),
 * sec. 3.2 (nonresonant rates for a constant S-factor). For p + p in the Sun's core (T = 15.7 MK) E0 is
 * about 6 keV.
 *
 * Written for Fermium; MIT license, like the rest of the repository.
 *
 * Build:  cc -O2 -shared -fPIC -o libstellar.so stellar.c -lm
 *
 * Units at this boundary: energies in keV, masses in atomic mass units (u), temperatures in kelvin.
 * Fermium converts to and from these units at every call, as the  import c  block declares.
 */
#include <math.h>

static const double MU_C2_KEV = 931494.10242;         /* m_u c^2 in keV (CODATA 2018) */
static const double ALPHA = 1.0 / 137.035999084;      /* fine-structure constant (CODATA 2018) */
static const double K_B_KEV = 8.617333262e-8;         /* Boltzmann constant in keV/K (exact since 2019) */

/* the Gamow energy E_G in keV */
double gamow_energy(int z1, int z2, double mu_u) {
    double x = M_PI * ALPHA * z1 * z2;
    return 2.0 * mu_u * MU_C2_KEV * x * x;
}

/* the Gamow peak E0 in keV */
double gamow_peak(int z1, int z2, double mu_u, double t_k) {
    double kt = K_B_KEV * t_k;
    return cbrt(gamow_energy(z1, z2, mu_u) * kt * kt / 4.0);
}

/* the 1/e full width of the Gamow window in keV */
double gamow_width(int z1, int z2, double mu_u, double t_k) {
    double kt = K_B_KEV * t_k;
    return 4.0 * sqrt(gamow_peak(z1, z2, mu_u, t_k) * kt / 3.0);
}

/* the rate integrand exp(-E/kT - sqrt(E_G/E)) at energy e_kev (a plain number) */
double gamow_integrand(double e_kev, int z1, int z2, double mu_u, double t_k) {
    double kt = K_B_KEV * t_k;
    return exp(-e_kev / kt - sqrt(gamow_energy(z1, z2, mu_u) / e_kev));
}

/* the trapezoidal rule: the integral of y over x, for n points */
double trapezoid(const double *x, const double *y, int n) {
    double s = 0.0;
    for (int i = 1; i < n; i++)
        s += 0.5 * (x[i] - x[i - 1]) * (y[i] + y[i - 1]);
    return s;
}
