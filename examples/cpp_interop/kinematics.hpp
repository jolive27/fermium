// kinematics.hpp - relativistic decay kinematics in C++, for Fermium's C++ interop example
// (examples/cpp_interop/cpp_interop.fm, spec C4).
//
// Particle-physics units at this boundary: masses in MeV/c^2, energies in MeV, momenta in MeV/c, lifetimes in
// seconds, lengths in metres. Fermium converts to and from these units at every call, as the  import cpp  block
// declares, and checks each argument's dimension before the program runs.
//
// References: Particle Data Group, R. L. Workman et al., Prog. Theor. Exp. Phys. 2022, 083C01, "Kinematics"
// (sec. 49.4.2: two-body decays; sec. 49.4.1: invariant mass); masses and lifetimes from the same Review.
//
// Written for Fermium; MIT license, like the rest of the repository.
//
// Build:  c++ -std=c++17 -O2 -shared -fPIC -o libkinematics.so kinematics.cpp
#pragma once
#include <stdexcept>

namespace kin {

// A particle of mass M at rest decays into two of masses m1 and m2.
struct TwoBody {
    // the momentum of each daughter (MeV/c); throws std::domain_error when M < m1 + m2
    static double momentum(double M, double m1, double m2);
    // the energy of daughter 1 (MeV)
    static double energy(double M, double m1, double m2);
};

// the Lorentz factor of a particle of mass m and momentum p
double gamma(double p, double m);

// the mean distance it travels before decaying in the lab: beta gamma c tau (m)
double decay_length(double p, double m, double tau);

// the invariant mass (MeV/c^2), three overloads:
double invariant_mass(double E, double p);                                            // one particle
double invariant_mass(double E1, double p1, double E2, double p2, double cos_theta);  // two, at an angle
double invariant_mass(const double *E, const double *px, const double *py, const double *pz, int n); // n

} // namespace kin
