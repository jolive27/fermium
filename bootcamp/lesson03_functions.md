# Lesson 3 — Functions

In physics you constantly use functions: the position x(t), the force F(x), the potential V(r). A **function** in programming is the same idea: a rule that takes some input and gives back an output. Once you've defined it, you can use it as many times as you like.

## Your first function

```fermium
square(x) = x^2
print square(3)
print square(3 m)
print square(2 s)
```

<!-- output -->
```
9
9 m²
4 s²
```

The first line **defines** the function: "`square` of something called `x` is `x^2`". Nothing is calculated yet. The `x` is a placeholder, called a **parameter**. Then `square(3)` **calls** the function: Fermium puts 3 in place of `x` and works out the answer.

Notice that the same function worked for a plain number, a length and a time. You never have to tell Fermium what units `x` has; it checks each call separately.

## Physics functions

A spring's force obeys Hooke's law, F(x) = −kx:

```fermium
k = 50 N/m
F(x) = -k x
print F(0.1 m)
print F(-2 cm)
```

<!-- output -->
```
-5.0 N
1 N
```

A function can use variables defined outside it, like `k` here. Functions can have several parameters, separated by commas:

```fermium
KE(m, v) = ½ m v²
print KE(2 kg, 3 m/s)
print KE(9.11e-31 kg, 1e6 m/s) in eV
```

<!-- output -->
```
9 J
2.84 eV
```

Inside `KE`, `m` is the parameter (a mass). No number comes right before `m`, so it is the mass, as on paper. In ASCII, write `KE(m, v) = (1/2) m v^2`.

You can use `where` to give a function its own constants:

```fermium
period(L) = 2 pi sqrt(L / g) where g = 9.81 m/s^2
print period(1 m)
print period(25 cm)
```

<!-- output -->
```
2.01 s
1.00 s
```

## Functions of time

Here's a ball thrown straight up at 20 m/s. Its height and velocity are functions of time:

```fermium
v0 = 20 m/s
g = 9.81 m/s^2
y(t) = v0 t - ½ g t²
v(t) = v0 - g t
print "after 1 s:", y(1 s), v(1 s)
print "after 3 s:", y(3 s), v(3 s)
print y
```

<!-- output -->
```
after 1 s: 15.1 m 10.2 m/s
after 3 s: 15.9 m -9.43 m/s
y(t) = v0 t - 0.5·g t²   [m, for t in s]
```

`print y` (without calling it) shows the function's formula, and in brackets the units it returns (m) for the units it takes (s). Fermium worked out by itself that `t` must be a time: `v0 t - ½ g t²` only makes sense if `t` is in seconds.

## Functions that call functions

Functions can use other functions. The energy levels of hydrogen are Eₙ = −13.6 eV / n², and a photon emitted when the electron drops from level n₂ to n₁ carries the difference:

```fermium
energy(n) = -13.6 eV / n^2
photon(n1, n2) = energy(n2) - energy(n1)
E = photon(2, 3)
print E in eV
print "wavelength:", h c / E in nm
```

<!-- output -->
```
1.89 eV
wavelength: 656 nm
```

656 nm is the famous red H-alpha line of hydrogen! The answer is shown in eV because that's the unit we used inside `energy`. `in eV` makes sure of it.

## Longer functions

When a function needs several steps, put them on the following lines, **indented** (pushed to the right) by 4 spaces. The last line is the result, or you can say `return` explicitly:

```fermium
# How fast is a ball going after falling from height h?
impact_speed(h) =
    g = 9.81 m/s^2
    v = sqrt(2 * g * h)
    return v

print impact_speed(10 m)
print impact_speed(1 m) in km/hr
```

<!-- output -->
```
14.0 m/s
15.9 km/hr
```

Indentation is how Fermium knows which lines belong to the function. The blank line and the unindented `print` mark the end of it. Use the **Tab** key or 4 spaces; VS Code converts Tab into spaces for you.

Variables created inside a function (here `g` and `v`) are **local**: they exist only while the function runs. After the function, `g` means nothing again.

## Checking the units of inputs

If a function only makes sense for one kind of quantity, you can say so with a unit in square brackets:

<!-- run as brackets.fm -->
```
spring_energy(x [m]) = 0.5 * 50 N/m * x^2
print spring_energy(10 cm)
print spring_energy(3 s)
```

<!-- output -->
```
brackets.fm, line 3: spring_energy expects x in m (length [m]), but got time [s]
    print spring_energy(3 s)
          ^^^^^^^^^^^^^^^^^^
```

`x [m]` means "x must be a length" (any length unit works: cm, km, AU, ...). This is optional, but it's a good safety net. Without the `[m]`, Fermium would happily work out ½ × 50 N/m × (3 s)², which is 220 kg (a mass!), and you would only find out later, when you used the result somewhere that needs an energy (for example `print spring_energy(3 s) in J` is an error).

## Built-in functions

Fermium comes with the usual maths functions:

| Function | Meaning |
|---|---|
| `sin(x) cos(x) tan(x)` | trigonometry (x in radians, or write `30 deg`) |
| `asin(x) acos(x) atan(x) atan2(y, x)` | inverse trig |
| `exp(x)` | eˣ (not `e^x`: `e` is the electron charge!) |
| `ln(x) log10(x)` | natural log, log base 10 |
| `sqrt(x) cbrt(x)` | square root, cube root |
| `abs(x)` | absolute value |
| `min(a, b) max(a, b)` | smaller / larger |
| `round(x) floor(x) ceil(x)` | rounding |

```fermium
print sin(30 deg), cos(pi), exp(1), ln(10)
print atan2(1, 1) in deg
print abs(-3 m), sqrt(16 m^2), cbrt(27)
```

<!-- output -->
```
0.500 -1 2.72 2.30
45°
3 m 4 m 3
```

⚠️ **Angles are in radians unless you say otherwise.** `sin(30)` is the sine of 30 *radians*. Write `sin(30 deg)` for degrees.

⚠️ Functions like `sin`, `exp` and `ln` need a **plain number** (no units). `exp(-t / tau)` is fine because a time divided by a time has no units; `exp(t)` with `t` in seconds is an error. That's a real physics rule, and Fermium enforces it.

## Summary

- Define: `f(x) = formula`. Call: `f(3 m)`.
- Several parameters: `KE(m, v) = ½ m v²`.
- Longer functions: indented lines under `f(x) =`, ending with the result or `return`.
- `f(x [m])` requires x to be a length.
- `print f` shows the formula and its units.
- Built-in: `sin cos exp ln sqrt abs ...`. Angles are in radians; write `deg` for degrees.

## Exercises

1. **Gravity.** Write a function `F_grav(m1, m2, r)` for Newton's law of gravitation, F = G m₁ m₂ / r². Use it to compute the force between the Earth (`M_earth`) and a 70 kg person at the Earth's surface (`R_earth`). Compare with 70 kg × 9.81 m/s².
2. **Nuclear radius.** The radius of a nucleus with mass number A is R = r₀ A^(1/3) with r₀ = 1.2 fm. Write `R(A)` and print the radius of carbon-12, iron-56 and uranium-238, in fm.
3. **Projectile.** Write a function `proj_range(v0, theta)` for the range of a projectile, R = v₀² sin(2θ)/g, and find the range at 30°, 45° and 60° for v₀ = 20 m/s. Which angle goes furthest?
4. **Temperature of a star.** Wien's law says a blackbody peaks at wavelength λ = b / T, with b = 2.898e-3 m K. Write `peak(T)` and print the peak wavelength of the Sun (5778 K) and of a human (310 K), in nm and μm.
5. **Multi-line function.** Write a multi-line function `fall_time(h)` that computes the time to fall from height h with g = 9.81 m/s² (t = √(2h/g)) and **returns** it. What's the fall time from the top of the Eiffel Tower (330 m)?

Solutions: [solutions/lesson03.md](solutions/lesson03.md)

**Next:** [Lesson 4 — Conditions & loops](lesson04_conditions_loops.md)
