# Fermium Bootcamp

Welcome! This is a course for people who have **never written a line of code** but know some physics. By the end you will have written a program that simulates a planet going around the Sun, from scratch.

You will learn to program in **Fermium**, a language made for physics. Fermium is a good first language for a physicist for three reasons:

- **Numbers carry units.** You write `9.81 m/s^2`, not just `9.81`. If you add a length to a time by mistake, Fermium tells you *before* your program runs.
- **It looks like physics on paper.** `E = ½ m v²` is real Fermium code.
- **Calculus is built in.** Derivatives, integrals and differential equations are one line each.

Everything you learn here (variables, functions, loops, lists) also exists in Python, Julia, MATLAB and every other language. Fermium is just a friendly place to learn it.

## How to use this course

1. Do the lessons **in order**. Each one uses what came before.
2. **Type the examples yourself** instead of copying and pasting. It feels slow, but it is how your fingers and brain learn the patterns. Then change them and see what happens.
3. Every lesson ends with **exercises**. Try each one for at least 10 minutes before looking at the solution in the `solutions/` folder.
4. Getting an error is normal, and it is not a failure. Professional programmers see error messages all day. Fermium's errors try to tell you exactly what went wrong. When you are stuck, look in [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
5. Keep [CHEATSHEET.md](CHEATSHEET.md) open (or print it).

Each lesson takes roughly 30–60 minutes.

## The lessons

| Lesson | Topic | You will learn |
|---|---|---|
| [0](lesson00_setup.md) | Setup | Install Fermium on your Mac and run your first program |
| [1](lesson01_numbers_units.md) | Numbers & units | Fermium as a calculator that understands units |
| [2](lesson02_variables_formulas.md) | Variables & formulas | Give values names, write formulas like on paper |
| [2b](lesson02b_symbols.md) | Symbols | Write `π`, `θ`, `√`, `²` (or their plain-keyboard spellings) |
| [3](lesson03_functions.md) | Functions | Make your own `f(x)` |
| [4](lesson04_conditions_loops.md) | Conditions & loops | Make decisions, repeat things |
| [5](lesson05_lists.md) | Lists & data | Work with many numbers at once |
| [6](lesson06_data_plotting.md) | Lab data & plots | Load a CSV file, fit a model, draw a graph |
| [7](lesson07_derivatives.md) | Derivatives | `x'`, `d/dt`: velocity from position |
| [8](lesson08_integrals.md) | Integrals | `∫ … dx`: work, areas, the blackbody |
| [9](lesson09_differential_equations.md) | Differential equations | Springs, decay, orbits with `solve` |
| [10](lesson10_final_project.md) | Final project | Simulate a planet's orbit from scratch |

Extras:
- [CHEATSHEET.md](CHEATSHEET.md): every symbol, how to type it, and the core commands, on one page.
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md): the 20 most common errors, what they mean and how to fix them.
- [solutions/](solutions/): worked solutions to every exercise.
- [data/](data/): the lab data files used in Lesson 6.

## How to read the examples

A grey box marked as Fermium code is a complete program. You can save it in a file and run it. Right after it you will usually see a box labelled *Output*, showing exactly what Fermium printed when we ran it:

```fermium
print 2 m + 30 cm
```

<!-- output -->
```
2.3 m
```

Lines you type into the **Terminal** (the Mac's command window) look like this:

```
fermium run hello.fm
```

Every Fermium example in this course is run automatically by Fermium's test suite, so the examples and outputs you see here really work.

## A note for teachers and maintainers

Each ```` ```fermium ```` block in `bootcamp/*.md` and `bootcamp/solutions/*.md` is executed by `tests/test_docs.py` (with the markdown file's folder as the working folder, so `load "data/pendulum.csv"` works). Blocks that deliberately fail are written as plain ```` ``` ```` blocks. Run `python3 -m pytest -q tests/test_docs.py` after editing.
