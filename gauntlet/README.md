# The textbook gauntlet

Standard physics textbook problems, solved in Fermium and checked against analytic answers or SciPy.
The point is to find where the language gets in the way, and fix it.

## Layout

- `gauntlet/<topic>/NN_<name>.fm`: one problem per file. The header comment states the problem as
  a textbook would, with the numbers. The program prints labelled results.
- `tests/test_gauntlet_<topic>.py`: runs every problem in the topic and compares the printed numbers
  with an independent answer: a closed-form formula evaluated in Python, or SciPy
  (`solve_ivp`, `quad`, `brentq`, `eigh`, ...). The tolerance is stated and justified.
- `gauntlet/<topic>/FRICTION.md`: everything that was awkward, impossible, surprising or wrong while
  writing the problems. For each item: what you wanted to write, what you had to write instead,
  how bad it is, and the language change that would fix it. `gauntlet/FRICTION.md` collects the
  items across topics and records how each was resolved.

Topics, in order: mechanics, oscillations, gravitation, thermodynamics, electromagnetism,
optics_waves, special_relativity, quantum, nuclear, astrophysics. The first pass has three
problems per topic; the second pass is harder.
