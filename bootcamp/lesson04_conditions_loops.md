# Lesson 4 — Conditions & loops

So far your programs run every line once, top to bottom. Two ideas make programs far more powerful:

- **Conditions** (`if`): do something only when it's true.
- **Loops** (`for`, `while`): do something many times.

With loops, you can already write a real physics simulation by the end of this lesson.

## Comparisons: true or false

A comparison gives `true` or `false`:

```fermium
x = 3
print x > 2, x < 2, x == 3, x != 3
print 1 m > 50 cm
print 0.1 + 0.2 == 0.3, 0.1 + 0.2 ~= 0.3
```

<!-- output -->
```
true false true false
true
false true
```

| Symbol | Meaning |
|---|---|
| `>` `<` | greater than, less than |
| `>=` `<=` | greater or equal, less or equal (`≥` `≤`) |
| `==` | equal (**two** `=` signs, because one `=` stores a value) |
| `!=` | not equal (`≠`) |
| `~=` | approximately equal, within one part in a million (`≈`) |

Why is `0.1 + 0.2 == 0.3` false? Computers store numbers in binary, and 0.1 can't be written exactly in binary (just as 1/3 can't be written exactly in decimal). The sum comes out as 0.30000000000000004. When comparing decimals, use `~=`.

Comparisons respect units: `1 m > 50 cm` is true, and comparing a length to a time is an error.

You can combine conditions with `and`, `or` and `not`: `x > 0 and x < 10`.

## `if`: making decisions

```fermium
T = 5 degC
if T < 0 degC
    print "ice"
else if T < 100 degC
    print "liquid water"
else
    print "steam"
```

<!-- output -->
```
liquid water
```

How it works:
- After `if` comes a condition. If it's true, the **indented** lines below it run.
- `else if` checks another condition, only if the ones before were false. You can have as many as you like.
- `else` catches everything else.
- Only one branch runs.

The indentation (4 spaces) is essential: it's how Fermium knows which lines are "inside" the `if`. (If you know Python: a colon at the end of the `if` line is allowed but not needed.)

> **In the REPL:** you can type blocks too. After a line like `if x > 2`, the prompt changes to `...` and waits for the indented lines. Press Return on an **empty line** to finish the block and run it:
> ```
> fm> x = 5
> fm> if x > 3
> ...     print "big"
> ... else
> ...     print "small"
> ...
> big
> ```
> For anything longer than a few lines, a `.fm` file is more comfortable.

For a quick choice between two values, there's a one-line form:

```fermium
x = -3 m
distance = if x > 0 m then x else -x
print distance
```

<!-- output -->
```
3 m
```

## `for`: repeating a fixed number of times

```fermium
for n from 1 to 5
    print n, "squared is", n^2
```

<!-- output -->
```
1 squared is 1
2 squared is 4
3 squared is 9
4 squared is 16
5 squared is 25
```

The variable `n` takes the values 1, 2, 3, 4, 5 in turn, and the indented block runs once for each. Note that **both ends are included**: `from 1 to 5` includes 5.

A classic use is adding things up. Young Gauss supposedly added 1 + 2 + … + 100 in his head:

```fermium
total = 0
for n from 1 to 100
    total += n
print total
```

<!-- output -->
```
5050
```

Read it slowly: `total` starts at 0; each time around the loop, the current `n` is added to it.

### Loops with units and steps

`step` sets the spacing. With units, a step is required (Fermium can't guess whether you want steps of 1 s or 1 ms):

```fermium
v0 = 20 m/s
g = 9.81 m/s^2
for t from 0 s to 4 s step 0.5 s
    y = v0 t - ½ g t²
    print t, y
```

<!-- output -->
```
0 s 0 m
0.5 s 8.77 m
1 s 15.1 m
1.5 s 19.0 m
2 s 20.4 m
2.5 s 19.3 m
3 s 15.9 m
3.5 s 9.91 m
4 s 1.52 m
```

This is a **table of values**, like you'd make by hand. Counting down works too: `for i from 10 to 1 step -1`.

## `while`: repeating until something changes

A `for` loop runs a known number of times. A `while` loop keeps going **as long as a condition is true**, which is useful when you don't know in advance how many steps you need.

Radioactive decay: each year, 10% of a sample decays. How long until less than half is left?

```fermium
N = 1000
years = 0
while N > 500
    N = N * 0.9
    years += 1
print "half-life is between", years - 1, "and", years, "years"
```

<!-- output -->
```
half-life is between 6 and 7 years
```

⚠️ If the condition never becomes false, the loop runs forever (an **infinite loop**). If your program seems stuck, press **Control + C**: Fermium stops it and prints `stopped by Ctrl+C`. Then look at the loop: does something inside it change the condition?

## `break` and `continue`

- `break` leaves the loop immediately.
- `continue` skips the rest of this round and goes to the next.

```fermium
# The first number whose square is over 200
for n from 1 to 100
    if n^2 > 200
        print n, "squared is", n^2
        break
```

<!-- output -->
```
15 squared is 225
```

```fermium
# Only odd numbers: mod(n, 2) is the remainder after dividing by 2
for n from 1 to 9
    if mod(n, 2) == 0
        continue
    print n
```

<!-- output -->
```
1
3
5
7
9
```

## Your first simulation: throwing a ball

Now combine everything. Instead of using the formula y = v₀t − ½gt², let's **simulate** the ball: chop time into small steps `dt`, and in each step update the position using the velocity, and the velocity using the acceleration:

- y ← y + v · dt
- v ← v − g · dt

Repeat until the ball comes back down (y < 0).

```fermium
g = 9.81 m/s^2
dt = 0.001 s        # time step
y = 0 m             # height
v = 20 m/s          # upward velocity
t = 0 s             # clock
y_max = 0 m

while y >= 0 m
    y += v dt
    v -= g dt
    t += dt
    if y > y_max
        y_max = y

print "flight time:", t to 4 digits
print "highest point:", y_max to 4 digits
v0 = 20 m/s
print "formula says:", 2 v0 / g, "and", v0^2 / (2 * g)
```

<!-- output -->
```
flight time: 4.079 s
highest point: 20.40 m
formula says: 4.08 s and 20.4 m
```

(We gave the launch speed a name, `v0 = 20 m/s`, so the formula reads like the textbook one. We asked for 4 digits because `0.001 s` has only one significant figure, so Fermium would otherwise show just 2.) The simulation agrees with the textbook formulas to about 0.1%, and making `dt` smaller makes it even closer. So why simulate? Because the formula only works in the simplest case. Add air resistance (try exercise 5!) and there's no neat formula any more, but the simulation needs just one extra line. This idea, **step forward in small time steps**, is how physicists simulate everything from planets to plasmas. You'll use it again in the final project.

## Checking your work with `assert`

`assert` stops the program if something that should be true isn't. It's a way to leave checks in your code:

```fermium
E_kinetic = ½ * 2 kg * (3 m/s)^2
assert E_kinetic ~= 9 J, "kinetic energy should be 9 J"
print "all good"
```

<!-- output -->
```
all good
```

If the check fails, the program stops with your message.

## Summary

- Compare with `> < >= <= == != ~=`. Combine with `and or not`.
- `if condition` / `else if` / `else`, with an indented block under each.
- `for n from 1 to 10` (both ends included); with units, add `step 0.1 s`.
- `while condition` repeats until the condition is false.
- `break` leaves a loop, `continue` skips to the next round.
- Simulation = a loop that updates position and velocity in small time steps.

## Exercises

1. **Grade converter.** Given a mark `score = 73`, print `"A"` for 70 or more, `"B"` for 60–69, `"C"` for 50–59, and `"fail"` otherwise.
2. **Sum of squares.** Use a loop to compute 1² + 2² + … + 10². Check against the formula n(n+1)(2n+1)/6.
3. **Unit table.** Print a table of distances from 0 to 10 km in steps of 1 km, with the time light takes to cross each one, in microseconds (`us`).
4. **Doubling.** A bacterial colony doubles every 20 minutes. Starting from 1 bacterium, use a `while` loop to find how long it takes to exceed one million.
5. **Air resistance.** Modify the ball simulation to add air drag: in each step also do `v -= k v abs(v) dt`, with a drag constant `k = 0.01 1/m` ("per metre"). How much lower is the highest point? And the flight time?

Solutions: [solutions/lesson04.md](solutions/lesson04.md)

**Next:** [Lesson 5 — Lists & data](lesson05_lists.md)
