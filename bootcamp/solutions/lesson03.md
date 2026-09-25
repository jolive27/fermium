# Solutions — Lesson 3

[Back to Lesson 3](../lesson03_functions.md)

## 1. Gravity

```fermium
F_grav(m1, m2, r) = G m1 m2 / r^2
print F_grav(M_earth, 70 kg, R_earth)
print 70 kg * 9.81 m/s^2
```

<!-- output -->
```
686 N
687 N
```

They agree: your weight *is* the gravitational pull of the Earth.

## 2. Nuclear radius

```fermium
R(A) = 1.2 fm * A^(1/3)
print "carbon-12: ", R(12) in fm
print "iron-56:   ", R(56) in fm
print "uranium-238:", R(238) in fm
```

<!-- output -->
```
carbon-12:  2.7 fm
iron-56:    4.6 fm
uranium-238: 7.4 fm
```

A uranium nucleus has 20 times the mass of a carbon nucleus but is less than 3 times as wide: nuclei all have about the same density.

## 3. Projectile

```fermium
proj_range(v0, theta) = v0^2 sin(2 theta) / g where g = 9.81 m/s^2
print "30 deg:", proj_range(20 m/s, 30 deg)
print "45 deg:", proj_range(20 m/s, 45 deg)
print "60 deg:", proj_range(20 m/s, 60 deg)
```

<!-- output -->
```
30 deg: 35.3 m
45 deg: 40.8 m
60 deg: 35.3 m
```

45° goes furthest, and 30° and 60° go equally far (because sin 60° = sin 120°).

## 4. Temperature of a star

`b` has units of m K, written `2.898e-3 m K`. Because there's a space before the `/`, `/ T` divides by your parameter `T` (Lesson 1). Written without the space, `m K/T` would mean "kelvin per **tesla**".

```fermium
peak(T) = 2.898e-3 [m K] / T
print "Sun:  ", peak(5778 K) in nm
print "human:", peak(310 K) in um
```

<!-- output -->
```
Sun:   501.6 nm
human: 9.348 μm
```

Giving the constant its own name is even clearer, and it can't go wrong:

```fermium
b = 2.898e-3 m K
peak(T) = b / T
print peak(5778 K) in nm
```

<!-- output -->
```
501.6 nm
```

The Sun peaks in visible light (green-blue); you glow in the infrared, which is what thermal cameras see.

## 5. Multi-line function

```fermium
fall_time(h) =
    g = 9.81 m/s^2
    t = sqrt(2 * h / g)
    return t

print fall_time(330 m)
```

<!-- output -->
```
8.20 s
```

(Ignoring air resistance, which matters a lot for a real fall of 330 m!)
