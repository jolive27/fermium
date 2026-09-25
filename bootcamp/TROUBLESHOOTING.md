# Troubleshooting: the 20 most common errors

Everyone gets errors, all the time. The trick is to read them calmly. A Fermium error message has four parts:

```
myprogram.fm, line 3: can't add length [m] to time [s]      <- where, and what's wrong
    y = x + t                                                <- the line itself
        ^^^^^                                                <- the exact spot
  hint: both sides of + and - must have the same units       <- a suggestion
```

**Errors** (like the one above) stop the program before it runs. **Warnings** start with `warning:`; the program still runs, but something is probably not what you meant. Read those too!

Below are the problems beginners meet most often, with the exact message Fermium prints, what it means, and how to fix it. Use ⌘F (Find) to search this page for words from your error message.

**Contents**
1. [command not found: fermium](#1-command-not-found-fermium)
2. [can't add length [m] to time [s]](#2-cant-add-length-m-to-time-s)
3. [… isn't defined](#3-isnt-defined)
4. [LT isn't defined (did you mean L T?)](#4-lt-isnt-defined)
5. [… isn't a unit Fermium knows](#5-isnt-a-unit-fermium-knows)
6. [x is length [m]; it can't now hold time [s]](#6-a-variable-cant-change-its-units)
7. [sin needs a plain number](#7-sin-needs-a-plain-number)
8. [e is the elementary charge](#8-e-is-the-elementary-charge)
9. [can't show … in …](#9-cant-show-in)
10. ['0.5 m' is ambiguous (a unit name that is also your variable)](#10-05-m-is-ambiguous-a-unit-name-that-is-also-your-variable)
11. [warning: this is read as a/(b c)](#11-warning-this-is-read-as-ab-c)
12. [index 4 is out of range](#12-index-out-of-range)
13. [expected ')' to close '('](#13-missing-parenthesis)
14. [expected an indented block here](#14-expected-an-indented-block-here)
15. [indentation doesn't match / indented but isn't inside a block](#15-indentation-problems)
16. [missing its closing quote](#16-missing-closing-quote)
17. [this range needs a step with units](#17-a-range-with-units-needs-a-step)
18. [unexpected '=' (use ==)](#18-using--instead-of-)
19. [f takes 1 argument but was given 2](#19-wrong-number-of-arguments)
20. [My program runs forever](#20-my-program-runs-forever)

[More problems](#more-problems): reserved words, `±`, files that can't be found, `solve` errors, `1,000`.

---

## 1. command not found: fermium

```
zsh: command not found: fermium
```

**Means:** the Terminal can't find the Fermium program. Either it isn't installed, or this Terminal window was opened before it was installed.

**Fix:**
1. Quit the Terminal (⌘Q) and open it again.
2. Run `python3 --version`. It must say 3.10 or higher (see [Lesson 0, Step 2](lesson00_setup.md#step-2-install-python-3)).
3. `cd ~/fermium` and run `python3 -m pip install -e ".[full]"` again. Read the last lines it prints: `Successfully installed` means it worked.
4. If pip warned that a script was installed in a folder "which is not on PATH", reinstall Python from python.org and repeat step 3.

Also run `fermium doctor` whenever something seems wrong with the installation itself.

## 2. can't add length [m] to time [s]

<!-- run as add.fm -->
```
x = 5 m
t = 2 s
y = x + t
```

<!-- output -->
```
add.fm, line 3: can't add length [m] to time [s]
    y = x + t
        ^^^^^
  hint: both sides of + and - must have the same units
```

**Means:** you added (or subtracted) two quantities with different units. In physics this is always a mistake: a formula is wrong somewhere.

**Fix:** find which side has the wrong units. Maybe you forgot a factor (`x + v t` instead of `x + t`), or used the wrong variable. The same error appears for a quantity plus a plain number, like `5 m + 3`: write `5 m + 3 m`.

## 3. … isn't defined

<!-- run as undefined.fm -->
```
print speed
speed = 3 m/s
```

<!-- output -->
```
undefined.fm, line 1: speed isn't defined
    print speed
          ^^^^^
  hint: give it a value first, e.g.  speed = 1.0 m
```

**Means:** you used a name that Fermium doesn't know (yet).

**Fix:**
- **Order:** a program runs top to bottom, so give a variable its value *before* using it.
- **Spelling:** `Speed` and `speed` are different names; so are `T` and `t`. For a mistyped command, Fermium suggests the right word:

<!-- run as typo.fm -->
```
pritn 5
```

<!-- output -->
```
typo.fm, line 1: pritn isn't defined
    pritn 5
    ^^^^^
  hint: did you mean print?
```

## 4. LT isn't defined

<!-- run as space.fm -->
```
L = 2 m
T = 3 s
print LT
```

<!-- output -->
```
space.fm, line 3: LT isn't defined
    print LT
          ^^
  hint: did you mean L T (L times T)? Fermium reads LT as one name; put a space between
```

**Means:** names can be longer than one letter, so `LT` is a single name, not L × T.

**Fix:** put a space between names that multiply: `L T`. The same goes for Greek letters: `ω t`, not `ωt`.

## 5. … isn't a unit Fermium knows

<!-- run as unit.fm -->
```
print 2 furlongs
```

<!-- output -->
```
unit.fm, line 1: 'furlongs' isn't a unit Fermium knows (or a variable you've defined)
    print 2 furlongs
            ^^^^^^^^
  hint: see the list of units in docs/reference.md §15
```

**Means:** the word after a number isn't one of Fermium's units.

**Fix:** check the spelling and the [list of units](../docs/reference.md#15-units). Common ones people guess wrong: hours are `hr` (not `h`, which is Planck's constant), years `yr`, days `day`, micrometres `um`, degrees Celsius `degC`, ohms `ohm`. If the unit really doesn't exist, define it through a known one: `furlong = 201.168 m`, then `2 * furlong`.

## 6. A variable can't change its units

<!-- run as keep.fm -->
```
x = 3 m
x = 2 s
```

<!-- output -->
```
keep.fm, line 2: x is length [m]; it can't now hold time [s]
    x = 2 s
    ^^^^^^^
  hint: each variable keeps its units; use a new name for a different quantity
```

**Means:** each variable keeps the kind of quantity it first held. This catches accidentally reusing a name.

**Fix:** use a different name for the new quantity (`t = 2 s`). Changing the value is fine as long as the units match (`x = 5 cm`).

## 7. sin needs a plain number

<!-- run as sin.fm -->
```
t = 2 s
print sin(t)
```

<!-- output -->
```
sin.fm, line 2: sin needs a plain number, but got time [s]
    print sin(t)
              ^
  hint: the argument of sin, cos, exp, ... must be a plain number (angles in rad are plain numbers)
```

**Means:** `sin`, `cos`, `exp`, `ln` and friends only accept plain numbers (no units). That's a real physics rule: sin(2 seconds) is meaningless.

**Fix:** you've usually forgotten a factor that cancels the units, such as ω in `sin(ω t)` or τ in `exp(-t / τ)`. Angles are plain numbers, so `sin(30 deg)` and `sin(0.5 rad)` are fine.

## 8. e is the elementary charge

<!-- run as e.fm -->
```
tau = 2 s
t = 1 s
print e^(-t / tau)
```

<!-- output -->
```
e.fm, line 3: e is the elementary charge (1.602×10⁻¹⁹ C) in Fermium
    print e^(-t / tau)
          ^
  hint: for the exponential function write exp(x)
```

**Means:** in Fermium, `e` is the charge of the electron, not Euler's number 2.718…

**Fix:** write `exp(x)`: for decay, `exp(-t / tau)`. (A fixed power like `e^2` is allowed, because e² appears in physics formulas such as e²/(4πε₀), but it means the *charge* squared, in C². If you meant exp(2), the units will give you away.)

## 9. can't show … in …

<!-- run as convert.fm -->
```
print 5 kg in N
```

<!-- output -->
```
convert.fm, line 1: can't show mass [kg] in N (force [N])
    print 5 kg in N
          ^^^^^^^^^
  hint: the units you convert to must measure the same kind of quantity
```

**Means:** `in` converts between units of the **same kind** (m to ft, J to eV), but a mass can't be shown in newtons.

**Fix:** check what you're converting. Often it reveals a mistake in a formula: if your "energy" can't be shown in J, the formula for it is wrong. The message tells you the units it actually has.

## 10. '0.5 m' is ambiguous (a unit name that is also your variable)

<!-- run as mass.fm -->
```
m = 2 kg
v = 3 m/s
E = 0.5 m v^2
print E
```

<!-- output -->
```
mass.fm, line 3: '0.5 m' is ambiguous: right after a number, m is a unit (metres), but m is also your variable m
    E = 0.5 m v^2
            ^
  hint: write  0.5*m  for 0.5 × your variable m, or  0.5 [m]  for the unit
```

**Means:** right after a number, `m` is the unit metre, but you also have a variable `m`. Multiplied by something else (`0.5 m v^2`), Fermium can't tell which you meant, so it stops instead of guessing. On its own (`x = 0.5 m`) it would be the metre, with a warning.

**Fix:** put a `*` after the number (`0.5 * m * v^2`), or use `½` (`½ m v^2`), or write `0.5 [m]` if you really meant half a metre. Giving the mass a longer name like `mass` avoids the question altogether. The same situation: `2 g h` (grams), `3 V I` (volts), `27 b²` (barns). After `/` the rule is kinder: `20 m/s / g` (with a space before `/`) divides by your variable `g`, but `20 m/s/g` (no spaces) would still mean *per gram*. When in doubt, use parentheses: `(20 m/s) / g`.

## 11. warning: this is read as a/(b c)

<!-- run as half.fm -->
```
mass = 2 kg
v = 3 m/s
print 1/2 mass v^2
```

<!-- output -->
```
warning: line 3: this is read as a/(b c), i.e. 1/(2 ...): implicit multiplication binds tighter than '/'
    print 1/2 mass v^2
           ^
  hint: if you meant (1/2) times the rest, write (1/2) with parentheses (or ½ for one half)
0.0278 s²/(kg m²)
```

**Means:** in Fermium, multiplication without `*` happens *before* division (so that `h c / λ k T` means (hc)/(λkT), as in textbooks). So `1/2 mass v^2` is 1/(2 · mass · v²).

**Fix:** `½ mass v^2` or `(1/2) mass v^2` or `0.5 * mass * v^2`.

## 12. index out of range

<!-- run as index.fm -->
```
xs = [10 m, 20 m, 30 m]
print xs[4]
```

<!-- output -->
```
index.fm, line 2: index 4 is out of range: the list has 3 elements (valid indexes are 1 to 3)
    print xs[4]
```

**Means:** you asked for an element that doesn't exist. This error happens while the program runs.

**Fix:** lists start at **1** and end at `len(xs)`; `xs[end]` is the last one. `xs[0]` is also out of range (unlike Python). In loops, use `for i from 1 to len(xs)`.

## 13. Missing parenthesis

<!-- run as paren.fm -->
```
print sqrt(2 * (3 + 4)
```

<!-- output -->
```
paren.fm, line 1: this '(' is never closed
    print sqrt(2 * (3 + 4)
              ^
  hint: add the missing ')'
```

**Means:** a `(` was never closed.

**Fix:** count your brackets: every `(` needs a `)`. VS Code highlights the matching bracket when you put the cursor next to one.

## 14. expected an indented block here

<!-- run as block.fm -->
```
x = 3
if x > 2
print "big"
```

<!-- output -->
```
block.fm, line 3: expected an indented block here
    print "big"
    ^^^^^
  hint: indent the lines that belong to this block (e.g. 4 spaces)
```

**Means:** after `if`, `else`, `for`, `while`, `solve` (with equations on separate lines) or a multi-line function `f(x) =`, the lines that belong to it must be **indented**.

**Fix:** indent the body by 4 spaces (press Tab in VS Code).

## 15. Indentation problems

<!-- run as indent.fm -->
```
for i from 1 to 3
    print i
  print i^2
```

<!-- output -->
```
indent.fm, line 3: this line's indentation doesn't match any block above it
      print i^2
      ^
  hint: line up the start of the line with the lines above it
```

<!-- run as indent2.fm -->
```
x = 1
    print x
```

<!-- output -->
```
indent2.fm, line 2: this line is indented but isn't inside a block
        print x
        ^
  hint: remove the spaces at the start of the line
```

**Means:** the lines of a block must line up exactly, and lines that aren't in a block must start at the left edge.

**Fix:** use 4 spaces per level, consistently. If you mix Tab characters and spaces, lines that look aligned may not be: in VS Code, choose **View → Command Palette… → "Convert Indentation to Spaces"**.

## 16. Missing closing quote

<!-- run as quote.fm -->
```
print "hello
```

<!-- output -->
```
quote.fm, line 1: this text (string) is missing its closing quote "
    print "hello
          ^
```

**Fix:** text must start and end with `"` on the same line: `print "hello"`.

## 17. A range with units needs a step

<!-- run as step.fm -->
```
for t from 0 s to 1 s
    print t
```

<!-- output -->
```
step.fm, line 1: this range is time [s], so it needs a step with units
    for t from 0 s to 1 s
    ^^^
  hint: add e.g.  step 0.1 s
```

**Means:** Fermium can't guess whether you want steps of 1 s, 0.1 s or 1 ms.

**Fix:** add a step with the same units: `for t from 0 s to 1 s step 0.1 s`.

## 18. Using = instead of ==

<!-- run as equals.fm -->
```
x = 3
if x = 3
    print "three"
```

<!-- output -->
```
equals.fm, line 2: unexpected '='
    if x = 3
         ^
  hint: use == to compare two values
```

**Means:** a single `=` *stores* a value; comparing needs `==`.

**Fix:** `if x == 3`. For numbers with decimals, prefer `~=` ("approximately equal"), because 0.1 + 0.2 isn't exactly 0.3 in a computer.

## 19. Wrong number of arguments

<!-- run as args.fm -->
```
KE(mass, v) = ½ mass v^2
print KE(2 kg)
```

<!-- output -->
```
args.fm, line 2: KE takes 2 arguments but was given 1
    print KE(2 kg)
          ^^^^^^^^
```

**Fix:** give the function as many values as it has parameters, in the same order: `KE(2 kg, 3 m/s)`.

## 20. My program runs forever

Nothing is printed and the Terminal doesn't come back to the prompt. Usually a `while` loop whose condition never becomes false:

```
x = 1
while x > 0
    x += 1
```

**Fix:** press **Control + C** to stop the program. Fermium prints `stopped by Ctrl+C` and you get the prompt back. (In the unlikely case that doesn't work, close the Terminal window.) Then check the loop: does something inside it change the condition, in the right direction? For a simulation, does the stopping condition ever become true (e.g. `while y >= 0 m` for a ball that never comes down)? Adding a `print` inside the loop shows what's happening.

---

## More problems

**A reserved word as a name.** Words that are part of the language can't be variable names:

<!-- run as reserved.fm -->
```
step = 0.1 s
```

<!-- output -->
```
reserved.fm, line 1: 'step' is a reserved word in Fermium, so it can't be a variable name
    step = 0.1 s
    ^^^^
  hint: pick another name, e.g. step_ or my_step
```

The reserved words are `if else then for from to step in while return break continue print plot vs solve with fit load and or not where true false integral partial sqrt cbrt assert`. Use `dt`, `my_step`, `v_s`, etc.

**± with the unit in the middle.**

<!-- run as pm.fm -->
```
L = 1.20 m +- 0.01
```

<!-- output -->
```
pm.fm, line 1: the uncertainty after ± is a plain number (no units) but the value is length [m]; both need the same units
    L = 1.20 m +- 0.01
                  ^^^^
  hint: write the unit once at the end, like  L = 1.20 ± 0.01 m
```

The value and its uncertainty need the same units. Write the unit once, at the end: `L = 1.20 ± 0.01 m` (it then belongs to both numbers), or give both a unit: `1.20 m ± 1 cm`. See [Lesson 12](lesson12_lab_report.md).

**A data file that can't be found.**

<!-- run as load.fm -->
```
data = load "pendullum.csv"
```

<!-- output -->
```
load.fm, line 1: can't find the file 'pendullum.csv'
    data = load "pendullum.csv"
           ^^^^^^^^^^^^^^^^^^^^
  hint: looked in /Users/ada/fermium/bootcamp
```

Check the spelling, and remember the path is relative to the folder **your program** is in: if the file is in a `data` folder next to your program, write `load "data/pendulum.csv"`. The hint shows the folder Fermium looked in.

**A column that doesn't exist.** `data.x` when the file's header has no `x` gives `the data has no column called x (columns: L, T)`. Check the header line of the CSV: each column should look like `name [unit]`.

**`solve` needs every initial condition.**

<!-- run as ic.fm -->
```
solve x'' = -x / 1 s^2
  with x(0) = 1 m
  for t from 0 s to 5 s
```

<!-- output -->
```
ic.fm, line 1: missing initial condition: x'(start)
    solve x'' = -x / 1 s^2
    ^^^^^
  hint: add them after 'with', e.g.  with x(0) = 0.1 m, x'(0) = 0 m/s
```

An equation with x″ needs both `x(0)` and `x'(0)`.

**Second derivatives written as `d²x/dt²`.** These work now, and so do `d^2x/dt^2`, `x''` and `d²/dt² x`. If you see `d isn't defined`, check that there's no space inside `d²x` and that the orders match (`d²x/dt²`, not `d²x/dt`).

**Asking a solution for a time outside its range.**

<!-- run as range.fm -->
```
solve N' = -N / 5 s with N(0) = 1000 for t from 0 s to 20 s
print N(30 s)
```

<!-- output -->
```
range.fm, line 2: asked for the solution at 30 s, outside the range it was solved for (it ends at 20 s)
    print N(30 s)
```

Solve over a longer range (`for t from 0 s to 30 s`).

**Comparing with a plain number.** `if x > 2` when `x` is a length gives `can't compare length [m] with a plain number`. Write `if x > 2 m`.

**Commas in numbers.** `print 1,000` prints `1 0`: the comma separates two things to print. Write `1000` or `1e3`.

**Something looks wrong but there's no error.** Print the intermediate values (`print` is the programmer's best debugging tool), check them with units (`print x in m`), and look for warnings above your output.

**Still stuck?** Run `fermium doctor`, re-read the lesson section, and look at the [language reference](../docs/reference.md). Most errors are one small typo. Take a break and come back: fresh eyes find bugs fast.
