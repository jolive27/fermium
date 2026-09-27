! nuclear.f90 - the semi-empirical (Bethe-Weizsaecker) mass formula, in Fortran, for Fermium's
! C/Fortran interop example (examples/c_interop/c_interop.fm).
!
!   B(Z, A) = aV A - aS A^(2/3) - aC Z (Z - 1) / A^(1/3) - aA (A - 2Z)^2 / A + delta(Z, A)
!   delta   = +aP / sqrt(A) (even Z, even N), -aP / sqrt(A) (odd Z, odd N), 0 (odd A)
!
! Coefficients: the least-squares fit quoted by R. A. Rohlf, "Modern Physics from alpha to Z0"
! (Wiley, 1994), ch. 11: aV = 15.75, aS = 17.8, aC = 0.711, aA = 23.7, aP = 11.18 MeV.
! (The same values as Fermium's stdlib nuclear.semf_binding, so the two can be compared.)
!
! Written for Fermium; MIT license, like the rest of the repository.
!
! Build:  gfortran -O2 -shared -fPIC -o libnuclear.so nuclear.f90
!
! Fortran passes every argument by reference, and gfortran names an external function in lowercase with a
! trailing underscore (binding_energy -> binding_energy_). bind(C) keeps the lowercase name; bind(C, name=...)
! sets it. Fermium's  import fortran  follows the same conventions.

! binding energy in MeV (symbol binding_energy_)
real(8) function binding_energy(Z, A)
    implicit none
    integer, intent(in) :: Z, A
    real(8), parameter :: aV = 15.75d0, aS = 17.8d0, aC = 0.711d0, aA = 23.7d0, aP = 11.18d0
    real(8) :: rA, delta
    rA = real(A, 8)
    if (mod(A, 2) == 1) then
        delta = 0d0
    else if (mod(Z, 2) == 0) then
        delta = aP / sqrt(rA)
    else
        delta = -aP / sqrt(rA)
    end if
    binding_energy = aV * rA - aS * rA**(2d0 / 3d0) - aC * Z * (Z - 1) / rA**(1d0 / 3d0) &
                     - aA * real(A - 2 * Z, 8)**2 / rA + delta
end function binding_energy

! binding energy per nucleon in MeV (bind(C): symbol binding_per_nucleon)
real(8) function binding_per_nucleon(Z, A) bind(C)
    use iso_c_binding, only: c_int
    implicit none
    integer(c_int), intent(in) :: Z, A
    real(8), external :: binding_energy
    binding_per_nucleon = binding_energy(Z, A) / real(A, 8)
end function binding_per_nucleon

! neutron separation energy S_n = B(Z, A) - B(Z, A - 1) in MeV (symbol semf_sn)
real(8) function neutron_separation(Z, A) bind(C, name="semf_sn")
    use iso_c_binding, only: c_int
    implicit none
    integer(c_int), intent(in) :: Z, A
    real(8), external :: binding_energy
    neutron_separation = binding_energy(Z, A) - binding_energy(Z, A - 1)
end function neutron_separation

! the most bound proton number for a mass number A, from dB/dZ = 0 (symbol most_stable_z_)
integer function most_stable_z(A)
    implicit none
    integer, intent(in) :: A
    real(8), parameter :: aC = 0.711d0, aA = 23.7d0
    real(8) :: rA
    rA = real(A, 8)
    most_stable_z = nint((4d0 * aA + aC / rA**(1d0 / 3d0)) * rA / (8d0 * aA + 2d0 * aC * rA**(2d0 / 3d0)))
end function most_stable_z
