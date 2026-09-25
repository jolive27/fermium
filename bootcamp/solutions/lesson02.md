# Solutions — Lesson 2

[Back to Lesson 2](../lesson02_variables_formulas.md)

## 1. Projectile range

```fermium
v0 = 20 m/s
theta = 45 deg
g = 9.81 m/s^2
R = v0^2 sin(2 theta) / g
print R
```

<!-- output -->
```
40.8 m
```

## 2. Kinetic energy of a car

Use `½` (or `0.5 * m`), not `0.5 m`, which would mean half a metre:

```fermium
m = 1500 kg
v = 100 km/hr
KE = ½ m v^2
print KE in kJ
```

<!-- output -->
```
579 kJ
```

## 3. Escape velocity

```fermium
v_esc = sqrt(2 G M_earth / R_earth)
print v_esc in km/s
```

<!-- output -->
```
11.2 km/s
```

(`2 G` is safe: `G` isn't a unit name in Fermium; gauss is written `gauss`. If in doubt, write `2 * G`.)

## 4. Swap

```fermium
a = 3 m
b = 5 m
temp = a
a = b
b = temp
print a, b
```

<!-- output -->
```
5 m 3 m
```

If you write `a = b` then `b = a`, both end up as 5 m: the old value of `a` is lost after the first line. That's why you need `temp`.

## 5. Spot the bug

`h / m_e * v` means (h / mₑ) × v, because `*` and `/` are done left to right. Fermium notices that the result isn't a length when you ask for `in nm`:

```
can't show a quantity with units [m³/s²] in nm (length [m])
```

Fix it with parentheses, or with implicit multiplication (which binds tighter than `/`):

```fermium
v = 0.01 * c
lam = h / (m_e * v)
print lam in nm
lam2 = h / m_e v
print lam2 in nm
```

<!-- output -->
```
0.24 nm
0.24 nm
```
