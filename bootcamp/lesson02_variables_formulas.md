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
31.8 ft/s²
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
- A few words are part of the language itself (**reserved words**) and can't be variable names, for example `if else for while from to step in print plot solve with fit load vs where and or not return true false`. If you try, Fermium tells you: `'step' is a reserved word in Fermium, so it can't be a variable name`. The full list is in [TROUBLESHOOTING.md](TROUBLESHOOTING.md#more-problems).

Long, descriptive names make programs easier to read:

```fermium
speed_of_sound = 343 m/s
distance = 1 km
time_to_hear = distance / speed_of_sound
print time_to_hear
```

<!-- output -->
```
2.92 s
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
Measured g = 9.70 m/s² which is 31.8 ft/s²
g to 6 digits: 9.69966 m/s²
```

`to 6 digits` overrides the automatic significant figures (Lesson 1) when you want to see more (or fewer) digits.

A variable can hold text, too. That's handy for a label you print several times:

```fermium
planet = "Mars"
print "Planet:", planet
print planet, "is the fourth planet"
```

<!-- output -->
```
Planet: Mars
Mars is the fourth planet
```

Text can be stored and printed, but you can't do arithmetic with it: `"Mars" + 1` is an error.

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

> **In the REPL this is allowed.** When you type the same two lines into the REPL (or a Jupyter cell), `v = 5` quietly starts a *new* variable v that replaces the old one, and `print v` shows `5`. That's on purpose: while you experiment, you often want to reuse a name. In a program (`fermium run file.fm`) a name keeps its units from start to end, which is where this check protects you.

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

## When one of your names is also a unit

Physicists love `m` for a mass and `g` for gravity, and Fermium also uses `m` for metres and `g` for grams. One short rule decides:

> 1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
> 2. If that unit is a single name that is also one of your variables (`2 g` with your own `g`), Fermium stops and asks which you mean: `2*g` for your variable, `2 [g]` for the unit.
> 3. In a longer unit (`3 m/s`, `2 kg m²`) the first name is always a unit; a later name that is also your variable gets the same question.

Anywhere else, a name is your variable, and spaces never change the meaning. Here is the question and its answer:

<!-- run as gravity.fm -->
```
g = 9.81 m/s^2
h = 20 m
print 2 g h
```

<!-- output -->
```
gravity.fm, line 3: '2 g' is ambiguous: right after a number, g is a unit (grams), but g is also your variable g
    print 2 g h
            ^
  hint: write  2*g  for 2 × your variable g, or  2 [g]  for the unit
```

```fermium
g = 9.81 m/s^2
h = 20 m
print 2*g*h
print 2 [g]
```

<!-- output -->
```
392 m²/s²
2 g
```

And a later name in a unit, `20 m/s/g`: brackets say what you mean.

```fermium
g = 9.81 m/s^2
print (20 m/s)/g
```

<!-- output -->
```
2.04 s
```

With a mass `m`, the kinetic energy reads just like the textbook, because no number comes right before `m`:

```fermium
m = 2 kg
v = 3 m/s
print ½ m v^2
print (1/2) m v^2
```

<!-- output -->
```
9 J
9 J
```

`½` is a single character (Lesson 2b shows how to type it); `(1/2)` is the same in plain ASCII. A fraction of plain numbers is one coefficient, so `1/2 mass v^2` works too.

**Gravity:** Fermium doesn't guess what `g` means. Put `g = 9.81 m/s^2` in your file (as above), or use the built-in standard gravity `g_n` (9.80665 m/s², also spelled `g_0`).

Implicit multiplication happens *before* division, the way physicists read `h c / λ k T` as (hc)/(λkT). Here's where that rule helps: the Planck distribution's exponent hc/(λk_BT) can be written exactly as on paper:

```fermium
T = 300 K
lam = 10 um
x = h c / lam k_B T
print x
```

<!-- output -->
```
4.80
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

The names a `where` defines are your variables in the formula before it, so `½ m v^2 where m = 2 kg, v = 3 m/s` works as written.

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
- `print "text", value` and `print x to 6 digits`. A variable can also hold text: `planet = "Mars"`.
- Right after a number comes a unit. If that unit is also one of your names (`2 g` with your own `g`), write `2*g` for your variable or `2 [g]` for the unit.
- Gravity: `g = 9.81 m/s^2` in your file, or the built-in `g_n`.

## Exercises

1. **Projectile range.** A ball is launched at v₀ = 20 m/s at 45° (write `45 deg`). Its range is R = v₀² sin(2θ)/g. Compute R. (`sin` is the sine function.)
2. **Kinetic energy.** A 1500 kg car drives at 100 km/hr. Compute its kinetic energy in kJ, using a variable called `m` for the mass.
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
