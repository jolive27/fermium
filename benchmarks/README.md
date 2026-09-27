# Fermium benchmarks

Spec §4, Tier 3: the same five physics workloads written idiomatically in
Fermium, Julia, pure Python and NumPy/SciPy, timed on the same machine.
Latest numbers: [`RESULTS.md`](RESULTS.md) (raw data in `results.json`).

## Running

```sh
python benchmarks/run.py                          # all languages, R=5 (R=3 for Python/NumPy)
python benchmarks/run.py -r 3 --langs julia,python,numpy
python benchmarks/run.py --langs fermium,julia --benchmarks nbody,unit_loop
python benchmarks/run.py --quick                  # Python/NumPy nbody use 100k steps
python benchmarks/run.py --interleave -r 7 --langs fermium,fermium-base,julia,python,numpy
                                                  # the full M5 table: before/after, vs Julia
```

`--interleave` runs the languages of each benchmark in turn (A B C A B C …) instead of all runs of
one language and then the next, so a change in the machine's load hits every language alike; use it on
a shared machine. `fermium-base` is Fermium with its M5 speed-ups switched off (`FERMIUM_DISABLE=all`:
no integer loop counters, `parallel for` on one thread); with both `fermium` and `fermium-base` in
`--langs`, RESULTS.md gets a "Before/after M5" table. `--threads T` sets the thread count of the
parallel benchmark (`forces`) for Fermium (`FERMIUM_THREADS`) and Julia (`--threads`); default: all
cores.

Options: `-r/--repeats`, `--slow-repeats` (pure Python / NumPy), `--warmup`
(untimed runs first, default 1), `--langs`, `--benchmarks`, `--timeout`, `--interleave`, `--threads`.
The runner exits non-zero if any language's printed results disagree with Julia's.

### Requirements

* **Julia** — official binaries in `.tools/julia` (gitignored), packages in the
  project-local depot `.tools/julia-depot`. To reinstall:
  ```sh
  mkdir -p .tools && cd .tools
  curl -fLO https://julialang-s3.julialang.org/bin/linux/x64/1.12/julia-1.12.7-linux-x86_64.tar.gz
  tar xzf julia-1.12.7-linux-x86_64.tar.gz && mv julia-1.12.7 julia && cd ..
  JULIA_DEPOT_PATH=$PWD/.tools/julia-depot .tools/julia/bin/julia --project=benchmarks/julia -e 'using Pkg; Pkg.instantiate()'
  ```
  (`benchmarks/julia/Project.toml` + `Manifest.toml` pin QuadGK and Unitful.)
* **Python** 3 with `numpy` and `scipy`.
* **Fermium** — the `fermium` binary (Fermium 2) on `PATH` (the release download, or `make install`), else
  a build in this checkout (`rust/target/release/fermium`, then `rust/target/fast/fermium`), or set
  `FERMIUM_CMD` (e.g. `FERMIUM_CMD=fermium-legacy` or `FERMIUM_CMD="python3 -m fermium"` for the
  deprecated Fermium 1.5, which measured the current RESULTS.md). `fermium-base` sets
  `FERMIUM_DISABLE=all`, a Fermium 1.5 switch: with Fermium 2 it measures the same thing as `fermium`.
  Programs live in
  `benchmarks/fermium/<name>.fm` and run as `fermium run benchmarks/fermium/<name>.fm`
  from the repository root. A missing file or failing run is reported as
  skipped/failed, not fatal.

## What is measured

* **Wall** — median wall-clock time of the whole process: runtime startup,
  package loading, compilation/JIT and the computation. Fermium 2 keeps a program's
  compiled code in its compile cache (DECISIONS D317), so after the warm-up run its
  timed runs load that code instead of compiling (as Julia would with a precompiled
  package image); run with `FERMIUM_NO_CACHE=1` in the environment for wall times
  that include compiling every time. Inner times are the same either way.
* **Inner** — median of the `TIME_INNER <seconds>` line each program prints,
  timed inside the program around the computation only. Julia programs first
  run the kernel on a tiny problem (warm-up) so Inner excludes JIT compilation.
* nbody Inner times are normalized per step, so languages may use different N.

## Output contract (every language, including Fermium)

Each program prints result lines `<key> <number>` (a bare number is stored as
`value`), then `TIME_INNER <seconds>` (not needed for `startup`). The runner
compares result keys across languages with per-key tolerances (`TOLERANCE` in
`run.py`). Fermium programs should print the same keys:

| Benchmark | Keys printed |
|---|---|
| nbody | `N`, `energy_before`, `energy_after` (9 decimals) |
| spring_rk4 | `steps`, `x_10s` (10 significant digits) |
| spring_adaptive | `x_100s`, `accepted_steps` |
| blackbody | `sum_integrals`, `ratio_5778K` |
| unit_loop | `E_J` (Julia also `E_unitful_J` and `TIME_INNER_UNITFUL`) |
| forces | `U_J`, `ax1`, `azN` (12 digits); Fermium and Julia also `TIME_INNER_SERIAL` |
| startup | the number g (bare) |

## The benchmarks

All use CODATA 2022 exact/recommended constants: h = 6.62607015e-34 J s,
c = 299792458 m/s, k_B = 1.380649e-23 J/K, σ = 5.670374419e-8 W m⁻² K⁻⁴.

1. **nbody** — Computer Language Benchmarks Game n-body: Sun + 4 gas giants,
   offset momentum, symplectic-Euler `advance` with dt = 0.01, N = 1,000,000
   steps. Expected: `-0.169075164` → `-0.169086185`.
2. **spring_rk4** — damped spring (m = 1 kg, k = 100 N/m, b = 0.5 kg/s,
   x₀ = 0.1 m, v₀ = 0), classic RK4, dt = 1e-5 s, 0 → 10 s (10⁶ steps).
   Matches the analytic solution: x(10 s) = 0.006835571546 m.
   **spring_adaptive** — same ODE, 0 → 100 s, Dormand–Prince RK45,
   purely relative error control at rtol = 1e-6 in every language (Fermium
   `tolerance 1e-6`, whose norm is purely relative, D17; the others
   rtol = 1e-6, atol = 1e-30). Julia and pure Python use a hand-written DP45
   whose step-size controller and initial-step heuristic mirror
   `scipy.integrate.solve_ivp(method="RK45")`, so all three take exactly the
   same 4903 accepted steps. Fermium's controller differs (4297 steps).
   Exact: x(100 s) = 1.1176166148e-12 m. Fermium prints 1.11735e-12 m
   (relative error 2.4e-4), the others 1.11745e-12 m (1.5e-4): comparable,
   not identical, accuracy. (Until red-team round 1 this benchmark compared
   Fermium at a pure-relative 1e-10 with the others at rtol 1e-8 + atol 1e-10,
   which is not a like-for-like setting.)
3. **blackbody** — ∫ B_ν(ν, T) dν over 1e11–1e16 Hz for 1000 temperatures
   1000–10000 K, adaptive Gauss–Kronrod with rtol = 1e-10 (the tolerance of every
   Fermium integral, which has no per-integral setting; until M5 the others used
   1e-8 and so did ~25% fewer integrand evaluations): Julia `QuadGK.quadgk`
   (GK 7/15), SciPy `quad` (QUADPACK QAGS, GK 10/21, `epsabs=0`), pure Python a
   hand-written globally adaptive GK 7/15 with QuadGK's strategy. Prints the sum
   over T and ∫B_ν(5778 K) / (σT⁴/π) (≈ 1; the truncated band misses ~3e-11).
4. **unit_loop** — `E += ½·m·v²` with v = i·1e-6 m/s, m = 2 kg, i = 1…10⁷.
   Julia has two versions in one process: plain `Float64` and type-stable
   Unitful.jl quantities (`TIME_INNER_UNITFUL`) — the "units cost nothing" bar
   Fermium is aiming for. NumPy is vectorized (`arange`, in-place scale, `v @ v`).
5. **forces** (M5) — all-pairs gravity for N = 2000 bodies (4·10⁶ pair terms): each body's
   acceleration and the total potential energy U = −½ Σ G mᵢ mⱼ / r. Fermium runs it as a
   `parallel for` over i (D152) and again as a plain `for`; Julia as `Threads.@threads` over i and
   again serially; both with `--threads` threads (TIME_INNER) and one thread (TIME_INNER_SERIAL,
   the "1 thread" rows). Same formulas in the same order, so `ax1`, `azN` agree to all printed digits;
   U is added up in a different order in each language (Fermium: 256 fixed blocks; Julia: one value
   per body, then `sum`), so it agrees to ~1e-13. Pure Python and NumPy run one thread.
6. **startup** — `g = 4π²·1.20/2.21²`; only whole-process wall time matters.

## Fairness notes

* Julia code is type-stable, lives in functions (no globals in hot loops), uses
  immutable structs/tuples for small vectors, and is run with default
  optimization (`-O2`), one thread, `--startup-file=no`. No `@fastmath`/`@simd`,
  so float reductions stay sequential exactly like the other languages.
* Pure Python uses the fastest idiomatic style (locals, plain floats/lists,
  precomputed pairs as in the Benchmarks Game entries), no C extensions.
* NumPy cannot vectorize a sequential time-stepping loop; nbody (5 bodies) and
  fixed-step RK4 on a 2-vector are dominated by per-call NumPy overhead and are
  slower than pure Python. That is the honest idiomatic result, not a crippled
  version — for spring_adaptive it uses SciPy's own `solve_ivp`.
* NumPy unit_loop allocates one 80 MB array; first-touch page faults make the
  first run noticeably slower on this VM (the warm-up run absorbs some of it).
* Julia's wall time for blackbody/unit_loop includes loading QuadGK/Unitful.
