# Solutions — Lesson 9

[Back to Lesson 9](../lesson09_differential_equations.md)

## 1. Carbon dating

```fermium
tau = 8267 yr
solve N' = -N / tau
  with N(0) = 1
  for t from 0 yr to 20000 yr
print "after 5730 years: ", N(5730 yr)
print "after 10000 years:", N(10000 yr)
```

<!-- output -->
```
after 5730 years:  0.500015
after 10000 years: 0.298308
```

Half is left after one half-life, as it should be. (You can also write the equation exactly as in the exercise: `solve dN/dt = -N / tau`.)

## 2. Big swings

A `solve` can go inside a loop, so we solve once per amplitude. The period is the time at which θ′ changes sign from positive to negative (the pendulum is back at its starting angle):

```fermium
g = 9.81 m/s^2
L = 1 m
for amp in [10 deg, 45 deg, 90 deg]
    solve theta'' = -(g / L) sin(theta)
      with theta(0) = amp, theta'(0) = 0 rad/s
      for t from 0 s to 5 s
    T = 0.01 s
    while not (theta'(T - 1 ms) > 0 rad/s and theta'(T) <= 0 rad/s)
        T += 1 ms
    print "amplitude", amp, "period", T to 4 digits
print "small-angle formula:", 2 pi sqrt(L / g) to 4 digits
```

<!-- output -->
```
amplitude 10° period 2.010 s
amplitude 45° period 2.087 s
amplitude 90° period 2.368 s
small-angle formula: 2.006 s
```

At 10° the formula is almost perfect; at 90° the real period is 18% longer.

## 3. Falling with air resistance

```fermium
mass = 80 kg
g = 9.81 m/s^2
rho = 1.2 kg/m^3
C = 1.0
A = 0.7 m^2
solve mass v' = mass g - ½ rho C A v^2
  with v(0) = 0 m/s
  for t from 0 s to 30 s
print "speed after 30 s:", v(30 s), "=", v(30 s) in km/hr
print "terminal velocity:", sqrt(2 mass g / (rho C A)) to 4 digits
```

<!-- output -->
```
speed after 30 s: 43.2269 m/s = 155.617 km/hr
terminal velocity: 43.23 m/s
```

## 4. Charging a capacitor

```fermium
R = 10 kohm
C = 100 uF
V0 = 5 V
solve R C V' = V0 - V
  with V(0) = 0 V
  for t from 0 s to 5 s
print "RC =", R C
print "V at t = RC:", V(R C), "which is", V(R C) / V0, "of V0"
```

<!-- output -->
```
RC = 1 s
V at t = RC: 3.1606 V which is 0.632121 of V0
```

1 − 1/e = 0.632.

## 5. Driven oscillator

```fermium
k = 50 N/m
mass = 0.5 kg
b = 0.4 kg/s
F0 = 1 N
omega_d = 10 rad/s
solve mass x'' = -k x - b x' + F0 cos(omega_d t)
  with x(0) = 0 m, x'(0) = 0 m/s
  for t from 0 s to 30 s
print "largest amplitude:", max(x)
print "theory at resonance, F0 / (b omega):", F0 / (b omega_d)
plot x vs t to "driven.png"
```

<!-- output -->
```
largest amplitude: 0.249998 m
theory at resonance, F0 / (b omega): 0.25 m
plot saved to driven.png
```

The amplitude grows and then levels off at F₀/(bω): **resonance**. With less damping (smaller b) it would grow much larger.
