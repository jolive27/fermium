# Performance of the LLVM back end (Fermium 2)

Re-measured **2026-09-26 12:31–12:42 UTC** (spec §B8.3) on the shared development machine: Intel Xeon @ 2.10 GHz,
4 logical cores (a VM), Linux 6.18. **The machine was not idle**: load average 1.4–2.4 during the runs, about
0.7 of a core taken the whole time by another agent's long-running `conformance/harvest.py` (it could not be
stopped). That was the quietest state available; earlier tables in this file were measured at load 7–15. To keep
the comparison fair the implementations run **interleaved** (A B C D, A B C D, …, so a load change hits all of
them alike) and the tables give **medians**.

- **Fermium 2**: `rust/target/release/fermium run --backend llvm` (release profile, LLVM 18 linked statically, the
  program JIT-compiled with MCJIT for this CPU), commit 657a1f0 (branch worktree-agent-ad7715677ca5c11bd).
- **Fermium 2 before**: the same, built from 99559f2 (claude/v2-rust before this work).
- **Fermium 1.5**: the Python implementation (`python3 -m fermium.cli run`, legacy/), which compiles with llvmlite.
- **Julia 1.12.7**: the programs in `benchmarks/julia/` (`julia --startup-file=no --project=benchmarks/julia`).

**Wall** = the whole process: start-up, compiling and running. **Inner** = the program's own `TIME_INNER` line,
the computation only (Julia's programs warm up first, so their Inner excludes JIT compilation). Every Fermium
row printed the same results as the LLVM back end ("Same results").

Reproduce: `python3 rust/tools/llvm_bench.py --bin rust/target/release/fermium --before OLD_BIN --no-interp -r 11`
(drop `--no-interp` to add the tree-walker, which takes minutes on nbody and forces). The same day's
`benchmarks/run.py --interleave -r 7 --langs fermium,fermium-1.5,julia,python,numpy` run (benchmarks/RESULTS.md,
load 1.4–2.9) agrees with this table within the noise described below.

## The benchmarks (medians of 11 runs after one warm-up, load average 1.4–2.4)

| Benchmark | Implementation | Wall | CPU | Inner | Same results |
|---|---|---:|---:|---:|:---:|
| blackbody | Fermium 2 | 34.2 ms | 33.7 ms | 3.6 ms | ref |
| blackbody | Fermium 2 before | 33.7 ms | 31.6 ms | 4.8 ms | ✓ |
| blackbody | Fermium 1.5 | 296.4 ms | 546.7 ms | 2.6 ms | ✓ |
| blackbody | Julia | 2.46 s | 2.54 s | 2.0 ms | n/a |
| forces (4 threads) | Fermium 2 | 119.6 ms | 129.2 ms | 9.9 ms | ref |
| forces (4 threads) | Fermium 2 before | 98.4 ms | 108.8 ms | 7.0 ms | ✓ |
| forces (4 threads) | Fermium 1.5 | 404.0 ms | 664.0 ms | 6.5 ms | ✓ |
| forces (4 threads) | Julia | 1.14 s | 1.36 s | 11.0 ms | n/a |
| nbody (10⁶ steps) | Fermium 2 | 126.8 ms | 125.6 ms | 67.8 ms | ref |
| nbody (10⁶ steps) | Fermium 2 before | 166.6 ms | 164.7 ms | 74.2 ms | ✓ |
| nbody (10⁶ steps) | Fermium 1.5 | 371.7 ms | 623.8 ms | 64.1 ms | ✓ |
| nbody (10⁶ steps) | Julia | 859.0 ms | 975.3 ms | 64.1 ms | n/a |
| spring_adaptive | Fermium 2 | 24.4 ms | 23.7 ms | 714 µs | ref |
| spring_adaptive | Fermium 2 before | 22.6 ms | 22.1 ms | 787 µs | ✓ |
| spring_adaptive | Fermium 1.5 | 272.0 ms | 525.2 ms | 566 µs | ✓ |
| spring_adaptive | Julia | 925.0 ms | 1.05 s | 674 µs | n/a |
| spring_rk4 | Fermium 2 | 69.8 ms | 69.3 ms | 33.2 ms | ref |
| spring_rk4 | Fermium 2 before | 113.9 ms | 112.3 ms | 83.3 ms | ✓ |
| spring_rk4 | Fermium 1.5 | 265.1 ms | 514.7 ms | 33.9 ms | ✓ |
| spring_rk4 | Julia | 798.1 ms | 919.7 ms | 18.2 ms | n/a |
| startup | Fermium 2 | 16.8 ms | 16.5 ms | — | ref |
| startup | Fermium 2 before | 15.1 ms | 14.7 ms | — | ✓ |
| startup | Fermium 1.5 | 158.6 ms | 276.7 ms | — | ✓ |
| startup | Julia | 235.8 ms | 308.2 ms | — | n/a |
| unit_loop | Fermium 2 | 28.8 ms | 27.9 ms | 6.2 ms | ref |
| unit_loop | Fermium 2 before | 26.6 ms | 26.2 ms | 6.1 ms | ✓ |
| unit_loop | Fermium 1.5 | 167.3 ms | 312.5 ms | 6.1 ms | ✓ |
| unit_loop | Julia | 2.06 s | 2.17 s | 6.1 ms | n/a |

forces with one thread (its `TIME_INNER_SERIAL` line, the same loop without `parallel`; medians of 8
interleaved runs, same session): Fermium 2 19.0 ms, Fermium 2 before 19.0 ms, Fermium 1.5 18.8 ms, Julia 17.5 ms.

**What this says, honestly:**

- **Whole programs (wall): Fermium 2 is the fastest of the three on every benchmark**: 2.9–11× faster than 1.5
  and 7–72× faster than Julia, because it starts in milliseconds (no Python, no Julia runtime) and compiles
  quickly. (Differences of 1–2 ms in wall time between the two Fermium 2 binaries are not code: copying the same
  binary to another file moves its start-up time by that much.)
- **The computation alone (Inner)**, Fermium 2 against Fermium 1.5 (same LLVM, same algorithms): **level** on
  nbody (1.06×), spring_rk4 (0.98×), unit_loop (1.0×) and forces on one thread (1.01×); **still slower** on
  blackbody (**1.4×**), spring_adaptive (**1.26×**) and forces on 4 threads (**1.5×**, but see below). Before this
  work (the "before" rows) the compiled loops were 1.2–2.5× slower than 1.5's on four of these (spring_rk4
  2.5×, blackbody 1.8×, spring_adaptive 1.4×, nbody 1.2×).
- **Against Julia (Inner)**: level on nbody (1.06×), unit_loop (1.0×) and spring_adaptive (1.06×), faster on
  forces with 4 threads (0.9×); **slower** on spring_rk4 (**1.8×**; Fermium also stores the whole 10⁶-step
  trajectory, 40 MB, D151), blackbody (**1.8×**) and forces on one thread (1.09×).
- **forces on 4 threads is too noisy here to rank.** Its times are bimodal (about 6.5 ms or about 10 ms, run by
  run, for every build and for Fermium 1.5), presumably depending on where the threads land next to the other
  agent's process. In the A/B runs of this session the new build's 4-thread median was 5–40% above the
  before build's (level in one run); with 1 or 2 threads the two builds are level. Switching off the versioned
  loops (below) did not give a consistent answer either. So forces on 4 threads may have lost some speed in this
  work; the table reports what the interleaved run measured.

## The tree-walker against the LLVM back end (earlier measurement, before this work: medians of 3, load average 12–15)

| Benchmark | LLVM JIT: Inner | tree-walker: Inner | speed-up |
|---|---:|---:|---:|
| blackbody | 8.2 ms | 295 ms | 36× |
| spring_adaptive | 0.73 ms | 10.8 ms | 15× |
| spring_rk4 | 205 ms | 1.83 s | 9× |
| unit_loop | 11.1 ms | 3.71 s | 330× |
| nbody (earlier run) | 0.2 s | 75 s | ~375× |
| forces (earlier run) | 62 ms | 16 s | ~260× |

(Measured at a different load and with the earlier LLVM back end: compare within this table only. The LLVM
back end's Inner is now 1.3–2.5× lower on blackbody, spring_rk4 and nbody than in this table.)

## Start-up and compile time (spec §B4: well under 50 ms for small programs)

`FERMIUM_LLVM_TIME=1` prints the back end's split. Best of 7 runs of the release binary, 2026-09-26 12:49 UTC
(load average 3.5):

| Program | LLVM IR + optimization | JIT (machine code) | whole process (best) |
|---|---:|---:|---:|
| `print 1` | 1.9 ms | 3.0 ms | 11.9 ms |
| benchmarks/startup.fm | 1.9 ms | 2.8 ms | 12.1 ms |
| unit_loop.fm | 3.6 ms | 4.8 ms | 21.5 ms |
| spring_adaptive.fm | 3.2 ms | 5.7 ms | 17.6 ms |
| blackbody.fm | 6.2 ms | 8.3 ms | 26.1 ms |
| examples/01_pendulum.fm | 6.2 ms | 9.9 ms | 53.9 ms |
| nbody.fm (largest main) | 22.7 ms | 18.4 ms | 119.6 ms |
| forces.fm (two O(N²) loops, one parallel) | 34.5 ms | 38.4 ms | 115.1 ms |

Small programs start, compile and run in 12–26 ms in total, well under the 50 ms target (Fermium 1.5 needs
≈ 160 ms, Julia ≈ 240 ms just to start). Large mains take tens of ms to compile (MCJIT at -O2). nbody now
compiles in about half the time it took before this work (constant trip counts leave the optimizer less to
do); forces takes about 13 ms more (its versioned loops are compiled twice).

## Research programs (the red team's round-10 cases), wall time, one run each (earlier measurement, load average ≈ 10)

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

## What made the compiled loops slower than v1's, and what was done (D273)

Before this work PERF.md listed four causes; the state now:

- **Loop bounds were run-time values** (`n = len(mass)`, a trip count computed in floating point on every entry).
  Now *module constants* (`llvm/consts.rs`): a module variable set once, before any user function is called,
  to a value known then, is a constant in the compiled code, and a list of known, fixed length keeps it, so
  `len(mass)` is 5 and nbody's loops have constant bounds; `for i from a to b` with whole a, b and step ±1
  counts in integers. v1 got the same facts from LLVM's globalopt.
- **The loop variable was a float.** Integer loop variables (5dcff42) now also in `parallel for` bodies and in
  comparisons (`j != i` is an integer compare).
- **Index checks and aliasing.** A small straight loop body indexed by integer loop variables is compiled
  twice, and one test before the loop picks the copy without index checks when every index is inside
  (`hoist.rs versioned_loop`; a failing program still fails in the checked copy, same message, same line). Lists
  bound once to a new list and only indexed get a TBAA type of their own, so a store into `vx[j]` no longer
  makes LLVM reload `x[i]` or `mass[j]` (v1's lists were separate global arrays, which told LLVM the same).
- **Numerics in fermium-runtime.** Fixed-step RK4 (`solve … step h`) now takes its steps in compiled code with
  the right side inlined and the state in registers (`ode.rs rk4_inline`, operation for operation
  `ode::rk4_plain`): spring_rk4 went from 83 to 33 ms, level with v1. The quadrature is still fermium-runtime's
  adaptive Gauss–Kronrod calling the compiled integrand through a function pointer; its sentinel cache no longer
  uses SipHash. Compiling the 15-point panel into the module with the integrand inlined (as v1 did) was tried
  and dropped: only about 15% fewer instructions per integral on blackbody (the rest is the adaptive
  bookkeeping and `exp`) for about 0.3 s more compile time.

**Still slower, and why:**

- **blackbody (1.4× v1, 1.8× Julia):** about 240 instructions per integrand evaluation, of which `exp` is ~60
  and the compiled integrand ~35; the rest is the adaptive quadrature in Rust (the u → x map, the node
  bookkeeping, the closure and its error-flag check). v1 ran the whole panel loop in compiled code.
- **spring_adaptive (1.26× v1, level with Julia):** the Dormand–Prince solver and its step control run in
  fermium-runtime, calling the compiled right side; v1 compiled the whole solver. Absolute cost: 0.15 ms.
- **spring_rk4 (1.8× Julia, level with v1):** the loop is bound by its dependency chain (each stage needs the
  previous one); Julia's version keeps only the current state, Fermium stores all 10⁶ samples (40 MB) for the
  dense solution, and no floating-point operation may be reordered or fused (identical results).
- **forces on 4 threads:** see above (noise, possibly a small loss).

Experiment switches (rust/BUILD.md): `FERMIUM_LLVM_PASSES='default<O3>'` gave no measurable gain on these
benchmarks for 3–30 ms more compile time, so the pipeline stays `default<O2>`.

## Correctness is not traded for speed

`python3 rust/tools/llvm_diff.py` runs all 3366 conformance programs with both back ends: every program the LLVM
back end compiles (2548 of the 2640 that get past the checker) prints exactly what the tree-walker prints,
except one where the tree-walker runs out of stack (functions/8c5765b57c68; the LLVM back end completes it).
The full conformance suite with the default back end (LLVM where it compiles the program): 3334 of 3366 pass
and the other 32 are documented divergences. `rust/tools/aot_diff.py` (executables from `fermium build`) on the
ode, lists, control-flow, parallel, integrals and functions areas: 931 of 931 identical to `fermium run`.
(All three on 2026-09-26, after this work.)
