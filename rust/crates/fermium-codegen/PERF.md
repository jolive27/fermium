# Performance of the LLVM back end (Fermium 2)

Measured 2026-09-26 on the shared development machine (Intel Xeon @ 2.10 GHz, 4 logical cores, Linux 6.18).
**The machine was heavily loaded** (load average 7–15 on 4 cores, from other agents' builds and test runs),
so every number here is noisier and slower than on an idle machine; they will be re-measured when it is quiet
(the same commands, below). To keep the comparison fair despite the load, the implementations run
**interleaved** (A B C D, A B C D, …, so a load change hits all of them) and the tables give **medians**; the CPU
column (user + system time of the process) is less sensitive to load than wall time.

- **Fermium 2, LLVM JIT**: `rust/target/release/fermium run --backend llvm` (release profile, LLVM 18 linked
  statically, the program JIT-compiled with MCJIT for this CPU).
- **Fermium 2, tree-walker**: the same binary, `--backend interp`.
- **Fermium 1.5**: the Python implementation (`python3 -m fermium.cli run`), which compiles with llvmlite.
- **Julia 1.12.7**: the programs in `benchmarks/julia/` (`julia --startup-file=no --project=benchmarks/julia`).

**Wall** = the whole process: start-up, compiling and running. **Inner** = the program's own `TIME_INNER` line,
the computation only (Julia's programs warm up first, so their Inner excludes JIT compilation). Every Fermium
row printed the same results as the LLVM back end ("Same results").

Reproduce: `python3 rust/tools/llvm_bench.py --bin rust/target/release/fermium -r 5` (add `--no-interp`
to skip the tree-walker, which takes minutes on nbody and forces).

## The benchmarks (medians of 5 runs after one warm-up, load average 7–9)

| Benchmark | Implementation | Wall | CPU | Inner | Same results |
|---|---|---:|---:|---:|:---:|
| blackbody | Fermium 2, LLVM JIT | 38.3 ms | 37.0 ms | 4.9 ms | ref |
| blackbody | Fermium 1.5 | 321.9 ms | 530.9 ms | 2.6 ms | ✓ |
| blackbody | Julia | 2.38 s | 2.48 s | 2.1 ms | n/a |
| forces (4 threads) | Fermium 2, LLVM JIT | 120.0 ms | 135.0 ms | 13.9 ms | ref |
| forces (4 threads) | Fermium 1.5 | 413.1 ms | 664.5 ms | 7.5 ms | ✓ |
| forces (4 threads) | Julia | 1.22 s | 1.36 s | 11.3 ms | n/a |
| nbody (10⁶ steps) | Fermium 2, LLVM JIT | 236.1 ms | 201.3 ms | 138.0 ms | ref |
| nbody (10⁶ steps) | Fermium 1.5 | 516.8 ms | 500.5 ms | 63.2 ms | ✓ |
| nbody (10⁶ steps) | Julia | 1.08 s | 930.0 ms | 75.7 ms | n/a |
| spring_adaptive | Fermium 2, LLVM JIT | 31.1 ms | 26.4 ms | 766 µs | ref |
| spring_adaptive | Fermium 1.5 | 304.2 ms | 432.1 ms | 599 µs | ✓ |
| spring_adaptive | Julia | 1.09 s | 1.07 s | 665 µs | n/a |
| spring_rk4 | Fermium 2, LLVM JIT | 144.9 ms | 118.3 ms | 101.0 ms | ref |
| spring_rk4 | Fermium 1.5 | 417.5 ms | 366.8 ms | 41.5 ms | ✓ |
| spring_rk4 | Julia | 1.18 s | 902.0 ms | 22.0 ms | n/a |
| startup | Fermium 2, LLVM JIT | 22.4 ms | 16.8 ms | — | ref |
| startup | Fermium 1.5 | 248.0 ms | 316.7 ms | — | ✓ |
| startup | Julia | 363.6 ms | 311.9 ms | — | n/a |
| unit_loop | Fermium 2, LLVM JIT | 44.1 ms | 29.3 ms | 11.4 ms | ref |
| unit_loop | Fermium 1.5 | 334.4 ms | 271.8 ms | 15.0 ms | ✓ |
| unit_loop | Julia | 2.50 s | 2.19 s | 6.4 ms | n/a |

**What this says, honestly:**

- **Whole programs (wall): Fermium 2 is the fastest of the three on every benchmark**, 2.2–11× faster than
  1.5 and 4.6–62× faster than Julia, because it starts in milliseconds (no Python, no Julia runtime) and
  compiles quickly.
- **The computation alone (Inner): the compiled code is 1.2–2.4× slower than Fermium 1.5's on four of the
  seven** (nbody 2.2×, spring_rk4 2.4×, forces 1.9×, blackbody 1.9×), about equal on spring_adaptive, faster on
  unit_loop (0.76×). Against Julia: 1.2–4.6× slower on those four (and 1.8× on unit_loop). So the compiled code is not yet at v1's
  (or Julia's) level for tight numeric loops; see "Why" below.

## The tree-walker against the LLVM back end (medians of 3, load average 12–15)

| Benchmark | LLVM JIT: Inner | tree-walker: Inner | speed-up |
|---|---:|---:|---:|
| blackbody | 8.2 ms | 295 ms | 36× |
| spring_adaptive | 0.73 ms | 10.8 ms | 15× |
| spring_rk4 | 205 ms | 1.83 s | 9× |
| unit_loop | 11.1 ms | 3.71 s | 330× |
| nbody (earlier run) | 0.2 s | 75 s | ~375× |
| forces (earlier run) | 62 ms | 16 s | ~260× |

(These two tables were measured at different loads: compare within a table, not across.)

## Start-up and compile time (spec §B4: well under 50 ms for small programs)

`FERMIUM_LLVM_TIME=1` prints the back end's split. Best of 7 runs of the release binary (loaded machine):

| Program | LLVM IR + optimization | JIT (machine code) | whole process (best) |
|---|---:|---:|---:|
| `print 1` | 2.0 ms | 2.9 ms | 11.5 ms |
| benchmarks/startup.fm | 2.0 ms | 3.0 ms | 13.2 ms |
| unit_loop.fm | 4.7 ms | 5.4 ms | 26.6 ms |
| spring_adaptive.fm | 8.4 ms | 12.0 ms | 39.5 ms |
| blackbody.fm | 9.7 ms | 11.9 ms | ≈ 40 ms |
| examples/01_pendulum.fm | 12.1 ms | 19.7 ms | — |
| nbody.fm (largest main) | 29.6 ms | 37.2 ms | — |
| forces.fm (two O(N²) loops, one parallel) | 42.9 ms | 73.0 ms | — |

Small programs start, compile and run in 11–15 ms in total, well under the 50 ms target (Fermium 1.5 needs
≈ 250 ms, Julia ≈ 360 ms just to start). Large mains take tens of ms to compile (MCJIT at -O2).

## Research programs (the red team's round-10 cases), wall time, one run each (load average ≈ 10)

Before mixed mode a single plot, fit or load sent the whole program to the tree-walker. Now the LLVM back end
compiles the program and hands only those statements to the tree-walker:

| Program | Fermium 2 before mixed mode | Fermium 2 now (LLVM) | Fermium 1.5 |
|---|---:|---:|---:|
| rutherford_mc (2×10⁷ Monte Carlo α's) | ≈ 100 s | 2.9 s | 2.0 s |
| hydrogen_levels | 15 s | 0.8 s | 1.7 s |
| bbn_network (stiff network) | > 900 s | 54 s (31 s CPU) | 31 s (27 s CPU) |
| u238_chain | — | 2.5 s | 14.0 s |
| shell_model_magic_numbers | — | 18.8 s | 34.8 s |
| the other six | — | 0.2–0.6 s | — |

## Why the compiled loops are slower than v1's, and what was done

The IR is in good shape (see `FERMIUM_DUMP_LLVM_OPT=1`): the n-body inner loop is ~40 instructions per pair
with no calls; list headers are read once per loop (`hoist.rs`), index checks are one integer compare (the
index is checked for being whole with a saturating conversion), TBAA metadata keeps variables and list lengths
in registers across element stores, the variable line bookkeeping is sunk out of loops, and code is generated
for this CPU. What still costs, compared with v1's compiled code:

- **Loop bounds are run-time values.** `for j from i + 1 to n` computes its trip count in floating point with
  the tree-walker's checks (NaN, step 0, the 2⁶² cap) on every entry, and `n = len(mass)` is a variable, so
  LLVM can't unroll the 5-body loops the way a constant bound allows. nbody's inner loop runs only 0–4
  iterations, so this per-loop overhead dominates.
- **The loop variable is a float** (`lo + i·step`, exactly as the tree-walker computes it) and every index
  converts it back to an integer; v1 had integer loop counters (D150, `int_loops`).
- **Numerics in fermium-runtime, not in compiled code.** spring_rk4 and blackbody spend most of their time in
  fermium-runtime's RK4 and quadrature (the same code the tree-walker uses; blackbody makes 214 919 integrand
  calls), each call going through a Rust closure and an error-flag check. v1 inlined its kernels into the
  compiled module.
- The per-call error check (a load and a branch after every call that can fail) and the `fdiv`/`powc`
  helpers' exact IEEE semantics (selects for 0 and NaN) add a few instructions to every division and power.

Next steps, in order of expected gain: integer loop counters when the start and step are whole numbers (with
the float value computed only where it is used as a number); constant trip counts for lists whose length is
fixed (literals never pushed to); compiling RK4 and the Gauss–Kronrod rule's inner loop into the module (the
integrand inlined); and caching compiled modules (for the compile times above).

## Correctness is not traded for speed

`python3 rust/tools/llvm_diff.py` runs all 3366 conformance programs with both back ends: every program the LLVM
back end compiles (2524 of the 2614 that get past the checker) prints exactly what the tree-walker prints,
except three where the tree-walker runs out of stack or time (the LLVM back end completes them). The full
conformance suite with the default back end (LLVM where it compiles the program): 3334 of 3366 pass and the
other 32 are documented divergences.
