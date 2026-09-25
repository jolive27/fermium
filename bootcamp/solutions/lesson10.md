# Solutions — Lesson 10 (project extensions)

[Back to Lesson 10](../lesson10_final_project.md)

## 1. Mars

Same program as version 3, with Mars's distance and speed, stopping after one lap:

```fermium
GM = G M_sun
dt = 1 hr
x = 1.524 AU
y = 0 AU
vx = 0 km/s
vy = 24.07 km/s
t = 0 s
period = 0 s
while period == 0 s
    r = sqrt(x^2 + y^2)
    vx += -GM x / r^3 * dt
    vy += -GM y / r^3 * dt
    y_before = y
    x += vx dt
    y += vy dt
    t += dt
    if y_before < 0 AU and y >= 0 AU
        period = t
print "simulated period:", period in yr
print "Kepler's third law:", 1.524^1.5, "yr"
```

<!-- output -->
```
simulated period: 1.87 yr
Kepler's third law: 1.881 yr
```

(The real Martian year is 1.88 Earth years.) The simulation gives a slightly shorter year because 24.07 km/s is a bit below the speed needed for a circle at 1.524 AU (24.13 km/s), so our Mars falls into a slightly smaller ellipse. Try 24.13 km/s!

## 2. Escape!

The energy per kilogram, E = ½v² − GM/r, decides: negative means bound (it comes back), zero or positive means it escapes.

```fermium
GM = G M_sun
r = 1 AU
for v in [40 km/s, 42 km/s, 44 km/s]
    E = ½ v^2 - GM / r
    if E < 0 J/kg
        print v, "bound, E =", E
    else
        print v, "escapes, E =", E
print "escape velocity:", sqrt(2 GM / r) in km/s
```

<!-- output -->
```
40 km/s bound, E = -8.71×10⁷ J/kg
42 km/s bound, E = -5.13×10⁶ J/kg
44 km/s escapes, E = 8.09×10⁷ J/kg
escape velocity: 42.1 km/s
```

The escape velocity from the Earth's orbit is √2 times the orbital speed: 42.1 km/s.

## 3. Time step study

Putting the whole simulation in a function lets us call it with different time steps:

```fermium
GM = G M_sun
orbit_period(dt) =
    x = 1 AU
    y = 0 AU
    vx = 0 km/s
    vy = 29.78 km/s
    t = 0 s
    period = 0 s
    while period == 0 s
        r = sqrt(x^2 + y^2)
        vx += -GM x / r^3 * dt
        vy += -GM y / r^3 * dt
        y_before = y
        x += vx dt
        y += vy dt
        t += dt
        if y_before < 0 AU and y >= 0 AU
            period = t
    period

for dt in [1 day, 6 hr, 1 hr, 10 min]
    print "dt =", dt in min, "period =", orbit_period(dt) in day to 7 digits
```

<!-- output -->
```
dt = 1440 min period = 366.0000 day
dt = 360 min period = 365.2500 day
dt = 60 min period = 365.1250 day
dt = 10 min period = 365.0903 day
```

The answer settles down as dt shrinks. Most of the error with a big step is simply that we only notice the crossing at the end of a step, so the period is rounded up to a whole number of steps. The error is roughly proportional to dt. (The converged value, 365.09 days, is slightly less than a real year because 29.78 km/s is a rounded starting speed.)

## 4. A function for the force

```fermium
GM = G M_sun
ax(x, y) = -GM x / (x^2 + y^2)^1.5
ay(x, y) = -GM y / (x^2 + y^2)^1.5

dt = 1 hr
x = 1 AU
y = 0 AU
vx = 0 km/s
vy = 29.78 km/s
for n from 1 to 24 * 365
    vx += ax(x, y) dt
    vy += ay(x, y) dt
    x += vx dt
    y += vy dt
print "after 365 days:", x in AU, y in AU
```

<!-- output -->
```
after 365 days: 1.00 AU -0.001453 AU
```

Now, to try a different force law (say, 1/r³ instead of 1/r²), you only change two lines at the top.

## 5 and 6: challenges

These are left for you. Hints:
- **Kepler's second law:** inside the loop, compute `dA = ½ abs(x vy - y vx) dt` and `push` it to a list. Then compare `min` and `max` of the list: they should be almost the same.
- **Jupiter:** give Jupiter its own `xj, yj, vxj, vyj`, move it with the Sun's gravity only, and add a second acceleration term to the Earth: `-G M_J (x - xj) / d^3` where `d` is the Earth–Jupiter distance. Jupiter's pull changes the Earth's orbit only very slightly.
