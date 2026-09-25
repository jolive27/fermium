# Solutions — Lesson 12

[Back to Lesson 12](../lesson12_lab_report.md)

## 1. A density

```fermium
m = 245.3 ± 0.1 g
d = 2.54 ± 0.01 cm
h = 5.08 ± 0.02 cm
ρ = m / (π (d/2)² h)
print "ρ =", ρ in g/cm³
print "relative:", rel(m) to 2 digits, 2 rel(d) to 2 digits, rel(h) to 2 digits
```

<!-- output -->
```
ρ = 9.530 ± 0.084 g/cm³
relative: 0.00041 0.0079 0.0039
```

The diameter limits the result: it is known to 0.4 %, but it's squared, so it contributes 0.8 %, twice as much as the height. The balance (0.04 %) hardly matters. To do better, measure d more precisely (a micrometer instead of a ruler).

## 2. Radioactive decay

```fermium
R0 = 1250 ± 35 s⁻¹
R = 410 ± 20 s⁻¹
t = 30.0 ± 0.1 min
t_half = t ln(2) / ln(R0 / R)
print "half-life:", t_half in min
```

<!-- output -->
```
half-life: 18.65 ± 0.94 min
```

The two count rates dominate; the ±0.1 min of the clock is negligible. Counting for longer (more counts, so a smaller relative uncertainty) is how to improve it.

## 3. Monte Carlo versus linear

```fermium
m = 0.500 ± 0.005 kg
v = 0.20 ± 0.15 m/s
print "linear rule:", ½ m v²
propagate montecarlo
    E = ½ m v²
print "Monte Carlo:", E
```

<!-- output -->
```
linear rule: 0.010 ± 0.015 J
Monte Carlo: 0.016 ± 0.017 J
```

With v uncertain by 75 %, v² is far from a straight line. A speed that is too high by 0.15 m/s gives a much bigger energy than a speed that is too low by 0.15 m/s takes away (and a negative v still gives a positive v²). So the average of v² is v² + σ_v² = (0.04 + 0.0225) m²/s², which makes the Monte Carlo mean about 1.56 times the linear value: ½ × 0.5 kg × 0.0625 m²/s² = 0.0156 J. The linear rule is only trustworthy when the relative uncertainties are small.
