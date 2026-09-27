// A small C++ library for Fermium's  import cpp  tests (spec C4, DECISIONS D290). tests/c4_cases.rs builds it
// as libphys4.so next to the programs:  c++ -std=c++17 -O2 -shared -fPIC -o libphys4.so phys.cpp
// Units at this boundary are whatever the Fermium signatures declare; the comments say what each expects.
#pragma once
#include <stdexcept>

namespace phys {

double kinetic_energy(double m, double v);          // ½ m v²
// three overloads of one name: the declared Fermium signature picks one
double energy(double m);                            // m c²
double energy(double m, double v);                  // γ m c²
float energy(float m);                              // never chosen: Fermium passes doubles
int twice(int n);
double sum_sq(const double *x, int n);              // Σ x²
double scale_sum(double *x, int n, double f);       // a non-const pointer: Σ f x (scales its copy in place)
double checked_sqrt(double x);                      // throws std::domain_error for x < 0
double throws_int(double x);                        // throws an int, not a std::exception
double lambda_max(double T);                        // Wien's displacement law: b / T

template <class T> T cube(T x) { return x * x * x; } // header only: a template, deduced from the signature
inline double half(double x) { return x / 2; }       // header only: inline

struct Particle {
    double m;
    static double compton_wavelength(double m);     // h / (m c): a static member function
    double rest_energy() const { return m * 299792458.0 * 299792458.0; } // non-static: can't be imported
};

namespace nested { namespace deeper { double third(double x); } }
namespace detail { double declared_only(double x); } // declared but defined nowhere: a link error

} // namespace phys
