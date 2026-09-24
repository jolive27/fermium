# Solutions — Lesson 8

[Back to Lesson 8](../lesson08_integrals.md)

## 1. Area

```fermium
print integral sin(x)^2 dx from 0 to pi
print pi / 2
```

<!-- output -->
```
1.5708
1.5708
```

Exactly π/2 (sin² averages to ½ over a half-period of length π).

## 2. Gravitational work

```fermium
m_sat = 1000 kg
F(r) = G M_earth m_sat / r^2
W = integral F(r) dr from R_earth to inf
print W
print ½ m_sat (11.2 km/s)^2
```

<!-- output -->
```
6.24955×10¹⁰ J
6.27×10¹⁰ J
```

They match (to the precision of 11.2 km/s): escape velocity is exactly the speed whose kinetic energy pays for this work.

## 3. Charging a capacitor

```fermium
I(t) = 2 mA * exp(-t / 3 s)
print "first 10 s:", integral I(t) dt from 0 s to 10 s in mC
print "total:     ", integral I(t) dt from 0 s to inf in mC
```

<!-- output -->
```
first 10 s: 5.78596 mC
total:      6 mC
```

The total is I₀τ = 2 mA × 3 s = 6 mC.

## 4. Hot plate

```fermium
L = 50 cm
T(x) = 300 K + 400 K/m * x
print (integral T(x) dx from 0 m to L) / L
```

<!-- output -->
```
400 K
```

For a straight line the average is the value in the middle: 300 K + 400 K/m × 0.25 m = 400 K.

## 5. Wien's peak

```fermium
T = 5778 K
B(lam) = 2 h c^2 / lam^5 / (exp(h c / (lam k_B T)) - 1)
best = 100 nm
for lam from 100 nm to 2000 nm step 1 nm
    if B(lam) > B(best)
        best = lam
print "loop:", best
print "Wien:", b_W / T in nm
```

<!-- output -->
```
loop: 502 nm
Wien: 501.518 nm
```
