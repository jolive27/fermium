# What changed in Fermium 2.5

Fermium 2.5 grows the language (spec Phase C). Programs that ran under 2.0 print the same; some programs 2.0
refused now run. Design details are in `DECISIONS.md` (from D280).

## C1. Memory and data structures

- **Lists are freed in compiled code too.** The LLVM back end and `fermium build` executables used to keep every
  list until the program ended (as Fermium 1.5's compiled code did), so a long loop that made lists grew without
  bound. They now run a collector at the top of loop iterations that make lists (D280): a loop that makes a million
  lists of 100 numbers stays under 100 MB instead of 800 MB. `FERMIUM_GC_STATS=1` reports it. (The tree-walker's
  lists were already reference counted.)
- **Lists of vectors, matrices, complex numbers and text** (D281): `[<1, 2> m, <3, 4> m]`, a list of matrices
  (`[[[1, 0], [0, 1]], [[0, 1], [1, 0]]] N/m`, or built with `push`), `[1 + 2i, 3i]`, and `[]` that becomes the kind
  of the first value pushed onto it. Indexing, `xs[i] = …`, `len`, `for … in`, `clear`, `print`, a number times the
  list, `sum` and `mean` work; units and sizes are checked per element. An N-body step can be written as loops over
  lists of position and velocity vectors ([docs/reference.md](docs/reference.md#lists-of-vectors-matrices-complex-numbers-and-text-fermium-25)).
  Text lists gained `names[i] = "…"`.
- **`solve` with a list of unknowns** (D282): an unknown whose initial value is a list (`N(0) = [1e6, 0, 0, 0]`) or
  a list of vectors (`r(0) = r0`) is sized when the program runs, so reaction networks and N-body problems are
  written as loops in a function (`solve N' = rates(N) …`, `solve r'' = accel(r) …`). `N(t)` is the list at t;
  `until`, `absolute`, and all four methods work, with ordinary unknowns alongside.
- Tests: `rust/c-cases/c1/` (each program on both back ends, and under `FERMIUM_GC_STRESS=1`, which collects at
  every safe point), run by `cargo test` (`crates/fermium-cli/tests/c1_cases.rs`).
