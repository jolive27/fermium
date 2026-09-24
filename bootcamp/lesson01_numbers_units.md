# Lesson 1 — Numbers & units

In this lesson you'll use Fermium as a very smart calculator, one that knows that a metre is not a second.

You can try everything in the REPL (type `fermium` in the Terminal) or put it in a file and run it with `fermium run file.fm`. The examples are written as programs so you can see several lines at once.

## Printing numbers

`print` shows a value on the screen. The usual arithmetic works:

| You write | It means |
|---|---|
| `+` `-` | add, subtract |
| `*` | multiply |
| `/` | divide |
| `^` | power: `2^3` is 2³ = 8 |
| `( )` | do this part first |

```fermium
print 2 + 3
print 2 + 3 * 4
print (2 + 3) * 4
print 10 / 4
print 2^10
```

<!-- output -->
```
5
14
20
2.5
1024
```

Fermium follows the rules you learned in school: powers first, then `*` and `/`, then `+` and `-`. Use parentheses whenever you're unsure; they never hurt.

### Big and small numbers

Physics is full of numbers like 6.67 × 10⁻¹¹. There are two ways to write them:

```fermium
print 6.674e-11
print 6.674×10^-11
print 3e8
```

<!-- output -->
```
6.674×10⁻¹¹
6.674×10⁻¹¹
3×10⁸
```

`e-11` means "× 10⁻¹¹". This is how almost every programming language writes it. (`*` also works: `6.674*10^-11`.)

## Units: the big idea

In Fermium a number can carry a **unit**. Put the unit right after the number, with a space:

```fermium
print 3 m
print 9.81 m/s^2
print 2 m + 30 cm
print 3 kg + 200 g
print 100 km / 2 hr
```

<!-- output -->
```
3 m
9.81 m/s^2
2.3 m
3.2 kg
13.8889 m/s
```

Look at what happened:
- `2 m + 30 cm` gave `2.3 m`. Fermium converted the centimetres for you.
- `100 km / 2 hr` came out in m/s, because a distance divided by a time is a speed. Fermium works out the units of every result.

Units are written the way you'd write them on paper:
- `m/s` is metres per second, `m/s^2` is metres per second squared.
- A space between two units multiplies them: `N m` is newton-metres, `kg m/s^2` is a newton.
- Prefixes work: `km`, `cm`, `mm`, `μm` (type it as `um`), `nm`, `MeV`, `GHz`, `kPa` ...

Common unit names: `m g s kg K A mol` (SI), `N J W Pa C V Hz` (derived), `min hr day year` (time; note it's `hr`, not `h`), `eV MeV fm u` (nuclear), `AU ly pc Msun` (astro), `inch ft mi mph lb` (imperial), `deg rad` (angles). There's a full list in the [reference](../docs/reference.md#15-units).

## Units catch mistakes

Here's the real reason units are built into Fermium. Try adding a length to a time. Save this as `oops.fm`:

<!-- run as oops.fm -->
```
print 5 m + 3 s
```

<!-- output -->
```
oops.fm, line 1: can't add length [m] to time [s]
    print 5 m + 3 s
          ^^^^^^^^^
  hint: both sides of + and - must have the same units
```

Fermium refuses to run the program and tells you:
- **where** the problem is (`line 1`, and the `^^^^` marks point at it),
- **what** is wrong, in physics words (a length plus a time),
- a **hint** about how to fix it.

In physics, adding a length to a time is always a mistake. Somewhere a formula is wrong. Catching that *before* the program runs is like having a friend check your dimensional analysis on every line.

Multiplying and dividing different units is fine, of course: that's how you get new quantities.

```fermium
print 3 N * 2 m
print 3 kg * 2 m / 1 s^2
print 2 A * 3 ohm
print 12 m / 4 s
```

<!-- output -->
```
6 J
6 N
6 V
3 m/s
```

Fermium even recognised that N·m is a joule and A·Ω is a volt.

## Converting units with `in`

Want the answer in different units? Add `in` and the unit:

```fermium
print 100 km/hr in m/s
print 1 mi in km
print 1 year in s
print 13.6 eV in J
print 20 degC in K
```

<!-- output -->
```
27.7778 m/s
1.60934 km
3.15576×10⁷ s
2.18×10⁻¹⁸ J
293.15 K
```

`in` only converts between units that measure the same kind of thing. `print 5 kg in N` is an error (a mass isn't a force).

## Significant figures

Fermium prints answers with sensible significant figures, like you would in a lab report. A number written with a decimal point counts its digits: `2.21` has 3 significant figures, `1.0` has 2. A result gets the fewest significant figures of the numbers that went into it (but at least 2):

```fermium
print 2.0 m * 3.14159
print 9.81 m/s^2 * 2.0 s
print 2 / 3
print 2.0 / 3
```

<!-- output -->
```
6.3 m
20 m/s
0.666667
0.67
```

Whole numbers like `2` and `3` count as exact, so `2 / 3` prints 6 digits. Fermium always *calculates* with full precision; this only changes how many digits are *shown*. (Lesson 2 shows how to ask for more digits.)

## Physical constants

Fermium knows the fundamental constants (the official CODATA 2022 values), with their units:

```fermium
print c
print h
print G
print m_e
print k_B
```

<!-- output -->
```
2.99792×10⁸ m/s
6.62607×10⁻³⁴ J s
6.6743×10⁻¹¹ m³/(kg s²)
9.10938×10⁻³¹ kg
1.38065×10⁻²³ J/K
```

The underscore `_` is how you write a subscript: `m_e` is mₑ, the electron mass. Others include `m_p` (proton), `m_n` (neutron), `e` (the elementary charge), `N_A`, `epsilon_0`, `g_n` (standard gravity, 9.80665 m/s²), `M_sun`, `M_earth`, `R_earth`, `AU`. The electron's rest energy mₑc²:

```fermium
print m_e * c^2 in MeV
print m_p * c^2 in MeV
```

<!-- output -->
```
0.510999 MeV
938.272 MeV
```

## ⚠️ The big gotcha: a name right after a number is a unit

This is the one rule you must remember from this lesson:

> **A unit name right after a number is always a unit.**

That's what makes `3 m` mean "3 metres". But it has a surprising side effect. Suppose you want 2 × g × h and you write `2 g h`:

```fermium
print 2 g h
```

<!-- output -->
```
1.32521×10⁻³⁶ kg² m²/s
```

Nonsense! Fermium read `2 g` as **2 grams**, and then multiplied by `h`, which is **Planck's constant**. The same thing happens with `3 m` (metres, not a mass `m`), `5 s` (seconds), `2 c` (the speed of light is also a unit!), `4 K`, `2 A`, `10 N`.

The fix is easy: **put a `*` after the number**, or put the number last:

```fermium
g = 9.81 m/s^2
h = 10 m
print 2 * g * h
print sqrt(2 * g * h)
```

<!-- output -->
```
196 J/kg
14.0 m/s
```

(Here `g` and `h` are *variables* that we made ourselves; that's the next lesson.) When you do write something like `2 g` after making your own `g`, Fermium notices and prints a **warning**, a yellow-flag message that doesn't stop the program:

```fermium
g = 9.81 m/s^2
h = 10 m
print 2 g * h
```

<!-- output -->
```
warning: line 3: 'g' right after a number is the unit g, not your variable g
    print 2 g * h
            ^
  hint: that's usually what you want; to multiply by the variable write 2*g, and to make the unit explicit write [g]
0.02 kg m
```

**Always read warnings.** They usually mean the program does something other than what you meant.

One more place this rule shows up: `c` (the speed of light) is also a unit, so `1 AU / c` is read as "1 AU-per-speed-of-light", a perfectly good unit of time. Fermium prints it with its value in SI units next to it:

```fermium
print 1 AU / c
print 1 AU / c in min
```

<!-- output -->
```
1 AU / c (= 499.005 s)
8.31675 min
```

And watch out for dividing by a variable whose name is also a unit: `20 m/s / g` means 20 m/s **per gram**, because `/ g` continues the unit. Put parentheses around the number with its unit: `(20 m/s) / g`.

## Summary

- `print` shows a value. `+ - * / ^` and parentheses work as in maths.
- Write big and small numbers as `6.674e-11`.
- A unit goes right after its number: `9.81 m/s^2`. Fermium converts and checks units for you.
- Adding things with different units is an error, caught before the program runs.
- `in` converts: `print 100 km/hr in m/s`.
- Constants like `c`, `h`, `G`, `m_e` are built in.
- **Gotcha:** `2 g h` is 2 *grams* × Planck's constant. Write `2 * g * h`.

## Exercises

1. **Light from the Sun.** How long does light take to travel 1 AU? Print it in seconds and in minutes. (Hint: `print 1 AU / c in s`.)
2. **Speed limits.** A car drives at 70 mph. What's that in km/hr and in m/s?
3. **Rest energies.** Print the neutron's rest energy `m_n c²` in MeV, and the difference between the neutron's and the proton's rest energies in MeV.
4. **Spot the bug.** A friend computes the kinetic energy of a 2 kg mass at 3 m/s with `print 0.5 * 2 kg * 3 m/s^2`. What's wrong, and what does Fermium print? Fix it.
5. **Photon energy.** A green photon has wavelength 530 nm. Its energy is E = hc/λ. Print it in joules and in eV. Careful with the gotcha: write `h * c / 530 nm`.

Solutions: [solutions/lesson01.md](solutions/lesson01.md)

**Next:** [Lesson 2 — Variables & formulas](lesson02_variables_formulas.md)
