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
- **Why:** "Numerics that must be fast are compiled to native code" (spec §2). A C compiler is **optional**: a beginner's Mac often has none (it comes with the Xcode command-line tools), and running programs, the REPL, `check` and `fmt` never need one, because llvmlite brings its own LLVM. Only `fermium build` (D25) needs a C compiler, to link the executable; `fermium doctor` reports whether one was found.
- **Alternatives:**
  - Calling SciPy from JIT code would add per-evaluation call overhead.
  - Writing the kernels in C would make a C compiler necessary for every program, not just for `fermium build`.

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
2. Bracketed units are always units: `3 [m/s]`, and parameter annotations `f(x [m]) = ...`. (A variable can't be declared with one: `x [m] = 3` is a parse error.)
3. Everywhere else an identifier is a variable.
4. Continuing a unit expression after the first unit:
   - `/` written without a space before it, and followed by a unit name, continues the unit (`50 N/m`, `3 m/s`), even when you have a variable with that name.
   - `/` with a space before it, followed by the name of one of *your* variables, divides by the variable. So with `g = 9.81 m/s²`, `20 m/s / g` is 2.04 s and not "per gram", and `2.898e-3 m K / T` divides by the temperature T, not by tesla. (Changed after the bootcamp author hit exactly this trap.)
   - A space or `·`/`*` followed by a unit name continues the unit **only if you haven't defined a variable with that name**. So `70 kg g` with your own `g` is 70 kg × g, with a warning.
5. If the first unit after a number is also one of your variables (`0.2 m` after `m = 0.5 kg`), Fermium prints a warning once per name: "that's fine if you meant the unit; to multiply by your variable write 0.2*m".
- **Unit names we deliberately left out, because they collide with physics variables:**
  - `h` for hour: use `hr`.
  - `t` for tonne: use `tonne`.
  - `G` for gauss: use `gauss` or `Gs`.
  - `d` for day: use `day`.
- **Why:** This matches what physicists write on paper while keeping the rule predictable.

## D8. Implicit multiplication binds tighter than `/` and `*`
- **What:** `h c / λ k_B T` means (h c)/(λ k_B T), as in textbooks and papers. As a result, `1/2x` = 1/(2x) and `1/2 mass v²` = 1/(2 mass v²). When the numerator is a number and the denominator is a number times a variable (`1/2x`, `1/2 x`, `1/2 mass v²`), Fermium warns and suggests `(1/2)` or `½`.
  - **Not warned:** when the name after the number is a **unit**, D7 applies first: `2 m` is two metres, so `1/2 m` is 0.5 1/m and `1/2 kg` is 0.5 1/kg, with no precedence warning. If you also have a variable `m`, you get D7's unit-or-variable warning instead (rule 5), and the value is still 0.5 1/m. Write `½ m v²` for the kinetic energy.
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
- **Differences (A47):** `a - b` with `b` in °C is always a temperature difference, shown in K, whatever unit `a` was written in. `300 K - 20 °C` is 6.85 K, and Newton's law of cooling `T' = -(T - Ta)/τ` works with `Ta = 20 °C` and `T(0) = 90 °C`. A value in K may be an absolute temperature or a difference, and both readings give the same number in K. Still errors: `°C + °C` and negating a °C value. `20 °C - 5 K` stays 15 °C (a change of 5 K). Alternative: tracking "absolute or difference" as part of the type. That is more precise, but it needs a new kind of type for one unit, so it was rejected for now.

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
- **What:** `e^x`, with a variable or negative exponent, is an error that says to write `exp(x)`. A fixed positive power, such as `e²`, `e^2` or `e⁴`, is the charge raised to that power: `e²/(4π ε₀)` is everywhere in nuclear physics. If someone meant exp(2), the unit checker catches it, because e² has units of C².
- **Why:** In physics code `e` is the charge far more often, and silently doing either would be dangerous.

## D17. ODEs: `solve`
- **Unknowns:** the names that appear with primes (`x'`, `x''`) or as `d/dt x`. Each equation is solved for its highest derivative by symbolic linear isolation (`calculus.isolate`), so `m x'' = -k x - b x'` works.
- **Checks:** Initial conditions give the units of the unknowns, and all of them are required. Both sides of each equation are dimension-checked.
- **Methods:** `step h` selects fixed-step RK4. The step is adjusted slightly so the range is covered exactly. Without a step, Fermium uses Dormand–Prince 5(4) with rtol = 1e-9 and a **scale-free** error norm: sc = rtol·(max(|y|,|y_new|) + 10⁻³·max|y| so far + 10⁻³·max|dy/dt| so far·(t₁−t₀)). This keeps the tolerance meaningful whether the values are 10⁻¹⁵ m or 10³⁰ kg. *(Revised after adversarial tests A5 and A15. The norm is now **purely relative**: sc = rtol·(max(|y|,|y_new|) + |y_new − y|). The step's own change stands in for the size when a component crosses zero. When shrinking the step no longer reduces the error, which happens for an unknown that starts at exactly 0 (x' = t⁴), the step is accepted. Decays such as e⁻⁶⁰ ≈ 10⁻²⁷ now keep full relative accuracy, and the earlier absolute floors are gone.)*
- **Results:** The result is a dense solution. `x(2 s)` uses cubic Hermite interpolation with the stored derivatives. `x'(t)` is the derivative of that cubic, which is third-order accurate. `x''(t)` also works, and so do `plot x vs t`, `plot y vs x` (phase or orbit plots), `values(x)`, `times(x)`, `x[end]` and `max(x)`.

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

## D24. Vectors (a Tier 5 feature)
- **Syntax:**
  - `<3, 4> m/s` or `<1 m, 2 m, 3 m>` builds a 2- or 3-vector. All components share one dimension.
  - `vec(a, b, c)` is the same thing written as a function call.
  - `<` starts a vector only where an expression starts, so `a < b` is still a comparison.
- **Operations:**
  - `|v|` or `norm(v)` for the length, `unit(v)` for the unit vector.
  - `a · b` or `a * b` is the dot product.
  - `a × b` or `cross(a, b)` is the cross product. In 2-D the cross product is a number (its z-component).
  - `v.x`, `v.y`, `v.z` or `v[1]` pick a component.
  - Vectors add and subtract, and scale by numbers.
- **`×` gets its own token:** it is the cross product for vectors and plain multiplication for numbers. `fmt --ascii` leaves `×` alone with a warning, because there is no ASCII operator for it. Use `cross(a, b)`.
- **Code generation:** vectors compile to LLVM `<n x double>` values, so they cost nothing at run time.
- **ODEs:** unknowns can be vectors: `solve r'' = -G M r/|r|³ with r(0) = <1, 0> AU, r'(0) = <0, 30> km/s ...`. Each vector unknown takes n slots of the state. `r.x` and `r.y` are ordinary solution components, so `plot r.y vs r.x` draws the orbit.
- **Not done:** lists of vectors. (Matrices, 4-vectors and a unit per component came later: D33.)

## D25. AOT executables: `fermium build`
- **What:** The same LLVM module the JIT runs is compiled to an object file (`TargetMachine.emit_object`) and linked by the system C compiler with `fermium/runtime/aot_rt.c`. That file is a C version of the printing and error callbacks. A generated `fm_tables.c` holds each print format's display unit and significant figures, and the program's texts.
- **Limit:** `plot`, `load` and `fit` were refused at first, because they need Python (matplotlib and SciPy). D31 lifts this.
- **Testing:** the tests build every example that doesn't use them and check that the executable prints exactly what `fermium run` prints.
- **Alternative:** embedding Python in the executable. Rejected as too heavy.

## D26. Lists are shared references, like Python lists
- **What:** A list value is a pointer to a heap header `{data, length, capacity}`. `ys = xs`, passing a list to a function and `for x in xs` all share the header. So `push` and `xs[i] = …` are seen through every name, exactly like Python, and the reference interpreter behaves the same way.
- **Growing:** `push` grows a list by allocating a new block and copying. The old block is never freed, so a loop that is still reading it stays valid. (The adversarial tester found that `realloc` caused a use-after-free.)
- **Memory:** there is no garbage collector yet, so memory is only reclaimed when the program exits. That is fine for scripts, and is noted in the known issues.
- **Alternative:** value semantics (copy on assignment) would surprise Python learners, and every function call would have to copy the list.

## D27. `rev`, `rpm` and Hz (A46)
- **What:** Angles stay plain numbers (D6), so `rev` = 2π and `rpm` = rev/min = 2π/60 1/s. `1 rev/min in Hz` is 0.10472 Hz, an angular frequency in rad/s, and **not** 1/60 Hz. When a value written with `rev`, `rpm`, `rad`, `°` or `arcmin`/`arcsec` is converted `in Hz` (or kHz, MHz, ...), the checker warns that angles are plain numbers and suggests `in rev/s` for counting turns. `60 rpm in rev/s` = 1 rev/s.
- **Why:** With rad = 1, Hz and rad/s are the same dimension. If `rpm` meant 1/60 Hz, then `60 rpm in rad/s` would be 1 rad/s, off by 2π the other way. Keeping `rev` = 2π is self-consistent, and the warning fires only in the one case that is surprising.
- **Alternatives:** Refuse `in Hz` for such values. This was rejected because `ω in Hz` from a variable would be refused or allowed depending on how it was written. Making the angle a base dimension was already rejected in D6.

## D28. Calculus edge cases from the adversarial pass (A17, A18, A26, A35, A38, A49, A52)
- **Real odd roots (A26):** `x^(n/q)` with `q` odd (1/3, 2/3, -2/3, 3/5, ...; the exponent is recognised as a fraction with denominator < 100) is the real root for negative `x` too: `(-8)^(2/3) = 4`, like `∛`. So `f(x) = x^(1/3)` and its derivative `1/(3x^(2/3))` agree at `x = -8`. Even roots of negatives stay NaN. Derivatives print such exponents as fractions (`x^(2/3)`, not `x^0.666666666667`). *Alternative:* NaN for every non-integer power of a negative number (what C's `pow` does) — rejected because `∛x` and `x^(1/3)` already gave the real root and physics formulas use it.
- **Stable evaluation of derivatives (A52):** a derivative is *printed* as the rules give it (`-exp(x)/(exp(x) + 1)²`), but *evaluated* after a rewrite (`calculus.stabilize`) that cancels the growth of `exp(u)/(exp(u) + r)^k` into `1/((exp(u) + r)^(k-1) (1 + r exp(-u)))` and `sinh(u)/cosh(u)^k` into `tanh(u)/cosh(u)^(k-1)`, splitting a sum over such a denominator into one fraction per term. So the Fermi function's derivative far from μ, `tanh''(1000)` and the Planck spectrum's derivative are 0 instead of ∞/∞ = NaN. *Alternative:* evaluate `a/b` as 0 when `|b| = ∞` — doesn't help, since the numerator overflows too.
- **`sign` of a vector (A18):** `sign(v)` is `v/|v|`, the unit vector (the usual generalisation of sign to complex numbers and vectors). The rule d|u| = sign(u) du then works for vectors: `d/dt |r(t)|` = `sign(r(t)) r'(t)` = r̂·r'. *Alternative:* a separate vector rule in the differentiator, which would need the types there (it works on the untyped AST).
- **Indefinite integrals with parameters (A35):** SymPy is asked for the generic answer (`conds="none"`), so `∫ cos(ω t) dt` is `sin(ω t)/ω`, as in a textbook, without a special case for ω = 0. A Piecewise that depends on the variable itself (`∫ |x| dx`) becomes an `if … then … else` formula. *Alternative:* declaring every parameter `nonzero` — doesn't cover `∫ x^a dx` (a = -1).
- **`partial^2/partial x^2` (A38):** the parser accepts `^n` orders on ∂ like it does on d (`d^2/dt^2`), so what `fmt --ascii` writes parses, and `fmt --pretty` turns it back into `∂²/∂x²`. The two orders must match (`∂/∂x²` is an error); the second may be left out, as before (`∂²/∂x f`).
- **`∫ 2 dm` (A17):** after a number, `dm` is decimetres (a name right after a number is a unit). But when an integral has no other differential, a trailing `d<unit>` (`dm`, `dV`, `dT`, `dg`: `d` followed by a unit name) right after the number is read as the differential: `∫ 2 dm from 0 kg to 1 kg` = 2 kg. So a constant integrand works like `∫ 1 dx` already did. `x = 2 dm` and `∫ 2 dm dx …` (which has its own `dx`) are unchanged: decimetres. *Alternative:* never read `dm` as decimetres inside an integral — rejected, since `∫ 2 dm dx` is legitimate.
- **`d/dt (…) where …` (A49):** when no binding names or uses the variable `t`, the bindings are substituted into the formula first, so `g = d/dt (a t^2) where a = 3` is the function g(t) = 6t. If a binding sets `t` itself, the derivative is evaluated there, as before. A function made by `g = d/dt (…)` or `F = ∫ … dx` is now shown as `g(t) = …` / `F(x) = …`.

## D30. Vector calculus: ∇f, ∇·F, ∇×F, ∇²f
- **What:** `∇` is an operator on a *function name*. `∇φ`, `∇·F`, `∇×F` and `∇²φ` are new functions of the same Cartesian coordinates (2 or 3; the curl needs 3). They are found by symbolic partial differentiation, then tidied with SymPy when it is installed. `∇φ(1 m, 0 m, 0 m)` calls one. ASCII: `grad(φ)`, `div(F)`, `curl(F)`, `laplacian(φ)`; `fmt --ascii` writes these, and `nabla` is the keyword behind `∇`.
- **Why:** this is how the formulas look in a textbook (`E = -∇φ`), and making the result a function keeps it printable and differentiable again (`curl(∇φ)` is 0, `div(curl A)` is 0, and the tests check both). Units come for free: V over m gives V/m.
- **Details:** a component that differentiates to exactly 0 is typed as a flexible 0, like a written `0`, so `∇×<-y, x, 0> T/m` is `<0, 0, 2> T/m`. SymPy tidying uses real (not positive) symbols, because coordinates can be negative.
- **Alternatives:** `∇` applied to an expression in x, y, z (like `d/dt (formula)`) — left for later, since the coordinate names would have to be guessed. Curvilinear coordinates — out of scope; write the formulas out.

## D32. Equations: `solve lhs = rhs for x from a to b`
- **What:** a `solve` with no derivatives and no `with` clause is an algebraic equation. It finds the first x in [a, b] where lhs = rhs, and assigns it to x, the same way an ODE `solve` binds its unknowns. The search scans 200 points for the first sign change when the ends don't bracket a root, then refines with Illinois regula falsi to full precision. A jump across 0 (a pole) is reported, not returned.
- **Why:** textbook problems keep needing roots: finite-well energies, shooting methods, Wien's law, projectile landing times. A `while` loop of bisection was the only way before. Re-using `solve` reads like the math ("solve tan z = … for z") and needs no new keyword.
- **Alternatives:** a `root(f, a, b)` built-in would need a function value and couldn't take an equation; brentq would be marginally faster, but Illinois is simpler to mirror exactly in the reference interpreter.

## D29. Browser playground: Pyodide + the reference interpreter
- **What:** `web/` is a static page. A Web Worker loads Pyodide, unpacks a wheel of the fermium package into site-packages, writes the bootcamp/examples CSV files into the virtual file system, and runs programs with `fermium.interp.run_interpreted`. Plots are saved as PNGs by the runtime as usual; the glue (`web/playground.py`) reads back every PNG the run wrote and the page shows it inline.
- **No llvmlite on the interpreter path:** Pyodide has no llvmlite, so the Gauss–Kronrod tables and `odd_root_numerator` moved from `codegen_llvm.py` to the pure `numerics.py`, and `finalize_tables` from `driver.py` to the new pure `tables.py` (`driver.finalize_tables` still works). `tests/test_interp_no_llvmlite.py` blocks llvmlite and runs programs.
- **Installing fermium in Pyodide:** `web/build.py` writes a pure-Python wheel directly with `zipfile` (a valid wheel with a RECORD; `pip install` accepts it) and the worker calls `pyodide.unpackArchive(bytes, "wheel")`, so micropip isn't needed and dependency resolution can't pull in llvmlite. *Alternatives:* `pip wheel . --no-deps` (fails with Debian's system setuptools, or needs the network for build isolation); fetching each source file (needs a file list and many requests).
- **Optional packages:** numpy is loaded at start. matplotlib (for `plot`) and scipy (for `fit`) are loaded when the program text mentions them. If a run still hits a missing optional module (sympy, for integrals without limits), the glue reports it, the worker loads the package and runs the program again.
- **Worker, not main thread:** an endless loop can't freeze the page, and Stop terminates the worker and starts a new one (Python can't be interrupted otherwise without SharedArrayBuffer, which needs special server headers).
- **Pyodide 0.29.5 from jsdelivr**, or a local copy in `web/pyodide/` (`build.py --local-pyodide`: the npm package for the core plus the checksummed package wheels from the CDN). The worker uses the local copy when its completion marker exists. The tests download it once so they don't depend on the browser reaching the CDN, and skip with the reason when neither works.
- **Editor:** a plain textarea (Tab completion of `\name` from `symbols.json`, exported from `fermium/symbols.py`; Tab otherwise indents; Enter keeps the indentation). *Alternative:* CodeMirror, deferred: another CDN dependency for little gain in a playground.


## D33. Matrices, 4-vectors and vectors with a unit per component (Phase 2, item 3)
- **Matrix syntax:** `[[1, 2], [3, 4]] N/m`, a list of rows with the unit written once after `]]`, or `[[1 N/m, 0 N/m], [0 N/m, 2 N/m]]` with a unit on each entry. Lists of lists weren't allowed before, so there is no clash; a unit right after `]]` is read like the unit after `<3, 4>`. All entries share one dimension, as D24 does for vectors: that covers stiffness, inertia, rotation and Lorentz matrices. 1 to 4 rows and 1 to 4 columns. *Alternatives:* a `mat(...)` function (less like paper); `[1 2; 3 4]` MATLAB style (`;` and juxtaposition already mean other things).
- **Operations:** `+ -` (same size, same units), scaling, `M v` / `M * v` / `M · v` (matrix times vector, a vector), `A B` (matrix product), `transpose(M)` and `Mᵀ` (`ᵀ` is its own postfix token, not part of a name; `\transpose` types it), `det(M)` (dimension^n), `inverse(M)` (dimension^-1), `identity(n)`, `solve_linear(M, b)` (x has dim(b)/dim(M)), `M[i, j]` (parsed as `M[i][j]`) and `M[i]` (row i as a vector). Indexes must be fixed numbers, like vector components. `v M` is an error (write `Mᵀ v`). `M \ b` was left out: `\` starts `\name` symbol completion in the REPL, editors and notebooks.
- **Display units:** `det` and `inverse` show the matrix's unit raised to n or −1 (`det` of a 2×2 in N/m prints in N²/m², `inverse` in m/N), and `K K` with both in N/m prints N²/m². Other products are shown in SI.
- **Code generation:** a matrix is a flat row-major `<r·c x double>`, so `+ - scaling` are single vector instructions. `matmul`, `det` (cofactor expansion, exact for whole numbers), `inverse` and `solve_linear` (Gaussian elimination with partial pivoting; the row swaps are `select`s, so it is straight-line code) are written once in `fermium/linalg.py` against an `ops` object: the code generator passes an ops that emits LLVM instructions, the interpreter one that computes floats. The two run the same operations in the same order, so they agree bit for bit. A zero pivot is a runtime error ("this matrix is singular") in the JIT, the interpreter and `fermium build`. *Alternative:* call LAPACK through a callback — slower for 2×2..4×4 and not available in `fermium build` executables.
- **Vectors with a unit per component:** `<1 m, 2 m/s>` is allowed now (it used to be an error). If the components' known dimensions differ, the vector is *mixed*: `VecTy.dims` holds one dimension per component and `VecTy.dim` is None. `+ -` unify component by component (the error names the component), scaling multiplies each, `v.x`/`v[i]` give the component's own unit, and `print` shows each component with its unit, `<1 m, 2 m/s>`. `|v|`, `norm`, `unit`, `sign`, `·`, `×`, `M v`, `solve_linear` and `in` need one shared unit and say so. A vector whose known components agree is uniform, exactly as before (unknown dimensions, like a plain `0`, join the others). *Alternative:* always track dimensions per component — the same results for uniform vectors, but unknown components (function parameters) could then never be forced to agree, so `|v|` inside a generic function would fail.
- **4-vectors:** `<t, x, y, z>` and `vec(a, b, c, d)` so 4×4 matrices have something to act on. Components are `v[1]`..`v[4]` (`.x .y .z` are the first three). The cross product needs 2 or 3 components.
- **Not done:** vector ODEs with a unit per component (the state must be separate unknowns: `x` and `v`), matrices as ODE unknowns, lists of vectors or matrices, element assignment `M[i, j] = …`, eigenvalues, and matrices with a unit per entry.
- **Also fixed:** vectors in REPL variables crashed when their slot wasn't aligned to the vector's size (the arena is 8-byte aligned; loads and stores now say `align 8`). Vectors and matrices local to a function can be used inside `∫`/`solve` there (each takes n slots of the environment).

## D31. `load`, `fit` and `plot` in standalone executables
- **What:** `fermium build` now builds every program. `runtime/aot_data.c` (included by `aot_rt.c`) holds C versions of the three Python callbacks, and `fm_tables.c` gets what the checker knows at compile time: each CSV's header and its columns' SI factors and offsets; each fit's parameter names, display units and columns; each plot's labels with units, the conversion to display units, and its options (log x/y, title, equal axes for orbits).
  - **load** reads the CSV when the program runs, using Python's `csv` rules (quotes, BOM, blank lines skipped) and `float()`'s number syntax. A missing file, a bad number or a wrong number of values give the same messages as `fermium run`. Paths are relative to the **current folder**, not the `.fm` file's folder: an executable is moved around and run from anywhere, and that is how every command-line program treats a relative path. The header must match the one the program was built with, because the units are compiled in; otherwise the program stops and says to rebuild.
  - **fit** is Levenberg–Marquardt with Marquardt's diagonal scaling and a forward-difference Jacobian. It repeats what `fitting.py` does around SciPy: the same scan for missing starting guesses (powers of ten, then 1-2-5 steps; the better of the two fits wins), non-finite residuals replaced by 1e300, and standard errors from (JᵀJ)⁻¹·rss/dof with SciPy's own finite-difference step (`√ε·max(1, |p|)`). The compiled model `lam.<name>` gets a C-callable wrapper `fm_model_<i>` in the LLVM module. On the repo's fits (the examples plus the fit tests' data) the output is identical to `fermium run`. When a parameter can't be determined (`A B exp(-t/τ)`), both print the warning, but they can stop at different values of the undetermined combination.
  - **plot** writes an SVG: axes, 1-2-2.5-5 ticks (decades on log axes), grid, axis labels with units, a legend for more than one series, the title, matplotlib's colours, markers for measured data, and equal scales for orbits. Solutions are sampled with the same 600-point cubic Hermite interpolation. A `.png` (or any non-`.svg`) name becomes `.svg`, and the program prints `plot saved to x.svg (standalone programs write SVG)`.
- **Why:** a standalone executable shouldn't need Python, NumPy, SciPy or matplotlib. SVG is plain text, so C can write it in about 200 lines with no libraries, and every browser opens it. Levenberg–Marquardt is what SciPy's `method="lm"` (MINPACK) runs, so the minimum is the same.
- **Alternatives:** calling `gnuplot` or `python3 -c "import matplotlib"` from the executable was rejected as fragile (it depends on what is installed where the program runs). Writing PNG would need zlib and a rasteriser. Embedding the CSV data in the executable was rejected because new measurements then need a rebuild. Resolving paths relative to the executable's own folder is less predictable than the current folder and isn't portable C.
- **Testing:** `tests/test_aot.py` builds every example (including the load/fit/plot ones) and compares the output with `fermium run`, compares nine fits on test data, checks the load error messages, and parses the SVGs with `xml.etree` to count the series and find the labels.

## D34. Eigenvalues by Jacobi rotations: `eigenvalues(M)`, `eigenvectors(M)`, `eigenvalues(K, M)` (friction #22)
- **What:** `eigenvalues(M)` of a symmetric 2×2..4×4 matrix is a vector sorted ascending, in the matrix's units (N/m in, N/m out). `eigenvectors(M)` is a dimensionless matrix whose column j is the unit eigenvector of eigenvalue j, with its largest-magnitude entry made positive so the output is deterministic. `eigenvalues(K, M)` / `eigenvectors(K, M)` solve K v = λ M v for symmetric K and symmetric positive-definite M (normal modes, λ = ω² in dim(K)/dim(M)): Cholesky M = L Lᵀ, the symmetric problem L⁻¹ K L⁻ᵀ y = λ y, then v = L⁻ᵀ y scaled to unit length.
- **How:** cyclic Jacobi with a fixed 10 sweeps (n ≤ 4 reaches machine precision in about 5; later rotations are exact no-ops), branch-free rotations (`select` for the sign of θ and for a zero off-diagonal entry) and a bubble-sort network, all in `fermium/linalg.py` against the same `ops` object as `solve` (D33), so the JIT, `fermium build` and the interpreter run identical operations. The ops gained `const`, `sqrt`, `abs`, `lt`, `eq`. Runtime errors 21 (not symmetric: some |a_ij − a_ji| > 10⁻¹⁰ · max|a|) and 22 (M not positive definite: a Cholesky pivot ≤ 0).
- **Why not `eigenvalues(inverse(M) K)`:** M⁻¹K is not symmetric, so it fails the symmetry check; the message points to `eigenvalues(K, M)`. The two-argument form keeps the symmetric (always real) problem.
- **Alternatives:** QR iteration or LAPACK via a callback (not available in standalone executables, and needs branches/convergence tests); a closed form for 2×2 only (doesn't reach 3- and 4-mass chains); non-symmetric matrices (complex eigenvalues — out of scope for normal modes).

## D35. Vector integrands: one quadrature per component (friction #19)
- **What:** `∫ <f, g, h> ds from a to b` and any vector-valued integrand (Biot–Savart's `dl(φ) × r / |r|³`) give a vector: the checker notices the integrand is a vector, discards the lambdas it made while finding that out, and checks `<∫ (integrand)[1] ds …, ∫ (integrand)[2] ds …, …>` instead. Each component keeps its own units (mixed vectors work).
- **Why:** no new IR, code generation or interpreter code — each component is an ordinary adaptive Gauss–Kronrod integral, so all the existing machinery (infinite ranges, singularities, error messages) applies. The cost is evaluating the whole vector integrand once per component.
- **Alternatives:** a vector quadrature sharing nodes and one error estimate (faster by up to n×; left for later); matrix integrands (still an error).

## D36. Differentiating integrals under the integral sign (friction #18)
- **What:** the symbolic differentiator applies the Leibniz rule to definite integrals: d/dx ∫ f(x, s) ds from a(x) to b(x) = ∫ ∂f/∂x ds from a to b + f(x, b) b′ − f(x, a) a′. So `∂/∂x V`, `∇V`, `∇²V` and `V'` of a one-line function defined by an integral are functions whose bodies are integrals of the derivative (tidied by SymPy when available). When the derivative variable is also the integration variable (`f(s) = ∫ s² ds from 0 to s`), the inner term is skipped (the integrand's `s` is bound) and only the boundary terms apply. `(∫ … ) + x³` now prints with parentheses, since `∫ … to x + x³` would read as a limit.
- **Why:** the potential of a charge distribution is naturally an integral; the field should follow from `E = -∇V` without deriving a closed form by hand.
- **Alternatives:** numerical differentiation of the integral (inaccurate, and loses the printable formula).

## D37. Indefinite integrals: safe positivity, |b| for even constants, and a verified antiderivative (friction #20)
- **What:** SymPy now receives positive symbols only where that is safe: physical constants, literal quantities in the formula (`0.5 m` becomes a placeholder symbol and is put back afterwards, so `∫ 1/√((x − s)² + (0.5 m)²) ds` works), and program variables whose every assignment is a positive literal and that no loop, `solve`, `fit`, parameter or element assignment binds (never in the REPL, where later lines could change them). A real constant that only enters through even powers is replaced by a positive symbol and then by `abs(b)`. `asinh`, `acosh`, `atanh`, `erfc`, `sign` and `Abs` map to Fermium functions, and `u·f(|u|c)/|u|` with odd f is rewritten to f(u c) (removing a 0/0 at u = 0). Every antiderivative is checked by differentiating it at random real points; a formula that holds for only one sign (SymPy's `|a| asinh(|s|/|a|) s/(a|s|)` is wrong for a < 0) is refused with a clear message. Undefined names are reported before SymPy runs, and every failure carries the ∫'s line and a hint to give limits.
- **Alternatives:** positive symbols for every quantity with units (wrong for coordinates and charges, which can be negative); trusting SymPy's real-symbol answers (wrong signs, as above).
