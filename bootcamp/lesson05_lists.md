# Lesson 5 — Lists & data

In the lab you never measure something just once. You measure the period of a pendulum five times, or the distance at ten different times. A **list** holds many values under one name.

## Making a list

Put the values in square brackets, separated by commas:

```fermium
times = [2.21 s, 2.19 s, 2.24 s, 2.20 s, 2.23 s]
print times
print times in ms
```

<!-- output -->
```
[2.21, 2.19, 2.24, 2.20, 2.23] s
[2210, 2190, 2240, 2200, 2230] ms
```

All the elements of a list must have the same kind of units (you can't mix lengths and times in one list). Fermium prints the unit once, at the end.

## Getting elements out: indexing

Each element has a position number, its **index**. The first element is number 1:

```fermium
times = [2.21 s, 2.19 s, 2.24 s, 2.20 s, 2.23 s]
print times[1]
print times[2]
print times[end]
print len(times)
```

<!-- output -->
```
2.21 s
2.19 s
2.23 s
5.00
```

- `times[1]` is the first element, `times[2]` the second, and so on.
- `times[end]` is the last one.
- `len(times)` ("length") is how many elements there are.

> **Heads up for later:** Fermium counts from 1, like maths (x₁, x₂, …), Julia, MATLAB and Fortran. Python, C and JavaScript count from 0. If you learn Python later, remember that its first element is `xs[0]`.

Asking for an element that doesn't exist stops the program with a clear message:

<!-- run as index.fm -->
```
times = [2.21 s, 2.19 s, 2.24 s]
print times[4]
```

<!-- output -->
```
index.fm, line 2: index 4 is out of range: the list has 3 elements (valid indexes are 1 to 3)
    print times[4]
```

You can change an element with `times[2] = 2.18 s`.

## Statistics in one line

This is where lists shine. The mean, standard deviation and standard error of five measurements:

```fermium
times = [2.21 s, 2.19 s, 2.24 s, 2.20 s, 2.23 s]
print "mean:", mean(times)
print "std dev:", std(times)
print "std error:", std(times) / sqrt(len(times))
print "min and max:", min(times), max(times)
print "sum:", sum(times)
```

<!-- output -->
```
mean: 2.21 s
std dev: 0.0207 s
std error: 0.00927 s
min and max: 2.19 s 2.24 s
sum: 11.1 s
```

Other list functions: `sort(xs)`, `reverse(xs)`, `cumsum(xs)` (running total), `diff(xs)` (differences between neighbours), `first(xs)`, `last(xs)`.

## Arithmetic on whole lists

Do arithmetic on a list and it happens to **every element**. This is one of the most useful ideas in scientific programming:

```fermium
ts = [0 s, 1 s, 2 s, 3 s, 4 s]
ys = 20 m/s * ts - ½ * 9.81 m/s^2 * ts^2
print ys
```

<!-- output -->
```
[0, 15.1, 20.4, 15.9, 1.52] m
```

One line computed the height at five times. Two lists of the same length combine element by element:

```fermium
masses = [1.0 kg, 2.0 kg, 0.50 kg]
speeds = [3.0 m/s, 4.0 m/s, 10.0 m/s]
KE = ½ masses speeds^2
print KE
print "total:", sum(KE)
```

<!-- output -->
```
[4.5, 16, 25] J
total: 46 J
```

Functions work on lists too: `sin(xs)`, `sqrt(xs)`, and your own functions:

```fermium
g = 9.81 m/s^2
period(L) = 2 pi sqrt(L / g)
lengths = [0.25 m, 0.5 m, 1 m, 2 m]
print period(lengths)
```

<!-- output -->
```
[1.00, 1.42, 2.01, 2.84] s
```

### Example: Kepler's third law

Kepler found that T²/a³ is the same for every planet. Let's check with real data for Mercury, Venus, Earth, Mars and Jupiter:

```fermium
a = [0.387 AU, 0.723 AU, 1.000 AU, 1.524 AU, 5.203 AU]   # orbit size
T = [0.241 yr, 0.615 yr, 1.000 yr, 1.881 yr, 11.86 yr]   # orbital period
print T^2 / a^3 in yr^2/AU^3
```

<!-- output -->
```
[1.00, 1.00, 1.00, 1.00, 0.999] yr²/AU³
```

All 1.00 (within the precision of the data). Kepler was right!

## Making lists

Typing every value is tedious. Some helpers:

```fermium
print linspace(0 s, 2 s, 5)      # 5 evenly spaced values from 0 s to 2 s
print range(1, 10, 2)            # from 1 to 10 in steps of 2
print zeros(3)
```

<!-- output -->
```
[0, 0.5, 1, 1.5, 2] s
[1, 3, 5, 7, 9]
[0, 0, 0]
```

You can also start with an empty list `[]` and add elements one at a time with `push`. This is how you collect results from a loop:

```fermium
squares = []
for n from 1 to 6
    push(squares, n^2)
print squares
print len(squares)
```

<!-- output -->
```
[1, 4, 9, 16, 25, 36]
6
```

## Looping over a list

`for x in list` runs the loop once for each element:

```fermium
lengths = [0.25 m, 0.5 m, 1 m]
for L in lengths
    print "L =", L, "T =", 2 pi sqrt(L / 9.81 m/s^2)
```

<!-- output -->
```
L = 0.25 m T = 1.00 s
L = 0.5 m T = 1.42 s
L = 1 m T = 2.01 s
```

If you also need the position, loop over the indexes:

```fermium
planets = [0.387 AU, 0.723 AU, 1.000 AU]
for i from 1 to len(planets)
    print "planet", i, "is at", planets[i]
```

<!-- output -->
```
planet 1 is at 0.387 AU
planet 2 is at 0.723 AU
planet 3 is at 1.00 AU
```

## ⚠️ Gotcha: two names, one list

When you write `ys = xs` with a list, you **don't** get a copy. You get a second name for the *same* list:

```fermium
xs = [1 m, 2 m]
ys = xs
ys[1] = 5 m
print xs
```

<!-- output -->
```
[5, 2] m
```

Changing `ys` changed `xs` too. (Python behaves the same way.) If you want an independent copy, compute a new list, e.g. `ys = 1 * xs`.

## Summary

- A list: `xs = [1 m, 2 m, 3 m]`. All elements share their kind of units.
- `xs[1]` is the first element, `xs[end]` the last, `len(xs)` the count.
- `mean std sum min max sort` work on lists.
- Arithmetic and functions act on every element: `2 xs`, `xs^2`, `sin(xs)`, `f(xs)`.
- `linspace(a, b, n)` makes n evenly spaced values; `push(xs, x)` adds to the end.
- `for x in xs` loops over the elements.

## Exercises

1. **Lab statistics.** Five students measured g: 9.78, 9.83, 9.81, 9.75 and 9.86 m/s². Print the mean, the standard deviation and the standard error of the mean.
2. **Free-fall table.** Use `linspace` to make 11 times from 0 s to 1 s, then compute the distance fallen, d = ½gt², at every time in one line. Print both lists.
3. **Kepler again.** Saturn is at a = 9.537 AU. Use T²/a³ = 1 yr²/AU³ to predict Saturn's orbital period, and compare with the real value (29.45 yr).
4. **Building a list.** Use a loop and `push` to make a list of the first 10 powers of 2 (2, 4, 8, …, 1024). Then print their sum.
5. **Find the maximum.** Without using `max`, write a loop that finds the largest value in `[3.2 m, 7.1 m, 1.4 m, 6.9 m]`. (Hint: keep track of "the largest so far" in a variable, like `y_max` in Lesson 4.) Check your answer with `max`.

Solutions: [solutions/lesson05.md](solutions/lesson05.md)

**Next:** [Lesson 6 — Loading lab data & plotting](lesson06_data_plotting.md)
