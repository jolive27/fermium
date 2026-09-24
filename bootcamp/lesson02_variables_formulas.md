# Lesson 2 — Variables & formulas

In Lesson 1 every calculation was a single line. Real physics problems have several steps, and you want to give the numbers names, like `L` for a length and `T` for a period. Named values are called **variables**.

From now on, write your programs in a file (for example `lesson2.fm`) and run them with `fermium run lesson2.fm`.

## Making a variable

```fermium
L = 1.20 m
T = 2.21 s
print L
print T
```

<!-- output -->
```
1.20 m
2.21 s
```

`L = 1.20 m` means "**store** the value 1.20 m under the name `L`". After that, writing `L` anywhere means 1.20 m.

> **`=` is not the maths "equals".** In maths, `x = 5` is a statement that's true or false. In programming, `x = 5` is an **instruction**: "put 5 in the box labelled x". The name always goes on the left, the value on the right. `5 = x` is an error.

## Formulas

Now you can write formulas the way you would on paper. A simple pendulum has period T = 2π√(L/g), so g = 4π²L/T²:

```fermium
# Measure g with a pendulum
L = 1.20 m
T = 2.21 s
g = 4 pi^2 L / T^2
print g
print g in ft/s^2
```

<!-- output -->
```
9.70 m/s²
31.8 ft/s^2
```

A few things to notice:
- `pi` is π. (In [Lesson 2b](lesson02b_symbols.md) you'll learn to write the actual `π` symbol.)
- **You don't need `*` between things that multiply**: `4 pi^2 L` means 4 × π² × L, and `k x` means k × x. This is called *implicit multiplication*, and it's why Fermium code looks like a textbook.
- Fermium figured out that the answer is an acceleration and printed it in m/s².

### Names

- A name can use letters, digits and underscores `_`, but can't start with a digit: `v0`, `v_0`, `mass`, `speed_of_sound` are all fine.
- **Upper and lower case are different:** `T` (a period) and `t` (a time) are two different variables.
- **Names can be longer than one letter.** That means `LT` is *one* name ("LT"), not L × T. Put a space between them: `L T`.
- `_` makes a subscript in physics notation: `v_0` is read as v₀.
- A few words are reserved by Fermium and can't be variable names: `if else for while from to step in print plot solve with fit load vs where and or not return`. If you try, you get `didn't expect 'step' here`.

Long, descriptive names make programs easier to read:

```fermium
speed_of_sound = 343 m/s
distance = 1 km
time_to_hear = distance / speed_of_sound
print time_to_hear
```

<!-- output -->
```
2.91545 s
```

## Printing text and values together

`print` can take several things separated by commas. Text goes in double quotes `"..."`:

```fermium
L = 1.20 m
T = 2.21 s
g = 4 pi^2 L / T^2
print "Pendulum length:", L
print "Measured g =", g, "which is", g in ft/s^2
print "g to 6 digits:", g to 6 digits
```

<!-- output -->
```
Pendulum length: 1.20 m
Measured g = 9.70 m/s² which is 31.8 ft/s^2
g to 6 digits: 9.69966 m/s²
```

`to 6 digits` overrides the automatic significant figures (Lesson 1) when you want to see more (or fewer) digits.

## Variables keep their units

Once a variable holds a speed, it can only ever hold speeds. This catches mistakes where you accidentally reuse a name:

<!-- run as reuse.fm -->
```
v = 3 m/s
v = 5
```

<!-- output -->
```
reuse.fm, line 2: v is speed [m/s]; it can't now hold a plain number (no units)
    v = 5
    ^^^^^
  hint: each variable keeps its units; use a new name for a different quantity
```

You can change the *value* of a variable, as long as the units still make sense:

```fermium
t = 0 s
t = t + 1.5 s
t += 2 s
print t
```

<!-- output -->
```
3.5 s
```

`t = t + 1.5 s` looks wrong in maths, but in programming it's perfectly normal: "take the current value of t, add 1.5 s, and store the result back in t". `t += 2 s` is a shortcut for the same thing. There are also `-=`, `*=` and `/=`.

## ⚠️ Gotcha: the mass `m` and the metre `m`

Physicists love the letter `m` for mass. Fermium also uses `m` for metres. Remember the rule from Lesson 1: **a unit name right after a number is a unit.** So:

```fermium
m = 2 kg
v = 3 m/s
print 0.5 m v^2
```

<!-- output -->
```
warning: line 3: 'm' after the number means the unit m, not your variable m
    print 0.5 m v^2
              ^
  hint: to multiply by the variable write *m (e.g. 0.5*m) or put the number in a name; ½ m also works
4.5 m³/s²
```

`0.5 m` is half a metre, not half the mass, so the answer has silly units. Fermium spotted the clash and warned us. Three correct ways to write kinetic energy:

```fermium
m = 2 kg
v = 3 m/s
print 0.5 * m * v^2
print (1/2) m v^2
print ½ m v^2
```

<!-- output -->
```
9.0 J
9 J
9 J
```

`½` is a single character (Lesson 2b shows how to type it). Because it isn't a digit, the unit rule doesn't apply to it. Also note `v = 3 m/s` was fine: there `m/s` is clearly a unit.

## ⚠️ Gotcha: `1/2 m v^2`

Fermium reads implicit multiplication *before* division, the way physicists read `h c / λ k T` as (hc)/(λkT). So `1/2 m v^2` means 1/(2 m v²), which is not the kinetic energy. Fermium warns about this too. Write `(1/2) m v^2` or `½ m v^2`.

Here's where that rule helps: the Planck distribution's exponent hc/(λk_BT) can be written exactly as on paper:

```fermium
T = 300 K
lam = 10 um
x = h c / lam k_B T
print x
```

<!-- output -->
```
4.79592
```

(Here `h`, `c` and `k_B` are the built-in constants, and `lam` is short for λ. The answer has no units, as the argument of an exponential must.)

## Naming things just for one line: `where`

Sometimes you want to plug numbers into a formula without creating variables for the rest of the program. `where` does that:

```fermium
p = mass * v where mass = 2 kg, v = 3 m/s
print p
```

<!-- output -->
```
6 kg m/s
```

The metre gotcha applies here too: `0.5 m v^2 where m = 2 kg` means 0.5 *metres* (Fermium warns you). Use `½ m v^2` or a longer name like `mass`.

## Comments

Anything after `#` on a line is a **comment**, ignored by Fermium. Use comments to explain *why* you're doing something:

```fermium
# Free fall from a height h
g = 9.81 m/s^2    # acceleration due to gravity
h = 20 m          # height of the building
t_fall = sqrt(2 * h / g)
print "It takes", t_fall, "to fall."
```

<!-- output -->
```
It takes 2.02 s to fall.
```

(`sqrt` is the square root.) Six months from now, you will thank yourself for writing comments.

## Summary

- `name = value` stores a value. The name is on the left.
- Formulas look like physics: `g = 4 pi^2 L / T^2`. No `*` needed between things that multiply.
- `LT` is one name; write `L T` for a product.
- A variable keeps its units forever.
- `x += 1 m` updates a variable.
- `print "text", value` and `print x to 6 digits`.
- **Gotchas:** `0.5 m v^2` uses *metres*; `1/2 m v^2` means 1/(2mv²). Write `½ m v^2` or `0.5 * m * v^2`.

## Exercises

1. **Projectile range.** A ball is launched at v₀ = 20 m/s at 45° (write `45 deg`). Its range is R = v₀² sin(2θ)/g. Compute R. (`sin` is the sine function.)
2. **Kinetic energy.** A 1500 kg car drives at 100 km/hr. Compute its kinetic energy in kJ, using a variable called `m` for the mass. Make sure you don't get a warning!
3. **Escape velocity.** Compute v = √(2GM/R) for the Earth using the constants `G`, `M_earth` and `R_earth`. Print it in km/s.
4. **Swap.** Make two variables `a = 3 m` and `b = 5 m`, then swap their values so that `a` is 5 m and `b` is 3 m. (Hint: you'll need a third variable.)
5. **Spot the bug.** This program was supposed to print the de Broglie wavelength of an electron moving at 1% of the speed of light. What's wrong? Fix it.
   ```
   v = 0.01 * c
   lam = h / m_e * v
   print lam in nm
   ```

Solutions: [solutions/lesson02.md](solutions/lesson02.md)

**Next:** [Lesson 2b — Symbols](lesson02b_symbols.md)
