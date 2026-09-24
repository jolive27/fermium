# Design decisions

Each entry has **what** was decided, **why**, and the **alternatives** we considered. Newest entries are at the bottom.

---

## D1. Architecture: Python front end, typed IR, LLVM JIT through llvmlite
- **What:** The pipeline is lexer → parser → AST → checker, which does name resolution, types and dimension inference and produces a typed IR (`fermium/ir.py`). Codegen turns that into LLVM IR, which MCJIT compiles to native code. Symbolic calculus runs on the AST inside the checker (`calculus.py`).
- **Why:** This follows the spec, and the user is learning Python. Units are fully erased before codegen, so `codegen_llvm.py` contains no unit logic.
- **Alternatives:**
  - A tree-walking interpreter would be simpler but too slow for Tier 3.
  - Generating C would need a C compiler on the user's Mac.

## D2. Numeric kernels are generated as LLVM IR too
- **What:** Adaptive Gauss–Kronrod (G7/K15) quadrature, fixed-step RK4, adaptive Dormand–Prince RK45 and Hermite evaluation of ODE solutions are all built with llvmlite's IR builder (`ModuleGen._k_*`). The integrand or right-hand side is passed as a function pointer. The kernel is `alwaysinline`, so after inlining LLVM sees a direct call and can inline the user's formula into the solver loop.
- **Why:** "Numerics that must be fast are compiled to native code" (spec §2). Clang is not available on a beginner's Mac.
- **Alternatives:**
  - Calling SciPy from JIT code would add per-evaluation call overhead.
  - Writing the kernels in C would need a C compiler.

## D3. Non-hot-path features call back into Python
- **What:** `print`, `plot`, `load` and `fit` are ctypes callbacks (`runtime/core.py`). `fit` compiles the model to a native vectorized function, `void model(p*, cols**, n, out*)`, and SciPy's `least_squares` calls it through ctypes.
- **Why:** Spec §2 allows NumPy, SciPy and matplotlib here.

## D4. Runtime errors use setjmp/longjmp
- **What:** The generated entry point `fm_run` calls `_setjmp`. A runtime error (an index out of range, asking for an ODE solution outside its range, too many ODE steps, a failed `assert`) records a message through a callback and then `longjmp`s back to the entry point. The driver turns that into a one-line `FermiumRuntimeError`.
- **Why:** The user must not see Python tracebacks, and the REPL must survive errors.
- **Alternative:** Checking error flags after every call is slower and more code.

## D5. Dimensions: 7 SI base quantities with rational exponents, inferred by unification
- **What:** A dimension is a vector of `Fraction` exponents. During checking, dimensions are *affine expressions over unknowns* (`types.py`). `a + b` unifies the two dimensions (a linear equation on the exponents), which is solved by substitution (Kennedy-style inference).
  - A plain `0` gets a fresh unknown dimension, so `E = 0` followed by `E += ½ m v²` infers that E is an energy.
  - Unknown function parameters are inferred the same way, so `x(t) = A cos(ω t)` knows t is a time.
  - `fit` uses the same mechanism to work out the units of the fitted parameters.
  - Unknowns that are never constrained default to dimensionless.
- **Why:** Users should almost never write a type or unit annotation (spec §3.2).
- **Alternative:** Requiring an annotation on every function parameter was rejected as unfriendly.

## D6. Angles are dimensionless
- **What:** Following SI, `rad` is the unit 1 and `°` (also `deg`) is π/180. `sin`, `cos`, `exp` and the like require a dimensionless argument. `30°` is simply 0.5236.
- **Why:** This is simple and matches SI, SciPy and Julia's Unitful default.
- **Cost:** `ω` prints as `1/s`, not `rad/s`. Write `print ω in rad/s` to see `rad/s`. Hz and rad/s are the same dimension, so mixing them up is **not** caught.
- **Alternative:** Making the angle an 8th base dimension catches that mix-up, but then `sin(ω t)` needs `rad` everywhere and small-angle formulas break. Rejected.

## D7. Units after numbers
The rule (spec §3.4.2), refined:
1. A unit name right after a **digit literal** is a unit: `3 m`, `9.81 m/s²`. This does *not* apply after `½`, `π` or other symbols, so `½ m v²` is one half times the mass m times v².
2. Bracketed units are always units: `3 [m/s]`, `x [m]`, `f(x [m]) = ...`.
3. Everywhere else an identifier is a variable.
4. Continuing a unit expression after the first unit:
   - `/` followed by a unit name continues the unit (`50 N/m`, `3 m/s`), even if a variable has that name. You get a warning when it does.
   - A space or `·`/`*` followed by a unit name continues the unit **only if you haven't defined a variable with that name**. So `70 kg g` with your own `g` is 70 kg × g, with a warning.
5. If the first unit after a number is also one of your variables (`0.2 m` after `m = 0.5 kg`), Fermium prints a warning once per name, suggesting `2*m` or `3 [m]`.
- **Unit names we deliberately left out, because they collide with physics variables:**
  - `h` for hour: use `hr`.
  - `t` for tonne: use `tonne`.
  - `G` for gauss: use `gauss` or `Gs`.
  - `d` for day: use `day`.
- **Why:** This matches what physicists write on paper while keeping the rule predictable.

## D8. Implicit multiplication binds tighter than `/` and `*`
- **What:** `h c / λ k_B T` means (h c)/(λ k_B T), as in textbooks and papers. As a result, `1/2x` = 1/(2x) and `1/2 m v²` = 1/(2 m v²). When the numerator is a number and the denominator starts with a number (`1/2 m`), Fermium warns and suggests `½ m v²` or `(1/2) m v²`.
- **Powers bind tighter than juxtaposition:** `4π² L` = 4·π²·L and `x^2y` = x²·y.
- **Other rules:**
  - Unary minus applies to the whole implicit product: `-A ω sin(ω t)`.
  - `LT` is one name. If it is undefined but `L` and `T` exist, the error suggests `L T`.
  - `f(x)` with no space is a call, or multiplication if `f` is a number (`k(x+1)`). `f (x)` with a space is multiplication, unless `f` is a function.
- **Alternative:** Python/C precedence, where `h c / λ k_B T` = ((h c)/λ)·k_B·T, which surprises physicists.

## D9. Unicode ⇄ ASCII: one canonical spelling
- **What:** The lexer maps each ASCII spelling to its symbol. Every `_`-separated *segment* of a name that is a Greek letter name is replaced by the letter: `omega_0` ≡ `ω₀` ≡ `ω_0`, `theta` ≡ `θ`, `hbar` ≡ `ħ`, `inf` ≡ `∞`. Subscript digits become `_N`.
  - `π` is always a single token, so `2πf` is 2·π·f.
  - Other Greek letters are ordinary identifier letters, so `ωt` is one name, and the error for it suggests `ω t`.
  - `sqrt` ≡ `√`, `integral` ≡ `∫`, `partial` ≡ `∂`, `x^2` ≡ `x²`, `*` ≡ `·` ≡ `×`, `+-` ≡ `±`, `<=` ≡ `≤`, `deg` ≡ `°`, `degC` ≡ `°C`, `angstrom` ≡ `Å`, `Msun` ≡ `M☉`, `um` ≡ `μm`.
- **`fermium fmt`** rewrites tokens and copies the spaces and comments between them unchanged. Round-trips are tested by comparing ASTs.
  - A name like `ΔE` has no ASCII spelling (`Delta_E` is a *different* name). `fmt --ascii` leaves it alone with a warning, so meaning is preserved.

## D10. Look-alike characters
- **Normalized silently, because they are the same letter:** micro sign µ → μ, ohm sign Ω → Ω, Kelvin sign K → K, Å sign → Å, ϵ → ε, ϕ → φ, ℏ → ħ, and the minus sign −, primes ′ ″ and smart quotes.
- **Replaced with a warning:** Greek and Cyrillic letters that are pixel-identical to Latin ones (Greek capital Α, Cyrillic а, ...).
- **Warned about:** a program that uses two names that differ only by a look-alike, such as `v` and `ν`, `o` and `ο`, or `p` and `ρ`. They stay different variables, as in physics, but the warning says so.

## D11. Printing: significant figures and display units
- **Significant figures:** A literal with a decimal point carries significant figures (`1.20` → 3). Integers are exact.
  - A computed result prints with max(2, the fewest significant figures among its inputs). All-exact inputs print up to 6 significant figures with trailing zeros trimmed.
  - A value that came straight from a literal prints exactly as written.
  - This reproduces the spec's `9.70 m/s²`, `10 1/s` and `1.0 J`.
- **Display unit:**
  - The unit the user wrote (literal or `in`) is kept through `+`, `-` and scaling by plain numbers.
  - Otherwise Fermium uses a preferred SI unit for the dimension (N, J, W, Pa, N/m, J s, ...), falling back to base SI units (`kg m/s³`).
  - Very large or small numbers print as `6.674×10⁻¹¹`.

## D12. Temperatures with °C/°F are absolute
- **What:** `20 °C` means 293.15 K, and `T in °C` subtracts 273.15. `°C` can't be combined with other units (`J/°C` is an error). Use K for temperature differences and in formulas.
- **Why:** An affine unit is only well defined for absolute values.

## D13. Static typing: a variable keeps its dimension
- **What:** Reassigning `x` with different units is an error ("x is length [m]; it can't now hold time [s]"). The REPL allows redefinition.
  - `solve` may re-bind a name (the spec's snippet re-uses `x`).
  - Built-in constants (`c`, `h`, `e`, `G`, ...) can be shadowed by your own variables. This is common: `h = 10 m` for a height.
- **Why:** Unit safety comes first (spec §1).

## D14. Functions are monomorphised on argument dimensions
- **What:** `F(x) = k x` is checked separately for each combination of argument types and dimensions it is called with. Codegen emits one native function per instance.
  - Calling a scalar function with a list maps it over the list (`f(xs)`). A parameter counts as a list parameter only if the body indexes it, loops over it or passes it to a list function.
  - Recursive functions work: a recursive call's return dimension starts as an unknown and is unified at the end.

## D15. Lists are 1-based, and `for` ranges include both ends
- **What:** `xs[1]` is the first element and `xs[end]` the last, as in Julia, Fortran and maths notation (x₁). `for i from 1 to 10` includes 10. `for t from 0 s to 1 s step 0.1 s` computes t = 0 + i·step, so errors don't accumulate, and it includes the end (up to 1e-9 relative slack).
- **Alternative:** Python's 0-based indexing, which is less natural for physics notation.

## D16. `e` is the elementary charge
- **What:** `e^x` is an error that says to write `exp(x)`.
- **Why:** In physics code `e` is the charge far more often, and silently doing either would be dangerous.

## D17. ODEs: `solve`
- **Unknowns:** the names that appear with primes (`x'`, `x''`) or as `d/dt x`. Each equation is solved for its highest derivative by symbolic linear isolation (`calculus.isolate`), so `m x'' = -k x - b x'` works.
- **Checks:** Initial conditions give the units of the unknowns, and all of them are required. Both sides of each equation are dimension-checked.
- **Methods:** `step h` selects fixed-step RK4. The step is adjusted slightly so the range is covered exactly. Without a step, Fermium uses Dormand–Prince 5(4) with rtol = 1e-9 and a **scale-free** error norm: sc = rtol·(max(|y|,|y_new|) + 10⁻³·max|y| so far). This keeps the tolerance meaningful whether the values are 10⁻¹⁵ m or 10³⁰ kg.
- **Results:** The result is a dense solution. `x(2 s)` uses cubic Hermite interpolation with the stored derivatives. `x'(t)` and `x''(t)` also work, and so do `plot x vs t`, `plot y vs x` (phase or orbit plots), `values(x)`, `times(x)`, `x[end]` and `max(x)`.

## D18. `fit`
- **Parameters** are the names in the model that are not data columns, constants or functions. If that set is empty, names that already have a value are fitted, using their value as the starting guess. This is exactly the spec's `fit T = 2π √(L / g) to data`, where `g` already exists.
- **Units** of the parameters are inferred by dimension unification against the column units.
- **Starting guesses:** missing guesses are found by scanning powers of ten. Then SciPy `least_squares` (Levenberg–Marquardt) runs.
- **Report:** values with units and standard errors, which are plain numbers until uncertainties (§3.7) exist.

## D19. Uncertainties (§3.7) are reserved
- **What:** `±` and `+-` are lexed as operators and give a friendly "planned for a future version" error.
- **Why this makes them easy to add later:** The IR carries `ty`, `sf` and `hint` per expression. Adding an uncertainty is a new `NumTy` flavour whose LLVM type is `{double value, double sigma}`, plus propagation rules in `arith`. See BACKLOG.

## D20. Blocks use indentation, and the colon is optional
- **What:** `if x > 0 m` followed by an indented block. A trailing `:` is accepted for people coming from Python, and `then` is accepted after an `if` condition. A line ending in a binary operator or comma continues on the next line.

## D21. `~=` / `≈` means "equal within 10⁻⁶ relative"
- **Why:** It is handy for `assert` in lessons and tests.

## D22. Loading data
- **What:** `load "file.csv"` reads the header **at compile time**, so column units are known to the checker. Headers look like `L [m], T [s]`. The values are read at run time and converted to SI. Paths are relative to the program's folder.

## D23. LLVM optimization level is O2
- **What:** Measured on the benchmarks, O3 took 300 ms to compile nbody against O2's 100 ms, and the run times were the same. Compile time counts toward "time to first result", so the default is O2.
