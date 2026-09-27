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

## D7. Units after numbers — *rules 4 and 5 superseded by D235*
The rule (spec §3.4.2), refined:
1. A unit name right after a **digit literal** is a unit: `3 m`, `9.81 m/s²`. This does *not* apply after `½`, `π` or other symbols, so `½ m v²` is one half times the mass m times v².
2. Bracketed units are always units: `3 [m/s]`, and parameter annotations `f(x [m]) = ...`. (A variable can't be declared with one: `x [m] = 3` is a parse error.)
3. Everywhere else an identifier is a variable.
4. Continuing a unit expression after the first unit:
   - `/` written without a space before it, and followed by a unit name, continues the unit (`50 N/m`, `3 m/s`), even when you have a variable with that name.
   - `/` with a space before it, followed by the name of one of *your* variables, divides by the variable. So with `g = 9.81 m/s²`, `20 m/s / g` is 2.04 s and not "per gram", and `2.898e-3 m K / T` divides by the temperature T, not by tesla. (Changed after the bootcamp author hit exactly this trap.)
   - A space or `·`/`*` followed by a unit name continues the unit **only if you haven't defined a variable with that name**. So `70 kg g` with your own `g` is 70 kg × g, with a warning.
5. **Collisions: a single unit name right after a number that is also one of your variables** (revised at 03:45 UTC after the gauntlet hit it in every topic and a review called it the biggest usability flaw):
   - **Combined with other factors, it's an error** that asks which you mean: `2 g h`, `0.5 m v²`, `2 m v`, `2 g * h`, `h * 2 g`, `2 m / t`. The message says `'2 g' is ambiguous: right after a number, g is a unit (grams), but g is also your variable g`, with the hint `write 2*g for 2 × your variable g, or 2 [g] for the unit`.
   - **Standing alone, it's the unit, with a warning** (the spec §3.4.2 behaviour): `x(0) = 0.1 m`, `from 0 m to 0.2 m`, `x = 3 m`, `f(2 L)`. A unit error later still gets the note naming the cause (D34, gauntlet #43).
   - Compound units (`9.81 m/s²`, `3 m²`, `2 kg m`) and bracketed units (`2 [g]`) are never ambiguous. Constants that look like units (`2 G`, `2 h`: neither G nor h is a unit in Fermium) warn when alone.
   - **Tradeoff.** In a product, "2 grams times h" is almost never meant when you have a variable g, and each gauntlet occurrence (`2 g h`, `2 m m2`, `3 V`, `27 b²`) was a silent or confusing wrong reading. So an error that costs one keystroke (`*` or `[ ]`) is cheap. Standing alone, the unit is almost always meant: an initial position `0.1 m` next to a mass `m` is the spec's own example, and it is protected by the unit check anyway. So erroring there would add friction for no safety.
   - *Rejected alternatives:*
     - Always an error on a collision: this breaks the spec's example and every spring program with a mass `m`.
     - Choose whichever reading type-checks: the meaning of `2 m` would depend on distant code, and error messages would become unpredictable.
     - A space-sensitive rule (`2m` a unit, `2 m` a product): invisible in print, and a trap of its own.
- **Unit names we deliberately left out, because they collide with physics variables:**
  - `h` for hour: use `hr`.
  - `t` for tonne: use `tonne`.
  - `G` for gauss: use `gauss` or `Gs`.
  - `d` for day: use `day`.
- **Why:** This matches what physicists write on paper while keeping the rule predictable.

## D8. Implicit multiplication binds tighter than `/` and `*` — *the pure-number case superseded by D236*
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
  - A computed result prints with max(2, the fewest significant figures among its inputs).
  - **When the precision is ambiguous or unspecified, the default is 3 significant figures** (user instruction, 04:20 UTC Sep 25). This covers every input being exact: integers, π, constants such as c or G. Trailing zeros are kept, so `1/2` prints `0.500`, `2π` prints `6.28` and `c` prints `3.00×10⁸ m/s`. Until this change the rule was "up to 6 significant figures, trailing zeros trimmed".
    - **Display only:** the rule changes the text `print` shows, never the value. Every calculation uses the full double-precision number (`x = 1/3` prints `0.333`, yet `x * 3 - 1` prints `0`; tests/test_default_sigfigs.py).
    - A list, vector or matrix written out in the program prints each element as written when the list's fewest significant figures would round one of them (`[0, 0.5, 1, 1.5]`, which earlier printed as `[0, 0.5, 1, 2]`).
    - Exception 1: a whole number below 10⁷ prints exactly (`4*5` → `20`, a count `len(x) - 1` → `1000000`). Counting is not a measurement.
    - Exception 2: a value that came straight from a literal prints exactly as written (`12345678`, `3.14159`).
    - A value within 10⁻¹³ (relative) of a whole number counts as whole. So rounding in the last bits doesn't turn `∛(27 m³)` into `3.00 m`, or M☉ in astro units into `1.00 M☉`.
    - A loop variable prints as written when it runs over a `from … to … step …` grid (`0.07 s`, not `0.0700 s`) or over a written-out list (`for n in [0, 1, 1.5]` shows `1.5`).
    - One style per list, vector (including one with a unit per component) or matrix. NaN and ∞ don't count when choosing it (`[1, 2, 3, NaN]`).
    - When rounding leaves two or more non-significant zeros before the decimal point, the value is shown with a power of ten: `1000000/3` → `3.33×10⁵`, not `333000` (red team round 3 #9). One such zero stays in fixed notation (`9550 rpm`), and whole numbers still print exactly.
    - **Ties round half to even:** `0.125` to 2 figures is `0.12`, and `3.25×10⁶` is `3.2×10⁶`. This is the IEEE-754 rule that C's printf and Python's formatting use (NumPy and Julia too). Only values that are exactly a tie in binary are affected, so `0.135` shows `0.14`, because its double is slightly above the tie (red team round 4 #16: documented, not changed).
    - A written-out whole number prints as written, in a list too: `[1.2345, 2]` and `10000000`, not `2.0000` or `1×10⁷` (round 4 #14). The SI echo of a value written with a constant unit uses the value's own significant figures: `2 kg c²` gives `(= 1.80×10¹⁷ J)` (round 4 #13).
    - (Reverted by D230: an integral at or below its rounding level was printed as 0 for round 4 #12, but that also zeroed real small integrals. Rounding noise is shown as it is.)
    - **Sums and differences** keep the significant figures of their most precise operand: `293.15 K + 0.5 K` is `293.65 K`, not `290 K` (red team round 7 #4). The textbook decimal-place rule needs the magnitudes, which are known only at run time. This choice never shows a value less precisely than its inputs, at the price of sometimes showing more (`1.00 m - 0.999 m`, D95).
    - `print x to N digits` overrides the default, for lists and vectors too (`[0.333333, 0.666667]`). Programs and tests that compare many digits say so explicitly.
    - Lists, vectors and matrices follow the same rule per element. The C runtime of `fermium build` mirrors it (`fmt_default`, `FM_DEFAULT_SF` in aot_rt.c; `format_default` in units.py).
    - **Why 3:** it is the textbook convention for answers given without a stated precision, and physics inputs are rarely known better. Six digits suggested a precision nobody asked for. **Alternatives considered:** 4 digits (Mathematica-like), or treating exact integers as infinitely precise and keeping 6. The user asked for 3.
  - This reproduces the spec's `9.70 m/s²`, `10 1/s` and `1.0 J`.
- **Display unit:**
  - The unit the user wrote (literal or `in`) is kept through `+`, `-` and scaling by plain numbers.
  - Otherwise Fermium uses a preferred SI unit for the dimension (N, J, W, Pa, N/m, J s, ...), falling back to base SI units (`kg m/s³`).
  - Very large or small numbers print as `6.674×10⁻¹¹`.

## D12. Temperatures with °C/°F are absolute
- **What:** `20 °C` means 293.15 K, and `T in °C` subtracts 273.15. Inside a compound unit a degree can only be a step, so `°C/min`, `J/(g °C)` and `°F/s` are K-sized (no offset; gauntlet friction #21). Use K for temperature differences and in formulas.
- **Why:** An affine unit is only well defined for absolute values.
- **Differences (A47):** `a - b` with `b` in °C is always a temperature difference, shown in K, whatever unit `a` was written in. `300 K - 20 °C` is 6.85 K, and Newton's law of cooling `T' = -(T - Ta)/τ` works with `Ta = 20 °C` and `T(0) = 90 °C`. A value in K may be an absolute temperature or a difference, and both readings give the same number in K. Still errors: `°C + °C` and negating a °C value. `20 °C - 5 K` stays 15 °C (a change of 5 K). A difference shown `in °C` or `in °F` is shown without the offset, with a warning, and `2 T` of a °C value warns that it scales the absolute temperature (gauntlet friction #6). Alternative: tracking "absolute or difference" as part of the type. That is more precise, but it needs a new kind of type for one unit, so it was rejected for now.

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

## D19. Uncertainties (§3.7) are reserved (superseded by D120–D124: implemented in M4)
- **What:** `±` and `+-` are lexed as operators and give a friendly "planned for a future version" error.
- **Why this makes them easy to add later:** The IR carries `ty`, `sf` and `hint` per expression. Adding an uncertainty is a new `NumTy` flavour whose LLVM type is `{double value, double sigma}`, plus propagation rules in `arith`. See BACKLOG.

## D20. Blocks use indentation, and the colon is optional
- **What:** `if x > 0 m` followed by an indented block. A trailing `:` is accepted for people coming from Python, and `then` is accepted after an `if` condition. A line ending in a binary operator or comma continues on the next line.

## D21. `~=` / `≈` means "equal within 10⁻⁶ relative" — *formula superseded by D260 (isapprox, `within`)*
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
- **Why:** With rad = 1, Hz and rad/s are the same dimension. If `rpm` meant 1/60 Hz, then `60 rpm in rad/s` would be 1 rad/s, off by 2π the other way. Keeping `rev` = 2π is self-consistent. *(Revised in D95: the warning now fires in both directions, Hz → rev/rpm/rad/s too, since `1 Hz in rpm` = 9.55 rpm is just as surprising.)*
- **Alternatives:** Refuse `in Hz` for such values. This was rejected because `ω in Hz` from a variable would be refused or allowed depending on how it was written. Making the angle a base dimension was already rejected in D6.

## D28. Calculus edge cases from the adversarial pass (A17, A18, A26, A35, A38, A49, A52)
- **Real odd roots (A26):** `x^(n/q)` with `q` odd (1/3, 2/3, -2/3, 3/5, ...; the exponent is recognised as a fraction with denominator < 100) is the real root for negative `x` too: `(-8)^(2/3) = 4`, like `∛`. So `f(x) = x^(1/3)` and its derivative `1/(3x^(2/3))` agree at `x = -8`. Even roots of negatives stay NaN. Derivatives print such exponents as fractions (`x^(2/3)`, not `x^0.666666666667`). *Alternative:* NaN for every non-integer power of a negative number (what C's `pow` does) — rejected because `∛x` and `x^(1/3)` already gave the real root and physics formulas use it.
- **Stable evaluation of derivatives (A52):** a derivative is *printed* as the rules give it (`-exp(x)/(exp(x) + 1)²`), but *evaluated* after a rewrite (`calculus.stabilize`) that cancels the growth of `exp(u)/(exp(u) + r)^k` into `1/((exp(u) + r)^(k-1) (1 + r exp(-u)))` and `sinh(u)/cosh(u)^k` into `tanh(u)/cosh(u)^(k-1)`, splitting a sum over such a denominator into one fraction per term. So the Fermi function's derivative far from μ, `tanh''(1000)` and the Planck spectrum's derivative are 0 instead of ∞/∞ = NaN. *Alternative:* evaluate `a/b` as 0 when `|b| = ∞` — doesn't help, since the numerator overflows too.
- **`sign` of a vector (A18):** `sign(v)` is `v/|v|`, the unit vector (the usual generalisation of sign to complex numbers and vectors). The rule d|u| = sign(u) du then works for vectors: `d/dt |r(t)|` = `sign(r(t)) r'(t)` = r̂·r'. *Alternative:* a separate vector rule in the differentiator, which would need the types there (it works on the untyped AST).
- **Indefinite integrals with parameters (A35):** SymPy is asked for the generic answer (`conds="none"`), so `∫ cos(ω t) dt` is `sin(ω t)/ω`, as in a textbook, without a special case for ω = 0. A Piecewise that depends on the variable itself (`∫ |x| dx`) becomes an `if … then … else` formula. *Alternative:* declaring every parameter `nonzero` — doesn't cover `∫ x^a dx` (a = -1).
- **`partial^2/partial x^2` (A38):** the parser accepts `^n` orders on ∂ like it does on d (`d^2/dt^2`), so what `fmt --ascii` writes parses, and `fmt --pretty` turns it back into `∂²/∂x²`. The two orders must match (`∂/∂x²` is an error); the second may be left out, as before (`∂²/∂x f`).
- **`∫ 2 dm` (A17):** after a number, `dm` is decimetres (a name right after a number is a unit). But when an integral has no other differential, a trailing `d<unit>` (`dm`, `dV`, `dT`, `dg`: `d` followed by a unit name) right after the number is read as the differential: `∫ 2 dm from 0 kg to 1 kg` = 2 kg. So a constant integrand works like `∫ 1 dx` already did. `x = 2 dm` and `∫ 2 dm dx …` (which has its own `dx`) are unchanged: decimetres. *Alternative:* never read `dm` as decimetres inside an integral — rejected, since `∫ 2 dm dx` is legitimate.
- **`d/dt (…) where …` (A49):** when no binding names or uses the variable `t`, the bindings are substituted into the formula first, so `g = d/dt (a t^2) where a = 3` is the function g(t) = 6t. If a binding sets `t` itself, the derivative is evaluated there, as before. A function made by `g = d/dt (…)` or `F = ∫ … dx` is now shown as `g(t) = …` / `F(x) = …`.

## D29. Browser playground: Pyodide + the reference interpreter
- **What:** `web/` is a static page. A Web Worker loads Pyodide, unpacks a wheel of the fermium package into site-packages, writes the bootcamp/examples CSV files into the virtual file system, and runs programs with `fermium.interp.run_interpreted`. Plots are saved as PNGs by the runtime as usual; the glue (`web/playground.py`) reads back every PNG the run wrote and the page shows it inline.
- **No llvmlite on the interpreter path:** Pyodide has no llvmlite, so the Gauss–Kronrod tables and `odd_root_numerator` moved from `codegen_llvm.py` to the pure `numerics.py`, and `finalize_tables` from `driver.py` to the new pure `tables.py` (`driver.finalize_tables` still works). `tests/test_interp_no_llvmlite.py` blocks llvmlite and runs programs.
- **Installing fermium in Pyodide:** `web/build.py` writes a pure-Python wheel directly with `zipfile` (a valid wheel with a RECORD; `pip install` accepts it) and the worker calls `pyodide.unpackArchive(bytes, "wheel")`, so micropip isn't needed and dependency resolution can't pull in llvmlite. *Alternatives:* `pip wheel . --no-deps` (fails with Debian's system setuptools, or needs the network for build isolation); fetching each source file (needs a file list and many requests).
- **Optional packages:** numpy is loaded at start. matplotlib (for `plot`) and scipy (for `fit`) are loaded when the program text mentions them. If a run still hits a missing optional module (sympy, for integrals without limits), the glue reports it, the worker loads the package and runs the program again.
- **Worker, not main thread:** an endless loop can't freeze the page, and Stop terminates the worker and starts a new one (Python can't be interrupted otherwise without SharedArrayBuffer, which needs special server headers).
- **Pyodide 0.29.5 from jsdelivr**, or a local copy in `web/pyodide/` (`build.py --local-pyodide`: the npm package for the core plus the checksummed package wheels from the CDN). The worker uses the local copy when its completion marker exists. The tests download it once so they don't depend on the browser reaching the CDN, and skip with the reason when neither works.
- **Editor:** a plain textarea (Tab completion of `\name` from `symbols.json`, exported from `fermium/symbols.py`; Tab otherwise indents; Enter keeps the indentation). *Alternative:* CodeMirror, deferred: another CDN dependency for little gain in a playground.

## D30. Vector calculus: ∇f, ∇·F, ∇×F, ∇²f
- **What:** `∇` is an operator on a *function name*. `∇φ`, `∇·F`, `∇×F` and `∇²φ` are new functions of the same Cartesian coordinates (2 or 3; the curl needs 3). They are found by symbolic partial differentiation, then tidied with SymPy when it is installed. `∇φ(1 m, 0 m, 0 m)` calls one. ASCII: `grad(φ)`, `div(F)`, `curl(F)`, `laplacian(φ)`; `fmt --ascii` writes these, and `nabla` is the keyword behind `∇`.
- **Why:** this is how the formulas look in a textbook (`E = -∇φ`), and making the result a function keeps it printable and differentiable again (`curl(∇φ)` is 0, `div(curl A)` is 0, and the tests check both). Units come for free: V over m gives V/m.
- **Details:** a component that differentiates to exactly 0 is typed as a flexible 0, like a written `0`, so `∇×<-y, x, 0> T/m` is `<0, 0, 2> T/m`. SymPy tidying uses real (not positive) symbols, because coordinates can be negative.
- **Alternatives:** `∇` applied to an expression in x, y, z (like `d/dt (formula)`) — left for later, since the coordinate names would have to be guessed. Curvilinear coordinates — out of scope; write the formulas out.

## D31. `load`, `fit` and `plot` in standalone executables
- **What:** `fermium build` now builds every program. `runtime/aot_data.c` (included by `aot_rt.c`) holds C versions of the three Python callbacks, and `fm_tables.c` gets what the checker knows at compile time: each CSV's header and its columns' SI factors and offsets; each fit's parameter names, display units and columns; each plot's labels with units, the conversion to display units, and its options (log x/y, title, equal axes for orbits).
  - **load** reads the CSV when the program runs, using Python's `csv` rules (quotes, BOM, blank lines skipped) and `float()`'s number syntax. A missing file, a bad number or a wrong number of values give the same messages as `fermium run`. Paths are relative to the **current folder**, not the `.fm` file's folder: an executable is moved around and run from anywhere, and that is how every command-line program treats a relative path. The header must match the one the program was built with, because the units are compiled in; otherwise the program stops and says to rebuild.
  - **fit** is Levenberg–Marquardt with Marquardt's diagonal scaling and a forward-difference Jacobian. It repeats what `fitting.py` does around SciPy: the same scan for missing starting guesses (powers of ten, then 1-2-5 steps; the better of the two fits wins), non-finite residuals replaced by 1e300, and standard errors from (JᵀJ)⁻¹·rss/dof with SciPy's own finite-difference step (`√ε·max(1, |p|)`). The compiled model `lam.<name>` gets a C-callable wrapper `fm_model_<i>` in the LLVM module. On the repo's fits (the examples plus the fit tests' data) the output is identical to `fermium run`. When a parameter can't be determined (`A B exp(-t/τ)`), both print the warning, but they can stop at different values of the undetermined combination.
  - **plot** writes an SVG: axes, 1-2-2.5-5 ticks (decades on log axes), grid, axis labels with units, a legend for more than one series, the title, matplotlib's colours, markers for measured data, and equal scales for orbits. Solutions are sampled with the same 600-point cubic Hermite interpolation. A `.png` (or any non-`.svg`) name becomes `.svg`, and the program prints `plot saved to x.svg (standalone programs write SVG)`.
- **Why:** a standalone executable shouldn't need Python, NumPy, SciPy or matplotlib. SVG is plain text, so C can write it in about 200 lines with no libraries, and every browser opens it. Levenberg–Marquardt is what SciPy's `method="lm"` (MINPACK) runs, so the minimum is the same.
- **Alternatives:** calling `gnuplot` or `python3 -c "import matplotlib"` from the executable was rejected as fragile (it depends on what is installed where the program runs). Writing PNG would need zlib and a rasteriser. Embedding the CSV data in the executable was rejected because new measurements then need a rebuild. Resolving paths relative to the executable's own folder is less predictable than the current folder and isn't portable C.
- **Testing:** `tests/test_aot.py` builds every example (including the load/fit/plot ones) and compares the output with `fermium run`, compares nine fits on test data, checks the load error messages, and parses the SVGs with `xml.etree` to count the series and find the labels.

## D32. Equations: `solve lhs = rhs for x from a to b`
- **What:** a `solve` with no derivatives and no `with` clause is an algebraic equation. It finds the first x in [a, b] where lhs = rhs, and assigns it to x, the same way an ODE `solve` binds its unknowns. The search scans 200 points for the first sign change when the ends don't bracket a root, then refines with Illinois regula falsi to full precision. A jump across 0 (a pole) is reported, not returned.
- **Why:** textbook problems keep needing roots: finite-well energies, shooting methods, Wien's law, projectile landing times. A `while` loop of bisection was the only way before. Re-using `solve` reads like the math ("solve tan z = … for z") and needs no new keyword.
- **Alternatives:** a `root(f, a, b)` built-in would need a function value and couldn't take an equation; brentq would be marginally faster, but Illinois is simpler to mirror exactly in the reference interpreter.

## D33. Matrices, 4-vectors and vectors with a unit per component (Phase 2, item 3)
- **Matrix syntax:** `[[1, 2], [3, 4]] N/m`, a list of rows with the unit written once after `]]`, or `[[1 N/m, 0 N/m], [0 N/m, 2 N/m]]` with a unit on each entry. Lists of lists weren't allowed before, so there is no clash; a unit right after `]]` is read like the unit after `<3, 4>`. All entries share one dimension, as D24 does for vectors: that covers stiffness, inertia, rotation and Lorentz matrices. 1 to 4 rows and 1 to 4 columns. *Alternatives:* a `mat(...)` function (less like paper); `[1 2; 3 4]` MATLAB style (`;` and juxtaposition already mean other things).
- **Operations:** `+ -` (same size, same units), scaling, `M v` / `M * v` / `M · v` (matrix times vector, a vector), `A B` (matrix product), `transpose(M)` and `Mᵀ` (`ᵀ` is its own postfix token, not part of a name; `\transpose` types it), `det(M)` (dimension^n), `inverse(M)` (dimension^-1), `identity(n)`, `solve_linear(M, b)` (x has dim(b)/dim(M)), `M[i, j]` (parsed as `M[i][j]`) and `M[i]` (row i as a vector). Indexes must be fixed numbers, like vector components. `v M` is an error (write `Mᵀ v`). `M \ b` was left out: `\` starts `\name` symbol completion in the REPL, editors and notebooks.
- **Display units:** `det` and `inverse` show the matrix's unit raised to n or −1 (`det` of a 2×2 in N/m prints in N²/m², `inverse` in m/N), and `K K` with both in N/m prints N²/m². Other products are shown in SI.
- **Code generation:** a matrix is a flat row-major `<r·c x double>`, so `+ - scaling` are single vector instructions. `matmul`, `det` (cofactor expansion, exact for whole numbers), `inverse` and `solve_linear` (Gaussian elimination with partial pivoting; the row swaps are `select`s, so it is straight-line code) are written once in `fermium/linalg.py` against an `ops` object: the code generator passes an ops that emits LLVM instructions, the interpreter one that computes floats. The two run the same operations in the same order, so they agree bit for bit. A zero pivot is a runtime error ("this matrix is singular") in the JIT, the interpreter and `fermium build`. *Alternative:* call LAPACK through a callback — slower for 2×2..4×4 and not available in `fermium build` executables.
- **Vectors with a unit per component:** `<1 m, 2 m/s>` is allowed now (it used to be an error). If the components' known dimensions differ, the vector is *mixed*: `VecTy.dims` holds one dimension per component and `VecTy.dim` is None. `+ -` unify component by component (the error names the component), scaling multiplies each, `v.x`/`v[i]` give the component's own unit, and `print` shows each component with its unit, `<1 m, 2 m/s>`. `|v|`, `norm`, `unit`, `sign`, `·`, `×`, `M v`, `solve_linear` and `in` need one shared unit and say so. A vector whose known components agree is uniform, exactly as before (unknown dimensions, like a plain `0`, join the others). *Alternative:* always track dimensions per component — the same results for uniform vectors, but unknown components (function parameters) could then never be forced to agree, so `|v|` inside a generic function would fail.
- **4-vectors:** `<t, x, y, z>` and `vec(a, b, c, d)` so 4×4 matrices have something to act on. Components are `v[1]`..`v[4]` (`.x .y .z` are the first three). The cross product needs 2 or 3 components.
- **Not done:** vector ODEs with a unit per component (the state must be separate unknowns: `x` and `v`), matrices as ODE unknowns, lists of vectors or matrices, element assignment `M[i, j] = …`, eigenvalues, and matrices with a unit per entry.
- **Also fixed:** vectors in REPL variables crashed when their slot wasn't aligned to the vector's size (the arena is 8-byte aligned; loads and stores now say `align 8`). Vectors and matrices local to a function can be used inside `∫`/`solve` there (each takes n slots of the environment).

## D34. Parser rules from the gauntlet (FRICTION #7, #8, #9, #10, #14, #15, #28) — *#10 and #15 superseded by D235*
- **Integral upper limit (#8):** a `/` with a space before it, outside any bracket opened in the limit, ends the upper limit. `∫ B dz from -∞ to ∞ / (μ₀ I)` is (∫ …) / (μ₀ I); `from 0 to 1/2` (no space), `to (L / 2)` and `to |a / b|` keep the division inside the limit. When the `/` that ends a limit is followed by a plain number or name (`to L / 2`), Fermium warns that it divides the whole integral, because some people mean the limit there. *Why:* the old parse silently put everything after `to` in the limit, and a gauntlet problem got a wrong answer from it; the space rule is the one D7 already uses for `20 m/s / g`. *Alternatives:* always require brackets around a compound limit (breaks `to 2π/ω`-style writing); only warn (keeps the silent-looking parse).
- **`a/b (c)` warnings (#9):** implicit multiplication still binds tighter than `/` (D8), but Fermium warns when the denominator is an implicit product and the writing suggests (a/b)·c:
  - a tight `/` (no space on either side) followed by factors separated by spaces: `c²/g (…)`, `n R/(γ−1) (T3−T2)`, `μ₀ I/(4π) dl`, `a/b x`;
  - a bracketed factor next to another factor with a space between them, whatever the spacing of the `/`: `c² / g (…)`, `a / (b) x`;
  - in a `solve`, a denominator factor that is one of the ODE's unknowns: `ψ'' = -2 m_e E / ħ² ψ` (dividing by the unknown is rare and usually a slip).
  - Not warned: `h c / λ k_B T` (the D8 textbook form), `a/(b c)`, `x/2π` (tight throughout), `G M m / r²`, and the trailing `du` of `∫ 1/u du`. The hint gives both spellings: `…/g * (…)` and `…/(g (…))`.
  - *Alternative:* change the precedence (rejected: D8 is spec, and `h c / λ k_B T` must keep working).
- **Vector literal by juxtaposition (#7):** `R <cos(φ), sin(φ), 0>` and `v0 <cos(θ), sin(θ)>` multiply by a vector when `<` has a space before it and none after, and a matching `>` with no space before it follows on the same line with a comma between them at the top level. `a < b`, `x <y`, `x <4 or x >2` and `print x <4, x >2` stay comparisons.
- **`2 L` in a line with a unit error (#10):** the spec §3.4.2 reading is kept (a unit right after a digit literal is a unit even when you have a variable with that name). The parser records every such collision by line; if a statement then fails with a unit error on that line, the error gets a note: "'2 L' here is 2 L, volume [m³] (a unit right after a number); for 2 × your variable L write 2*L". *Alternative:* prefer the variable when the unit reading fails to type-check (rejected: the meaning of `2 L` would then depend on the rest of the program).
- **`m_π` (#14):** `π` (like `∞`) right after `_` is part of the name, so `m_π` is the same name as `m_pi`. Alone, `π` is still the constant (`2πf`).
- **`15.3 / min / g` (#15):** right after a number, `/ min` with spaces is the minute, since `min` the function is never divided by. `2 / min(4, 8)` is still the function, and a variable named `min` is divided by.
- **`h² = …` (#28):** assigning to something that isn't a name says "can't store a value in h²: the left side of = must be a variable name", and the hint points to `solve h² = … for h from h_min to h_max` (or to `solve … with … for t …` when the left side has a derivative).

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

## D38. Eigenvalues by Jacobi rotations: `eigenvalues(M)`, `eigenvectors(M)`, `eigenvalues(K, M)` (friction #22)
- **What:** `eigenvalues(M)` of a symmetric 2×2..4×4 matrix is a vector sorted ascending, in the matrix's units (N/m in, N/m out). `eigenvectors(M)` is a dimensionless matrix whose column j is the unit eigenvector of eigenvalue j, with its largest-magnitude entry made positive so the output is deterministic. `eigenvalues(K, M)` / `eigenvectors(K, M)` solve K v = λ M v for symmetric K and symmetric positive-definite M (normal modes, λ = ω² in dim(K)/dim(M)): Cholesky M = L Lᵀ, the symmetric problem L⁻¹ K L⁻ᵀ y = λ y, then v = L⁻ᵀ y scaled to unit length.
- **How:** cyclic Jacobi with a fixed 10 sweeps (n ≤ 4 reaches machine precision in about 5; later rotations are exact no-ops), branch-free rotations (`select` for the sign of θ and for a zero off-diagonal entry) and a bubble-sort network, all in `fermium/linalg.py` against the same `ops` object as `solve` (D33), so the JIT, `fermium build` and the interpreter run identical operations. The ops gained `const`, `sqrt`, `abs`, `lt`, `eq`. Runtime errors 21 (not symmetric: some |a_ij − a_ji| > 10⁻¹⁰ · max|a|) and 22 (M not positive definite: a Cholesky pivot ≤ 0).
- **Why not `eigenvalues(inverse(M) K)`:** M⁻¹K is not symmetric, so it fails the symmetry check; the message points to `eigenvalues(K, M)`. The two-argument form keeps the symmetric (always real) problem.
- **Alternatives:** QR iteration or LAPACK via a callback (not available in standalone executables, and needs branches/convergence tests); a closed form for 2×2 only (doesn't reach 3- and 4-mass chains); non-symmetric matrices (complex eigenvalues — out of scope for normal modes).

## D39. `solve` towards smaller t, and the stop condition `until` (gauntlet #12, #33)
- **Towards smaller t:** a decreasing range (`for t from 5 s to 0 s`) integrates backwards, with the initial conditions at the start. RK45 steps with a signed step (the controller works with its size); RK4 takes the direction from the range, so `step 0.01 s` is a size, as in a `for` loop over a solve. The solution keeps the integration order: `times(x)` decreases, `x[end]` is the value at the end of the range, `x(t)` searches along the direction and `plot` samples from start to end. *Alternative:* reverse the stored arrays so times always increase (then `x[end]` would be the value at the *start*, which reads wrong). An empty range is its own error ("the range of t is empty"), instead of a message about a `step` nobody wrote.
- **`until lhs = rhs`:** after each accepted step the kernel evaluates g = lhs − rhs (an event function with the same arguments, state and captured variables as the right-hand side, so it shares its env). When g changes sign relative to the first nonzero value (a start exactly on the condition, like a ball launched from y = 0, doesn't count), the crossing is found by Illinois on the step's dense output — Dormand–Prince's 4th-order continuous extension (Hairer–Nørsett–Wanner's DOPRI5 coefficients), or the cubic Hermite for RK4 — to full double precision in t. The solution ends at the crossing (state from the dense output, derivative from f), so `x[end]`, `times(x)[end]` and `plot` all stop there and `x(t)` after it is out of range. The first crossing in either direction counts, like SciPy's `events` with `terminal=True` and no direction. If it never happens before the end of the range, that's a run-time error (M1: a silently returned end of range looked like an answer). The condition may use t, the unknowns and their derivatives below the highest.
- **Syntax:** `until` on its own line of the solve (or after the initial conditions on one line). The parser (another work stream) reads such a line as an equation whose left side is the product `until · y`, and `solve.py` takes the leading `until` factor off; `A.Solve.until` exists for a parser that recognises the clause directly. *Not yet:* `for t from 0 s to 9 s until y = 0 m` on one line, which needs `until` to end the range expression in the parser. *Alternatives:* `stop when` (two words, and `when` reads like a condition on the whole block); a `<` condition (`until y < 0 m`) — a crossing is what's located either way, and `=` is how the physics is written.

## D40. Jumps in t inside an ODE (gauntlet #11)
- **Problem:** an `if` on the independent variable (a force that switches on, a potential step in x) makes f jump. A step across the jump has an O(h) error that Dormand–Prince's error estimate can badly underestimate (the estimate's weights nearly cancel for some positions of the jump among the stages), so steps across it were accepted with 30–650× the requested error, and at 10⁻¹² the "stalled" rule kept accepting bad steps until the step size underflowed.
- **Fix:** when a step is rejected (the first rejection of a run of rejections) and the right side mentions t itself, the kernel checks f with the state held fixed at t, t + h/2 and t + h. If f at the midpoint sits at one end (more than 0.4 of the way from the average of the ends, in a per-component relative measure), it bisects in t down to adjacent floating-point numbers and confirms a jump if most of the change happens across that single rounding step. The integrator then lands exactly on the near side (every stage of the landing step sees the old f), records the point, restarts on the far side with f evaluated there, and records that point too (so the dense output has the right slope on each side). A confirmed jump right at the start of a step only restarts. Smooth time dependence costs two extra evaluations per rejection run (plus a bisection in rare false alarms); an autonomous system costs nothing.
- **Result:** `y' = r(t) y` with r jumping at 0.3 s meets 10⁻⁶, 10⁻⁹ and 10⁻¹² to within 0.35× the tolerance, with fewer steps than before (260 instead of 469 evaluations at 10⁻⁹).
- **Not covered:** jumps that depend on the unknowns (`if x > 0 m`, a bouncing ball); those need an event and a restart. *Alternatives:* find the switch points symbolically from `t < const` comparisons (breaks for `x > 0 nm and x < a` with a variable a, or a function of t); refuse to accept stalled steps with a large error (fails at the jump instead of passing it).

## D41. Run-time errors report where they happened (gauntlet #13, #32)
- Callbacks (integrands, right-hand sides, the equation of a `solve … for x`) save the current line and print format on entry and restore them on return, so a kernel's own error (no root, no crossing, too many steps) reports the line and units of the statement that called it, even if the callback ran a solve on another line. When the body can't change them the load/store pair is dead, and LLVM's optimizer can remove it (the RK4 and RK45 benchmarks run as fast as before). An error *inside* a callback (an index out of range in a function) reports its own line; a one-line function reports its definition line (an integral in `K(k) = ∫ …` has line 2, not none). The interpreter does the same: callbacks mark errors that pass through them, and kernels label only their own.
- ODE errors name the equation's own variable and its units: "too many steps (reached ξ = …)", "the right side of the equation is NaN or infinite at ξ = 0 (0/0? 1/0?)" (checked on the derivative at the start, which is where a 0/0 of a singular point shows up).

## D42. Stiff equations: `using radau` / `using bdf`, and a stiffness warning in RK45 (gauntlet #25, N1)
- **What:** `solve … for t from a to b using radau` solves with SciPy's Radau IIA (implicit, order 5, L-stable); `using bdf` with SciPy's variable-order BDF. The radon chain Po-218 → … → Po-214 (164 μs) over 720 min takes 8 363 steps (2.7 s) instead of RK45's ~5×10⁷ (RK45 managed 120 min in 9.2 million steps and failed at 720 min). Robertson's kinetics to t = 10⁵ s: ~1 250 steps, agreeing with a 10⁻¹² SciPy reference to 4×10⁻¹³; Van der Pol with μ = 1000: period 1614.4 against the asymptotic (3 − 2 ln 2)μ = 1613.7.
- **Where it runs (D3):** in Python, like `fit`. `runtime/stiff.py` has one `stiff_solve` that both back ends call, so the compiled program and the reference interpreter take the same steps and print the same digits. The compiled code calls the `fm_stiff` ctypes callback with the address of the right-hand side (the same `ODE_FN` lambda RK45 uses) and of `fm_ode_guard`, a small LLVM function that calls it with its own `setjmp`: a run-time error inside the right side (an index out of range) must not `longjmp` through Python's frames, so the guard saves the program's jump buffer, catches the error, restores the buffer and returns 1; the callback then returns 2 and the compiled code jumps out with the message and line the right side already set (D41). Solver errors return 1 with the message set (`fail_if`). The solution is a malloc'ed `SolStruct` (t, y, dy per accepted step), so `x(t)`, `x'(t)`, `times`, `values`, `x[end]` and `plot` are unchanged: cubic Hermite between steps, with dy = f(t, y) (Radau's own f at the new point).
- **Stepping:** the step-by-step solver objects (`Radau(...).step()`), not `solve_ivp`, so Fermium keeps its own semantics: the NaN check at the start (#32), the empty range (D39), backwards ranges (SciPy supports them), and `until` located exactly as in D39 — sign relative to the first nonzero g, start on the condition doesn't count, Illinois to full precision on the step's dense output (SciPy's collocation polynomial), solution ends at the crossing, never crossing is an error. A failed step (SciPy: "step size too small"), a non-finite state, or inf/NaN reaching the Newton matrix is the "step became too small … may blow up" error at the last good t. Over 10⁶ steps is its own "stiff ODE solver needed too many steps" error.
- **Tolerance:** SciPy's scale is atol + rtol·max(|y|, |y_new|); Fermium's (D17) is rtol·(max(|y|, |y_new|) + |y_new − y|), purely relative. So after every step atol is set to rtol·|y_new − y| of that step: the |Δy| term, lagging one step. It follows a decay down (N1 falls to 10⁻⁶⁴ with ~10⁻⁹ relative accuracy, like RK45), and still covers components passing through zero. Before the first step there is no Δy, and a zero atol makes SciPy's initial-step choice and Newton test divide by zero; the first floor is rtol·10⁻⁶ of the size each component starts at or could reach at its initial rate over the range, borrowing the smallest other size for a component that starts at 0 with zero slope. `tolerance r` sets rtol (floored at 10⁻¹³, SciPy's limit is 100 ε).
- **Stiffness warning in RK45:** Hairer's DOPRI5 test, h·|λ| ≈ |h|·‖k₇ − k₆‖/‖y₇ − y₆‖ (stages 6 and 7 are both at t + h, so this is the Jacobian's size along the step's error direction). It runs only after 100 000 steps (a solve slow enough to notice), every 1000th step and then every step while a run lasts; 15 in a row above 1.8 give one warning: "this equation looks stiff: rk45 has taken N steps, held small by stability rather than accuracy …; add `using radau`". The threshold is lower than Hairer's 3.25 because this controller settles between 2.0 and 3.22 on stiff problems; accuracy-limited steps measured at 10⁻⁶ stay under 0.4 (single steps up to 2.4 at 10⁻³ near a Kepler pericentre, never 15 in a row). It costs one compare per accepted step (the spring benchmark is unchanged). The "too many steps" error now suggests `using radau` as well. Same code in the LLVM kernel, the interpreter and (through `fm_warn`) `fermium build`.
- **`fermium build`** refuses `using radau`/`bdf` with a clear error: executables don't carry Python or SciPy. *Alternative:* a C Radau5 (Hairer's is ~1 500 lines of Fortran with its own LU); later if needed.
- **Alternatives considered:** automatic switching (RK45 detects stiffness and continues with Radau): attractive, but it silently changes the error behaviour and the stored steps; a warning keeps the choice visible and the fix is two words. Matrix exponential for linear constant-coefficient systems (Bateman chains): exact but narrow. `solve_ivp(dense_output=True)`: can't express `until`'s semantics or Fermium's norm; the step objects can.

## D43. Functions passed to functions by compile-time specialization (gauntlet #24, Q5)
- **What:** an argument of a user function can be a function: a name (`shoot(V1, E)`), a derivative (`g'`, `d/dt (3 t²)`, `∇φ`) or a one-argument built-in (`sin`, `exp`, `sqrt`, …, wrapped as `sin(x) = sin(x)`). The checker binds the parameter to that function inside the instance (the parameter name maps to the passed `FuncInfo` in the instance scope), so `V(x)`, `V'(x)`, `d/dx V`, `∫ V(x) dx`, `solve … V(x) …`, `plot V vs x` and passing `V` on all resolve to it. The instance cache key includes the passed function's identity, so each passed function gets its own instance, checked with its own units. Function arguments are dropped from the run-time call: instances take only the numeric arguments, and the code generator, the interpreter and `fermium build` see ordinary calls (no function pointers, no indirect calls; the passed function can be inlined).
- **Errors:** a function parameter used as a value says `V is a function here; call it like V(x)`. A number passed where the body uses the parameter as `V'`, `d/dx V`, `∇V` or `V(a, b)` is an error at the call (`force uses V as a function (V'), but was given force [N]`). A body that only writes `V(x)` is ambiguous (with a number, `V(x)` is `V × x` everywhere in Fermium), so a number there keeps that meaning with a warning at the call. Errors inside an instance get the existing "this happened when calling … (with V = the function V2, x = length [m])" note. A function with a function parameter that is never called isn't checked generically (there is no function to bind), and an ODE solution can't be passed yet (clear error).
- **Why:** functions were already specialized per call-site unit types; specializing on the function too keeps unit checking exact per use (the point of Fermium) and costs nothing at run time. Recursion works through the same cache (`iterate(f, f(x), n - 1)`, or swapping two function arguments).
- **Alternatives:** function pointers / closures at run time (would need a function type with units in the type system, loses per-use unit checking and inlining); higher-rank unit polymorphism (much more type-system work for no gain here). Lambdas (`x -> x²`) were left out: a named one-line function does the same job.

## D44. Quadrature convergence judged against ∫|f| too, and vector components retried (gauntlet #46, E10)
- **Problem:** the stopping test was error ≤ 10⁻¹⁰ |total|. An integral whose value is 0 by symmetry (∫ sin(x) from -1 to 1, the y component of the field of a disk off axis) has a total made of rounding noise, and no relative test on noise passes: "doesn't converge … 0.00297 ± 3.7×10⁻¹³", which reads as a blow-up.
- **Fix 1 (every integral):** each Gauss–Kronrod panel also gives the Kronrod estimate of ∫|f| (QUADPACK's `resabs`), and the integral is also accepted when the total error is ≤ 50 ε ∫|f| (1.1×10⁻¹⁴ ∫|f|, QUADPACK's rounding level): the result is then exact to rounding in the only scale the integral has. An integral that cancels but isn't 0 (∫ sin(x) + 0.01 from 0 to 20π) still has to meet 10⁻¹⁰ of its own value unless its error is at rounding level. Divergent integrals don't reach rounding level and are still reported.
- **Fix 2 (vector integrals):** a nested vector integral's zero component is noise whose size varies from one outer node to the next, so even the ∫|f| test fails. The vector's tolerance is what matters: each component is first tried quietly (`atol = -1`: a budget of 250 panels, NaN instead of an error), and a component that failed is computed again with an absolute tolerance of 10⁻¹⁰ × Σ|other components|. A vector whose components all converge costs nothing extra; the retry costs at most 250 panels more. The retry lives in the checker (`vector_integral`: an `ILet` of the quiet tries, an `IIf` per component), so both back ends run the same thing.
- **Alternatives:** an absolute floor like 10⁻¹⁴ (meaningless in SI units: 10⁻¹⁴ m² is huge in nuclear physics); one joint adaptive integration of all components with a norm-based test (SciPy's `quad_vec`; needs a vector-valued integrand kind and a second kernel); printing "0 to within rounding" (the number itself is fine).

## D45. NaN and ∞ in an integrand: a point is dropped, a stretch is an error that says where (gauntlet #45, M11, T10)
- **Problem:** a single NaN node made the whole integral "doesn't converge … NaN ± NaN", whether it was a harmless 0/0 at one point or an overflow over a whole stretch (`x⁴ eˣ/(eˣ − 1)²` is ∞/∞ above x ≈ 710).
- **Rule:** a node where the integrand is NaN or ±∞ counts as 0, and its panel gets an infinite error, so it is split first. When the worst panel is such a panel and is a few ulps wide in x (8 ε of max(|x|, the range's length, or the length scale on a half-line)), the run of such panels around it is found (by walking panels that share an end); if the whole run is a few ulps wide and it has a finite neighbour, it is one point as far as floating-point numbers can tell: it counts as 0 with error = its width × the neighbours' largest |f|, which must then fit in the tolerance like any other error. That is "treat the point's contribution as 0, but only when the neighbouring values are finite and the total converges". Otherwise the NaN panels keep splitting until the budget runs out, and the error says where: `the integrand is NaN at x = 767.102 (0/0? ∞/∞? an overflow like exp(710)?)` with the variable's own name and units, on the integral's line, and a hint with the two usual rewrites. When the non-finite values are ±∞ and never NaN it is `this integral doesn't converge: the integrand is infinite at x = … (1/0? …), so it may blow up there`, which is what 1/x at 0 looks like (x = 4×10⁻³⁰⁹, where 1/x overflows).
- **Not done:** `1 − cos θ` (exactly 0 below θ ≈ 10⁻⁸) makes M11's integrand 1/0 over a stretch 10⁻⁸ wide, which could hold up to 10⁻⁸ of the answer: an error, not a silently dropped piece. Rewriting `exp(x)/(exp(x) − 1)²` or `1 − cos` automatically (SymPy) was left out; the message gives the rewrites.
- **Alternatives:** dropping NaN nodes silently (SciPy's quad returns NaN with a warning; dropping a stretch can be a wrong answer); evaluating slightly inside the range (moves the problem); stopping at the first NaN (the old behaviour).

## D46. `x'(t)` from the right-hand side at the interpolated state (gauntlet #42, S9)
- **Problem:** `x'(t)` of an unknown in `x' = f(…)` was the derivative of the cubic Hermite interpolant, an order less accurate than `x(t)`: 1.2×10⁻⁶ relative at `tolerance 1e-11` for a relativistic charge.
- **Fix:** the solution keeps a pointer to its right-hand side and a heap copy of its env (two more fields of the solution struct, set by `FuncGen.attach_rhs` only for solutions whose highest derivative is ever evaluated), and `fm_sol_eval` with `use_dy` interpolates the whole state at t and calls f there. That is as accurate as the state (10⁻¹¹ in S9), for RK4 and RK45 solutions alike, and it covers `x''(t)` of a second-order unknown. The env is a snapshot: it holds the captured values and also the module-level numbers the right side reads (an evaluation-only copy of the lambda takes them from the env instead of the globals), so `k = 7 /s` after the solve doesn't change the old solution's derivative. The interpreter mirrors it with a snapshot frame. At the stored step points f gives exactly the stored derivative.
- **Alternatives:** differentiating DOPRI5's 4th-order dense output (needs the stage data stored per step, and RK4 has none; still an order lower than f); keeping the Hermite derivative with a warning. `plot x' vs t` still samples the interpolant's derivative (a plot doesn't need 10⁻¹⁰).

## D47. Equations linear in several highest derivatives: the mass-matrix form (gauntlet #44, M9)
- **What:** Lagrange's equations for coupled systems (a double pendulum) have θ₁'' and θ₂'' in both equations. When an equation of a `solve` contains more than one highest derivative, all the equations are treated together: `calculus.linear_coeffs` replaces the highest derivatives by placeholders, differentiates symbolically to get the coefficient a_ij = ∂eq_i/∂x_j^(n) and the rest r_i, and checks that no coefficient still contains a highest derivative (else a clear error: "a'' and b'' both appear in one equation; that works when every equation is linear in a'', b''… and this one isn't"). The right-hand side then solves M x^(n) = −r with fermium.linalg's Gaussian elimination with partial pivoting (the same unrolled code as `solve_linear`) at every evaluation; the solution vector is computed once per evaluation (an `ILet` in the first highest-derivative slot). Each coefficient is checked like any expression of the right side; units are consistent because each equation was, and the matrix mixes units, which is fine once they are erased.
- **Limits:** up to 4 unknowns, all numbers (a vector unknown in a coupled equation is an error that says to split it into components), linear in the highest derivatives. A singular matrix at run time is an ODE error: "the equations don't determine a'' and c'' at t = 0 s: the matrix of their coefficients is singular". A highest derivative that drops out of every equation is a check-time error.
- **Also:** `0.5 b''` with an unknown `b` was read as the quantity 0.5 barn with a prime on it. In a `solve`, a prime on a quantity whose unit is the name of an unknown (from the initial conditions) now means that unknown's derivative times the number.
- **Alternatives:** asking for the accelerations by hand with `solve_linear` in a helper function (the old workaround); Cramer's rule in the AST (unstable, and large for 4×4); a DAE solver (overkill for a regular mass matrix).

## D48. ODE solutions inside functions: captured as pointers; not returned (gauntlet #48, Q16, A7, Q17) — *storage superseded by D261*
- **What:** a solution made inside a function can be used by an integrand or an equation (`∫ u(r)² dr …`, `solve y(T) = 2 for T …`) in that function: the checker lets a nested lambda capture a solution symbol, and the env slot holds the pointer's bits (`ptrtoint`/`bitcast` to a double, and back in the lambda). The interpreter's closures already saw the frame. Captures propagate through nested integrands (E9's rule).
- **Not yet:** returning a solution. The error names it by its unknowns: "a function can't return the ODE solution u yet; return a number made from it instead, like u(…), ∫ u(t) dt or a root found with solve". Other messages that named the handle (`__sol.3`) now say "the solution y". Returning solutions needs a solution type in function signatures and instance keys, and a lifetime for the env copies.

## D50. Small syntax from the gauntlet: chained comparisons, if-expressions over lines, units after names and vectors, prefixed radians (gauntlet #52, #53, #55, #57)
- **Chained comparisons (#57):** `a < x < b` means `a < x and x < b`, like Python and like paper. Any number of `<`, `<=`, `>`, `>=` may be chained (`E_1s < E_1p3 < E_1p1`), or only `==`; a chain mixing `==`/`!=`/`~=` with an ordering is an error (`1 < x == y` has no settled meaning). The parser desugars the chain into `and`s; an inner operand that isn't a plain name or number is bound once with a hidden `where` (`0 < f(x) < 1` calls f once). *Alternative:* keep refusing (the old error was clear, but the rewrite doubles long expressions).
- **If-expression continued on the next line (#53):** in the lexer, a line that starts with the word `else`, is indented more than the current block, and follows a line containing `then`, continues that line (no NEWLINE/INDENT). So a piecewise function is written as on paper, one branch per line. An `else` at the block's own indentation is still the `else` of an if-statement. When an if-expression's line ends without `else`, the error says to indent the `else` line or use brackets. *Alternative:* implicit continuation after `then`/`else` everywhere (would break one-line if-statements such as `if c then print 1`).
- **Units after names and vectors (#55):** `<0, 0> /s` and `<1, 2> 1/s` take the unit, exactly as after a number (the space-slash form after `>` was the odd one out). A unit name after a *variable* (`A_d u`) is still not a unit (spec §3.4.2: units follow numbers); the "u isn't defined" error now says `to multiply A_d by u write A_d * 1 u or A_d [u]`. *Alternative:* read `A_d u` as the unit with a warning (rejected: silently changes meaning when a variable `u` is added later).
- **`mrad`, `μrad`, `krad/s` (#52):** `rad` takes SI prefixes, like `arcsec`. None of the prefixed spellings collides with a common name.

## D51. One-line sums: `Σ(term for k from a to b [step s])` (gauntlet #49)
- **Syntax:** `Σ(k² for k from 1 to 10)`, ASCII `sum(…)` or `Sigma(…)`; the `for k from a to b step s` part is the `for` loop's own syntax, and the range counts exactly like the loop (both ends, an empty range gives 0; a range with units needs a step with units). The parser recognises the form by a top-level `for` inside `Σ(`/`sum(`, so `sum(xs)` of a list is unchanged, and `Σ(xs)` means `sum(xs)`. The ends can be expressions and parameters (`S(N) = Σ(k for k from 1 to N)`).
- **Checking and code:** the term is a scalar lambda of k, exactly like an integrand (captures, nesting inside integrals and ODE right-hand sides work the same way), and `ISum` loops over the range calling it (native code and interpreter). A vector term sums per component (as D35 does for integrals). `Σ(… to ∞)` is refused at compile time: an infinite series needs a convergence test, which a `for … to inf` loop with `break` states explicitly.
- **Calculus:** d/dx Σ f(k, x) = Σ ∂f/∂x, term by term, so `f'`, `∂/∂x`, `∇`, `∇²` of a series written as a one-line function work and print as sums (`square'(x) = Σ(4 cos(n x)/π for n from 1 to 99 step 2)`). Differentiating with respect to a variable in the limits is an error (the number of terms changes in steps).
- **Why this syntax:** it reads like the notation (Σ, then the term, then the range) and reuses the `for` loop's words, so there is nothing new to learn; a comprehension-like order also keeps the term first, where a physicist looks. *Alternatives:* `Σ(n from 1 to N step 2) term` (E12's suggestion; the term's extent after the bracket is ambiguous with implicit multiplication), `sum(f, 1, N)` (needs a named function per series).

## D52. Bessel functions and complete elliptic integrals (gauntlet #50)
- **What:** `besselj(n, x)`, `bessely(n, x)`, `besseli(n, x)`, `besselk(n, x)` for whole-number n, and `ellipk(m)`, `ellipe(m)` with the **parameter m = k²** (SciPy's and Abramowitz & Stegun's convention; Gradshteyn uses the modulus k, so the docs say so). All take and return plain numbers. A non-integer order written as a number is a compile error; computed at run time it gives NaN.
- **How:** J and Y are the C library's `jn`/`yn` (native code calls them; the interpreter calls the same functions through ctypes, falling back to SciPy, mpmath, or Bessel's integral with the trapezoidal rule in the browser). I_n is its power series (all terms positive, no cancellation); K_n the trapezoidal rule on ∫₀^∞ exp(−x cosh t) cosh(nt) dt with step min(0.1, 0.5/(x² + n²)^¼), stopping past the peak once terms are below 10⁻¹⁸ of the sum. K and E use the AGM with 12 fixed steps (enough for 1 − m ≥ 10⁻³⁰⁰), written once against an `ops` object (as fermium/linalg.py) so the native code and the interpreter run the same operations; K(1) = ∞ and E(1) = 1 are set directly. All in `fermium/special.py`; the I and K kernels are LLVM functions built there. Agreement with scipy.special: 1e-11 relative or better on the tested grid (n ≤ 12, 0.01 ≤ x ≤ 150; m from −5 to 1 − 10⁻¹²).
- **Derivatives:** the differentiator knows J′, Y′, I′, K′ (recurrences in n) and dK/dm, dE/dm, so `I(x) = (2 besselj(1, x)/x)²` can be differentiated and solved for its maxima directly.
- **Alternatives:** non-integer orders (need a general algorithm such as Amos's; left out, integer orders cover the gauntlet's cases: Airy disk, cylinders, the charged ring); the incomplete integrals F(φ|m), E(φ|m) (left for later).

## D53. Vectors and matrices indexed at run time; trace, angle, row, column, element-wise abs (gauntlet #54)
- **Run-time indexes:** `v[i]`, `M[i, j]`, `M[i]` with an index that isn't a compile-time number (a loop variable, a function parameter, `k + 1`) compile to an `IVecIndex`: each index is checked to be a whole number from 1 to the size (the error is `index 4 is out of range: valid indexes here are 1 to 3`, reusing the list-index error with a negative size to select the wording), then the entries are extracted with a run-time `extractelement`. The type is still exact: all components share one unit, so a vector with a different unit per component still needs a fixed index (the result's unit would depend on i).
- **Functions:** `trace(M)` (square matrices), `row(M, i)`, `column(M, j)` (vectors; fixed or run-time index), `angle(a, b)` = atan2(|a × b|, a · b) for 2- and 3-vectors (accurate near 0 and π, unlike acos of the normalised dot product; the vectors may have different units, like a force and a velocity), and `abs(v)`/`abs(M)` entry by entry. They are built from existing IR (a `where`-style binding evaluates the argument once), so native code and the interpreter needed no new operations except `IVecIndex`.
- **Not done:** `max(v)` of a vector and a matrix norm (S11's other wishes).

## D60. Natural units: `units natural(ħ = c = 1)`, `units nuclear`, `units astro` (moonshot M1)
- **What:** a `units` line at the top level switches the unit system for the rest of the program (`units natural(ħ = c = 1)`, `units SI` switches back), or, ending in `:`, for an indented block only. `units natural` alone means ħ = c = 1; any independent subset of {ħ, c, k_B, G, ε₀} may be set to 1 (`natural(ħ = c = k_B = 1)`, geometrized `natural(G = c = 1)`, Planck `natural(ħ = c = G = 1)`). `units nuclear` is ħ = c = 1 shown in MeV and fm. `units astro` sets nothing to 1: ordinary SI checking, with values that carry no unit of their own shown in M☉, AU, yr, L☉, km/s (`print G` is 39.4769 AU³/(M☉ yr²)).
- **Design (kept the SI checker; quotient by the constants):** for a system S of constants set to 1, every SI dimension D splits *uniquely* and exactly (Fraction arithmetic, a 7×7 rational solve) as D = Σ aᵢ dim(Cᵢ) + Σ βⱼ Bⱼ, where the kept base dimensions Bⱼ are energy, A, K, mol, cd (minus those the constants eat; length first when ħ isn't set). The *canonical* dimension is Σ βⱼ Bⱼ and a quantity is stored as its canonical value φ = SI value · Π Cᵢ^(−aᵢ). φ is a group homomorphism, so ordinary unit checking on canonical dimensions **is** checking modulo ħ and c: a mass and an energy both have canonical dimension J (adding them is fine), a length and a time are both J⁻¹, and length + energy is still an error (J⁻¹ vs J). The checker applies φ in exactly three places: units (`resolve_unit`, which also covers `in`, `[unit]` parameters and plot units, and `to(x, unit)`), constants (ħ and c become exactly 1.0, m_e becomes m_e c², G becomes 1/(M_Planck c²)² = 6.70883×10⁻³⁹ GeV⁻²), and `clock()`. Nothing downstream changes: the IR, the LLVM code generator, the interpreter and `fermium build` see ordinary numbers and ordinary display Units (a unit like `fm` inside the region is a Unit with dimension J⁻¹ and factor 1 fm / ħc).
- **Output:** `print x in fm` converts with the unique factor (this is the inverse split). `print x` with no unit of its own shows the system's default: MeV powers for `natural` (`r = 0.005068 MeV⁻¹`), MeV for positive and fm for negative energy powers in `nuclear` (`1.4138 fm`, cross sections in fm²), metres for G = c = 1, plain numbers for Planck units. A value that carries a unit the user wrote keeps it (`m = 1 kg; print m` → `1 kg`), which is correct because a display Unit is canonicalized like any other. Error messages describe canonical dimensions in natural terms: "can't add energy or mass [MeV] to length or time (1/energy) [MeV⁻¹]".
- **Boundaries (the airtight part):** SI → natural is unique, natural → SI is not (is 1/MeV a length or a time?). So: (1) an SI variable used inside a natural region is converted automatically (it must have known units); (2) a variable computed in a natural region is *ambiguous* outside it and can only leave through `x in unit`, which names the SI dimension (`r_m = r in m` is then an ordinary SI length; `print r` outside is an error that says so); (3) a variable set outside a region can't be reassigned inside it; (4) a function defined in SI can be called anywhere and is checked again (instance cache key includes the system) under the caller's system, which is exact because φ is a homomorphism (`f(m) = m c²` gives the same joules); a function defined inside a natural region can't be called where those constants aren't 1; (5) ODE solutions can't cross a region boundary; `load` is refused inside a natural region (CSV columns are SI; load before the `units` line and the columns convert on use); (6) `units` lines are top-level only. Systems with the same constants (natural and nuclear) share values freely; a system with more constants set to 1 accepts values from one with fewer.
- **Why this design:** reducing the base dimensions inside the region would need a second checker and conversions at every boundary; the quotient keeps one checker, one set of error paths, zero run-time cost and exactness by construction (every value, unit and hint is in the same canonical representation, so any display fallback is still correct).
- **Alternatives:** treating `units nuclear` as a display preset only (then `E = m` and `r = 1/m_π` would be unit errors, defeating the purpose); `units astro` with G = 1 (G = 1 alone needs a choice of scale; `units natural(G = c = 1)` is available for geometrized units instead); magnitude-dependent prefixes (MeV vs GeV) at run time (would break `fermium build` parity, whose display units are fixed at compile time).
- **Limits:** `units` can't appear inside functions, loops or ifs; a mixed-unit vector (like `<1 m, 2 m/s>`) set in SI can't be converted into a region; no `load`/`fit` inside natural regions; plots inside a region label axes in the system's units; the language server shows canonical dimensions (in SI names) on hover inside a region.

## D70. Dimensional analysis: `analyze name: T [s] depends on L [m], m [kg], g` (Buckingham Π, M2)
- **Syntax:** `analyze [name:] target [unit] depends on q [unit], q, ...`. Each quantity is a name with an optional bracketed unit (only its dimension is used; `[1]` is a pure number); without brackets it must be a variable with concrete units or a built-in constant. The parser recognises the statement only when the line starts with the name `analyze` and contains the name `depends`, so `analyze`, `depends` and `on` stay ordinary names (no new keywords, no existing program changes meaning). *Alternatives:* a keyword `analyze` (breaks programs using it as a variable); `analyze T(L, m, g)` (reads like a function call and has no room for units); a built-in function returning text (the answer needs to define a function and print several lines).
- **Algorithm (`fermium/dimanalysis.py`, exact `Fraction` arithmetic, no SymPy):** the classical repeating-variables form of the Π theorem. The inputs are walked in the order written and each one that raises the rank of the dimension matrix is kept as a repeating quantity (r of them, r = rank). If the target's exponent vector isn't in their span (a unique rational solve by Gauss–Jordan), there is no formula: an error naming the base dimension only the target has ("v has length, but nothing it depends on has length"), plus "no dimensionless group at all" when n = r. Otherwise every non-repeating quantity q gives the group q · Π rep^(−a) with rep^a ~ q: n − r groups, the target in exactly one with exponent 1, so `target = Π rep^a · f(other groups)`. Other groups are rescaled (by ±1/e for each exponent e, or ±1) to minimise (a denominator above 2, Σ|exponent|, the number of negative exponents): Re = ρ v √A/μ rather than μ²/(ρ² v² A). An input with exponent 0 in every group drops out, with "nothing else has <dimension>" when that is the reason. Tests check each classic case against the textbook exponents and cross-check the group count against a nullspace basis and SymPy's `Matrix.nullspace` (optional).
- **Why repeating variables rather than an arbitrary nullspace basis:** a raw nullspace basis is correct but arbitrary (F ρ/μ² and Re² are as valid as F/(ρ v² A) and Re); textbooks choose repeating variables, and the written order gives the user control over the form (list ρ, v, A first to get F = ρ v² A f(Re); list μ first to get the Stokes form). *Alternative:* a search over bases for the "simplest" (sum of |exponents|): opaque, and it can't know that ρ v² A is the conventional prefactor.
- **Output:** printed at run time, in program order, as text lines (`SPrint` of table strings), so both back ends, the REPL and `fermium build` handle it with no new IR. The formulas are valid Fermium (`√(L/g)`, `∛(…)`, `(E t²/ρ)^(1/5)`), with the names spelled as written (ε₀, not ε_0).
- **Making it usable:** with a name, the statement also defines `name(params) = prefactor` through the ordinary `FuncDef` path (so calls are unit-checked and specialised like any function); the parameters are the non-constant quantities with a non-zero exponent, bracketed ones keep their unit annotation, constants (`G`, `ħ`) are read as globals. `fit T = C pendulum(L, 9.81 m/s²) to data` then fits the pure number (≈ 2π on the bootcamp data). When the prefactor has only constants (`planck: ℓ [m] depends on G, ħ, c`) the name is a plain variable and its value is printed. *Alternative:* defining a function for every Π group (noisy; the prefactor is what people plot and fit).
- **Limits:** top level only (it defines a function); no vectors/matrices; only the 7 SI base dimensions (angles are pure numbers, so an angle is a group by itself); one target per statement; C and f can't be found by dimensional analysis, by definition.

## D80. Seeded random numbers: one xoshiro256** in LLVM IR and in Python (M3)
- **What:** `rand()`, `rand(a, b)`, `randn()`, `randn(μ, σ)`, `seed(n)` and `sample(expr, N)`. The generator is xoshiro256** (Blackman–Vigna), seeded by expanding the seed through splitmix64; `rand()` is the top 53 bits × 2⁻⁵³ (uniform in [0, 1)), `randn()` is Box–Muller from two `rand()` values (sqrt(−2 ln(1 − u₁)) cos(2π u₂), one normal per pair, no cached second value so the stream is easy to mirror). `seed(s)` truncates s to a whole number (|s| ≥ 2⁶³ or NaN → 0).
- **Same numbers everywhere:** the generator is written in LLVM IR (`codegen_m3.py`) and in Python (`rng.py`), and tests check both against Vigna's reference vectors and against each other. Under `fermium run` and in the REPL/Jupyter the four state words live in a ctypes array owned by the `Runtime` (compiled code addresses it directly), so the REPL continues one stream across inputs, and the interpreter uses the same array. `fermium build` puts the state in a module global initialised to the seed-0 state. Before: `drand48` in compiled code and Python's `random` in the interpreter, so the two back ends disagreed and nothing could be seeded.
- **Unseeded programs are reproducible:** a program that never calls `seed` starts from `seed(0)`. Reproducibility (and the differential tests) matter more here than fresh randomness per run; `seed(clock() / (1 s))` gives a varying seed. *Alternative:* seed from the time by default (NumPy, Julia) — then two runs of a homework program disagree, and so would the JIT and interpreter.
- **Units:** `rand(a, b)` and `randn(μ, σ)` need both arguments in the same units and return them; the seed and the sample count must be plain numbers. `seed` is a statement (`x = seed(1)` is an error), so its effect is visible in the program's order.
- **`sample(expr, N)`:** the expression becomes a scalar lambda (like an integrand) called N times, so each `rand()` inside draws afresh; it returns a list with the expression's units, which `mean`, `std` (N − 1) and `len` turn into a Monte Carlo estimate ± its error. *Alternatives:* a `montecarlo … over N` block (more syntax for the same thing), list comprehensions (not in the language yet); `sample` also reads well for error propagation (`sample(2π sqrt(randn(L, σL) / g), 20000)`).
- *Alternatives for the generator:* PCG64 (NumPy's default; 128-bit multiply is awkward in IR), Mersenne Twister (2.5 kB of state), drand48 (48-bit LCG, weak low bits, platform-specific). xoshiro256** is 4 words, a handful of shifts and one multiply, and passes BigCrush.

## D81. Fourier transforms without complex numbers: fft_re / fft_im / spectra with units (M3)
- **What:** `fft_re(xs)`, `fft_im(xs)`, `ifft(re, im)`, `amplitude_spectrum(xs)`, `power_spectrum(xs, dt)`, `frequencies(xs or n, dt)`, plus `argmax`/`argmin` to find a peak. Fermium has no complex type, so the raw transform is returned as two real lists (NumPy's unnormalised convention, so results compare directly with `numpy.fft.fft`), and the physically common questions — "which frequency, how strong?" — get their own functions that return real lists with units: amplitudes in the signal's units (a sine of amplitude A at a bin frequency peaks at exactly A: |Xₖ|/n doubled for 0 < k < n/2), a one-sided power spectral density in units²·time (shown as V²/Hz when the signal has a display unit; Parseval Σ P Δf = mean(x²) holds exactly), and the bin frequencies k/(n dt) in 1/time (shown in Hz).
- **Where it runs (D3):** the transform is a runtime callback, `fm_fft(kind, a, b, n, dt, out)`: NumPy (`runtime/spectral.py`) under the JIT and in the interpreter (so they print the same digits), and a C FFT in `aot_rt.c` for `fermium build` (iterative radix 2 for powers of two, Bluestein's chirp-z through a power-of-two FFT otherwise; agrees with NumPy to ~10⁻¹³ in tests). The compiled code allocates the result list, checks empty input and mismatched `ifft` lengths itself, and the callback fills it.
- *Alternatives:* a complex number type (large change across checker, IR, both back ends; worth doing later, and these functions would stay as conveniences); returning |X| only (loses phase, can't invert); an FFT written in LLVM IR (possible, but NumPy's pocketfft is faster and better tested, and the C version covers `fermium build`); `fft(xs)` returning interleaved re/im (error-prone to index).

## D82. Bound states: `solve … lowest N` as an eigenvalue problem, matrix and shooting (M3)
- **Syntax:** `solve -ħ²/(2m) * ψ'' + V(x) ψ = E ψ with ψ(a) = 0, ψ(b) = 0 for x from a to b lowest N [grid M] [using matrix|shooting]`. It is an ordinary `solve` with one new clause, `lowest N` (optionally `states`), which is what makes it an eigenvalue problem. The eigenvalue is inferred: the one name in the equation that has no value yet (0 or ≥ 2 such names are errors naming them). *Alternatives:* `for E` (clashes with the `for x from …` range clause and reads like a loop); an `eigen` statement (a new keyword for something that reads like solving an equation); explicit `eigenvalue E` (more words; inference is safe because exactly one unknown name is required).
- **Results:** the eigenvalue name becomes a list of the N eigenvalues (ascending, with the units the equation implies), and the states are bound as `ψ₁ … ψ_N` (canonical `ψ_1`): solution views on one `SolStruct` whose "time" axis is the x grid, with components [ψ₁, ψ₁', …, ψ_N, ψ_N', E₁ … E_N]. So `ψ₁(x)`, `ψ₁'(x)`, `ψ₁''(x)`, `values`, `times`, `plot ψ₁ vs x` and interpolation reuse the ODE machinery unchanged (cubic Hermite in x with exact ψ'' = (α − wE)ψ as the slope of ψ'). ψ is normalised (∫ψ² dx = 1, so its dimension is length^−½ — the checker assigns it, since the equation is homogeneous in ψ) and made positive in its first lobe. The eigenvalue list is read from the constant columns at x = a.
- **How the equation becomes numbers:** the checker builds the ODE right-hand side f(x, [ψ, ψ', E]) → [ψ', ψ'', 0] by isolating ψ'' as for any ODE (E is a constant state — exactly the shooting method's formulation). The solver probes it: α(x) = f(x, [1, 0, 0]), w(x) = α − f(x, [1, 0, 1]), so ψ'' = (α − wE)ψ; spot checks refuse a ψ' term (not symmetric; a Sturm–Liouville transform would handle it later) and nonlinearity, and w must be positive (w < 0 has no lowest states). Units are all checked before this, at compile time.
- **Matrix method (default):** second-order differences on the interior points; W^−½(−D² + diag α)W^−½ is symmetric tridiagonal, and `scipy.linalg.eigh_tridiagonal(select='i')` finds only the N lowest (bisection to full relative accuracy: `tol` at the underflow limit — the default absolute tolerance ε‖T‖ limited E₁ to 10⁻⁹). Solved on 2M, M and M/2 intervals: Richardson extrapolation (4E_2M − E_M)/3 when the error visibly shrinks 3–5× per halving (smooth V: 10⁻¹¹–10⁻¹² relative for the well and oscillator), else the finest value. A jump in α or w between grid points (a finite well's wall) is located by bisection on the right-hand side and the coefficient of the node whose cell it cuts is the cell average: the error drops from O(h) (10⁻³ at 4000 intervals) to ~10⁻⁷.
- **Shooting method:** Numerov from ψ(a) = 0 in summed form (increments of y = (1 − h²f/12)ψ; the textbook form loses 10⁻⁹ in the 1 ± h²f/12 factors), with overflow rescaling. Sturm's theorem (sign changes including the end = eigenvalues below E) brackets the n-th state by bisection, then brentq solves ψ(b; E) = 0. States are integrated from both ends and joined at the rightmost classically allowed point. It shares only the coefficient probing with the matrix method, so the two agree as an independent check (10⁻¹² on smooth potentials in tests).
- **Where it runs (D3):** Python (`runtime/eigen.py`, NumPy/SciPy) for both back ends: compiled code calls `fm_eigen` with the right side's address and `fm_ode_guard` (as `using radau`, D42); the interpreter calls the same solver. `fermium build` refuses it with a clear error.
- **Limits:** Dirichlet (ψ = 0) ends only; no ψ' term; one equation; real symmetric problems only (no complex potentials); N ≤ 500.

## D83. 1-D PDEs: `solve ∂u/∂t = …` with two ranges, a probe function, and animated plots (M3)
- **Syntax:** `solve ∂u/∂t = D * ∂²u/∂x² with u(x, 0 s) = …, u(0 m, t) = …, ∂u/∂x(L, t) = … for x from 0 m to L, t from 0 s to T [step dt] [grid M] [using crank_nicolson|implicit|explicit]`. It is the ordinary `solve` with a second range after a comma (first range = space, second = time), so nothing new has to be learned beyond the ∂ notation. The Leibniz forms `∂u/∂t`, `∂²u/∂x²` are new in the parser (before, only `∂/∂x f` parsed); they also work on ordinary functions (`∂f/∂x(1, 2)`). Initial conditions are recognised by the space variable as first argument, boundary conditions by the time variable as second. *Alternatives:* subscripts `u_t = D u_xx` (clash with names like `u_t` a user might define, and don't look like the textbook ∂); a separate `pde` statement (another keyword for the same idea); `u(x, t)` declared up front (more ceremony).
- **One probe function:** the checker compiles one ODE-shaped lambda f(x, [u, u_x, u_xx, t, u_t, i]) → [rhs, u₀, phase₀, v₀, left, right]: the equation solved for its highest t-derivative (by the existing linear `isolate`), the initial value (and phase, and initial velocity for waves) and the two boundary values. All units are checked there at compile time (u's units come from the initial value; boundary slopes must be u/x). The solver probes it on the grid: the coefficient of u_xx is f(…, u_xx = 1) − f(0), and so on, plus spot checks for linearity and for coefficients that change with t (refused; a t-dependent source is re-probed each step). Probing keeps the solver generic (heat, drift–diffusion, reaction terms, damped waves) without a second symbolic pass.
- **Complex without complex numbers:** in a PDE, `i` is the imaginary unit. The probe passes i as a real number; the isolated right side is R + G/i + K i in i (the only forms a linear Schrödinger-type equation takes), so three probes (i = 1, −1, 2) recover the true complex coefficient R + i(K − G). The initial value `A(x) exp(i φ(x))` is split syntactically into amplitude and phase (φ with i := 1). The solution stores Re and Im, and `ψ(x, t)` is the 2-vector <Re ψ, Im ψ>, so `|ψ(x, t)|²` is the density with no new syntax. *Alternative:* a complex type (the right long-term answer; this keeps the surface small for now).
- **Numerics (`runtime/pde.py`, Python/NumPy/SciPy for both back ends, D3):** method of lines with second-order central differences; Neumann ends by a ghost point. First order in t: the θ-method with one sparse LU (`splu`) — Crank–Nicolson by default (second order, A-stable; unitary for Schrödinger, so the discrete norm is conserved to 10⁻¹² in tests), backward Euler, forward Euler (with the stability limit checked, and chosen automatically when no step is given). Second order in t: explicit leapfrog with a Taylor first step and optional damping; default dt is the largest step with Courant number ≤ 1 (exact for constant c at 1, as the d'Alembert tests show). At most ~1000 snapshots are kept (with ∂u/∂t from the semi-discrete right side, for Hermite interpolation in t). A jump in a coefficient between grid points (a barrier's walls) is located by bisection on the probe and cell-averaged, as in D82: the transmission through a 1 nm barrier went from jumping by 20 % between grids (O(h), walls sampled pointwise) to converging to 0.3 %.
- **Evaluation:** the solution is a `SolStruct` whose rows are snapshots and whose components are grid points (Re then Im), so the existing Hermite-in-t kernel `fm_sol_eval` is reused per grid point; `fm_pde_eval` (LLVM) / `pde_eval_py` (interpreter) combine 4 neighbours with cubic Lagrange weights in x (or their derivatives for ∂u/∂x). The grid's ends live in hidden variables, so reassigning L later doesn't move the grid.
- **Animation:** `plot u vs x animate over t [frames N] to "f.gif"` (also `with animate over t`) is a statement of its own (`SAnimate`): the runtime draws frames with matplotlib's `FuncAnimation` and `PillowWriter` (pillow is installed here); with a non-.gif name (or without pillow, which matplotlib itself needs anyway) it writes PNG frames into a folder and says so. A complex solution is shown as |ψ|² (per nm when x is in nm). `plot u vs x` without `animate` draws 6 snapshots in one PNG.
- **Validated against:** heat decay of a sine mode exp(−Dk²t) (3×10⁻⁵ at 200 intervals, second-order convergence checked), Neumann cosine mode and heat conservation, a steady state with a source and a moving boundary, the time stepping against `scipy.linalg.expm` of the semi-discrete system (10⁻⁷), d'Alembert's solution including inverted reflections at fixed ends, and a free Gaussian packet (centre ħk₀t/m and width σ√(1 + (t/τ)²) within 10⁻³ at 4000 intervals, converging with the grid; norm drift < 10⁻¹⁰).
- **Limits:** one unknown, 1-D, linear, coefficients constant in t, uniform grid, second order in x (a moving packet needs k₀h ≲ 0.1 for 10⁻³ accuracy); `fermium build` refuses PDEs; `∫` over a wide range with a narrow solution can miss it (the known quadrature limitation, §19).

## D90. Complex numbers: the literal `4i` and the constant `𝑖` (gauntlet #23, Q6)
- **What:** a number followed *directly* by `i` is an imaginary literal: `4i`, `2.5i`, `1e3i`, `3×10⁸i`. The lexer makes an `IMAG` token (only when the next character can't continue a name, so `3inch` is untouched) and the parser turns it into `4 × 𝑖`, where `𝑖` (U+1D456, Tab `\imag`) is a built-in constant, standalone like `π` (`𝑖ħ` is 𝑖 · ħ). A unit may follow the literal (`4i Ω`). `complex(a, b)`, `polar(r, θ)` and `cis(θ)` build complex numbers from real ones; `1i` is the name `𝑖` itself (so `fmt` round-trips it); `fmt --ascii` writes `𝑖` as `1i`, or `(1i)` before a name (`𝑖ħ` → `(1i)hbar`, since `1i hbar` would read `hbar` as a unit after the literal).
- **Why:** programs everywhere use `i` as a loop variable (`for i from 1 to n`), so a bare `i` must stay an ordinary name. A digit suffix reads like paper (`3 + 4i`) and like a unit after a number, which Fermium already has; it can't collide with a variable because names don't start with digits (`4 i`, with a space, is still 4 times a variable i: no program in the repository used `2i` as a product). Making the literal `4 × 𝑖` in the AST means symbolic differentiation, simplification and `to_source` need nothing new: `𝑖` is a constant name (SymPy gets it as `sympy.I`, not as a real symbol).
- **Alternatives:** `im` as the constant (Julia; but `im(z)` is the natural name for the imaginary part, which physics texts write Im z), `j` (engineering; clashes with loop variables and with current density J), making `i` a keyword (breaks every loop), `1i` only (the same rule, just less general).
- **Input with units:** units follow numbers (spec §3.4.2), so `(3 + 4i) Ω` doesn't parse; write `3 Ω + 4i Ω`, `(3 + 4i) [Ω]` or `R + 1i ω L`. The *output* is `(3 + 4i) Ω` (D94).

## D91. `ComplexTy(dim)`: one unit for both parts, stored like a 2-vector
- **What:** `ComplexTy` is a subclass of `VecTy` with n = 2 (`kind = "cplx"`): an LLVM `<2 x double>` (real, imaginary) in native code and a Python tuple in the interpreter. So variables, globals, REPL arena slots, captures in integrands and ODE right-hand sides, function arguments and results, `if` expressions and ODE state slots all reuse the vector code with no change. The checker gives complex values their own rules *before* the vector rules (arithmetic, powers, `|z|`, comparisons, `.re/.im`, indexing is an error, built-ins), in `fermium/cplx.py`; a built-in without complex support (`floor`, `dot`, …) says "doesn't work on complex numbers". Function instances are keyed by the complex argument's units like real ones.
- **Units:** both parts share one dimension, like an impedance in Ω or a wavefunction in m^(-1/2). `+ −` need equal units (a real operand is promoted to x + 0i), `* /` multiply and divide units, `abs`, `re`, `im`, `conj` keep them, `arg` is a plain number, `sqrt` and fixed powers take the units to the power, `exp ln sin cos …` need plain numbers. °C/°F can't be used (an offset makes no sense for a complex number).
- **Why a subclass:** the whole storage/ODE/capture machinery is typed on `VecTy`; a separate class would have touched a dozen places in the code generator, the interpreter and the solver. The risk (vector semantics applying silently, like `z · w` as a dot product) is closed by intercepting complex values first in each checker rule and refusing every built-in not written for them.
- **Not done:** lists, vectors and matrices of complex numbers (lists hold doubles; clear errors), `values(ψ)`/`plot ψ` of a whole complex solution (use `ψ.re`, `ψ.im`).

## D92. Complex arithmetic kernels, shared by native code and the interpreter
- **What:** the operations are written once in `fermium/cplx.py` against an `ops` object (as `fermium/linalg.py` does), and run as LLVM instructions (`LLOps` plus libm calls) or as Python floats (IEEE-safe wrappers of `math`), so `fermium run` and `fermium run --interp` agree to the last bit. Multiplication is the textbook formula; division is Smith's algorithm (no overflow for large divisors); `√z` is the cancellation-free t = √((|x| + |z|)/2), then y/(2t) (the principal branch, cut on the negative real axis, `√0 = 0`); `exp`, `ln` (log|z| + i arg z), `sin cos sinh cosh` by their real/imaginary formulas, `tan tanh` as quotients; `z^n` for a whole constant |n| ≤ 64 by repeated squaring (then 1/zⁿ for n < 0); `z^p` for another fixed p in polar form (|z|^p, pθ); `z^w` as exp(w ln z) with 0^w = 0 (and 0^0 = 1). `==`/`!=` compare both parts; `≈` is |a − b| ≤ 10⁻⁶ max(|a|, |b|) (D21). `<` and friends are errors.
- **Accuracy:** agreement with Python's `cmath` to 10⁻¹³ relative on the tested cases (`tests/test_complex.py`), which is all one can ask of double precision for these formulas.
- **Alternatives:** LLVM/C `_Complex` calls (`cexp` etc.: not available the same way in the interpreter and the browser; ABI details for passing complex values), `cmath` in the interpreter (would differ from native code in the last bits and break differential tests).

## D93. Complex unknowns in `solve`; complex integrals and sums
- **ODEs:** a complex unknown takes two real state slots (real and imaginary parts), so the RK4, Dormand–Prince, Radau/BDF kernels, dense output, `until`, backward ranges and the error norm work unchanged (the step control treats the two parts as separate components). An unknown is complex if one of its initial values is complex, or if its equation turns out complex: `𝑖ħ ψ' = E ψ with ψ(0) = 1` is first checked with ψ real; when a side of the equation comes out complex, the solve is checked again with ψ complex (the real initial values are promoted to x + 0i). `ψ(t)`, `ψ'(t)`, `ψ[end]` are complex; `ψ.re` and `ψ.im` are real solution components (like `r.x` of a vector solution), so they can be plotted, sampled with `values` and searched with `max`.
- **Integrals and sums:** a complex integrand is split into `∫ re(f) dx + i ∫ im(f) dx`, each its own adaptive quadrature with the usual tolerance (the same design as vector integrands, D35); `Σ` does the same per part. The integrand is evaluated once per part at each node (twice the work of one real integral).
- **Alternatives:** complex state in the kernels (a new kernel family for no gain: the split is exact for ODEs), one quadrature on the pair with a joint error estimate (would need a vector-valued kernel; per part is simpler and each part meets its own tolerance).

## D94. Printing complex numbers
- **What:** `3 + 4i`, `3 - 4i`, `0 + 4i`, `-1 + 0i`; with a unit, `(3 + 4i) Ω` (the display unit, `in`, `to(z, unit)`, significant figures and `to N digits` follow the rules for real numbers, applied to each part). When the inputs give no precision, each part gets 3 significant figures with trailing zeros kept (`0.540 + 0.841i`, the default for real numbers, D11), and whole numbers print exactly only when both parts are whole (`3 + 4i`, but `0.500 + 1.00i`, as for the entries of a vector). A part smaller than 10⁻¹⁴ |z| is floating-point rounding and prints as 0, so `exp(𝑖 π)` prints `-1 + 0i` rather than `-1 + 1.22465×10⁻¹⁶i` (a genuinely small complex number, `exp(𝑖 π) + 1`, still prints its value). Negative zero prints as `0`. The C runtime of `fermium build` has the same formatter (`fm_print_cplx`), tested against the JIT's output; the language server's hover says "a complex number of resistance [Ω]".
- **Alternatives:** `3+4im` (Julia) or `(3+4j)` (Python): less like paper; `3 + 4i Ω` without brackets (reads as if only the imaginary part had the unit).

## D95. Red-team round 1: Hz as rad/s, confusable units, °C sums, poles in `solve`, a check on fixed-step RK4 (REDTEAM.md)
- **Hz in both directions (extends D27):** angles stay plain numbers, so Hz *is* rad/s, and `1 Hz in rpm` is 9.55 rpm, not the 60 a physicist expects. Any conversion between a value whose display unit is in the Hz family and one in rev, rpm, rad/s or °/s now warns, in either direction, with the numbers for that case ("so 1 Hz is 9.5493 rpm, not 60 rpm"; "1 rpm is 0.10472 Hz here, not 0.0166667 Hz"). `rad/s in Hz` keeps the ω-specific warning from gauntlet friction #5.
- **A light tag, not a type:** `checker._unit_kind` reads the display unit the user wrote (the hint) and tags it `cycles` (Hz, kHz, …), `angular` (rad, rev, rpm, °, arcmin/arcsec in the name), `Gy`, `Sv`, `Bq`, `energy` (J) or `torque` (N m). Adding or subtracting two values whose tags are a known confusable pair (cycles/angular, Bq/Hz, Bq/angular, Gy/Sv, J/N m) warns. `2π f` of a Hz value and `ω/(2π)` of an angular one drop the hint (they are the conversion), so `2π f in rad/s` is quiet and shows 1/s by default.
  - *Why:* the hint already travels with every value and costs nothing at run time; the checks only fire on a written unit, so there are no false alarms on `3 /s`.
  - *Alternatives:* an angle dimension (rejected in D6); a "cycles" dimension for Hz (then `f = 1/T` is not Hz without a cast); errors instead of warnings (`1 Bq + 1 Hz` could be deliberate in a units exercise).
- **°C (extends D12):** `sum`/`cumsum` of a list in °C is an error, like `°C + °C`. `T / 2` warns like `2 T`, and the warning's example uses the actual factor when it is a constant (no warning for `T * 1`).
- **Poles in `solve … for x` (extends D32):** a scan point where lhs − rhs is ±∞ is skipped (no sign change is taken across it); if no crossing is found and the finite values on both sides of such a point have opposite signs, that is the "jump" error at that point. A NaN at an Illinois iterate is the same error, instead of being returned as the root. Mirrored exactly in `fm_root` and `interp.root`.
- **Fixed-step RK4 check (extends D17):** after an RK4 solve, `fm_rk4_check`/`interp.rk4_error` take 8 evenly spaced stored steps, do one RK4 step of 2h from each, and compare with the stored point two steps on. The difference is 30× the local error of one step; times the number of steps it estimates the global error, relative to D17's scale-free size (max(|y|, |y₂|) + |y₂ − y|). Above 10⁻³ a run-time warning (`fm_warn` kind 7, once per line) says the step is too coarse. Cost: 24 right-hand-side calls. *Alternatives:* a full second solve at h/2 (doubles the cost); an embedded 3rd-order estimate inside the loop (slows the benchmark's inner loop).
- **Not changed:** significant figures through `+`/`−` (`1.00 m - 0.999 m` prints 0.00100 m). Carrying decimal places needs the result's magnitude, which is only known at run time, so the print format would have to be computed at run time in the JIT, the interpreter and the C runtime. Logged in REDTEAM.md.

## D100. Modules: `import name [as alias]`, `import "file.fm" [as alias]`, `from name import a [as b], c` (M7)
- **Syntax (Python's, because it is what physics students meet first):** `import mechanics` binds the module, whose names are then written `mechanics.kinetic_energy(…)`; `import astro as a` binds it as `a`; `from nuclear import semf_binding, Q_value as Q` binds those names directly; `import "lib/springs.fm"` imports a file by path (bound to its stem, or `as` a name when the stem isn't a valid name). `import` and `as` are recognised only at the start of a statement / after the module name, and `from` is already a keyword that can't start a statement, so no existing program changes meaning (`as = 3 m` still works). One module per `import` line. *Alternatives:* `import mechanics` dumping every name into the program (short to write, but two modules defining `energy` would clash silently and a reader can't tell where a name comes from); `use mechanics` (reserved for Python interop, M6); `include "file.fm"` textual inclusion (no namespaces, line numbers in errors would be wrong, a module could see and depend on the program's variables).
- **Qualified names** are the existing `Field` syntax (`a.b`): when the target of `.name` is a name bound to a module, the checker looks the member up in the module; otherwise `.x`/`.t`/data columns work as before. So the parser change is only the new statement, and `mechanics.kinetic_energy'`, passing `astro.wien_peak` to a function and `∫ shapes.sq(x) dx` all work through the ordinary paths.
- **Top level only:** an import inside `if`, a loop or a function is an error (a module's constants become globals, and conditional imports would make the names that exist depend on run-time values).

## D101. What a module is: its own scope, checked once per compilation, functions instantiated at the call
- A module is a `.fm` file whose top level has only function definitions, constants (`name = expr`) and imports. Anything else (print, plot, solve, loops, `x += 1`) is an error naming the module file and line: *a module can only define functions and constants, but this line has a print*. *Alternative:* running top-level side effects once (Python's model): importing a physics helper should never print or plot, and "run once" is surprising in the REPL; forbidding them keeps a module a library.
- **Scope:** the module is checked in its own `Scope` whose parent is the built-in constants (not the program's globals), so a module's functions see the module's own names and never the importing program's variables; the program sees only what it imports. Names starting with `_` are private (not importable, not reachable with `.`).
- **Constants** are checked with the importing main function as their owner, so they become ordinary globals (in the REPL, arena slots) computed at the point of the (first) import; a second import of the same file in one compilation reuses the loaded module and emits nothing.
- **Functions** are the usual generic `FuncInfo`s: each call is instantiated with the caller's argument units (D43 machinery unchanged), so units are checked across modules exactly as within a file. Parameters with units (`kinetic_energy(mass [kg], v [m/s])`) give the clearest error at the call.
- **Errors inside modules:** an error while loading (syntax, units of a constant, a forbidden statement) is reported at the import line as `in the module springs (lib/springs.fm, line 4): …`. An error inside a module function's body during a call is reported at the call in the user's program, with `(in springs.hooke, springs.fm, line 5)` appended; nested module calls keep the innermost module location and the outermost user line. Warnings from module code are relocated the same way. (Diagnostics carry only a line/column, not a file, so this relocation is what keeps `FermiumError.format(source)` showing the user's own line.)
- **Clashes are errors:** the same name imported from two modules (`f is already imported from m1 (line 1)…`, suggesting `as`), an imported name that the program defines (before or after the import), and a module name that the program assigns. **Cycles** are detected with a loading stack: `circular import: a → b → a`.
- **Cost/limits:** modules are parsed and checked again by each compilation (no cache of compiled modules); the REPL keeps a loaded module for the session. Functions of a module that the program never calls are not checked (the stdlib is checked by its tests).

## D102. Module search path and `fermium.toml`
- `import name` looks for `name.fm` in: the importing file's folder, the program's folder, the folders listed under `[paths] modules = [...]` in the nearest `fermium.toml` (program folder or a parent; paths relative to it), then the standard library `fermium/stdlib/`. The user's folders come first, so a local `mechanics.fm` shadows the standard one (like Python). A missing module lists the folders searched and suggests a close name.
- `fermium.toml` is minimal: `[project] name, version` and `[paths] modules`. It is read (with `tomllib`, or a tiny fallback parser on Python 3.10 without `tomli`) by the checker when it resolves an import, so `fermium run`, `check`, the REPL (from the current folder), Jupyter and the language server (from the document's folder) all get it with no per-tool code. A broken file is an error at the import line; a program with no imports never reads it. *Alternative:* the CLI passing search paths down (every entry point would need to repeat it).
- The standard library ships as package data (`pyproject.toml` `"fermium" = ["stdlib/*.fm"]`; the playground's wheel builder includes `.fm` files).

## D103. The standard library as Fermium source
- `fermium/stdlib/{mechanics,em,nuclear,astro,quantum,stats}.fm`, written in Fermium (not Python built-ins), so they are readable examples, unit-checked like user code and cost nothing when not imported. Every function has a comment line above it (its documentation) and, except for the generic `stats` helpers and quantities like `remaining(N0, …)` that work in any unit, unit annotations on its parameters.
- `docs/stdlib.md` is generated from the source by `python3 -m fermium.stdlib_doc`: signature with units, the comment, and the units of the result found by the checker (calling the function with arguments in the annotated units). A test fails when the page is out of date, when a function lacks a comment, or when a function isn't tested.
- `tests/test_stdlib.py` checks every function against a closed form or SciPy (`scipy.special.ellipk`, `scipy.integrate.quad` for the ΛCDM distance, `solve_ivp` for Bateman, `scipy.stats.linregress`/`sem` for the fits) to ~1e-9, plus textbook anchors (hydrogen −13.598 eV, d_L(z=1) ≈ 6608 Mpc, ⁵⁶Fe SEMF within 1 %).
- **Choices:** SEMF coefficients from Rohlf's least-squares fit (15.75, 17.8, 0.711, 23.7, 11.18 MeV, pairing ±a_P/√A); gravity is an argument (`projectile_range(v, θ, g_n)`) so the Moon works; `cyclotron_frequency` is in Hz (turns per second) and `cyclotron_angular_frequency` in rad/s, to avoid the ω/f confusion; the relativistic kinetic energy is written as m v²/(γ⁻¹(1 + γ⁻¹)) so it doesn't cancel at small v; the luminosity distance integrates 1/E(z) with the built-in quadrature (flat ΛCDM, radiation neglected).
- **Language fix found while writing `stats`:** a function whose list parameter was used only inside a reduction's argument (`sum(xs / σs^2)`) was applied element-wise to lists (only a bare `sum(xs)` counted as "takes lists"). `sum`/`mean`/`std` of any expression mentioning a parameter now marks the function as taking lists.

## D110. An integral that is exactly 0 because every sample was 0 warns (review priority 3)
- **What:** `fm_quad` (and `interp.quad`) add up the ∫|g| estimates of the panels they accept (the QUADPACK resabs of D44) in `fm.qabs`, saved and restored around each call so that an integral inside the integrand doesn't count. If the result is exactly 0, that sum is 0, the range isn't empty and this isn't the quiet first try of a vector component (D44), a run-time warning (kind 3, once per line) says so: *this integral came out as exactly 0 because the integrand was 0 at every point where it was sampled; if it is non-zero somewhere narrow (a peak in a wide range), integrate over a range that fits it*. The JIT, the interpreter and `fermium build` print it alike.
- **Why:** it is the one silent wrong answer of the quadrature that can be recognised for certain. `∫ exp(-x²) dx from -1e6 to 1e6` used to print 0 with no hint. When the adaptive loop sees a spike at one node and bisects that panel, the children's nodes can miss it too, and it then converges happily to 0. A result that is 0 by symmetry has non-zero samples, so it stays quiet.
- **Cost:** one add per accepted integral. A true zero (`∫ 0 * x dx`, or `f(y) = ∫ x y dx` at y = 0) also warns. The message says "if", and that is rare in practice.
- **Not done:** a peak that is partly sampled still gives a plausible but wrong number with no warning. That would need a second, independent quadrature (double the cost) or a scan of the integrand, which is what the half-line code does for its length scale. Tried: 7 starting panels, so the middle of the range is a node. The node does see the peak, but the panel is then bisected and its children miss it again, so it only changed the digits of every other integral; dropped.

## D111. Solve options in any order (research friction #60)
- **What:** after `for t from a to b [step h]`, the options `tolerance r`, `using m` (or `method m`) and `until lhs = rhs` may come in any order. The option words stay excluded from juxtaposition while all of them are parsed, so `tolerance 1e-11 using radau` no longer reads `1e-11 using` as a product. An option given twice is an error.
- **Why:** reference §10 listed the order `[tolerance r] [using method]`, but the parser restored the juxtaposition set before reading the tolerance, so even that documented order failed with "using isn't defined"; research code wants both a tight tolerance and a stiff solver. Fixing the order would only move the trap.
- **Alternatives:** one fixed order with a clearer error (still makes people remember an order for no reason).

## D112. The upper-limit division warning covers bracketed divisors (research friction #61)
- **What:** D34's warning ("the ' / ' after the upper limit divides the whole integral, not the limit") now also fires when the divisor after a limit-ending spaced `/` starts with `(`, `|`, `√` or `∛` (`to 1 / (1 + z)`), not only for a plain number or name. It does not fire when the upper limit is ±∞ (with or without a unit): ∞ divided by anything positive is still ∞, so both readings of `to ∞ / (μ₀ I)` agree. The lower limit needs no rule: `to` must follow it, so `from 1 / (1 + z) to 1` already keeps the division in the limit.
- **Why:** a research reproduction (a cosmology integral up to 1/(1+z)) got the silent "whole integral" reading; the unbracketed case already warned, so the bracketed one looked safe but wasn't. The parse itself is unchanged (D34's space rule), so existing programs keep their meaning; one gauntlet program that meant the average (`… from 0 to 2π / (2π)`) now brackets the integral explicitly.
- **Alternatives:** make a bracketed divisor part of the limit (breaks `to ∞ / (μ₀ I)`, D34's motivating case); an error instead of a warning (too strict for a parse that is well defined).

## D113. ODE solutions over lists; `cot`, `sec`, `csc` (research frictions #62, #63)
- **What:** `u(ts)` with a list of times gives the list of `u` at each time (also `u'(ts)`), like `f(xs)` for a function: the checker gives the `ISolEval` node a list type, and the compiled code (`map_list` around `fm_sol_eval`) and the interpreter loop over the list. A vector solution over a list is an error, since lists of vectors don't exist yet. `cot`, `sec`, `csc` are one-argument maths built-ins computed as 1/tan, 1/cos, 1/sin (IEEE division, so `cot(0)` is ∞, in both backends); they have derivative rules, SymPy names (so `∫ csc(x)² dx` gives `-cot(x)`), and work element-wise on lists.
- **Why:** research code evaluates solutions on a grid (`u(rs)`) to compare with tabulated data, and textbook formulas use cot and csc (Rutherford scattering's csc⁴(θ/2)).
- **Alternatives:** for #62, a separate `sample(u, ts)` built-in (a new name for something `f(xs)` already teaches); for #63, rewriting `cot(x)` to `cos(x)/sin(x)` in the parser (the printed formulas and derivatives would no longer show `cot`).

## D114. List slices `xs[a:b]`: 1-based, both ends included
- **What:** `xs[a:b]` is a new list of elements a to b, both included; `xs[:b]` starts at 1, `xs[a:]` ends at `end`, and `end` works inside. `xs[a:a-1]` is the empty list (so `xs[k+1:end]` is empty when k is the length); any other b < a is a run-time error that suggests `reverse(xs)`; the ends are checked like single indexes (whole numbers from 1 to the length). Slices also work on an ODE solution's samples (`x[2:end]`), are copies, and can't be assigned to. Compiled as the built-in `slice` (a checked copy loop), mirrored in the interpreter and in `fermium build`.
- **Why:** Fermium counts from 1 and `xs[end]` is the last element, so an inclusive range matches `for i from a to b` and the physicist's "elements 2 to 5". Julia, MATLAB and Fortran do the same; Python's half-open `a:b` would make `xs[2:5]` have three elements, which contradicts every other range in the language.
- **Alternatives:** half-open slices (Python); allowing any b < a to give an empty list (Julia) — rejected because a reversed slice written by mistake (`xs[5:1]`, expecting a reversed list) would silently give nothing; negative indexes from the end (use `end-1`).

## D115. Plot options without `with` (research friction #65)
- **What:** after the last `y vs x` series, the options `title "…"`, `log [x|y]` and `points` may follow a comma (or, for `title "…"`, a space) without `with`: `plot N vs t, title "Decay"`. The word counts as an option only when it isn't one of the program's names and the next token fits (a string after `title`; `x`, `y`, a comma or the end after `log`); `title` is kept out of juxtaposition while the series is parsed, so `plot y vs x title "…"` doesn't read `x title` as a product. A string or `title` where `vs` was expected gets its own error showing where the title goes.
- **Why:** `, title "…"` is how people write it by analogy with the comma-separated option list, and the old error ("expected 'vs'") pointed at the wrong thing.
- **Alternatives:** only a clearer error (keeps an arbitrary rule people keep tripping on).

## D120. Uncertainties: `5.0 ± 0.2 m` is a value with a sparse gradient over independent error sources (M4)
- **Syntax:** `a ± b` (ASCII `a +- b`) binds tighter than `+`/`-` and looser than `*`/`/` (`2 x ± 0.1` is `(2 x) ± 0.1`; `1 + 2.0 ± 0.1` is `1 + (2.0 ± 0.1)`, the same number either way). A unit written after the uncertainty belongs to both numbers when the value is a bare number: `5.0 ± 0.2 m`; `(5.0 ± 0.2) m` also works. `x ± 3%` is relative (σ = 3 % of |x|). `20.0 ± 0.5 °C` treats the 0.5 as a temperature difference (0.5 K, not 273.65 K). A list takes one σ or a list of σs (`[0.90 s, 1.27 s] ± 0.02 s`): each element is its own measurement. `(a ± b) ± c` adds a second, independent uncertainty (statistical ± systematic); `a ± b ± c` without brackets is an error, since it usually is a typo.
- **Representation:** `fermium/uncertain.py::UFloat` = nominal value + `{source id: ∂f/∂xᵢ·σᵢ}`; σ² = Σ entries². Each evaluation of a `±` (each list element), and each set of fitted parameters, makes new sources: a `±` inside a loop is a new measurement on every pass. Operations are first-order (linear) with exact correlations, the model of Python's `uncertainties` package: `x - x` is `0 ± 0`, `x/x` is `1 ± 0`, `x x` has σ = 2|x|σ while `x y` (independent, same size) has √2|x|σ. Math functions use their analytic derivatives (`sin`, `exp`, `ln`, `asin`, `erf`, `gamma` via digamma, …), `atan2`/`hypot` analytic partials, Bessel/elliptic functions central differences. User functions, `f'` and `Σ` need nothing special: they are arithmetic.
- **Units:** the checker treats `a ± b` as a number of a's dimension and unifies b's dimension with it (`the uncertainty after ± is time [s] but the value is length [m]`), so everything downstream is checked exactly as for plain numbers; the IR node is `IBuiltin("pm"|"pm_rel")`.
- **Comparisons** (`<`, `==`, `min`, `if`) use the nominal value.
- **Alternatives:** a `{value, σ}` pair propagated in quadrature (the D19 sketch: loses correlations, so `x - x` would be `0 ± 0.28`; wrong for any formula that uses a measurement twice); compile-time gradients over a fixed set of inputs (fast, but loops and lists create sources at run time); second-order propagation (rarely what a lab course teaches; `propagate montecarlo` covers nonlinear cases honestly).

## D121. Printing and reading uncertain values
- **Rule:** σ is rounded to **2 significant figures**, always, and the value to the same decimal place: `5.00 ± 0.20 m`, `9.806 ± 0.017`, `47 ± 12 J`. When the common exponent is ≤ −3 or ≥ 5, or σ ≥ 100 in the display unit, both share one power of ten: `(1.234 ± 0.056)×10⁻³ m`, `(1.235 ± 0.012)×10⁵`. `x in unit` converts both parts; `%` and `°` go after the bracket: `(30.0 ± 0.5)°`. σ = 0 prints `0 ± 0 m`. Lists print element by element in one unit. The rule doesn't depend on the default precision of plain numbers, and `to N digits` doesn't change it.
- **Why 2 figures:** the simplest rule that doesn't throw information away (a 1-figure σ of 0.15 → 0.2 is a 33 % change), and what most lab manuals ask for. *Alternative:* the PDG rule (1 or 2 figures depending on the leading digits 100–354/355–949); easy to switch (`uncertain.SIG`, `_round_sig`) if a course wants it.
- **Parts:** `value(x)`, `uncertainty(x)` and `rel(x)` (σ/|value|) give plain numbers (element-wise for lists). `err(x)` still means the standard error of a fitted parameter, and in a program that uses uncertainties it also works on any uncertain value (the same as `uncertainty(x)`), so there is no clash with D18's `err(g)`.

## D122. Where uncertain values run: the reference interpreter (honest limitation)
- **What:** a program whose check sees `±`, `value/uncertainty/rel` or `propagate montecarlo` (`CheckedModule.uses_unc`) is not compiled to LLVM: `Program.run` runs the same typed IR in `fermium/interp.py`, where `UFloat`'s Python operators do the propagation. `fermium run` and `fermium run --interp` therefore print the same thing by construction. Programs without uncertainties are untouched (native code, `fermium build`).
- **Costs, stated:** such a program runs at interpreter speed (fine for a lab report; slow for a big simulation). `fermium build` refuses it with a clear message, and so does the REPL/Jupyter kernel (its variables live in a native arena of doubles). Anything that needs a plain number — vectors and matrices, integrals, `solve` ODEs and `solve … for x`, eigen/PDE solves, `std` of a list — stops with `… can't use uncertain values (±) yet; put it inside a propagate montecarlo block, or use value(x)`. Nothing silently drops an uncertainty: `UFloat.__float__` raises.
- **Alternatives:** an LLVM struct `{double, double}` (no correlations) or `{double, sparse map}` (a run-time hash map in generated code; a lot of code for both back ends and the AOT C runtime). The interpreter already exists, mirrors the JIT's printing and kernels, and keeps one implementation of the propagation rules.

## D123. `propagate montecarlo [N samples]`
- **What:** a block of formulas (`name = …`, plus `if`/loops/`solve`, but no `print`/`plot`/`fit`) re-run with every uncertain input replaced by a sample: each error source gets N standard normals from the seeded generator (D80, so `seed(n)` makes it reproducible and an unseeded program is reproducible too), and an input's sample is value + Σ contributionᵢ·zᵢ, so correlated inputs are sampled consistently. A `±` inside the block is a new source. Default N: 100 000.
- **Two speeds:** first the whole block runs once on NumPy arrays of N samples through the ordinary interpreter code; if it can't (an `if` on a sample, an integral, an ODE, a vector), it runs once per sample (default N then 10 000) with the same z values, so both paths give the same numbers (tested).
- **Results:** each assigned number y is regressed on the source samples jointly (least squares, y ≈ ȳ + Σ βᵢ zᵢ). Its contributions are the βᵢ (the ± inside the block become sources shared by all outputs) plus one new source for the residual variance (the nonlinear part); its value is the intercept, i.e. the sample mean corrected with the zᵢ as control variates (their true mean is 0). So σ² = Σβᵢ² + residual variance, which equals the sample variance up to O(1/√N) sampling noise; a formula that is exactly linear in the inputs gets exact coefficients and value, so identities survive (`c = b + a` inside the block gives `c - b - a` = 0 ± 0 afterwards, tested) and later formulas keep the correlations with the inputs. Non-finite samples (√ of a negative sample) are an error that says how many.
- **Validated:** against linear propagation for g = 4π²L/T² (agrees within the statistical bounds and the second-order bias) and against the exact lognormal mean and σ for exp(3x), x = 1.0 ± 0.5, where linear propagation is off by a factor of ~3 in the mean and ~6 in σ (`tests/test_uncertainty.py`).
- **Alternatives:** an expression form `montecarlo(expr, N)` (a block reads better for several related results and matches the plan's wording; `sample(expr, N)` from D80 already exists for hand-rolled Monte Carlo); returning results as new independent sources (loses the link to the inputs, so later combinations with the inputs would get wrong uncertainties); separate one-variable regressions per source (in-sample correlations between the zᵢ break exact identities); plain sample mean ± sample std with no link (simplest, but no correlations).

## D124. Fits and plots with uncertainties
- **Fit:** `fitting.least_squares_fit` now also hands back the covariance (JᵀJ)⁻¹·rss/dof. In a program that uses uncertainties, the fitted parameters become uncertain values with that full covariance (cov = VΛVᵀ; each eigenvector is one new source), so `A + B` or `τ ln 2` later get the right σ, including the parameter correlations (tested against `scipy.optimize.curve_fit`'s `pcov`). If a standard error couldn't be estimated the parameter stays a plain number. In a program without uncertainties nothing changes (plain parameters, native code, `fermium build`), which keeps every existing fit program and its output as it was. *Alternative:* always uncertain — correct physics, but it would move every fit program into the interpreter and break `fermium build` for them; the opt-in by using `±`/`uncertainty()` anywhere was chosen as the compromise and is documented.
- **Not done:** weighted fits from uncertain data (`fit` with σᵢ of the measurements) — `fit` still reads the CSV columns and treats all points equally.
- **Plot:** a list of uncertain values plots as points with error bars (x and/or y); a function that returns uncertain values plots as a line with a ±1σ band. Only `fermium run` plots (matplotlib); the SVG writer of `fermium build` isn't involved because such programs can't be built.

## D130. `2 c` follows the unit-after-number rule once you define your own c (red team round 2 #1) — *superseded by D235 (`2 c` alone is now an error too)*
- **What:** the D7 rule (a single bare unit right after a number, when you also have a variable of that name, warns alone and is an error next to other factors) now applies to `c` like every other unit. `c = 340 m/s; d = 2 c * t` is the error "'2 c' is ambiguous … write 2*c or 2 [c]", and `print 2 c` warns.
- **Why:** the parser had exempted the name `c` since the first collision warnings, most likely so that the built-in constant wouldn't trigger them. But `known` holds only names *you* assigned, so the exemption only ever applied when you had your own c, which is exactly when `2 c` silently meant 2 × the speed of light (1.2×10⁹ m instead of 1360 m).
- **Kept:** `3 c`, `0.5 c` and `v = 0.9 c` are still the speed of light, with no warning, when you haven't defined c.
- **Alternative:** always reading `2 c` as 2 × your c (rejected: D7 treats every unit name the same way, and `c` as a unit is also a legitimate choice after you have defined c).

## D131. PDEs: step-doubling accuracy control and an L-stable start-up for Crank–Nicolson (red team round 2 #2, #3)
- **Accuracy (#2):** Crank–Nicolson is stable for any step but not accurate for any step (with r = Dk²Δt/2 > 1 a decaying mode alternates in sign). `pde_solve` now checks by step doubling: the solutions with n and 2n steps are compared at 8 common times, and the difference (÷ 3 for CN, ÷ 1 for backward Euler, Richardson) estimates the error relative to the solution's largest value (initial, boundary and all snapshots). With the default step (1000 steps) the step is halved until the estimate is under 10⁻³ (the RK4 threshold, D17/round 1 #5), up to 32 000 steps, then warns; the answer returned is the finest run. With a step of your own, Fermium keeps your step and warns when the estimate for it is over 10⁻³ ("the time step is too coarse for this PDE …"), like fixed-step RK4. Costs ~1.5× (default) or 3× (your step) the old single run. Explicit (stability-limited) and the wave equation's leapfrog (Courant-limited) are not checked.
- **Start-up (#3):** a jump between the initial value and a boundary condition excites grid-scale modes whose CN factor is ≈ −1, so they never decay (a ±0.0025 K sawtooth at the walls, a Neumann slope of −0.75 instead of −1). Rannacher smoothing damps them with a few L-stable start-up steps. We use the two-stage, stiffly accurate SDIRK method (γ = 1 − 1/√2) for CN's first 4 steps instead of backward Euler: SDIRK2 is L-stable (damps the stiffest mode by ~0.4/(γ²λΔt) per step, 10⁻¹¹ after 4 steps at the defaults) *and* second order, whereas backward-Euler start-up steps at the full step cost ~(λΔt)²/4 relative error on smooth data, which test_m3_pde's CN test at `step 0.5 s` catches (7×10⁻⁴ against its 3×10⁻⁴ tolerance). Complex (Schrödinger) equations keep pure CN, which is unitary; an L-stable start-up would lose ∫|ψ|² conservation.
- **Validated:** the repro tests against the Fourier series (5×10⁻¹¹ K at the wall, 3.4×10⁻⁹ K in the middle) and the steady Neumann line (slope −1 to 10⁻⁵); all of test_m3_pde.py unchanged.
- **Alternatives:** a fully adaptive embedded method (more code for 1-D linear problems whose single LU is the main saving); refining a step you wrote (rejected, as for RK4: your step is a request, and it also sets the snapshots).

## D132. `analyze` in natural units works in SI dimensions (red team round 2 #8)
- **What:** inside `units natural(…)` (any system that sets constants to 1), `analyze` builds its dimension matrix from SI dimensions: bracketed units are read as SI units (`Checker.resolve_unit_si`, which is `resolve_unit` without the map into the system), constants by their SI dimensions, and SI variables by their types. A line under the header says so: "(in SI dimensions: under units natural (ħ = c = 1) those constants are pure numbers, so length, mass and time would collapse into powers of energy and hide the groups)". A variable computed inside the region has no SI dimension (D60), so it asks for a bracketed unit. The function `analyze` defines is an ordinary function of the region (checked there, which is exact because the map into natural units is a homomorphism).
- **Why:** before, the rank was computed in the natural dimension space (1 for the pendulum) while the text named SI dimensions ("1 independent dimension (among length, mass, time)"), and the defined `pendulum(L) = L` was useless. With ħ = c = 1 dimensional analysis loses exactly the information it needs.
- **Alternatives:** refusing `analyze` in natural regions (the SI analysis is well defined and useful there); analysing modulo the constants and listing them as implicit inputs (correct but rarely what a physicist wants from a pendulum).

## D133. `std` of one value is an error (red team round 2 #4)
- **What:** `std` (the sample standard deviation, N − 1) of a list with one value stops with "std needs at least 2 values: it is the sample standard deviation, which divides by N − 1, so one value says nothing about the spread (quote the instrument's uncertainty for a single measurement)". Error kind 24, in the JIT (`reduce`), the interpreter and the C runtime of `fermium build`; `stats.standard_error` inherits it.
- **Why:** it used to divide by 1 instead of 0 and return 0, which reports one measurement with zero uncertainty.
- **Alternative:** NaN (what NumPy's `std(ddof=1)` gives). An error that says why is clearer for a beginner, and a NaN would propagate silently into a lab report.
- Also (round 2 #14): `sample(expr, N)` needs N to be a whole number ≥ 0 (error 25, which shows N) and `randn(μ, σ)` needs σ ≥ 0 (error 26). Before, 2.5 samples were 2, −1 samples were none, and a negative σ was used as |σ|.

## D134. `em.cyclotron_frequency` is a turning rate shown in rev/s (red team round 2 #5)
- **What:** `cyclotron_frequency(q, B, m) = abs(q) B / mass in rev/s`. Its value is ω = |q|B/m (since 1 rev = 2π, D27), so `in rev/s` and `in rpm` give the turns per second |q|B/(2πm) and `print f` shows `… rev/s`. `in Hz` shows ω, with the D95 rev ↔ Hz warning. The module header and docs/stdlib.md say that turning rates ending in `_frequency` are shown in rev/s and that a frequency you pass in (skin_depth's f) is written in Hz as the number of cycles per second.
- **Why:** with angles as plain numbers, Hz is rad/s, so no value can be right both `in Hz` (cycles) and `in rev/s`. The old `… / (2π mass) in Hz` was right only in Hz, and wrong by 2π in rev/s and rpm, with a warning whose advice ("write it in rev/s instead of Hz") asked for a change inside the stdlib. rev/s is the unit whose conversions are right under D27. test_stdlib's check of this function now reads it in rev/s (same value, same tolerance): under D27 the old Hz check tested the inconsistent convention.
- **Alternatives:** a "cycles" dimension or hint-dependent conversions (rejected in D95: the hint is a light tag, and values must not depend on it); documenting the old behaviour only (leaves a silent 2π in `in rpm`).

## D140. Python from Fermium: `use python numpy as np`, units checked at the boundary (M6)
- **Syntax:** `use python <module> [as <name>]` at the top level, optionally followed by `:` and signatures, one per indented line (or `;`-separated on the same line): `energy(m [kg], v [km/s]) -> [J]`, `grid(a [m], b [m], n: int) -> list [m]`, `sum(xs) -> number`. A dotted module needs `as` (`use python scipy.special as sp`). Calls are the existing `Field` syntax (`np.sinc(x)`); the parser now also accepts a keyword after the dot when a `(` follows (`np.sqrt(x)`), and keeps the raw spelling (`sp.gamma`, not `sp.γ`; `fmt --pretty` no longer prettifies a name after `.`). *Alternatives:* reusing `import` (`import python numpy`): `use` was reserved for this in D100, and a separate word makes it obvious that the names are Python's, not Fermium modules'; a signature syntax borrowed from Python type hints (`def f(x: float) -> float`): Fermium already writes units as `x [m]`, so the signature reads like a Fermium function header.
- **The unit contract:** Python functions take and return plain numbers. Without a signature every argument must be dimensionless; a dimensionful one is a *compile-time* unit error with the fix in the hint (*divide by a unit, like r / (1 m), or declare the unit in the use line: jv(x1, x2 [m])*). With a signature, each argument must have the declared unit's dimension and is passed as a number in that unit (SI value / factor), and the result is multiplied by the result unit's factor, so `energy(m [kg], v [km/s]) -> [J]` gets 3 for 3000 m/s and a joule result. Units with an offset (°C) are refused in signatures. The result's display hint is the declared unit. Everything else about units is unchanged: the arguments are ordinary checked expressions, and generic functions that call Python are checked per call (D43), so `f(x) = np.sin(x)` called with `2 m` fails at that call.
- **Shapes:** a Fermium list is passed as a float64 NumPy array; the result is a list when any argument is a list, else a number, unless the signature says `-> list` / `-> number`. Parameters marked `n: int` are passed as Python ints (and must be whole); everything else as floats. (Passing whole floats as ints automatically was rejected: `np.power(2, 100)` with ints overflows silently.) Vectors, matrices, functions and text can't be passed (clear error).
- **Checked early:** the module is imported and each called attribute looked up when the program is checked, with the program's folder on `sys.path` (so `mylib.py` next to the program works): a missing module (with a `pip install` hint), a misspelt function (with a close match) or a number attribute called like a function are compile errors. A number attribute (`np.pi`) is read at check time as a dimensionless constant.
- **Run time:** each call site gets an entry in `tables.pycalls` (module, function, per-argument unit factor and int flag, result factor and shape) and an `IPyCall` IR node. The JIT calls the ctypes callback `fm_pycall(id, double** ptrs, i64* lens, double* out)` (a number argument has length −1); a list result returns its length, and the compiled code allocates the list and calls `fm_pyfetch` to fill it. The interpreter calls the same `runtime/pycall.call()`, so conversions and messages are identical (tests compare both back ends). A Python exception, None, text, a complex number, a list where a number was expected (or the reverse) or a 2-D array stops the program with a one-line message and the line. NumPy floating-point warnings are silenced (Fermium's arithmetic is IEEE: NaN/∞, no warnings).
- **Not supported:** symbolic derivatives of a Python call (an error suggesting a finite difference), passing a Python function to a Fermium function, calls inside `units natural` regions, `use python` inside a Fermium module, and `fermium build` (an executable has no Python; refused with a clear error through the existing `python_only` table, like eigenvalue problems and PDEs).
- **Trust:** `use python` executes the module's import-time code when the program is *checked*, so also in `fermium check` and the language server, exactly like `import` in a Python script. Fermium programs were already trusted code (they write files, run for as long as they like); this is stated in the reference.
- **Cost:** about 10 µs per call from compiled code on the test machine (ctypes transition, argument conversion); a formula written in Fermium is compiled and much faster.

## D141. An uncalled function that takes a list is checked with lists
- **Problem (found while writing the Python API):** `total(ys) = sum(ys)` that the program never calls was checked with a number argument (to report errors in uncalled functions), so it failed with *sum needs a list, but got a plain number*. A library of functions called from Python is mostly uncalled functions.
- **Fix:** when that check fails with "needs a list", the function is checked again with list arguments; if that fails too (some parameters are numbers, others lists), it is left to be checked at each call, as functions taking functions already are (D43).
- **Alternatives:** inferring which parameters are lists from the body (a shape inference pass the checker doesn't have).

## D142. Fermium from Python: `fermium.compile(src)`, `fermium.load(path)`, `mod.f(…)`, `mod["x"]` (M6)
- **API:** `mod = fermium.compile(source)` / `fermium.load("file.fm")` checks and compiles a program (FermiumError on errors). `mod.f(…)` calls the function `f`; `mod["x"]` / `mod.x` reads a top-level variable; `mod.functions`, `mod.variables`, `mod.run()`. The top level runs once, on the first call or read (like importing a Python module), or explicitly with `run()`. `fermium/__init__.py` imports the API lazily, so `import fermium` and the CLI stay light.
- **Units at the boundary:** an argument is a plain number **in SI units**, with the dimension its parameter declares (`period(L [m])` takes 1.0 as 1 m) or dimensionless if it declares none; `fermium.Q(50, "cm")` (a `Quantity`) passes any unit; lists, tuples and NumPy arrays (of numbers or of Quantities with one unit, or a `QuantityArray`) pass a Fermium list. Results are `Quantity` (a `float` subclass holding the SI value, with `.unit`, `.value`, `.to()`) or `QuantityArray` (an ndarray subclass for lists, vectors and matrices). Python arithmetic on them returns plain floats / arrays (`__array_wrap__` drops the unit), so a unit is never carried through a Python computation it doesn't describe. *Alternatives:* plain floats in and out (loses the unit on the way back, and "which unit was that in?" is the bug Fermium exists to prevent); a full Python unit library (pint-like): out of scope, and Python code would still not be checked; interpreting a plain float in the parameter's declared unit (`L [cm]` taking 2.0 as 2 cm): one rule (SI) for every parameter is easier to remember, and `Q(2, "cm")` says it explicitly.
- **Generic functions are instantiated on demand** from the Python arguments: the (function, shapes, dimensions) key picks a cached entry point; a new key compiles one line, `fmpy_<n>_f_r = f(fmpy_<n>_f_a0, …)`, whose arguments are hidden arena variables of exactly those types. So the Fermium compiler checks the units of each call, and a unit mistake is a FermiumError naming the call (*calling period from Python with (time [s]): period expects L in m …*). *Alternative:* requiring declared parameter units: rejects most of the language's generic functions.
- **Mechanism:** the program is compiled like a REPL session (ReplSession: one LLVM module per input, top-level variables in an arena of 8-byte slots that Python reads and writes directly; lists are `{data, len, cap}` headers malloc'd with the C library, as compiled code makes them), but with the REPL's conveniences off (no echo of bare expressions, no silent redefinition with new units: `Checker.arena` is separate from `Checker.repl`). The argument lists Python allocates are freed after each call (the result has been copied out); list results stay owned by the program. Calls run on one long-lived thread with a 512 MB stack, so the runaway-recursion check works as under `fermium run` and a call costs ~0.1 ms instead of a thread start. `ReplSession.execute` was split into `compile_input` + `run_entry` for this.
- **Limits:** arguments must be numbers or lists (no vectors, functions or text); a function that takes a function argument (D43) can't be called from Python; the first call with a new argument signature compiles for tens of milliseconds; programs are not cached between Python processes.

## D150. Integer loop counters (M5)
- **What:** a `for` loop whose start and step are whole numbers known to the compiler (integer constants, the variable of another such loop, and sums and differences of those) and whose body never sets the loop variable keeps its value as a 64-bit integer k = start + n·step. The variable itself is the double of k. An index built the same way (`x[i]`, `x[j]`, `x[i + 1]`) is checked with two integer comparisons instead of a float-to-integer conversion, a round-trip test for "is it whole?" and float comparisons.
- **Same numbers:** start + n·step in doubles is exact below 2⁵³, so the double of k is the value the old code computed; a loop would need 2⁵³ iterations (months) to get there. The interpreter is unchanged.
- **Measured** (interleaved A/B, median of 7 runs, machine loaded by other jobs, `FERMIUM_DISABLE=int_loops` for "off"): nbody inner time 216 ms → 74 ms (2.9×; Julia 77 ms in the same session); the all-pairs `forces` loop 53 ms → 21 ms. LLVM can now see that `j` runs from i+1 to 5 and drops most checks; before, the fptosi/sitofp pairs blocked that.
- **Alternatives:** typing loop variables as integers in the checker (a new type everywhere, and `i/2` would change meaning); removing bounds checks (unit safety first: an index out of range must stay an error).

## D151. M5 performance work that was measured and not kept, and benchmark fairness fixes
- **Kept:** host CPU target (it was already `get_host_cpu_name()` + features, so AVX2/AVX-512 and FMA instructions are available); O2 pipeline.
- **Not kept (no measurable gain, more compile time):** O3 instead of O2 (nbody 72 → 70 ms, spring_rk4 39 → 42 ms, unit_loop the same: noise); a copy of the GK15 panel routine per integrand so the integrand is called directly and inlined (blackbody 2.99 → 2.99 ms median; the integrand is dominated by exp and three divisions, not by the call; +100 ms compile time).
- **Not done, on purpose:** `fast-math` flags. `reassoc` reorders sums (differential tests compare the JIT with the interpreter digit for digit, and a user's sum must not change with the optimiser); `contract` (FMA) changes the last bits of `E += ½ m v²` relative to the interpreter, which has no FMA (Python 3.11 has no `math.fma`). Julia doesn't contract or reassociate by default either, so the comparison stays like for like.
- **Where the remaining time goes:** spring_rk4 (Fermium ~1.8× Julia) stores the whole dense solution (t, x, x', and both derivatives: 40 MB for 10⁶ steps) so that `x(t)` works afterwards; first-touch page faults for 40 MB cost ~17 ms on this VM, measured with a C program (plain malloc + touch: 17–38 ms; transparent huge pages via madvise: 3–15 ms but with 170–345 ms stalls from memory compaction, so not used). The arithmetic loop itself is the same as Julia's. blackbody (Fermium ~1.2–1.4× Julia at equal work): the smoothstep substitution, NaN/∞ tracking per node (D45) and ∫|f| (D44) cost ~4 ns per node over QuadGK.
- **Benchmark fix:** blackbody now uses rtol = 1e-10 in Julia, pure Python and SciPy, the tolerance every Fermium integral uses (Fermium has no per-integral tolerance). Before, the others used 1e-8 and did ~25% fewer integrand evaluations (Julia: 157 vs 211 per integral; Fermium 205), which was not like for like.

## D152. `parallel for`
- **Syntax:** `parallel for i from a to b [step s]` + an indented body. Ranges only (`parallel for x in xs` is an error that suggests `parallel for i from 1 to len(xs)`).
- **What an iteration may do (checked at compile time):** set variables that are new in the body (each thread has its own copy, and they have no value after the loop; an inner loop's variable is always new, and the variable of that name from before the loop is visible again afterwards); write `xs[i]` (index = the loop variable itself) of lists made before the loop, and read `xs[i]` of such lists; read anything else; add to a variable made before the loop with `s += …` / `s -= …` (a reduction; it can't be read in the loop). Errors, with the reason in physics-free words: any other write to a shared variable, `xs[i + 1] = …`, reading a written list at another index or as a whole, `print`/`plot`/`solve`/`fit`/`push`/`break`/`return`, random numbers (the order would change the draws), a nested `parallel for`, calling a function that prints or calls itself (directly or not). Two names for the same list (`ys = xs`) when one is written: a run-time check before the loop starts, in both back ends (error kind 40).
- **Deterministic sums:** the range is cut into nb = min(n, 256) blocks, block k = [kq + min(k, r), (k+1)q + min(k+1, r)) with q, r = divmod(n, nb) (`ir.par_blocks`). Each block starts its sums at 0 and adds its terms in order; the block sums are added in block order at the end. The blocks don't depend on the number of threads, so the result is the same on every run and machine, with any `FERMIUM_THREADS`, in `fermium build`, and in the interpreter (which runs the blocks one after another). It differs from the plain `for` loop's sum only by rounding (~10⁻¹⁵ relative).
- **JIT and `fermium build`:** the same LLVM code, with pthreads (no OpenMP in llvmlite; MCJIT has no thread-local storage: "allocation of TLS not implemented"). The body is emitted as a worker function `par.N(ctx)`; ctx holds lo, step, n, nb, a block counter, the partial-sum array and the addresses of the enclosing function's variables (each thread copies their values once: they can't change during the loop). `fm_par_run` starts min(threads, nb) − 1 threads (64 MB stacks), works in the calling thread too, joins, and the calling thread adds up the partials. Threads take blocks with an atomic counter (load balancing without changing the sums). Threads: `FERMIUM_THREADS`, else the number of online processors (`sysconf`), at most 256.
- **Errors in a worker:** `fm_error` stores the message; `fm_raise` (which replaces the direct `longjmp` everywhere) looks up the current thread in the worker table and jumps to that thread's own jump buffer; the worker stops, and after the join the program stops with the message. If several iterations fail, which message wins can change between runs (documented). The recursion check measures from the main thread's stack, so it is switched off during the loop (`fm.stackbase = 0`), which is why recursive functions are rejected in the body. Known benign race: `fm.line`, `fm.errfmt` and `fm.qvar` (the line and units shown in a kernel's error message) are shared, so an error from inside an integral in a parallel loop can name another line of the same body. `fm.qabs` (D110) was a shared global; it is now a variable of each `fm_quad` call.
- **A trap found on the way:** `fm_raise` was first marked `cold`. LLVM then puts it in a section of its own (`.text.unlikely`), and after such a module was freed, a later `backtrace()` through JIT frames (NumPy's temporary-elision check calls one) crashed in libgcc's `_Unwind_Find_FDE`: the test suite segfaulted at random places, 6 runs out of 6, while the base commit passed. Keeping the programs alive made it go away, and so did dropping `cold`. No JIT function may be `cold`.
- **Cost:** starting threads is ~0.1 ms, so tiny loops are faster as plain `for`. On this 4-core VM, loaded by other jobs, a plain C pthreads test gets only ~1.3× from 4 threads, and so do Fermium and Julia; the speed-up claim is left to a quiet machine.
- **Alternatives:** OpenMP (not reachable from llvmlite); a Python thread pool calling the worker through ctypes (works, but `fermium build` would need a second implementation); dynamic chunk sizes (faster on uneven loops but the sums would depend on timing); allowing any `s = s + …` shape or `max` reductions (later).
- **Known limit (merge note):** the zero-integral counter for vector integrals (`fm.qzero`, red team round 3 #5) is a global, so vector integrals on several threads of one `parallel for` can race on it. The worst case is a missed or extra zero-integral *warning*; values are unaffected.

## D160. Absolute tolerances for ODE solves: `absolute a[, b …]` (research friction #82)
- **What:** after the range, `absolute 1e-16` (in any order with `tolerance`, `using` and `until`, D111) gives the adaptive solvers an absolute tolerance. The values are constants with units, one per unit (`absolute 1e-16, 1e-16 MeV` for abundances and a temperature); each unknown takes the value in its units. A derivative slot of a higher-order unknown (x' of `x''`) takes a value in its own units if given, else x's value divided by |t1 − t0|. No value for an unknown's units, two values in one unit, a value nobody uses, a non-constant or non-positive value, `step`, an algebraic solve, an eigenvalue problem or a PDE: errors. radau/bdf: SciPy's atol is at least the user's (initially and after every step, over D42's |Δy| floor). RK45 (LLVM kernel, interpreter, and so `fermium build`): the error scale becomes rtol·(max(|y|, |y_new|) + |Δy|) + atol_j. Without `absolute` the array is zeros (RK45) or absent (stiff), so every step is bit-for-bit what it was; a negligible `absolute 1e-300 m` prints the same (tested).
- **"The step became too small"** now has two messages. `runtime/stiff.step_small_kind` (mirrored in the LLVM kernel) checks the last good state: if some component isn't finite or has grown to over 10³ × max(its start, the largest start), it is kind 8, "the solution may blow up there" (unchanged: `1/(1 − t)`, `tan t` from 0); otherwise kind 34, "no unknown has grown there, so this is probably not a blow-up: the error control asks for relative accuracy on values that are tiny or rounding noise …; add an absolute tolerance after the range, e.g.  tolerance 1e-9 absolute 1e-16". The largest start lets an unknown that starts at 0 count as blowing up once it outgrows the others' starts, while a species growing from 0 to 10⁻²³ does not. With mixed units in SI (a mass of 10³⁰ kg next to a position) a real blow-up could get the second message; it says "probably".
- **Validation:** the research/bbn_network network started at 10 MeV (`tolerance 1e-10 absolute 1e-16, 1e-16 MeV`, radau) takes ~3 900 steps and matches SciPy's Radau (rtol 1e-10, atol 1e-16) to 10⁻⁵ in Y_p, D/H, ³He/H, ⁷Li/H and T (tests/test_research.py); without `absolute` it stops at t = 0.00738 s with the new message. The published two-stage program is unchanged.
- **Why:** reaction networks, radiative transfer and anything with components that settle to rounding noise need an absolute floor; D17 removed absolute floors because a fixed one breaks scale-freedom (10⁻¹⁵ m vs 10³⁰ kg). Making it opt-in, with units, keeps the default scale-free and makes the floor mean something physical.
- **Alternatives:** one plain-number atol applied in SI units to every component (meaningless across units, and silently wrong for an unknown in J); a relative floor per component (`floor 1e-12` of the largest value): hides the choice; automatic detection of "rounding-noise" components: unpredictable.

## D161. Plot axis options: ranges, labels, reversed axes (research friction #83)
- **What:** `with y from a to b` / `x from a to b` (constants in the axis's units, checked against the first series; smaller first; > 0 for a log axis), `xlabel "…"` / `ylabel "…"`, and `reversed x` / `reversed y`, in the existing option grammar (after `with`, comma-separated, or after the last series without `with`; `x from`/`y from` count as options even with a variable named x or y, since no series starts that way). The range is stored in SI and shown in the first series' display unit. A given label replaces the name; the display unit is still appended in brackets (the existing convention) unless the label contains `[` (it names its own unit, like `"T [MeV]"`). A range turns off the equal-aspect rule for orbit plots. matplotlib: `set_xlim`/`set_ylim`, `invert_xaxis`; `fermium build`'s SVG writer (aot_data.c) takes the range as the axis limits (no margins) and draws a reversed axis from the right. PDE plots refuse them (only `title` and `animate`).
- **Why:** the BBN figure needs a floor on a log axis (⁷Be starts at 10⁻⁶⁸), labels that aren't list names, and temperature decreasing to the right.
- **Alternatives:** `ylim 1e-12 1` (terse, not the `from … to` of every other range in Fermium); `with x decreasing` (less standard than "reversed").

## D162. `max`/`min` with a list and numbers work element by element (research friction #84)
- **What:** `max(xs, 1e-12)`, `min(ys, 1.5 m)`, `max(0 m, ys, zs)`: when any argument of a 2+-argument max/min is a list, the result is a list, element k being the max/min over the numbers and the lists' k-th elements (same NaN rule as scalar max/min: minnum/maxnum ignore a NaN). All arguments need the same units; lists must have the same length (the usual "different lengths" run-time error). Builtins `max_ew`/`min_ew` in codegen (a map loop), the interpreter, and so `fermium build`.
- **Why:** flooring abundances for a log plot, clamping; it's how NumPy's `maximum` and Julia's `max.` read, and the one-list form `max(xs)` (a reduction) stays unambiguous because it has one argument.
- **Alternatives:** a separate `maximum(xs, y)` (another name to learn); `clamp` only.

## D163. A whole unit used as a value is an error that suggests `1 unit` (research friction #85)
- **What:** when an undefined name is a unit, the checker looks at the expressions being checked around it (a stack kept by `Checker.expr`) for the largest one made only of units that aren't program names, `*`, `/` and number powers. If that is more than the name (`cm³/(mol s)`, `km/s`, `N/m`), the error is `cm³/(mol s) is a unit, not a value`, hint `for the quantity write  1 cm³/(mol s)  (a number, then its unit)`. A lone `m` keeps its old hint; `x km` keeps the `x * 1 km` hint.
- **Why:** the old message named only the first unit ("cm isn't defined"). Accepting a bare unit as the quantity 1 was the alternative; it was rejected (conservative, as the task allowed) because `v = km/s` would then silently mean 1 km/s, and the collision rules (D7) show how easily a unit name and a variable name get mixed up: an explicit `1` keeps "units follow numbers" the only rule.

## D164. A unit read in place of your variable is the error message, not a note (research friction #86) — *superseded by D235*
- **What:** when a line with a D7 collision (`4/3 T` with T a variable, read as 3 tesla) fails a unit check, the message becomes `T here is read as the unit tesla (T), not your variable T: '3 T' is a unit right after a number; write 4/3 * T`, and the hint is `the units then don't match: <the old mismatch message>` (plus "this happened when calling …" if it was there). The fix is built from the program: `a/3 T` → `a/3 * T`, `π²/15 T⁴` → `π²/15 * T⁴`, else `3 * T`. The unit's word comes from the spelled-unit table (tesla, liter, gram); otherwise its dimension. Errors on a later line keep the D34/#43 note naming the earlier line.
- **Why:** the old message was a mismatch in powers of kg, A and s, and the cause (the only useful part) was in a note after it. The reading itself (spec §3.4.2, D7) is unchanged; two tests that checked the old wording (tests/test_friction_parser.py) now check the new one.
- **Alternatives:** changing the reading (make `3 T` your variable when T is defined): rejected in D7 (meaning would depend on distant code).

## D170. The D7 collision rule covers a compound unit that starts with your variable (gauntlet #66) — *superseded by D235*
- **What:** right after a number, a compound unit whose **first** factor is also one of your variables, and which continues with a space or `*` (`2 m c²`, `2 m * c²`, `2 N m` with your own `m` or `N`), is the D7 rule 5 error: `'2 m c²' is ambiguous: right after a number, m c² is a unit (m is metres there), but m is also your variable m`, hint `write 2*m c² … or 2 [m c²] for the unit`. Before, `m = 2 kg; print 1 J / (2 m c²)` silently divided by 2 metre·c² (`c` is also a unit), while `2 m v` was already an error.
- **Kept (D7 unchanged):** a compound continued with `/` (`9.81 m/s²`, `50 N/m` with your own `m` or `N`) or only a power (`3 m²`) stays the unit. D7 rule 4 already says a tight `/` followed by a unit continues the unit even when you have a variable of that name, and D7 lists `9.81 m/s²` as never ambiguous; `g = 9.81 m/s²` next to a mass `m` is in almost every mechanics program, and nobody means 9.81 kg/s². A first factor that isn't your variable (`2 kg m`) is never ambiguous.
- **Why the space/`*` join is the line:** `2 m c²` and `2 m v` look the same on paper and are written for the same reason (2 × your m × something); only the fact that `c` happens to be a unit made one silent. The error costs one keystroke.
- **Alternatives:** an error for every compound (breaks `9.81 m/s²` next to a mass); a warning only (the reading is almost never the intended one, and the answer is silently wrong).

## D171. `/unit` and `/(units)` right after a number: the unit, or an error when it names your variable (gauntlet #68, #72) — *superseded by D235*
- **What:**
  - `8 /m³` (a space before `/`, none after) right after a number is the unit 1/m³, as `0 /s` already was, and now also `0.300 /(m s²)`: `/` then brackets holding only unit names and exponents is a unit denominator (`0.300/(m s^2)` too). `2 /(1 + z)` and `600 /(k T)` (not all units) stay divisions.
  - **When a name in it is your variable** (`m = 2 kg; n = 8 /m³`, or `0.300 /(m s²)` with your m), it's an error: `'8 /m³' is ambiguous: right after a number, /m³ is the unit 1/m³, but m is also your variable m`, hint `write 8 [1/m³] for the unit, or 8/m³ (no spaces) to divide by your variable m`.
  - `8/m³` with no spaces and `8 / m³` with spaces on both sides divide by your m, as before.
- **Why an error and not a guess:** the two D7 rules disagree here: rule 1 (a unit right after a number) says unit, rule 4 (a spaced `/` followed by your variable divides) says variable. Before, the division won silently, and the unit error surfaced lines later at an unrelated `in nK`. Neither reading is safe to guess, so the writer chooses.
- **Alternatives:** always the unit, with a warning (D7 rule 5 for a lone unit): silently changes the meaning of `x /m` for anyone who meant rule 4; always the variable (the old behaviour, the friction).

## D172. Eigenvalue problems never evaluate the equation at the ends (gauntlet #69)
- **What:** `solve … lowest N` probes the equation only at interior grid points. ψ = 0 at both ends, so the end values are never needed: the matrix method only uses interior points, and the shooting method (Numerov) now starts from ψ(a) = 0 with y₀ = −h²/12 · lim f ψ (extrapolated from the first two interior points: zero for a regular equation, finite for a Coulomb f ~ 1/r) and ends on y(b) = 0, which is equivalent to ψ(b) = 0. So `V(r) = -1 eV nm / r` with `r from 0 nm` works: the hydrogen-like radial equation gives E_n = −m k²/(2ħ² n²) to 10⁻⁸ (matrix) and 10⁻⁶ (shooting) with `grid 3000` (tests/test_m3_eigen.py).
- **Why:** the singular point is exactly where the boundary condition removes the need for the equation; the old workaround (a wall at r = 10⁻²⁰ m) was a trick.
- **Side effects:** for regular potentials the matrix results are unchanged (the ends were never used); the shooting start moves y₀ by O(h⁴), within Numerov's own error. Coefficient jumps in the first or last cell are no longer detected (the end values are copies of their neighbours). An interior singular point is still the error it was.

## D173. A spaced `+`/`-` in an integral's upper limit warns (gauntlet #67)
- **What:** `2 ∫ x dx from 0 to 1 - π` keeps its parse (the upper limit is 1 − π, D34), but now warns: `the ' - π' is part of the upper limit: this integral goes up to 1 - π`, hint `if that's what you meant, write to (1 - π); to subtract it after integrating, write (… to 1) - π`. Only a binary `+`/`-` with spaces on both sides, outside brackets in the limit, warns: `to 1-0.5`, `to (1 - π)`, `to 2 * -1` don't. The lower limit needs no rule (`to` ends it), and `Σ(… for k from a to b)` has its limits inside brackets.
- **Why:** δ = 2∫…du − π is how the light-deflection integral is written, and the silent reading was a wrong answer. As with D112's divisor, a warning keeps `to L - a` programs working while making the reading visible; one research program (`research/bbn_network/bbn.fm`, `to 60 + x`) now brackets its limit.
- **Alternatives:** end the limit at a spaced `+`/`-` (like D34's `/`): would silently change the meaning of every `to L - a`; an error: too strict for a well-defined parse.

## D174. The 2022 prefixes only on grams and metres, and not on common names (gauntlet #77)
- **What:** quetta- (Q), ronna- (R), ronto- (r) and quecto- (q) are accepted only in `Rg`, `Qg` (ronnagram, quettagram: Earth ≈ 6 Rg, Jupiter ≈ 1.9 Qg), `qg` (quectogram) and `Qm` (quettametre). The rontogram `rg`, `rm`, `Rm` and `qm` are left out because they are common physics names (a gravitational radius r_g, radii, the magnetic Reynolds number), and every other 2022-prefixed unit (about 150 names) is gone.
- **Why:** they turned common two-letter physics names into units: `rg`, `rs`, `rm` (radii), `RC`, `RL`, `RT`, `Rs`, `qV`. With D7 that gives confusing "is a unit" messages for your own variables, and for a name you forgot to define (`2 rs`) a silent unit. Almost no physicist writes rontoseconds. `Qm` and `qg` stay because existing tests use them and they collide with nothing common.
- **Alternatives:** keep them all with a warning (noise for everyone to serve nobody); drop all of them (loses the planetary-mass units); a blocklist of the colliding names (never complete: every letter pair is someone's variable).

## D180. `36 km/h` is an error that says h is Planck's constant (red team round 3 #1) — *spacing part superseded by D235*
- **What:** a unit followed by a tight `/h` (no spaces: `36 km/h`, `2 eV/h`) is an error: *'36 km/h': in Fermium h is Planck's constant, not the hour, so this would divide by Planck's constant*, with the hint *write 36 km/hr for km per hour, or (36 km) / h to divide by Planck's constant* (or *36 km / h (with spaces) to divide by your h* when the program has its own h). Inside brackets and after `in` (`f(v [km/h])`, `print v in km/h`) it is the error *'km/h' isn't a unit …* instead of the old parse error "expected ']'". `2 [h]` after a number is read as a (wrong) unit and gets the checker's "not a unit" error with the hint *h is Planck's constant, not the hour; for hours write hr*, instead of silently being 2 × the list [h]. A spaced `2 eV / h` still divides by the constant, and `km/hr`, `kph` are unchanged.
- **Why an error and not hours:** reading `h` as the hour after a unit would make `2 eV/h` (the frequency of a 2 eV photon, a formula a physicist does write) silently mean eV per hour. Both readings are plausible there, so neither can be chosen silently; the error costs one letter (`hr`), and km/h — the most common speed unit in everyday problems — can no longer produce 5×10³⁷ s/(kg m) with no warning. D7 already keeps `h` out of the unit list for this reason (it collides with heights, and with the constant).
- **Alternatives:** `h` as the hour only in unit position after `/` (rejected: the `eV/h` case above, and `h` would then mean different things in `2 h` and `2 km/h`); a warning (rejected: a warning that fires on the most common way to write a speed is noise, and the value would still be wrong).
- **Overlap:** the parser change is local to `unit_expr` (one check before the `/` continuation) and `_bracket_is_unit`; another change in flight rewrites the unit-after-number rule for `2 m c²`, `8 /m³`, `/(m s²)` (gauntlet #66–#72).

## D181. A °C/°F reading used as a temperature change (red team round 3 #2)
- **What:** D12 reads `10 °C` as the absolute temperature 283.15 K. That is right in `p V = n R T` and 28× wrong in `Q = m c ΔT`, and both formulas multiply a temperature by a quantity in J/K-something, so the unit check can't tell them apart. Three rules now catch the common mistakes:
  - **A name that says "a change" can't hold a reading:** assigning a °C/°F value (not a difference of readings) to `ΔT`, `δT`, `delta_T` (the lexer spells it `δ_T`) or `dT`, or passing one to a parameter with such a name (declared `[K]` or not, from Fermium or from Python through `fermium.compile`), is an error: *ΔT looks like a temperature change, but a value in °C is an absolute temperature (10 °C is 283.15 K)*, hint *write a change of temperature in K (10 °C → 10 K), or as a difference like T2 - T1*.
  - **A °C/°F number written out and multiplied by a quantity with units warns:** `4186 J/(kg K) * 1 kg * 10 °C` → *10 °C is an absolute temperature, so it enters this formula as 283.15 K*, with both readings in the hint (write 10 K for a change; 283.15 K to make an absolute temperature explicit). Dividing such a number by a quantity (ΔT/Δx) warns too; dividing by it (`b / T`, Wien) doesn't, since 1/T only makes sense for an absolute temperature.
  - The existing rule stays: scaling a °C value by a plain number warns (`2 * (20 °C)`).
- **Not warned:** a variable in °C used in a formula (`T = 25 °C` then `k_B T` or `n R T`): that is the usual, correct way to write an absolute temperature, and the gauntlet's thermodynamics problems do exactly this. A difference of readings (`T2 - T1`) is a difference (A47) and never warns.
- **Why names:** Δ is the physicist's own marker for "a change", so it is the one signal in the program that says which reading was meant. The rule only ever turns a silent wrong answer into an error; it can't make a correct program wrong, since a Δ quantity holding an absolute temperature is never what anyone means.
- **Alternatives:** tracking "absolute or difference" in the type (D12's rejected alternative: a new kind of type for one unit); warning on every °C value in a product (fires on every correct `n R T`); refusing °C in products altogether (breaks the ideal-gas law with a thermometer reading, the common case).

## D182. `20.0 °C ± 3%` is 3 % of the reading (red team round 3 #3)
- **What:** a relative uncertainty on a °C/°F value is a percentage of the number as written: `20.0 °C ± 3%` is 20.00 ± 0.60 °C, `68.0 °F ± 1%` is 68.00 ± 0.68 °F (σ = 0.378 K). The checker passes the unit's offset as a third argument of `pm_rel`, and the interpreter takes σ = |value − offset| × p. In K nothing changes (`300.0 K ± 1%` is ± 3.0 K).
- **Why:** a thermometer's "±3 %" is a percentage of its reading; 3 % of 293.15 K (±8.8 °C) is never what is meant. An error was the alternative, but the reading-relative meaning is unambiguous for a value written in °C, and it matches how `± 0.5 °C` is already read as a difference (D120).

## D183. Crank–Nicolson step doubling also checks the start of the run (red team round 3 #4)
- **What:** D131's step doubling compared the two runs only at 8 evenly spread steps. It now also compares them at the first steps (the 4 SDIRK2 start-up steps and the next 2 CN steps) and at 8 finer samples of the first 1/8 of the run. And every step of the first 8 snapshot intervals is kept as a snapshot, so `u(x, t)` early in the run is read from the solution itself instead of a Hermite interpolation across a mode that decays within one snapshot interval.
- **Result:** the two-mode repro (sin πx + sin 20πx over 1 s) now gives u(0.525 m, 0.5 ms) = 1.130 K (exact 1.1309 K; was 0.864 K) and 1.006 K at 1 ms, with no warning. A single fast mode over 100 s (sin 20πx) can't be resolved at its start with at most 32 000 steps, and now says so ("the time step could not be made fine enough … 5.8 %"), where it used to print -0.0120 K silently; its value at 0.1 s is 1.4×10⁻⁷ K (exact ~0).
- **Cost:** a few more comparisons; programs whose early transient was under-resolved now refine the step (tests/test_m3_pde.py: 54 s, unchanged results). Complex (Schrödinger) equations are checked the same way.

## D184. The imaginary unit in a Schrödinger PDE: 𝑖, or a bare i only when you have no i (red team round 3 #10)
- **What:** the PDE front end reads `𝑖` (D90, Tab `\imag`, also written `1i`) as the imaginary unit, the same as the bare `i` it always accepted. A bare `i` still works when the program has no variable, function or loop variable called `i` in scope; if it has one, the solve is an error (*in this PDE, i is your own variable i, not the imaginary unit*, hint *write the imaginary unit as 𝑖 … or 1i*), instead of the variable being silently ignored. `ψ(x, t)` of a complex PDE is now a complex number (D91), read with `.re`/`.im`, `re()`, `im()`, `|ψ|`, `arg`, instead of the 2-vector `.x`/`.y`; the storage is the same, so nothing else changed. Reference §20 and example 41 use 𝑖 now, and §20's "Fermium has no complex numbers" is gone.
- **Why keep the bare i:** every existing TDSE program (and the reference until now) writes `i ħ ∂ψ/∂t`; breaking them would be churn for no safety gain when no `i` exists. The one unsafe case — your own `i` silently replaced by √−1 — is the error.

## D185. Run-time errors inside a module name the module and the calling line (red team round 3 #11)
- **What:** `stats.standard_error([5 m])` on line 4 now stops with *line 4: std needs at least 2 values … (in stdlib/stats.fm, line 6)*, in the JIT, the interpreter and `fermium build` alike (was "line 6" of a 4-line program). Warnings raised in a module's code get the same suffix.
- **Mechanism:** when a module function is instantiated (and for a module's top-level constants), the lines in its IR, including its lambdas (integrands, right-hand sides), become codes `(k << 20) | line`, where k − 1 is the text id of the module's file name (`errors.encode_module_line`). Calling a module function from program code stores the program line in `fm.callline` (the interpreter keeps it in its `line` property); a module line code at run time is `callline << 32 | code`. `errors.decode_line` (Python) and `fm_where` (aot_rt.c) turn it back into "line 4" plus "(in stats.fm, line 6)". Program lines are unchanged, so nothing else sees a difference. A module function called from a program function reports the line of that call inside the program function.
- **Alternatives:** a real call stack (a push/pop per call in compiled code: a cost in every hot loop for a message); reporting only the module line (the program line with the call is what the user can change).

## D190. Eigenfunctions of `solve … lowest N` from Numerov's discretisation (gauntlet #70)
- **Problem:** the eigenvalues were Richardson-extrapolated (10⁻¹¹ on smooth potentials) but the eigenvectors were the plain second-order finite-difference ones, with `np.gradient` (also second order) for ψ' and trapezoid normalisation: ⟨1/r⟩ of hydrogen 2s/2p was 1.5×10⁻⁵ off at the default grid, ⟨x²⟩ of the oscillator 4×10⁻⁶–10⁻⁵. `grid 32000` was needed for 10⁻⁷.
- **What:** after LAPACK (unchanged) each state is refined to the eigenvector of **Numerov's** fourth-order discretisation, `(ψ_{i-1} − 2ψ_i + ψ_{i+1})/h² = (f_{i-1}ψ_{i-1} + 10 f_iψ_i + f_{i+1}ψ_{i+1})/12` with f = α − wE: the pencil (K − E G)ψ = 0 is banded, and inverse iteration shifted by the extrapolated finite-difference eigenvalue (within ~10⁻⁹ of Numerov's own) converges in two or three `solve_banded` calls. f·ψ at an end is not 0 for a Coulomb-like f ~ 1/x, so it is the quadratic extrapolation 3g₁ − 3g₂ + g₃ (the end itself is still never evaluated, D172; linear extrapolation left 2s at 6×10⁻⁸). ψ' uses fourth-order differences (one-sided near the ends), and ψ is normalised so that ∫ψ² of the cubic Hermite interpolant that `ψ(x)` evaluates is 1 exactly (4-point Gauss per cell).
- **Eigenvalues too:** the same iteration gives Numerov's eigenvalue λ; it is computed on the grids of h, 2h and 4h and extrapolated (Aitken: λ_h + (λ_h − λ_2h)/(r − 1)) when the differences shrink by a steady factor r of 12–40 per halving (16 for smooth potentials, i.e. Richardson for O(h⁴); ~32–36 for the Coulomb s states). Otherwise the finite-difference value is kept. Hydrogen 1s went from 6×10⁻⁸ to 2×10⁻¹⁰, the oscillator from 2×10⁻¹¹ to 10⁻¹³.
- **Results at the default grid:** ⟨1/r⟩ of 2s and 2p to 6×10⁻¹⁰ and 3×10⁻¹⁰, ⟨x²⟩ of the oscillator's first three states to 10⁻¹⁰, fourth-order convergence checked (tests/test_friction_d190.py). The gauntlet problem 31_hydrogen_fine_structure dropped its `grid 32000`.
- **Not changed:** where the coefficients jump (a finite well), Numerov is no better than O(h²), so those states stay the finite-difference vectors. The shooting method's vectors were already Numerov's; they get the new derivative and normalisation.
- **Alternatives:** Richardson on the vectors (needs interpolation to the fine grid's odd points at O(h⁴), and consistent normalisation/sign across grids); Numerov as a generalised symmetric eigenproblem for LAPACK (B is not diagonal, so it isn't tridiagonal-symmetric; the refinement costs O(n) per state instead); Numerov shooting for every state (a Python loop per energy; far slower for many states).

## D191. A function over several lists: `f(xs, ys)` element by element (gauntlet #56)
- **What:** a scalar function called with more than one list argument is applied pairwise (`IMap` now carries a list of list positions): `f(xs, ys)[i] = f(xs[i], ys[i])`, number arguments are shared. The lists must have the same length, checked at run time with the error that `xs + ys` already gives (*these two lists have different lengths (3 and 2)*). Units are checked once, with each list's element units, as for one list. Before, it was an error: *can't apply a function element-wise over two lists at once*.
- **Alternatives:** broadcasting to all pairs (an outer product; surprising, and not what `xs + ys` does); a `map` built-in (more to learn for the same thing).

## D192. A unit after a list literal: `[1, 2, 3] m` (gauntlet #56) — *collision part superseded by D235*
- **What:** the rule that already let a unit follow a matrix literal (`[[1, 2], [3, 4]] N/m`, D29) now covers every list literal: `[1, 2, 3] m` is `[1 m, 2 m, 3 m]`. As for matrices, a unit name that is one of your variables keeps its meaning as a product (`m = 3` then `[1, 2] m` is `[3, 6]`), and `[1, 2] [m]` is always the unit. Before, `[1, 2, 3] m` was *m isn't defined* with a hint to write `[m]`.
- **Alternatives:** only in brackets (the old rule: consistent, but everyone writes `[1, 2, 3] m` first); the D7 note for a colliding variable as with numbers (lists had no such rule; the product reading is what the matrix rule does).

## D193. `table(x = xs, y = ys)`: lists as a data set, for `fit` (gauntlet #56)
- **What:** `table(name = list, …)` makes a data set from lists in memory, the same kind of value as `load "file.csv"`: `fit T = 2π √(L / g) to table(L = Ls, T = Ts)`, `data = table(t = ts, x = xs)` then `data.x`, `fit x = v t + x0 to data`, and `print data`. The column names are the names the fit's model uses, so they may shadow variables (`table(L = L, T = T)`). Each column's display unit is its list's (`[1, 2] cm` stays in cm). The lists are copied when the table is made; columns of different lengths are a run-time error. The fit report says where the points came from: *(5 data points from table(L = Ls, T = Ts))*.
- **Implementation:** a new expression `ITable` → `fm_table(ncols, data pointers, lengths)` returns a data-set handle, like `fm_load`: Python (`Runtime.table`) for the JIT and the interpreter, C (`aot_data.c`) for `fermium build`, so `fit` and `data.x` needed no change.
- **Alternatives:** `fit y = a x + b to xs, ys` (which list is which name? It needs a naming rule, and it's fit-only); fitting list variables directly, `fit Ts = 2π √(Ls / g)` (no `to`; implicit, and a list-valued model would be ambiguous with ordinary list arithmetic); a `table` with positional columns named after the variables (breaks for expressions like `2 xs`).

## D194. One-line helper functions inside a function (gauntlet #75)
- **What:** inside a function body, `g(s) = expression` defines a helper that can use the enclosing function's parameters and variables. Each call `g(a)` is expanded in place, like `(body where s = a)` (an `ILet`), with the body's other names resolved in the scope where g was defined, so `∫ g(x) dx` inside `h(x)` still means h's x inside g. Each call is unit-checked with its own arguments (the same per-use checking as top-level functions, D43), and it compiles to plain code on all three back ends (JIT, interpreter, `fermium build`).
- **Limits (clear errors):** one line only (no block body, no `where`, no `[unit]` on parameters); only called (not passed to a function, differentiated with `'` or `d/dx` by name); not recursive. An error inside the helper names the call: *this happened when calling g on line 3 (with s = length [m])*.
- **Why expansion:** closures (captures through lambdas, D43) exist only for integrands and ODE right-hand sides; a separate function instance would need the captured parameters passed as hidden arguments at every call, and every capture path (ODEs inside the helper, nested integrals) re-plumbed. Expanding one-line bodies reuses the `where` machinery and gets captures inside integrals and solves for free.
- **Alternatives:** lifting the helper to the top level with extra parameters (changes arity, confusing errors); allowing only helpers that use no outer names (the gauntlet case, Rabi's pulse shape, needs the outer τ); multi-line helpers (would need real closures).

## D195. Matrices up to 16×16, filled in a loop (gauntlet #51)
- **What:** matrices up to 16×16 (they stopped at 4×4) and vectors up to 16 components; `M[i, j] = x` and `v[i] = x` (also `+=` …) set one entry, with indexes known at compile time or only at run time (checked then, like reads, #54); `zeros(r, c)` makes an r×c matrix whose unit comes from its first use (like a plain 0), and `identity(n)` goes to 16. So a chain of coupled oscillators or a tight-binding Hamiltonian is built in a loop and diagonalised: `eigenvalues`, `eigenvectors` (also K v = λ M v), `det`, `inverse`, `solve_linear`, products.
- **Representation unchanged:** a matrix is still one LLVM vector `<r·c x double>` (up to `<256 x double>`), so storage, captures, function arguments, the REPL arena and printing needed nothing new. Entry assignment makes a copy with `insertelement` at the checked flat index and assigns it back to the variable (`IVecSet`); LLVM keeps it in registers or on the stack.
- **The algorithms beyond 4×4** (`fermium/linalg_big.py`): unrolled straight-line code grows as n³ (Gaussian elimination) and n³ × sweeps (Jacobi: ~220 000 instructions at 16×16), so the same algorithms are written once against an `ops` object that also has loops, arrays and whole-number indexes. The code generator's ops emit LLVM loops over stack arrays; the interpreter's ops are Python loops over lists. Both run the same floating-point operations in the same order, so the two back ends still print identically (tested), and `fermium build` needs no C code for them. Beyond 4×4: Gaussian elimination with partial pivoting (the largest entry of the column, swapped once) for `det` (sign of the swaps × pivots), `inverse` and `solve_linear`; cyclic Jacobi with 16 sweeps (quadratic convergence: 10⁻¹⁶ on 16×16 chains) for `eigenvalues`/`eigenvectors`, with Cholesky for the generalised problem; the symmetry and positive-definiteness checks as before. Up to 4×4 the unrolled routines are kept (`det` by cofactors stays exact for whole numbers).
- **Validated:** a spring chain's ω² = (4k/m) sin²(nπ/(2(N+1))) and a 16-site tight-binding chain's E_n = −2t cos(nπ/17) and eigenvectors to 10⁻¹³; `solve_linear`, `det`, `inverse`, symmetric and generalised eigenvalues of a 6×6 matrix against NumPy/SciPy to 10⁻¹⁰; JIT = interpreter = `fermium build` (tests/test_friction_d190.py).
- **Not done:** a unit per entry (still one unit per matrix); matrices larger than 16×16 (use lists, or Python via `use python`); matrices as ODE unknowns; lists of vectors. *Alternatives:* heap-allocated n×n matrices with runtime sizes (a new type through every back end, and runtime size errors instead of compile-time ones); LAPACK through a Python callback (fast, but `fermium build` would have to refuse or re-implement it, and the two back ends would no longer share one operation order).

## D196. Display units: S/m for a conductivity, m²/s² for a speed squared (gauntlet #80)
- **What:** the preferred display unit for s³ A²/(kg m³) is now `S/m` (it was the composite `F/(m s)`: ε₀ω_p²/γ, the Drude conductivity), and for m²/s² it is `m²/s²` (it was `J/kg`: `2 g h` and a²ω² printed as an energy per kilogram). J/(m³ K⁴) was added for the radiation constant 4σ/c (it printed `kg/(m s² K⁴)`, gauntlet T12, #59).
- **Why m²/s² over J/kg:** m²/s² is what a speed squared is, and it is also correct for a specific energy; the reverse reading (a speed squared as an energy per kilogram) surprised people. A specific energy can still be shown as `E in J/kg`, and a value whose unit came from the program (like `-5 J/kg`) keeps it. The bootcamp's two specific-energy printouts now say `in J/kg`; lesson 1's `2 g h` shows `196 m²/s²`.

## D197. Rounding noise in printed vectors and matrices shows as 0 (gauntlet #59, O12)
- **What:** in a computed vector or matrix, an entry smaller than 10⁻¹⁴ of the largest entry (all entries share one unit) prints as `0`: `inverse(A) A` prints the identity, and a mode shape's exact zero no longer shows as `-8.76×10⁻¹⁷`. The same rule and threshold as a complex number's parts (D94). Entries written in the program (`[[1, 1e-20], [0, 1]]`) are never changed, and neither are lists or vectors with a different unit per component. The value itself is untouched (printing only). Mirrored in the C runtime for `fermium build`, which also stopped shortening vectors of more than 12 components (only lists are shortened with `…`).
- **Alternatives:** an absolute threshold (meaningless across units); rounding every entry to the printed significant figures (hides real small entries of a well-scaled matrix no more than this does, but changes the look of all output).

## D198. Warnings quote numbers as written (gauntlet #81)
- **What:** the parser keeps each number's spelling (`Num.raw`), and the messages about a unit after a number (`'2.50e19 m' is the unit m, not your variable m`, the ambiguity error, the #10 note) and the a/(b c) warning quote it as written, not as Python formats it (`2.5e+19`). `ast.num_text` falls back to the formatted value for numbers the parser made itself.

## D199. Printed formulas: ∂²f/∂x², and no `2 g` for your variable g (gauntlet #59, E14, M13)
- **What:** a second partial derivative is named `∂²term/∂x²` (it was `∂2term/∂x2`), and a printed formula never puts a number right before a variable whose name is also a unit: `g/√(2·g h)` (ASCII `2*g`), not `g/√(2 g h)`, which pasted back into a program means 2 grams. Other products print as before (`2x`, `m v`).

## D200. `absolute 1e-6 °C` is a temperature step (red team round 4 #1)
- **What:** an absolute tolerance written in °C or °F is read as a size of error, a temperature difference: `absolute 1e-6 °C` is 10⁻⁶ K, as `absolute 1e-6 K` is. Before, D12's reading made it the absolute temperature 273.15 K + 10⁻⁶ K, which switched the error control off (Newton's cooling gave 26.65 °C instead of 23.49 °C, silently, in the JIT, the interpreter and `fermium build`).
- **Why:** a tolerance is always a difference; the same choice as `± 0.5 °C` (D120). `tolerance` is a plain number, so it has no such question.
- **Alternative:** refusing °C/°F in `absolute` (an extra error for a form whose meaning is clear).

## D201. An absolute tolerance at least as large as the starting values warns (red team round 4 #18)
- **What:** each `absolute` value is compared with the largest initial value (written as a constant) of the unknowns in its units. If it is at least as large, the solve warns: *this absolute tolerance is 1000× the largest starting value in its units (1 m), so the error control is effectively off …*, hint *… make it much smaller than the values … (check the unit: mm, not km?)*. Unknowns that all start at 0 give no scale and nothing is said (an abundance growing from 0 with `absolute 1e-16`).
- **Why:** `absolute 1 km` for a 1 m oscillator (a typo for mm) printed 89.2 m for −0.839 m with no message. A warning, not an error: the value is legal and the threshold is a heuristic.
- **Alternatives:** compare with the solution's range after the run (needs a run-time check in three back ends for a compile-time mistake); a stricter threshold like 1 % of the start (would fire on deliberately loose runs).

## D202. Hz and rev/rpm at the Python boundaries warn like `in Hz` (red team round 4 #2)
- **What:** `use python` functions with a parameter declared `[Hz]` (or `[rpm]`, `[rev/s]`) and Fermium functions called from Python through `fermium.compile` with such a parameter now give the D95 warning when the argument is in the other family: `np.positive(60 rpm)` for `f [Hz]` warns that 1 rpm is 0.10472 Hz here, not 0.0166667 Hz; `mod.g(Q(1, "Hz"))` for `w [rpm]` warns that 1 Hz is 9.5493 rpm, not 60 rpm. The warning text comes from one helper (`checker.hz_angle_mixup`) shared with `in`. From Python it goes to stderr (unless `warnings=False`) and to `mod.warnings`.
- **Why:** the value is still converted by the rad = 1 rule (D6, D27), which is self-consistent; what was missing was the warning that native code gives, and Python code receives the converted number with no trace.
- **Alternative:** converting cycles ↔ turns (Hz = rev/s) at the boundary only: the same number would mean different things inside and outside Fermium.

## D203. A prefixed unit that spells two of your variables warns (red team round 4 #3) — *kept; see D235*
- **What:** right after a number, a prefixed unit whose prefix and unit are both your variables (`1.5 kT` with your k and T: kilotesla) warns: *'1.5 kT' is the unit kT (the prefix k on the unit T: a magnetic field), not your k times your T*, hint *write 1.5 k T (with a space) … or 1.5 [kT] if you mean the unit*. Units that every course uses (`nm`, `kg`, `mV`, `MeV`, `ms`, `kPa`, … listed in `Parser.COMMON_PREFIXED`) never warn, so `500 nm` next to a refractive index n and an order m, or `2 kg` next to a spring constant k and g, stay quiet.
- **Why a warning, not the D7 error:** it is the lone-unit case of D7 rule 5 (a unit alone after a number is the unit, with a warning), and the output (`1.5 kT`) already shows the unit, so the warning is enough to catch the eye.
- **Alternatives:** an error (would need an exception list anyway); checking every prefixed unit (fires on `2 kg` in half the mechanics programs).

## D204. `/ (…)` after a unit is a unit denominator only if the bracket holds units (red team round 4 #4) — *spacing part superseded by D235*
- **What:** after a unit, `/` followed by a bracket continues the unit only when every name in the bracket is a unit name, and, with a space before the `/` (D7 rule 4), none of them is your variable. So `P 60 s / (m c_w)` divides by your m and c_w (14.3 K), and `9.81 kg / (m s²)` or `3 J/(kg K)` stay units. Before, the first name decided and the parser stopped with "expected ')'".
- **Alternative:** backtracking on the parse error (hides real errors in units written in brackets).

## D205. The integral-limit warnings (D112, D173) are decided by the units (red team round 4 #6, #7)
- **What:** the parser still reads `to T - t0` as the limit and `to E / (2 P0)` as a division of the whole integral (D34, D173), but it only records these places; the checker decides:
  - **Both readings type-check** → the old warning. For `+`/`-` that is when the integral has the integration variable's units (a dimensionless integrand, like δ = 2∫… − π); for `/`, when the divisor is a plain number (`to 2π / (2π)`, `to 1 / (1 + z)`).
  - **Only the written reading type-checks** → no warning: `∫ v dt from 0 s to T - t0` is 24 m (the other reading would be metres minus seconds).
  - **The written reading fails, the other works** → the error says how to write it: `∫ P0 dt from 0 s to E / (2 P0)` stops with *the limits of this integral are time [s] and energy [J]: the ' / ' after the upper limit divides the whole integral, not the limit*, hint *to divide the limit, write to (E / (2 P0))*; `to T - x0` with x0 in metres keeps the unit error with the hint *… to subtract it after integrating, write (… to T) - x0*.
- **Why not choose the reading that type-checks silently in the third case:** D7's rejected alternative: the meaning would depend on units defined far away. An error that names the fix costs one edit.
- **Limitation:** factors in front of the integral (`k ∫ … to T - t0`) are not included in the "other reading" check for `+`/`-`.

## D206. PDE step control measures the error against the solution's range and starts after a jump's layer (red team round 4 #8)
- **What:** (1) the step-doubling estimate (D131, D183) is now relative to the solution's range (largest − smallest value, boundary values included) instead of its largest absolute value, so a problem in °C, in K, or with an offset is judged the same way; a constant solution falls back to its size. (2) When the initial value and a Dirichlet boundary value disagree at t0 (a rod at 0 K whose end is held at 80 K), checks before t0 + 10 h²/D are skipped: there the jump is a layer thinner than a grid cell, which no time step resolves and which doesn't affect later times. Solutions without such a jump are checked exactly as before (the D183 fast-mode cases still warn).
- **Result:** the textbook step change gives u(0.1 m, 1000 s) = 65.8435 K (Fourier series 65.84355 K) with no warning, in ~2 s instead of ~3 s; 20 °C/100 °C gives 85.8435 °C, the same.
- **Alternatives:** raising the tolerance (hides real errors); smoothing the initial data (changes the problem).

## D207. The unit-after-number messages quote the source; an error replaces the warnings before it (red team round 4 #9, #10) — *warning part superseded by D235*
- **What:** the D7 lone-unit warning quotes the number as written and the whole unit (`'8.5e28 m^-3' is the unit m^-3 …`, hint `write 8.5e28 [m^-3] (or [1/m³])`), not `8.5e+28 [m]`. When the D7 rule 5 error ("'5000 m' is ambiguous") is raised, warnings the parser gave on the way to it for the same quantity ("'5000 m' is the unit m", "reading 'L' as your variable L") are dropped, since they contradict the error.

## D208. A cancellation down to rounding noise leaves no uncertainty (red team round 4 #11)
- **What:** when two contributions of one error source are added (`uncertain._sum2`, used by every +, −, ×, ÷) and the sum is below 10⁻¹³ of the terms, it is 0. So `T / sqrt(L)` with T = 2π√(L/g) prints `2.01 ± 0 s/m^(1/2)` and `(y / 3) * 3 - y` prints `0 ± 0 m`. A value with σ = 0 prints by the default rule of plain numbers (D11): `2.01 ± 0`, not 20 digits.
- **Why:** first-order propagation can only be as exact as double precision; a 10⁻¹⁸ σ from rounding in ∂f/∂x is not a measurement uncertainty, and D121's "value to the second digit of σ" turned it into 20 printed digits.
- **Alternative:** a relative threshold on the final σ (would also drop real, very small relative uncertainties, like a clock's 10⁻¹⁸).

## D209. Slicing a data table is an error about tables (red team round 4 #17)
- **What:** `fit … to data[2:5]` stops with *a data table can't be sliced with [a:b]; its columns are lists, and those can be*, hint *to fit some of the rows, slice the columns and make a table of them: fit … to table(L = data.L[2:5], T = data.T[2:5])* (D193's `table(…)`; tested). Slicing a table directly would need a table-valued slice in all three back ends, for a form that `table(…)` already covers.

## D210. A defined function's derivative in a solve is a value (gauntlet #87)
- **What:** in `solve` (ODEs and eigenvalue problems), a derivative of a function or ODE solution that already exists, applied to an argument (`f'(r)`, `d/dr f(r)`, `V'(x)`), is a value, not an unknown. Only derivatives of the other names count as unknowns: `_find_derivs` in fermium/solve.py takes the set of such known names (`_known_called`: called only with an argument, never bare, defined in scope, not named by a boundary/initial condition), and both `_check_solve` and `check_eigen` (fermium/m3solve.py) use it. The root-finding rule of #3 (`solve I'(θ) = 0 for θ …`) is unchanged.
- **Why:** writing the spin–orbit term as `f'(r)` is how the paper writes it; the unknown is always the function with the boundary or initial conditions, and a defined function is never an unknown.
- **Alternatives:** requiring `df = d/dr f(r)` outside the equation (the workaround); treating every defined name as known even when written bare (`x'` of a variable x that happens to exist would stop being the unknown).

## D211. In a solve, the unknown written with a prime is never a unit before its prime (gauntlet #88) — *lone-unit warning superseded by D235*
- **What:** before a `solve` is parsed, its tokens are scanned for names written with a prime (`u''`, `x'`): its unknowns. (1) Such a name right before its prime is never a unit (`eV nm² u''`, `0.5 u''`), and (2) while the equations are parsed the unknowns count as your variables, so the D7 rules apply: `nm² * u` isn't continued by u, `2 u * V0` is the D7 ambiguity error, and `2 u` alone is the unit with the D7 warning. The messages name the unknown: "'2 u' is ambiguous: right after a number, u is a unit (atomic mass units), but u is also the unknown u of this solve", and a resulting unit error becomes "u here is read as the unit u (atomic mass unit), not the unknown u of this solve: '2 u' is a unit right after a number; write 2 * u" (D164's message; `UNIT_NAMES_LONG` in units.py names u, b, l, L, Da). Ranges and `with` clauses are unchanged (`for t from 0 s …` with an unknown s still means seconds).
- **Why:** a prime on a unit means nothing, so (1) is always right; for a bare `2 u`, D7's rule for your variables (error when combined, unit with a warning alone) is the tested, predictable behaviour, and the reading is now named in the message.
- **Alternatives:** reading every `2 u` in the equations as the unknown (breaks a time unit `(2 s)` in an equation whose unknown is s, silently); only improving the message (the ask's minimum; the parse fix removes the common case).

## D212. `d/dr f(r, R)` with R undefined is a function of (r, R) (gauntlet #89)
- **What:** `d/dr` of a call whose arguments are all names, with some of them not defined, gives a function of those names in the call's order: `df = d/dr f(r, R)` is df(r, R) = ∂f/∂r. A defined argument stays a value (`R = 5 fm` first: df(r)). An undefined name inside a more complicated argument (`d/dr f(r, 2 R)`) is an error: "d/dr f(r, 2R): R isn't defined, so this can't be a function of r alone", hint `∂/∂r f`.
- **Alternatives:** always an error suggesting `∂/∂r f` (works, but the notation on paper is d/dr f(r, R)); taking R from the globals at call time (the old behaviour: a surprising "R isn't defined" later). A program that defined R after writing `d/dr f(r, R)` and called df with one argument now gets "df takes 2 arguments".

## D213. Overriding a well-known constant warns at the assignment (gauntlet #90)
- **What:** assigning h, c, G, e or k_B warns once per name, at the assignment, when the new value is a plain number or has the constant's own units: "h (Planck's constant) is now your variable: from here on, h means your value". The older warning (gauntlet friction #34: a constant redefined after being used as the constant) is unchanged.
- **Why the units condition:** a plain number is how the other conventions' h, e, G, c appear (the Hubble h, an eccentricity, G = c = 1), and a value in the constant's units would make later formulas silently wrong. A height `h = 10 m` or a step `h = 1e-6 s` is common and meant (D13, bootcamp lessons 1, 2 and 7 use it), and any later use as Planck's constant is a unit error anyway, so warning there would be noise for beginners. No style guide asks for silence on the number case.
- **Alternatives:** warn on every override (noisy in the bootcamp's first lessons); warn only on use (the old behaviour, which missed the Hubble h).

## D214. "Too many steps" says where the solver started and where it got (gauntlet #91)
- **What:** the RK45 limit (20 million steps) and the stiff solver's limit report "it got from z = 2500 only to z = 2499", each value with the fewest digits (3 upwards) that tell them apart. The error kind carries the variable's name: kind = 1 000 001 + text id (RK45) or 2 000 001 + text id (stiff), a = place reached, b = start; the Python runtime (JIT and interpreter) and aot_rt.c (`fermium build`) format it the same way. The interpreter's limit is `interp.ODE_MAX_STEPS` (tests lower it).
- **Why:** the place reached was reported, but at 3 significant figures 2499.4 printed as 2500, the start, so the message read as if nothing had happened.
- **Alternatives:** always 6 digits (still shows 2500.00 for 2499.9999); a fraction of the range (needs the end too, and says less).

## D215. A unit after a bracket, and after `number variable` (gauntlet #92) — *superseded by D238*
- **What:** right after a bracketed expression (`(51 - 33 (N - Z)/A) MeV`, also `2 (N - Z) MeV`), a unit name multiplies by 1 unit, as after a number (D7 rule 1). After a product that starts with a number and ends with your variable (`100 h km/s/Mpc`), a compound unit (two names or more) does the same; a single name (`2 a b²`) doesn't. The value may have units of its own (`(x + 1 m) s` is in m s): the parser marks the quantity `times_unit` and the checker multiplies.
- **Collisions (the D7 rule, adapted):** a name that is your variable (known so far, or assigned, looped over, a parameter or imported anywhere in the program) keeps its old reading after a bracket: `(v1 - v2) m` is times your m. The ask was an error here (D7 rule 5), but tested programs in the gauntlet and the research reproductions write `(…) m`, `(…) u`, `(…) g`, `(4/3) T` for their own m, u, g, T (8 tests failed with the error), and the old reading was never a unit, so it isn't a silent change. Constants that are also units (c, AU, M☉, R☉, L☉) keep the constant reading (the same value). After ½, π or a call (`f(3) MeV`) nothing changes (D7).
- **Alternatives:** the D7 error on every collision (breaks tested programs); only `* 1 MeV` (the old answer, the friction).

## D216. Text concatenation, str(x), clear(xs), plot continuation lines (gauntlet #93-#95)
- **Text:** `"3p" + "1/2"` joins texts. Two written texts are joined by the checker; others at run time by the runtime (`fm_text_concat`), which adds the new text to the program's text table (one id per distinct text, so a loop doesn't grow it). `str(x)` shows a number as print would (its unit and digits, `fm_text_num`). Text + a number is an error with the hint `str(x)`. `fermium build`: aot_rt.c keeps the table growable (the program's texts are `fm_texts0`, made ones follow).
- **Lists:** `clear(xs)` empties a list in place, from a function too (like push, which already changed the program's list). `xs = []` in a function makes a new local list, as before, but when the program has a list xs it warns: "xs = [] here makes a new list xs inside this function; the program's list xs is unchanged", hint `clear(xs)`. *Not done:* a `global xs` declaration (a new statement for one use; clear covers the case in the research code).
- **Plots:** a `plot` may continue on indented lines: more series (`zs vs xs`), `with …` options and `to "f.png"`.
- **Still open:** an if-expression can't hold `in MeV to 3 digits` (formatting belongs to print), and plots have no text labels on points (#96).

## D220. A failed REPL or Jupyter input leaves no trace (red team round 5 #1, #2)
- **What:** `ReplSession.execute` snapshots what the checker knows before each input (the global names, each variable's and function's own state and instance caches, the dimension substitution, the loaded modules, the names the parser treats as known) and restores it when the input fails, at compile time or at run time. So after `L = 1.20 ± 0.01 m` (refused in the REPL) or a block whose third line has a unit error, `L = 3 m` and `print ww` work (before, the half-defined variable had no arena slot, and every later use was an internal error). A failed `from mechanics import a, nothere` imports nothing. The Jupyter kernel's `do_execute` now catches every exception and always sends an `execute_reply` (status error with a one-line message), so a cell can never hang without output.
- **Limit:** a run-time error can't undo values already stored: if the failing input had already assigned a new value to an existing variable of the same units, that variable keeps the new value (the name and units are restored).
- **Alternatives:** keep a checker per input and copy it (deep copies of symbols break the arena slots compiled code already uses); roll back only on compile errors (a run-time error in a line that defines a new variable would leave it defined but with an unset value, which reads as 0).

## D221. `d/dt x(2 s)` is the derivative at 2 s; `∂/∂x ∂/∂y f` is a mixed partial (red team round 5 #4, #18)
- **What:** `d/dt x(2 s)`, `∂/∂x f(1, 2)` (a function called at arguments that don't involve the variable) mean the derivative of the function evaluated there: `(d/dt x)(2 s)` = `x'(2 s)`. Before, the operand was read as a formula that doesn't depend on t, so the result was a silent `d/dt (x(2 s)) = 0`. If the variable is not one of the function's parameters (`∂/∂z f(1, 2)` for f(x, y)), it is an error that names the parameters. `∂/∂x ∂/∂y f` differentiates the function ∂f/∂y again (Maxwell relations), for any order and mix of `d/d…` and `∂/∂…`. A derivative's label prints `∂/∂x (…)` for a partial and `d²/dx² (…)` (superscripts) for a higher order.
- **Alternatives:** an error suggesting `x'(2 s)` (the reviewer's other option; the derivative at a point is what a physicist reads, and it costs nothing to give it); a warning that the result is 0 (keeps a wrong answer).

## D222. A list or matrix literal followed by one of your variables' names: D7 rule 5 (red team round 5 #5) — *superseded by D235*
- **What:** `[1, 2, 3] m` when you also have a variable m keeps D192's reading (the product with your m, as for matrices, D29), but no longer silently: standing alone it warns *'[1, 2, 3] m' multiplies by your variable m, not the unit m (metres)* with the hint to write `[1, 2, 3]*m` or `[1, 2, 3] [m]`, and combined with other factors (`[1, 2] m v`) it is an error *… is ambiguous: after a list, m could be the unit (metres), but m is also your variable m*. `[1, 2] * m` (explicit) and `[1, 2] [m]` never warn.
- **Why the product reading stays:** changing it to the unit reading (as D7 does for a number) would silently change existing programs that multiply a matrix by a stiffness `k` or a list by a mass `m`; the warning makes either intent visible, and the error covers the case D7 found most dangerous.
- **Alternatives:** the unit reading with a warning (D7's choice for numbers; rejected above); an error always (breaks D192's and D29's documented examples).

## D223. Tool positions and messages (red team round 5 #6–#13, #16, #17)
- **Positions:** errors inside a `parallel for` point at the offending statement (its column and length), not at column 1. The language server converts between Fermium's code-point columns and the protocol's UTF-16 columns in both directions (diagnostics, hover, completion), so ranges are right after 𝑖 (two UTF-16 units). The °C-in-a-product warning (D181) points at the `10 °C` literal.
- **Tools:** Jupyter and the playground show run-time warnings like `fermium run` (Jupyter on stderr, in order with the cell's output; the playground in `warnings`). Hover works on `module.member`, and on a data table (its columns and units). Jupyter's Tab completes names, keywords and module members (via the language server's `completions`), not only `\name`. `:vars` describes values in physics words (*a 2-D vector of speed [m/s]*) and lists modules. `fermium check` prints its warnings in line order and ends with *units check out, N warnings* instead of *no problems found*. `fermium doctor` reports pygls and ipykernel (optional, never counted as problems).
- **Parse:** plot options may follow the file name without `with` (`plot y vs x to "a.png" title "…"`), as they may follow the last series.
- **Messages:** `print ψ` after an eigenvalue problem says the states are ψ₁, ψ₂ … and how to write them in ASCII.
- **Formatter:** `fmt --ascii` writes `2𝑖` as `2i` after a plain number (not after an exponent, where `2^3i` would mean 2^(3i), and not before a name).

## D230. Three conveniences reverted because they hid real values (red team round 6 #1–#4)
- **What:**
  - The integral "rounding noise is 0" snap (red team 4 #12) is removed. `∫ 1e6 sin(x) + 4e-9 dx from -1 to 1` is 8×10⁻⁹, not rounding noise, but it was printed as `0`.
  - The vector/matrix noise-zeroing (D197) is disabled. The 1s of a Minkowski metric next to c², and 1 mm written in AU, were printed as 0.
  - The PDE start-up skip after a jump between initial and boundary data (D206) is set to 0. It hid a 20 % error at early times (40.93 K where erfc gives 34.34 K).
- **Why:** each one made some outputs prettier, and each one printed a plausible, wrong number with no warning in a case nobody had tested. Priority (1) in CLAUDE.md is correctness. `∫ sin(x) dx from -π to π` now shows its honest rounding noise (`3.19×10⁻¹⁶`), and `inverse(A) A` shows its noise off the diagonal. The step-change heat problem may warn that its step can't be made fine enough (round 4 #8), which is true of its early times.
- **Alternatives considered:** a smarter noise test (per row and column scale, or tracking which entries came from cancellation). It may come later, but tonight there was no time to prove one never hides a real value.

## D231. The variable of `d/ds` is your variable in its operand; `d/ds h(2 s)` is an error (red team round 6 #6) — *message superseded by D235*
- **Problem:** with `h(s) = s^2`, `print d/ds h(2 s)` printed `4 s`: `2 s` right after a number is the unit (D7), so the argument was the constant 2 seconds and D221 gave h′ at 2 s. On paper d/ds h(2s) = 2h′(2s) = 8s. Same for `d/dm k(2 m)` and `∂/∂s`.
- **What:** while the operand of `d/dv` or `∂/∂v` is parsed, v counts as your variable (for D7 rule 5), and a lone unit of that name right after a number (`2 s`, `3 s^2`, alone or in a product) is an error: *'2 s' is ambiguous: s is the variable you differentiate by, but right after a number s is the unit seconds*, hint *write 2*s for 2 × the variable s, or 2 [s] for the unit*. `d/ds h(2*s)` (the formula, D221), `d/ds h(2 [s])` (h′ at 2 s) and names that aren't units (`d/dt x(2 s)`) are unchanged.
- **Why an error even alone** (D7 makes a lone collision a warning): D7's lone case is the unit reading because an initial position `0.1 m` next to a mass m is the common intent. Inside a derivative by s, the argument's dependence on s is the whole point, and the two readings give different silent answers (h′(2 s) vs 2 h′(2s)); one keystroke settles it.
- **Alternatives:** the variable reading (a special case against D7's "after a number, a unit"); the unit reading with a warning (keeps a likely-wrong number on screen).

## D232. The integration variable and a solve's independent variable are your variables for D7 (red team round 6 #7) — *lone-unit warning superseded by D235*
- **Problem:** `∫ 3 s^2 ds from 0 to 1` printed `3 s²` (3 square seconds, integrated over a plain number) with no warning, and `∫ 2 m dm` printed `2 m`, while `f(s) = 3 s^2` warns. The integrand's lookahead (A54) already made s your variable, but the unit reader took `s^2 ds` as one compound unit (s²·ds) before the `ds` was split off, and the lone-unit check skips compound units. A solve's independent variable (`solve y' = 3 s^2 … for s from 0 to 1`) wasn't counted at all.
- **What:** after the trailing `dv` is split off, the integrand is checked again with v as your variable: a lone `3 s^2` or `2 m` warns (*'3 s^2' is the unit s^2, not your variable s*, as for a parameter), and in a product (`∫ 3 s * s ds`) it was and is an error. The names after `for`/`,` and before `from` in a solve (its independent variables, `for s from …`, `for x from …, t from …`) count as your variables in its equations, like its unknowns (D211). `Σ(2 g^2 for g …)` already warned.
- **Alternatives:** the variable reading inside integrals (inconsistent with D7 elsewhere; the spec's rule is "after a number, a unit"); an error even alone (D7 keeps the lone case a warning).

## D233. Nearly degenerate levels of `solve … lowest N`: parity when the problem is symmetric, a warning otherwise (red team round 6 #5)
- **Problem:** a symmetric double well (splitting 5×10⁻¹¹ eV, ΔE/E = 4×10⁻¹¹) gave a ground state without parity (ψ₁(−1 nm)/ψ₁(1 nm) = 1.014, ⟨x⟩₁ = −0.0132 nm). Inverse iteration (D190) shifted by an eigenvalue within the splitting of the other state converges to a mixture; no eigenvector solver can resolve the pair at that splitting.
- **What:** after the vectors are computed (matrix or shooting), two neighbouring levels closer than 10⁻⁸ of the largest |E| are nearly degenerate. If the equation's coefficients are symmetric about the middle of the range at every grid point (V(a + b − x) = V(x)), the pair is replaced by its even and odd combinations (least squares in the pair's span, then projected), the one with fewer nodes first; they are the true eigenfunctions (a 1-D bound state of an even potential has definite parity). Now ψ₁(−1 nm)/ψ₁(1 nm) = 1.00000, ⟨x⟩ = 0, ψ₂ odd, ⟨ψ₁|ψ₂⟩ = 0, by both methods. Otherwise the run prints *levels N and N+1 are nearly degenerate (ΔE/E = …); their eigenfunctions ψ_N, ψ_N+1 can be any mixture of the two: use them only through combinations, or break the symmetry* (the same text from the JIT and the interpreter; ΔE is given relative, because the solver doesn't know E's unit).
- **Alternatives:** only the warning (the red team's first option; the symmetric case is the common one, ammonia-style inversion, and can be fixed exactly); solving in parity-restricted halves (needs the symmetry detected before the solve and doubles the code paths); a lower threshold (the Numerov refinement mixes pairs whose splitting is near its shift error, ~10⁻⁹ relative).

---

# Run 2: Spec 1.5 → 2 (`dev-notes/FERMIUM_SPEC_V1.5_V2.md`), from 2026-09-25 22:40 UTC

## D234. Starting point, branches and the install command
- **Branch:** `claude/v1.5` is created from `25adb47`, not `af43963` as the spec says. `25adb47` is `af43963` plus three commits made after the first run: the public-tester preparation (all runtime dependencies in core, MIT LICENSE, `.gitignore`), John's README note and the upload of this spec. Starting from `af43963` would drop John's own commits and the spec itself.
- **Install:** every runtime dependency stays in core `dependencies` (John asked for `pip install -e .` to work on a fresh Mac), and the documented command is `python3 -m pip install -e ".[full]"` (spec A6.1). `full` is an empty extra, so both commands install the same thing; the docs use one spelling so a beginner never sees two.
- **Branches of later phases** follow the spec (`claude/v2-rust`, `claude/v2.5`, `claude/v3`); `claude/lucid-gauss-9y1ov2` stays as the v1 record (PR #1).
- **Alternatives:** branching from `af43963` and cherry-picking (same content, more noise); making `[full]` hold the optional packages again (breaks plain `pip install -e .`, which John asked for).

## D235. One unit-name rule, independent of spacing (spec A1)
- **The rule, as the bootcamp states it:**
  1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
  2. If that unit is a single name that is also one of your variables (`2 g` with your own `g`, `0.1 m` with a mass `m`), Fermium stops and asks which you mean: `2*g` for your variable, `2 [g]` for the unit.
  3. In a compound unit (`3 m/s`, `2 kg m²`) the first name is always a unit; any later name that is also your variable is an error that asks the same question.

  Brackets are always units. A name that doesn't come right after a number is a variable. Spaces never change meaning.
- **What changed:** a lone collision (`x(0) = 0.1 m` next to a mass m, `B = 2 T`, `2 c` with your own c) is an error, not a unit with a warning (D7 rule 5, D130, D211, D232). A later name of a compound unit that is your variable is an error whatever the spacing: `20 m/s/g` and `20 m/s / g` both ask (D7 rule 4 read the first as grams and divided in the second; D204 likewise for `/ (…)`). `36 km / h` is the D180 error with or without spaces. The collision is the error itself, so the notes that explained a later unit mismatch (D34 #10, D164, gauntlet #43) are gone, with their code.
- **Refinements the rule needed (each keeps it at three sentences and spacing-free):**
  - **A unit continues through spaces, `/` and `·`; an explicit `*` ends it** (revised by D238): `1.2 fm * A^(1/3)` with a parameter A is 1.2 fm × A^(1/3), and `5 N*m` with your mass m is 5 N × m. `2 N·m` is the newton-metre.
  - **`c` continues a unit only after `/`:** `938 MeV/c²` is a unit, `0.9 c` is the speed of light, but `2 m c²` is `2 m` times c², so with a mass m it is the sentence-2 question instead of a silent metre·c² (the D170 case). `print 2 kg c²` now prints `1.80×10¹⁷ J` instead of `2 kg c² (= 1.80×10¹⁷ J)`: same value.
  - **After `/` right after a number:** a unit name that isn't your variable is the reciprocal unit (`0.5 /s`, `0.5/s`, `0.5 / s` all mean 0.5 per second); your variable is divided by (`1/T` is one over your period: `T` doesn't come right after the number). A bracket that mixes your variables with other units (`0.300 /(m s²)` with your m) asks (D171 made the spaced form an error and let the tight one divide).
  - **A list literal takes a unit as a number does** (D192), so `[1, 2, 3] m` with your m asks; `[1, 2]*m` multiplies (D222 multiplied with a warning).
  - **After `)`, a name is a variable:** `KE = (1/2) m v²` is ½·m·v² (a spec row). A unit after a bracket needs brackets, `(51 - 33 (N-Z)/A) [MeV]` (revised by D238; D215 read the unit when no variable had the name).
  - **Where-bindings:** the names a `where` defines are your variables in the expression before `where`; a binding's own units see only the bindings before it, so `ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg` (John's lesson2b) works.
  - **The unknown of a solve** is your variable inside its equations (D211), not in its `with` clause (`u(0) = 2 u` there is 2 atomic mass units).
- **The one conflict in the spec:** Appendix 1's lesson2.fm writes `a = 3 m` after `m = 1500 kg`, and must "run unchanged"; the A1 table requires `x(0) = 0.1 m` next to a mass `m` (and `B = 2 T`, `8 K`, `0.25 T`, `2 c`) to be an error. These are the same construction, so no rule can pass both. The rule wins: unit safety is the first priority, and the table's rows are real frictions (friction #74: `0.25 T` next to a period T printed 0.25 tesla). lesson2.fm stops at line 14 with *'3 m' is ambiguous … write 3*m for 3 × your variable m, or 3 [m] for the unit*, and after `fermium fmt --fix` (which writes `3 [m]` and `5 [m]`) it prints every value in the spec (tests/test_appendix1.py). The other four programs run unchanged.
- **Migration:** `fermium fmt --fix` rewrites each collision as a bracketed unit that keeps what Fermium 1 did there (`0.1 [m]`, `50 [N/m]`, `20 [m/s] / g` for the spaced form, `20 [m/s/g]` for the tight one, `[1, 2]*m` after a list); the language server offers the same edit as a quick fix. `tools/migrate_a1.py` ran it over every tracked .fm file and Markdown block (log: `dev-notes/A1_MIGRATION.md`: 25 edits in 16 files); `tools/fix_test_literals.py` did the same for programs inside tests. Each migrated program's output was compared with Fermium 1's on the original: only timing lines differ.
- **Cost, measured on the repo:** the rule's sentence 3 fires on `k = 50 N/m` in any spring program that defines the mass `m` first, and on `m/s` in programs with a variable `s`: that is the price of spacing-independence (`50 N/m` and `20 m/s/g` have the same shape). The error costs two brackets.
- **Alternatives:** keeping lone collisions a warning (passes Appendix 1, fails eight table rows); choosing the reading that type-checks (rejected in D7: meaning would depend on distant code); keeping `/` spacing-sensitive (the spec's main complaint).

## D236. A fraction of pure numbers is one coefficient (spec A2, friction #73)
- **What:** in `a / b rest`, when the numerator `a` is a pure number (digits, π, √ and powers of them, products of those, or any of them in brackets) and the denominator is an implicit product that starts with a pure number `b`, the product is (a/b)·rest: `73/24 e²` = (73/24)·e², `π²/12 t²`, `π⁴/80 t⁴`, `1/2 x` = x/2, and `1/2 kg` = 0.5 kg (D8 gave 0.5 1/kg: listed in CHANGES_1.5.md). `h / m_e v` is unchanged (the numerator is a name), and `1/2 m v²` with a mass m is the A1 question (D235) with the hint `½ m`. The D8 warning for `1/2x` is gone with the reading it warned about.
- **Two refinements found by the tests:**
  - **Dividing by exactly 1 is never a coefficient:** `k1 = 0.04 / 1 s` (per second; the stiff-solver tests, the van der Pol program) keeps meaning 0.04 1/s. Without this the Robertson and van der Pol programs changed value, which the spec says to stop and investigate.
  - **Another pure number after the denominator asks:** `4/3 π r³` (a sphere: (4/3)·π·r³) and `1/2π √(k/m)` (1/(2π)·√(k/m)) have the same shape and differ only in spacing, which must not change meaning. Either guess is silently wrong for one of them, so it's an error: *'4/3 π' is ambiguous: is it (4/3)·π or 4/(3 π)?* with both spellings. A bracketed denominator is always clear: `1/(2π) √(k/m)`.
- **Check:** every tracked .fm program prints exactly what Fermium 1 printed (outputs and warnings compared, tools as in D235).
- **Alternatives:** gluing `2π` to the denominator only when written without a space (reads both forms naturally, but is a spacing rule); reading `4/3 π` as (4/3)·π always (silently wrong for `1/2π`); keeping D8 (the friction: nine warnings in two graduate problems).

## D237. Error messages from real use (spec A3)
- **`ħ = c = 1`** (and `hbar = c = k_B = 1`): when every name in a chained `=` is a physical constant, the error says it is natural units and the hint is `units natural(ħ = c = 1)`. A chain of other names (`a = b = 1`) says Fermium gives one variable a value at a time. The `==` hint stays for other stray `=`.
- **An undefined `g`:** *g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: g = 9.81 m/s².* The eigenvalue hint ("the eigenvalue problem's states are g₀") came from the constant `g_0` looking like a state name; a hint now only names states that a program's eigenvalue problem made (constants are excluded).
- **Hint audit:** for a name of four letters or fewer, a "did you mean" match must start with the same letter and differ in length by at most one (or be the same letters in another order): `foo` no longer suggests `floor`, `tmep` still suggests `temp` (revised after red team 8 #19). The fallback hint says a value needs its own unit (`… = 2.5 m (with its own unit)`), where it used to read as if every value were a length. R4 (the `2*m` hint) went with D235, whose hints quote the number and unit as written.
- **Wrong kind of unit in `in`:** the hint names the kind of the value and 1–3 units of that kind, including products and ratios of two kinds (`units.suggest_units`): *h c is energy × length; try in J m or in eV nm*.
- **A terminal command at the REPL** (`fermium run ke.fm`, `ls`, `python3 …`, `pip …`) prints *this is the Fermium prompt; type :quit to go back to the terminal first*, unless the name is one of your variables or the line is an assignment.
- **Similar ones found by the triage:** a vector of uncertain values was an internal error (`UFloat.__round__`); it is now the clear "not supported yet" error (the feature is C7). `√(-4)`, `sqrt(-4)`, `log(-2)` and `factorial(-1)` written with a literal are compile-time errors (the square-root hint shows `√(-4 + 0i)`).
- **Not changed (recorded for Phase B):** a negative value computed at run time still gives NaN under √ and log, as in C and NumPy (Julia throws DomainError). A check in every square root touches three back ends and the hot loops of the benchmarks right before the freeze; the Rust compiler can add it natively (dev-notes/OPEN_ITEMS.md).
- **R1–R3, R5–R7** were already fixed in v1; tests/test_a3_messages.py keeps them.
## D250. `fermium doctor` prints one install command (spec A6.1)
- **What:** `doctor` lists every missing package on its own line (no per-package fix) and then prints one line to copy: `python3 -m pip install -e ".[full]"`, "in the fermium folder". It is printed once however many packages are missing, and never when nothing is missing. The other "needs X" messages (plot/animation skipped, `fit`, `using radau`, `fermium jupyter`, `fermium lsp`) give the same command.
- **Why:** every runtime package is a core dependency (D234), so that one command fixes any missing package, and a beginner shouldn't have to choose between five `pip install X` lines. If llvmlite is missing, the failed test program doesn't print a second fix (the install fixes it).
- **Kept:** a test program that fails for another reason still suggests reinstalling llvmlite; `use python X` keeps `pip install X` (X is the user's own module, not a Fermium dependency).
- **Alternatives:** one fix per package (what it did: five commands on a bare machine); `sys.executable -m pip` (exact, but not what the docs teach).

## D251. `plot saved to` prints the absolute path (spec A6.2)
- **What:** the compiled path and the interpreter (both use `Runtime.make_plot` and `m3rt.animate`) print `os.path.abspath` of the file actually written; `animation saved to …` and `… PNG frames in …` too. A `fermium build` executable writes relative to the folder it runs in (as before), and prints `getcwd()/name` for a relative name. The Jupyter kernel still replaces the line by the inline picture (it filters on the `plot saved to ` prefix, which is unchanged).
- **Why:** `to "gallery/p.png"` in `examples/01_pendulum.fm` printed `gallery/p.png` while the file was in `examples/gallery/` (the name is relative to the program's folder, not to where `fermium run` was typed). A full path is right wherever the program was run from.
- **Golden outputs:** the bootcamp's output boxes must be the same on every computer, so `bootcamp/update_outputs.py` (and with it `tests/test_bootcamp_outputs.py`) shows the repository folder as `/Users/ada/fermium`, the same stand-in it already used for "run as" blocks' temporary folder (`/Users/ada/fermium/bootcamp`). A box reads `plot saved to /Users/ada/fermium/bootcamp/pendulum_data.png`: what a reader on a Mac really sees, give or take the user name. Unit tests compare against `tmp_path / name`; the examples test checks the path is `examples/gallery/<file>`, absolute.
- **Alternatives:** printing a path relative to the current directory (right, but still ambiguous when the program is started from elsewhere, and different from what the spec asks); `~/…` abbreviation (shorter, but not a real path outside a shell); leaving the boxes unnormalised (machine-dependent, so the test would fail on every other computer).

## D252. The fitted curve over the data needs no new syntax (spec A6.3)
- **What:** `examples/01_pendulum.fm` now ends with `plot data.T vs data.L, 2π √(L / g) vs L from 20 cm to 120 cm to "gallery/pendulum_data.png"`: the data as dots and T = 2π √(L/g) with the fitted g as a line, in the data's units (cm, s). This already worked: a series may be a formula over a range, and in it `L` is the formula's own variable even though the program also has `L = 1.20 m`. Tested in matplotlib (both backends: 2 lines, dots + a 100+-point curve ending at 2π √(1.20 m / 9.818 m/s²)) and in the `fermium build` SVG (a marker path and a polyline).
- **Alternatives:** a `plot fit` shorthand that reuses the fit's model (less to type, but new syntax to freeze for v2 for something one line already says; can be added in C if John asks).

## D253. Axis labels name the column, and a formula next to a name stays in the legend (spec A6.4, BC-B22)
- **What:** a plotted `data.T` is labelled `T` (axis `T [s]`, legend `T`); inside a formula the data set's name is dropped too (`data.T^2` → `T²`, `model(data.t)` → `model(t)`). Only names bound to loaded data (or `table(...)`) are dropped: `r.x` of a vector solution keeps its dot. With several series, the y axis lists the distinct *named* series' labels and leaves formulas to the legend when at least one named series is there: data + fitted curve is `T [s]`, not `data.T [s], 2π √(L/g) [s]`; two named series stay `ys [s], zs [s]`; formulas alone stay on the axis. `fermium build`'s SVG does the same (the label is computed at build time and a formula's is left empty). The default file name (`data_T_vs_data_L.png`) is unchanged, so no program's output file moves.
- **Why:** John's report and BC-B22: the long two-series label `data.T [s], …` is noise; the column is the physics name.
- **Alternatives:** keeping `data.` in the legend (distinguishes two data sets with the same column name, which no program here has; `xlabel`/`ylabel` cover it); dropping every prefix before a dot (would turn `r.x` into `x`).

## D254. Research inputs checked against sources: Geiger–Marsden and ²⁰⁸Pb (spec A8.3)
- **Geiger–Marsden (research/rutherford_mc):** the 11 gold counts of Table II (*Phil. Mag.* 25, 604 (1913)) were checked against a typeset transcription of the paper (fisica.ufpr.br). All agree, and so does the paper's N sin⁴(φ/2) column. No number changed. The journal facsimile is paywalled, so the check is against the transcription; the README says so.
- **²⁰⁸Pb single-particle energies (research/shell_model_magic_numbers):** recomputed from AME2020 separation energies (IAEA AMDC `rct2_1.mas20.txt`: S_n(²⁰⁸Pb) = 7367.869 keV, S_n(²⁰⁹Pb) = 3937.373 keV) and ENSDF excitation energies of ²⁰⁷Pb and ²⁰⁹Pb (IAEA LiveChart API), rounded to 1 keV instead of 10 keV. Two values were off by 5–6 keV (4s1/2 −1.90 → −1.905, 2g7/2 −1.44 → −1.446 MeV); rms 0.476 → 0.478 MeV. A test recomputes the list from the cited numbers.
- **Convention kept:** each level is the lowest state of its spin and parity, as in the usual tables; the fragmentation of the strength (most for 1h9/2) is mentioned, not modelled.
- **Alternatives:** citing a textbook table (Ring & Schuck, B&M) instead of the evaluated data (not freely readable to check); centroids of the fragmented strength (needs spectroscopic factors per level, not in the scope of A8.3).

## D255. BBN rate coefficients checked against NUC123; one fixed (spec A8.3)
- **What:** the 11 nuclear rate fits in `research/bbn_network/bbn.fm` were compared with subroutine `rate2` of Kawano's NUC123 v4.1, the code implementing Smith, Kawano & Malaney 1993 (from a public copy of nuc123.f). Ten agree in every coefficient. ³He(α,γ)⁷Be's second term uses T₉/(1 + 0.1071 T₉) in NUC123; the from-memory version had 0.0495, CF88's scaling for their own different fit. Fixed in `bbn.fm` and in the test's NumPy model.
- **Effect:** ⁷Li/H 5.10191×10⁻¹⁰ → 4.36404×10⁻¹⁰ (SciPy, same network: 4.364051×10⁻¹⁰); Y_p, D/H and ³He/H unchanged in every printed digit. The README now says ⁷Li/H is 7 % below Fields 2020 and 22 % below PRIMAT, where the wrong value sat between them.
- **Attribution fixed:** ³He(α,γ)⁷Be and t(α,γ)⁷Li are the SKM93 forms, not CF88 as the README said (CF88's fits, checked on the CIAE mirror of the CF88 tables, are different one-term forms). ⁷Li(p,α) is CF88's ⁷Li(p,α) plus ⁷Li(p,γ)⁸Be, as in NUC123; the CF88 transcription and NUC123 differ in the sign of one coefficient of the (p,γ) part, which changes the rate by < 0.05 %; NUC123's sign is kept.
- **Not reached:** the SKM93 paper itself (ADS scan not downloadable as text) and Kawano's report (image scan). The check is against the authors' code; the README says so.
- **Alternatives:** switching to modern rates (PRIMAT/LUNA fits; a different reproduction, not a source check); keeping the old value because it looked closer to modern codes (that would be tuning to the answer).

## D260. `≈` is Julia's isapprox; `within` gives the tolerance; `≈ 0` without an absolute tolerance is an error (spec A8.1; supersedes D21's formula)
- **Rule:** `a ≈ b` is true when a == b, or when |a − b| ≤ max(atol, rtol·max(|a|, |b|)) and |a − b| is finite. Without `within`: rtol = 10⁻⁶ (D21's value, so every program away from zero prints what it printed) and atol = 0 (D21 had a hidden 10⁻³⁰⁰, which was 0 for any physical quantity). The `a == b` clause and the finiteness test are Julia's: `∞ ≈ ∞` is true, `∞ ≈ 1e308` is false (D21 said true: ∞ ≤ 10⁻⁶·∞), anything with NaN is false.
- **`within`:** `x ≈ 0 m/s within 1e-9 m/s` (ASCII `x ~= 0 m/s within 1e-9 m/s`). The tolerance states the whole test:
  - in the units of a and b (unit-checked: *the tolerance after 'within' must be velocity [m/s], like the values it compares, but it is time [s]*) it is absolute, and rtol becomes 0. Like Julia's `rtol = atol > 0 ? 0 : √eps`: `1000 m ≈ 1000.0005 m within 1e-9 m` must be false, since that is what the line says.
  - written as a percentage (`within 2%`, `within 2 percent`) it is relative and atol = 0. When a or b is itself shown in % (an efficiency), `within 1 %` is one percentage point: the same exception `x ± 3%` makes (D120), so % means one thing in both places.
  - A literal negative tolerance is an error. `within` is a word only right after the right-hand side of `≈`, and only when you haven't assigned a variable called `within` (the `absolute`/`until` rule of D111/D160): no new keyword, and no existing program used the name.
- **Literal zero:** comparing with a literal zero and no absolute tolerance is a compile error at the `≈`: *'v ≈ 0 m/s' is true only when v is exactly 0: ≈ allows a difference of 10⁻⁶ × the larger size, which is 0 here*, hint *give an absolute tolerance: write v ≈ 0 m/s within 1e-9 m/s (the difference you can accept)*. The fix repeats the program's own spelling and the zero's unit (`0 [m]` → `within 1e-9 [m]`). "Literal zero": `0`, `0.0`, `-0`, with any units (bracketed or not), or a vector literal whose components are all such zeros (`<0, 0> m/s`, `<0 m, 0 m>`); either side. `0 °C` and `0 °F` are not zero (273.15 K). `x ≈ 0 within 1%` is the same error (a percentage of 0 is 0). `0 ≈ 0` is allowed (true). A complex zero written as `0 + 0i` or `0i` is not recognised (rare; it gives the same never-true comparison as before). A variable that happens to be 0 is not checked: that is a run-time value, and it isn't what anyone writes when they mean "is this at rest".
- **Kinds:** numbers (|·|), vectors (Euclidean norms, as Julia's isapprox on arrays: a tiny component next to a large one doesn't make the vectors differ; the vectors must have the same length and one shared unit), complex numbers (moduli, D90's `c.approx` kernel with atol and rtol added). Lists and matrices stay an error ("must be a number"), as before; a list comparison would need a rule for its length and an answer that is one true/false.
- **Code:** the parser reads `within` and raises the zero error (`Parser._approx`, `_zero_literal`); the checker builds `IBuiltin("approx", [a, b, atol, rtol])` (complex: `c.approx` with the same two extra arguments); the formula is written once, in `cplx.approx_core`, against the ops objects, so native code, the interpreter and `fermium build` run the same operations in the same order (vector norms are summed left to right in both). `ICmp` no longer has a `~=` case.
- **Tests:** `tests/test_a8_approx.py` (every case through the JIT and the interpreter, and all of them in one `fermium build` executable). No existing test or program compared with a literal zero; none changed.
- **Alternatives:** a default atol (Julia has none; any fixed value is wrong for some unit: 10⁻⁹ m/s is huge for a drift velocity and tiny for a galaxy); a keyword `within` (would break a variable of that name); relative `within 1e-3` with no unit for dimensionless operands (ambiguous with an absolute tolerance on a pure number, so % marks relative); component-wise vector tolerance (the norm is what Julia and NumPy's `allclose`-on-norm users expect, and it is independent of the axes).

## D261. An ODE solution captured in an environment is stored as a pointer, not as a double's bits (spec A8.2; supersedes D48's mechanism)
- **What D48 did:** a nested function's environment is a `double*` buffer of 8-byte slots (numbers, vector components, booleans as 0/1). A solution handle (`SOLP`, a pointer to the solution struct) was written as `bitcast(ptrtoint(p), double)` and read back as `inttoptr(bitcast(load double))`. A user-space address such as 0x00007f3a12345678 read as a double is a subnormal, so any flush-to-zero/denormals-are-zero mode, a fast-math flag on an instruction that touched it, or a target whose FP loads canonicalise, could turn it into 0 (a null pointer).
- **Now:** the slot is written and read through a pointer-typed address: `store SOLP p, bitcast(&env[i], SOLP*)` and `load SOLP, bitcast(&env[i], SOLP*)`. The bits never live in a double SSA value or go through an FP load/store, so no FP mode can touch them. The buffer stays one array of 8-byte slots (a pointer fits in a slot on every 64-bit target, and on a 32-bit one it uses half), so `env_slots`, the heap copies of D46 and the parallel-for context are unchanged. Only solutions were affected: lists, texts and tables can't be captured (checker: "only numbers, vectors, matrices and ODE solutions").
- **Tests:** `tests/test_a8_handle.py`: the D48 programs (a root, an integral, a nested integral, a ratio of integrals, two solutions and a number captured together) print the right values; their IR (from `Program.llvm_ir` and `fermium run --emit-llvm`) has no ptrtoint → bitcast-to-double and no bitcast-double → inttoptr chain; and they print the same when every `load double` in the module is passed through a function that flushes subnormals to 0 (simulated FTZ/DAZ, compiled and run by the JIT in a subprocess). The old code fails both the IR check and the FTZ run (checked against the previous commit).
- **Speed:** no benchmark captures a solution, and the six compiled benchmarks (blackbody, forces, nbody, spring_adaptive, spring_rk4, unit_loop) generate byte-identical LLVM IR before and after, so their speed is unchanged by construction; nbody TIME_INNER spot check on a loaded machine: 0.061/0.066/0.084 s after vs 0.065/0.060/0.092 s before (noise). In a D48 program it is one store and one load per captured solution per call, as before, minus a ptrtoint/bitcast pair.
- **Alternatives:** an `{i8*, …}` struct for the environment (typed per capture: cleaner, but every lambda signature, the heap copies and the parallel context would change for one slot type); an i64 handle table in the runtime (an indirection and a lifetime to manage, for nothing a typed store doesn't give).

## D238. Red team round 8 fixes to the v1.5 rules (D235–D237)
- **Rates are not fractions (#1, #2):** a unit after the denominator makes a coefficient only when numerator and denominator are both whole-number literals: `1/2 kg` and `1/2 [kg]` are 0.5 kg (the spec's example), but `1 / 0.5 s`, `2π / 0.5 s` and `0.693 / 5730 yr` are rates again, as in Fermium 1. D236 had made them times.
- **Below the line (#4):** a number that is itself a denominator takes no reciprocal unit: `3 m / 2 / s` is "s isn't defined", not 1.5 m·s.
- **Every number-like value follows sentence 2 (#5, #6):** a power of ten (`10⁻² m`, `1.5 × 10⁻² m`) and a vector literal (`<1, 0> m`) with your own m ask, as a number and a list do.
- **Where-bindings (#7):** a binding's units see the bindings before it (`where g = 9.81 m/s², w = 2 g` asks).
- **`*` (#8):** a unit continues through spaces, `/` and `·`, never through `*`; `5 N*m` is 5 N times m (your m, or "m is a unit, not a value"). The three sentences say "compound unit", which is now exactly names joined by spaces, `/` or `·`.
- **After a bracket (#9):** a unit name after `)` or after `number variable` is read as a variable; if you have none of that name it is an error whose hint (and fix) is the bracketed unit: `(51 - 33 (N-Z)/A) [MeV]`, `100 h [km/s/Mpc]`; for `g` the hint adds `g_n`. `(m1 + m2) g` was silently grams. The integral's `ds` after a bracket is still the differential. A bracket that holds only plain numbers is a number, so `(3/4) kg`, `½ kg` (D241: `½` is `(1/2)`) and `(2 + 3) MeV` take their unit as a number does, and `(1/2) m v²` with your m is ½·m·v².
- **`J/kg K` (#12):** warns that only `kg` is below the line (the textbook meaning J/(kg K) needs the brackets); the reading is unchanged from Fermium 1.
- **fmt --fix (#3, #13):** a collision Fermium 1 already stopped on (`1/2 m v²`, `2 m c²` with a mass m) has no meaning to keep, so it is left and reported ("left for you to decide"); a collision followed by more unit (`2 kg m/s` with your m) brackets the whole unit.
- **Messages (#14–#17, #19–#21):** geometrized units (G = c = 1) say mass, length and time share one unit; a named kind comes first in conversion hints (`specific energy; try in J/kg`), plain numbers get no kind; `km/h` with your own h speaks of your h; short-name typos (`tmep`) are found again; `√(-4 m²)` is caught; hints keep a unit's exponent (`(9.81 m)/s²`); `[1, 2] m/s` with a mass m is the unit (the first name of a compound).
- **Docs (#22–#25):** the cheat sheet's solve uses `mass`; the missing-initial-condition hint writes `0.1 [m]`; the BBN README's ⁷Li sentence and the ²⁰⁸Pb table (−8.04) corrected; Lesson 2's rule is now true as stated.
- **Not changed (documented):** #10 `1/2L √(F/μ)` is (1/2)·L·√(F/μ) (a fraction of plain numbers is one coefficient; write `1/(2L)`), `1/π x` and `1/π² x` both divide as coefficients now; #11 `0.5 /s` with your own s divides by your s (after `/`, a name is a variable; the unit reading exists only for names you don't have); #18 `180/π deg` reads `deg` after π as a name (write `(180/π) [deg]`).

## D240. Preferred display units for common physics products: J m (spec A4.1)
- **What:** an energy times a length (kg m³/s²) prints in `J m`: `print h c` is `1.99×10⁻²⁵ J m`, `ħ c` and a Coulomb `k_e q²` too. Until now it fell through to the composite search (D11, friction #29), which tried N before J and wrote `N m²`. `J s` was already the preference for an action (`print h` is `6.63×10⁻³⁴ J s`).
- **eV nm and MeV fm only when asked:** `print h c in eV nm` → `1240 eV nm` and `print ħ c in MeV fm` → `197 MeV fm` keep working, and a value written in them keeps its unit (`x = 197.327 MeV fm` prints `197.327 MeV fm`, D11's written-unit rule). The default display stays SI, as for every other dimension (D11): a program that never mentions eV shouldn't answer in eV. In `units nuclear` (ħ = c = 1) ħc is a plain 1, so no preference is needed there.
- **"N m for torque when written that way":** a torque written in N m stays in N m (`τ = 5 N m`, `2 τ`, `r F in N m`), because the unit the user wrote is kept (D11) and N m is a unit kind of its own (`_unit_kind` 'torque', D95: adding J to N m warns). A *computed* force times a length prints in J: the compiler can't tell a torque r F from the work F d (same dimension, no vectors needed for either), and J is the right answer for the more common one. Showing `N m` for every product of a force and a length would print work in N m.
- **Alternatives:** a per-product rule (N × m → N m, J × m → J m) from the operands' display units (changes the display of many products — m g h would print kg m²/s² × … — and every golden output with it); `eV nm` as the default for h c (the atomic physicist's unit, but not SI and not what a mechanics program expects).

## D241. `½` means exactly `(1/2)`; `fmt --pretty` writes (1/2) as ½ (spec A4.2)
- **The glyphs:** the lexer reads every Unicode vulgar fraction: ½ ⅓ ⅔ ¼ ¾ ⅕ ⅖ ⅗ ⅘ ⅙ ⅚ ⅐ ⅛ ⅜ ⅝ ⅞ ⅑ ⅒ (⅖ ⅗ ⅘ ⅚ ⅐ ⅜ ⅝ ⅞ ⅑ ⅒ are new). One table (`_VULGAR_FRACS` in lexer.py) gives the value, the ASCII spelling and the reverse lookup.
- **½ is (1/2) in every respect**, which is what makes the formatter safe:
  - *Printing:* `print ½` gave `0.5` and `print ⅓` gave `0.333333333333333`, because a glyph counted as a literal "printed as written" (D11 exception 2) that can't be printed as written. Now a glyph is an exact number like `(1/2)`: `0.500`, `0.333` (the 3-figure default). `½ kg` prints `0.500 kg`, like `(1/2) kg`.
  - *A unit after it:* `½ kg` was an error ("kg isn't defined"), while `(1/2) kg` is 0.5 kg (D235: after a bracket, a unit name that isn't one of your names is a unit). Now a glyph follows the bracket rule: `½ kg` is 0.5 kg, and `½ m v²` with your mass m is still ½·m·v² (spec §1, unchanged).
- **The formatter:** `--pretty` replaces `(a/b)` by its glyph when a glyph exists and the bracket isn't one of: a call or an index (`f(1/2)`, `xs[1](1/2)`: the bracket belongs to the call), an exponent (`x^(1/3)` and `x^-(1/2)` stay: that is how people write roots, and an existing test asserts it), right after a number with no space (`2(1/2)` would become the mixed number `2½` = 2.5), or followed directly by a number. `√(1/2)` becomes `√½`. Only the plain spelling is replaced (`(01/2)` and `(2/4)` stay). `--ascii` writes a glyph back as `(1/2)`, with a space after a name so `x½` doesn't become the call `x(1/2)`, and `√½` as `sqrt(1/2)`. Round trips keep the AST and the output (tests/test_fmt.py: specific cases, eight programs compared AST- and output-wise, and the existing corpora, including the ASCII corpus's exact round trip of `E = (1/2) m v^2`).
- **`(0.5)` is left alone:** `0.5` is a measured value with 1 significant figure, `½` is exact: `print (0.5) kg` shows `0.5 kg`, `print ½ kg` shows `0.500 kg`. Rewriting it would change printed output (and the meaning of the precision), which fmt must never do (spec §1 item 4).
- **Alternatives:** keeping glyphs "as written" and teaching the printer to show `½` (one more output form for a number; lists and arithmetic would mix `½` and decimals); converting inside exponents too (`x^⅓` is legal but unusual, and it would change the documented behaviour for no gain).

## D242. Significant figures of list elements in loops, and "exactly" up to rounding (spec A4.3)
- **Loops over a written list:** `for E in [0.50 eV, 0.75 eV, 1 eV]` printed `0.5 eV` (the trailing zero lost: mixed precisions made the loop variable "as written" by value) and `2 E` printed `1 eV`. The loop variable now prints each element the way `print` shows the list (D11): the list's fewest significant figures where that shows the element exactly, else as written, and a whole number written without a decimal point exactly. So `0.50 eV`, `0.75 eV`, `1 eV`. `[0, 1, 1.5]` still prints `0`, `1`, `1.5`, and `[0.5, 0.75]` prints `0.75`, not `0.8`.
- **Display only:** values computed from the loop variable keep the rule they had (`2 E` prints `1 eV`, `1.50 eV`, `2 eV`). A first version gave the loop variable the list's fewest figures for arithmetic too; the full suite showed why not: the gauntlet's `for f in [0.25, 0.5, 3]` (exact multipliers of a period) then printed its positions to 2 figures instead of 3, because `0.5` counts as one significant figure. Mechanism: the symbol keeps `sf` as before and gets `list_sf` (the fewest figures) with `direct` 5 (whole items exact) or 4; the print format uses `list_sf`, and `format_quantity` (and `fmt_value_w` in aot_rt.c) print `direct` 4/5 with `format_written`, as the list printer does. `to(E, unit)` drops back to the literal rule, since `format_written`'s "as written" makes no sense in another unit.
- **"Exactly" allows for rounding in the last bits:** `format_written` compared the rounded text with the value bit for bit, so `1.20 mm` in a list (1.2000000000000002 after the trip through metres) printed as written, `1.2 mm`. It now accepts a relative difference of 10⁻¹³, as `_whole` does (D11). Mirrored in aot_rt.c.
- **Checked and left as they are:** `[20 °C, 30 °C] in K` → `[293, 303] K` (RT4-n2) is what the scalar `20 °C in K` gives: exact inputs, 3 figures (D11). A decimal-places rule for offset conversions (20.0 °C → 293.2 K) would be a new D11 rule for both scalars and lists, not a list fix. BC-B29 (`[1 hr, 10 min] in day` printed 16 figures) already prints `[0.0417, 0.00694] day`; a test now holds it. F-59/S13 (a Lorentz matrix from b = 0.6 prints γ = 1.25 as `1.2` and an exact 1 as `1.0`) is D11 working as designed: b has 1 significant figure, so results get 2, one style per matrix; `1.2` rather than `1.3` is Python's and C's round-half-even on the exactly representable 1.25. Changing either (per-element exactness in computed matrices, or round-half-up) is a D11 change that moves golden outputs, so it is listed for the main agent rather than done here.
- **Alternatives:** per-iteration figures chosen at run time from a table of the elements' figures (exact "as written" for every element, but a new runtime table in three back ends for a cosmetic difference); leaving the loop variable at 3 figures (hides the stated precision).

## D243. `fft(xs)` returns a list of complex numbers; `fft_re`, `fft_im`, `ifft(re, im)` are deprecated (spec A5)
- **What:** `fft(xs)` gives X_k = Σⱼ xⱼ e^(−2πi jk/n), k = 1 … n in Fermium's 1-based indexing (NumPy's unnormalised convention, as D81), as a list of complex numbers in the signal's units. `ifft(X)` inverts it ((1/n) Σ), also giving complex numbers. Both accept a real or a complex list, so `ifft(fft(xs))` gives the signal back as `[1 + 0i, 2 + 0i, …]` and `re(ifft(X))` as real numbers. `complex(re_list, im_list)` builds a complex list from two real ones.
- **A complex list (new type `ComplexListTy`, fermium/clist.py):** one unit for every element, like a complex number (D91). What works: `X[k]` (a complex number; bounds checked; `X[end]`), `len(X)`, `for z in X`, `print X` (`[10 + 0i, -2 + 2i, -2 + 0i, -2 - 2i] V`: each element as D94 prints a complex number, one number style for the list, shortened above 12 elements like a real list), and element by element `re`, `im`, `abs`/`|X|`, `arg` (real lists) and `conj`; `X.re`, `X.im`. Anything else (`sum(X)`, `X + Y`, `plot`) is an error that lists what works. That is enough for what an FFT is used for (a spectrum `abs(X)/n`, a phase `arg(X)`, filtering by element and transforming back); lists of complex numbers in general stay spec C1 (L-8).
- **Storage:** compiled code keeps the ordinary list header with 2n doubles, (re, im) interleaved, and the header's length set to 2n, so any code that copies or frees a list sees the whole block; each complex-list operation divides by 2. The interpreter keeps a list of (re, im) tuples, the pairs a complex number already is there. `fm_fft` got four kinds (5 fft of a real list, 6 ifft of a complex one, 7 fft of a complex one, 8 ifft of a real one), in NumPy (`runtime/spectral.py`) for `fermium run` and the interpreter and in C (`aot_rt.c`) for `fermium build`, which also got `fm_print_clist`. Tests compare with `numpy.fft` in all three.
- **Deprecation:** `fft_re(xs)`, `fft_im(xs)` and `ifft(re, im)` still work and give the same numbers, with a warning naming the replacement (`re(fft(xs))`, `im(fft(xs))`, `ifft(complex(re, im))`); they go after 1.5. `amplitude_spectrum`, `power_spectrum` and `frequencies` stay: they answer the physical question directly, with units (D81).
- **Alternatives:** a general list-of-complex type usable everywhere a list is (spec C1: sums, arithmetic, plots, functions; far larger); returning a 2-row matrix or a list of 2-vectors (not complex numbers, so `X[k] * 𝑖` wouldn't work); interleaved storage visible to the user (D81 rejected it as error-prone).

## D244. Spec A5: workarounds from before a feature existed, audited
- **Fixed here:** D81 (D243). **Already done, now tested:** `fit … with` continuing on the next line (examples #12) and lists of text (examples #7): tests/test_a5_consistency.py.
- **Not cheap or not safe here (one line each, for OPEN_ITEMS):** D18 fit standard errors as plain numbers without a ± in the program (making every fit uncertain forces the interpreter, D122: C7); D83 the PDE `i` found by probing with i = 1, −1, 2 (a rewrite of the PDE probe onto complex values: B, with the Rust port); D184 the bare `i` of legacy TDSE programs (belongs to A1's one-rule decision); D48 an ODE solution pointer stored in a double, and D21 `≈` near zero (both spec A8); D26 no garbage collector (B2); D122 uncertain programs only in the interpreter (C7); D82 eigenproblems without a ψ′ term, N ≤ 500 (C2); D29 the playground on the interpreter because Pyodide has no llvmlite (B7); D30/D35/D52 ∇ of an expression, shared-node vector quadrature, non-integer Bessel orders and incomplete elliptic integrals (C2/C6); D230 honest noise instead of zeroing (by design). D39's "not yet: `until` on the range line" is out of date: the parser reads `for t from 0 s to 9 s until y = 0 m` directly (parser.py, `until` in the range clause); the older product-stripping path in solve.py is left in place (harmless, and removing it is not needed for the freeze).

## D262. A degenerate fit is detected with a scale-free test, the same in fermium run and fermium build (macOS CI)
- **Problem:** the first macOS CI run failed `test_built_degenerate_fit_warns`: for `fit N = A B exp(-t/τ)` the built executable printed standard errors of 1.4×10⁷ instead of "the fit may not have converged". Both runtimes only noticed an *exactly* singular JᵀJ (a zero pivot in C, `np.linalg.inv` raising in Python); with A and B appearing only as a product, JᵀJ is singular in exact arithmetic but only nearly so after rounding, and macOS's rounding gave a tiny non-zero pivot.
- **What:** `fitting.degenerate` and `aot_data.c:degenerate` normalise JᵀJ to unit diagonal (a correlation matrix, so the test doesn't depend on the parameters' units or sizes) and call it degenerate when Gaussian elimination with partial pivoting meets a pivot below 1e-9 (1e-12 was not enough on macOS, where the C runtime's finite-difference Jacobian leaves proportional columns correlated just below 1; 1e-9 means |ρ| > 1 − 5×10⁻¹⁰, far beyond any fit whose standard errors mean something). Degenerate fits report "standard error could not be estimated" and the warning on every platform.
- **Alternatives:** a condition-number limit on JᵀJ itself (flags healthy fits whose parameters differ by 20 orders of magnitude, like an amplitude of 10²⁰ and a time of 10⁻³ s); SVD rank (the same idea, more code in C).

## D263. The v1.5 freeze point: a local tag and a branch (the tag can't be pushed from here)
- **What:** v1.5 is commit `ccdb288`, where CI passed on Linux and macOS (PR #2). It is tagged `v1.5` locally, but pushing tags from this session is refused (HTTP 403: the session's git access covers branches only), so the same commit is published as the branch `claude/v1.5-freeze`. Phase B's branch `claude/v2-rust` starts from that commit, as the spec asks ("created from the v1.5 tag").
- **For John:** `git tag -a v1.5 ccdb288 -m "Fermium 1.5" && git push origin v1.5` creates the tag on GitHub (or the GitHub web UI: Releases → Draft a new release → tag v1.5 on branch claude/v1.5-freeze).

## D264. The conformance runner compares strictly (red team round 9)
- **What:** `conformance/run` compares stdout exactly (spacing and punctuation included). The one tolerance: a number
written with a decimal point or a power of ten may differ by one unit in its last printed digit, and only when it
has the same shape (digits, decimal places, power of ten). Errors need the same message, line and hint; warnings
the same lines and messages; the exit code must match. The Rust binary runs on a copy of the program in an empty
temporary directory with an empty PATH. CONFORMANCE.md lists every failure.
- **Why:** red team round 9 built a fake implementation (every integer +1, every last digit changed, notation
rewritten, error words reversed) that scored 3034/3037 under the first runner: ±1 on integers, value-only number
comparison (so significant figures and notation were never scored), stripped brackets, and a 60 % bag-of-words
message match. `tests/test_conformance_suite.py` now checks that such fakes fail.
- **Alternatives:** keep a loose default and a strict mode (rejected: the scoreboard must be honest by default);
compare numbers by value with a relative tolerance in the numerics areas only (kept for later if a documented
divergence needs it; then it goes in DIVERGENCES.md with the case ids).

## D265. Documented divergences are checked, not just listed
- **What:** a deliberate difference from v1 (spec B2: v1 limitations fixed natively; or v1 behaviour that isn't
  deterministic) is a section of `rust/DIVERGENCES.md` plus, for each affected conformance case,
  `conformance/divergences/<id>.json` holding what the Rust implementation prints instead (written by
  `conformance/add_divergence.py` after reading the output). `conformance/run` counts such a case as a
  *documented divergence* only while the Rust output matches that file exactly; CONFORMANCE.md shows passes and
  documented divergences separately. The B8 gate ("100 %, or every failure is a documented divergence") reads
  the sum.
- **Why:** a divergence that is only listed could hide a later regression in the same case (the integral that
  gave the true value starts giving something else). Checking the recorded output keeps the scoreboard honest.
- **Not divergences:** features that aren't ported yet (e.g. `use python` until B5.14) stay failures.

## D266. The playground runs the Rust compiler as WebAssembly, through a C ABI without JS glue (B5.12)
- **What:** `rust/crates/fermium-wasm` (a cdylib for `wasm32-unknown-unknown`, built with `--profile wasm`: release,
fat LTO, stripped; 3.6 MB, 1.2 MB gzipped) runs parse → check → the tree-walking back end, as `fermium run
--backend interp`. It exports `alloc`/`dealloc`/`put_file`/`run_program`/`panic_message`/`partial_stdout` over
numbers and byte buffers; `run_program` returns length-prefixed JSON. `web/fermium.js` is the only JS that
talks to it (shared by the worker and the Node tests); each run instantiates the compiled module afresh, so the
runtime's per-process state (warnings shown once, RNG, the recursion check's base) behaves as in separate `fermium
run` processes. Files go through `fermium_runtime::vfs` (the file system natively, an in-memory map on wasm32), and
run-time warnings through `vfs::stderr_line` (captured by the driver). Shared crates change only behind
`cfg(target_arch = "wasm32")` (no threads for the parser's big stack, `clock()` from the page) plus the two
routings through `vfs`, which are identical natively.
- **Why:** one implementation everywhere (the page prints what `fermium run` prints; `web/test/compare_native.js`
checks every example in CI); a 3 MB download instead of Pyodide's ~50 MB; no Python/SciPy/SymPy loading on demand.
- **Alternatives:** wasm-bindgen (generated JS glue and a CLI tool pinned to the crate's version: more build-time
parts for six functions); `wasm32-wasip1` with a WASI shim for files and stderr (a JS shim or a dependency either
way, and std's WASI file layer for three files); keeping one instance across runs (needs every thread-local reset;
a trap in the middle of a run leaves Rust state inconsistent); the LLVM back end (LLVM doesn't run in the browser).

## D267. Distribution (B7): one plain binary per platform from a tag-triggered workflow; macOS CI only on PRs
- **What:** `.github/workflows/release.yml` runs on `v*` tags and by hand. It builds `cargo build --release -p
fermium-cli` on ubuntu-latest and macos-14, strips the binary and uploads it as `fermium-linux-x86_64` /
`fermium-macos-arm64` (plain files, not archives) plus `SHA256SUMS`; one `publish` job attaches them to the tag's
release (softprops/action-gh-release), so the two builds never race to create it. A hand-started run only keeps
workflow artifacts. In CI, the Rust compiler on macOS is its own job (`rust-macos`: build, `cargo test`, the whole
conformance suite against `conformance/RUST_FLOOR`) with the same gate as the Python `macos` job: pull requests
and workflow_dispatch only. `tests/test_workflows.py` loads the workflows with a duplicate-key-refusing YAML loader
and checks those gates. Lesson 0 now installs the binary (download, `~/bin`, PATH, Gatekeeper's quarantine,
`fermium doctor`), with the pip install of Fermium 1.5 kept in a labelled section until the B8 cutover.
- **Why:** "download one file, put it on your PATH" (spec §B7) is literal with a plain file; beginners don't have to
unpack anything. macOS minutes cost 10x on the private repo (CLAUDE.md rule 11). Separate macOS jobs run side by
side, so neither approaches its timeout. The Linux binary is built on ubuntu-latest, so it needs a glibc as new
as the runner's.
- **Alternatives:** .tar.gz archives (keep the executable bit, but one more step for beginners); a Homebrew tap or
an install script (later: needs a public download URL); building Linux on an older image for an older glibc (the
apt LLVM 18 path is only tested on ubuntu-latest); running the Rust steps inside the existing macOS job (over its
45-minute timeout with make check).

## D268. PDE time steps: a tridiagonal LU, not a port of SuperLU and OpenBLAS (documented divergence)
- **What:** the Rust PDE solver factors each implicit step's tridiagonal matrix with a plain LU. v1 used SciPy's `splu`. Three conformance cases differ at the rounding level: two in the 14th–15th digit, and one noise-around-zero value. They are recorded as divergences (rust/DIVERGENCES.md, "PDE linear solves").
- **Why:** both are exact to rounding; the differences are invisible at printed precision unless a program prints 14+ digits or a pure rounding-noise value. A bit-for-bit port would mean SuperLU's column elimination with its row and column pivot order, plus OpenBLAS's FMA arithmetic in the two blocks SuperLU hands to BLAS. That is several hours of work, a fragile dependence on library internals, and it would slow the solver.
- **Alternatives:** port SuperLU and model OpenBLAS (rejected, as above); call a system SuperLU (rejected: spec §B6 wants no runtime dependencies).

## D269. The B8 cutover: the Rust binary is `fermium`; Fermium 1.5 moves to legacy/ as `fermium-legacy`
- **What:** the Python implementation moved with `git mv` to `legacy/fermium` (the package), `legacy/tests` (its
  pytest suite, test programs and John's Appendix 1 programs) and `legacy/tools` (the 1.5 migration scripts).
  `pyproject.toml` maps the package from `legacy/` (`package-dir = {"" = "legacy"}`), so
  `pip install -e ".[full,dev]"` still installs it under the import name `fermium` (conformance/run --impl legacy,
  conformance/harvest.py, rust/tools/*.py and the legacy tests keep working); its console script is renamed
  `fermium-legacy` (`python3 -m fermium` still works) and its output is untouched, since the conformance goldens
  come from it. `fermium` is the Rust binary: the release download (bootcamp Lesson 0) or `make install`
  (`cargo install --locked --path rust/crates/fermium-cli`). Tests and scripts say which one they mean: the v1
  tests run `fermium-legacy` or `python3 -m fermium`; benchmarks/run.py, the VS Code extension and the docs mean
  the Rust binary. `make check` (check.sh) runs (a) ruff and the legacy pytest suite, (b) `cargo build` and
  `cargo test` (profile fast; plus fermium-pyapi and fermium-wasm), (c) the whole conformance suite against
  `rust/target/fast/fermium` with `--min conformance/RUST_FLOOR`, writing its report to a temporary file. Every
  docs, bootcamp, examples, gauntlet and research program is harvested into the suite, so (c) is what checks them
  against the Rust binary. `FERMIUM_SKIP_RUST=1` skips (b) and (c), and a missing cargo skips them; both print
  that they were skipped. In CI the legacy jobs (`linux`, `macos`, named "legacy (Fermium 1.5, deprecated)") run
  make check with `FERMIUM_SKIP_RUST=1`, because the `rust-linux` / `rust-macos` jobs already build, test and
  score the binary; the macOS gate (pull requests and hand-started runs only) and the concurrency rule are
  unchanged. Conformance cases keep their folder names `tests/programs[/john]` (a case's id hashes its folder,
  and one case prints a path in it): `conformance/run` reads those folders from `legacy/tests/…` and
  `conformance/harvest.py` writes `legacy/tests/…` back as `tests/…`, so ids and goldens don't change. The Rust
  build already finds the standard library and the unit database under `legacy/fermium/` (build.rs).
- **Why:** spec §B8 (Rust at 99.7 % with every other failure a documented divergence): the Rust binary becomes
  *the* fermium, and legacy/ stays in CI for one more phase, deprecated. `git mv` keeps the history (`git log
  --follow`). Keeping the import name means nothing that reads the oracle has to change, and a different console
  script name means a machine can have both without one shadowing the other.
- **Alternatives:** delete the Python implementation now (rejected: it is the conformance oracle, and the spec
  keeps it one more phase); keep it at the top level and only rename the script (rejected: the layout would
  still present it as the compiler); rename the package to `fermium_legacy` (rejected: every oracle reader,
  the harvest and ~120 test files import `fermium`, and a rename risks changing v1's output, e.g. module names
  in messages); re-harvest the suite with `legacy/tests/…` folders (rejected: new ids and one changed golden for
  no behaviour change); run the Rust part of make check in the legacy CI job too (rejected: duplicates the
  rust-linux job's 20+ minutes of build and conformance).

## D270. The tree-walker remembers pure calls within one ODE right-hand side
- **What:** while `ode_call` evaluates a `solve`'s equations once, a call to a function that only computes
(numbers in, a number out; assignments to its own locals, if, loops, return, arithmetic, `where`, pure math
built-ins, integrals and sums, calls of such functions) and that integrates or sums somewhere returns the number
remembered from an identical earlier call in the same evaluation (`fermium-codegen/src/eval_memo.rs`). The cache
is emptied for every evaluation and after any call of a function that may do more than compute; Monte Carlo
propagation turns it off.
- **Why:** research/bbn_network (conformance 6de08e3389a9) calls n_b(T), which integrates the e± plasma twice,
from each of 42 rate terms: ~100 quadratures per evaluation where 5 are distinct. Under the tree-walker a step
took 250 ms against v1's 5 ms (v1 compiles the right-hand side), and the program ran 25 minutes; with the cache
it runs in about 200 s with exactly the same steps and numbers (a hit returns the bits the call would compute; a
run-time warning is shown once per text anyway).
- **Alternatives:** compile the right-hand side (the LLVM back end; the program has a plot, which the tree-walker
had to run); common-subexpression elimination in the checker (changes the IR both back ends see); a cache kept
for the whole solve (more hits in the Jacobian columns, but must prove no global can change between evaluations).


## D271. Platform maths libraries: goldens are glibc's; macOS is compared within a few ulp, with listed cases skipped
- **What:** the numerics fixtures and the conformance goldens come from Linux (glibc's libm, and OpenBLAS's SkylakeX kernels for v1's linear algebra). On a platform with another libm (macOS), the Rust fixture tests assert bit-identity only where the libm is the fixtures' (`FIXTURE_LIBM`); elsewhere they compare within a few ulp (ODE trajectories at the solvers' accuracy, noisy dense-output derivatives within 1e-3). The macOS conformance job skips the 28 cases in `conformance/libm_sensitive.txt`, whose printed last digits or rounding-noise values differ with Apple's libm. The runner lowers the floor by the number skipped and names them in the report. CI's legacy job pins `OPENBLAS_CORETYPE=SkylakeX` on AVX-512 runners, and otherwise skips `conformance/blas_sensitive.txt`.
- **Why:** these values are the platform's C library, not Fermium: v1 on macOS differs from the Linux goldens the same way. Printing 17 digits or a value that is pure rounding noise exposes the last bits of `sin`/`exp`. Linux keeps the strict, bit-for-bit claim.
- **Alternatives:** a bundled correctly-rounded libm (e.g. CORE-MATH) for bit-identical results everywhere. That is the better long-term fix, but it would change digits against v1's goldens on Linux too, so it is a v2.5 candidate (BACKLOG). A separate macOS golden set was rejected: it would need v1 on macOS to regenerate, and a second truth to maintain.

## D272. Phase C work starts in agent branches once B8's criteria are met; nothing merges before the v2.0 tag
- **What:** the B8 gate's criteria are met: 3366/3366 pass or are documented divergences, CI is green on Linux and macOS (PR #3, run 127), the cutover is done, and docs/architecture.md is written. What remains is the B8.3 benchmark re-run and the tag. Phase C items start in agent worktrees now, based on claude/v2-rust. They are merged into `claude/v2.5` (branched from the v2.0 tag) only after the tag, each through tests and the full conformance suite.
- **Why:** the end time is fixed. Waiting two idle hours for a benchmark run would waste time the plan needs, and the gate's purpose (don't grow the language on a compiler that doesn't yet match v1) is already served.
- **Alternatives:** start Phase C only after the tag (strict ordering, but idle time); merge Phase C work into claude/v2-rust (rejected: v2.0 must be the v1-compatible cut).

## D273. The LLVM back end's inner loops: facts found at compile time instead of per-iteration checks
- **What:** (1) *Module constants* (`llvm/consts.rs`): a module variable set exactly once, at the top of the main
block before any user function is called, to a value known then, is that constant wherever the compiled code
reads it (`m = 1 kg`, `N = 1000000`); a list bound once to a list of known length and used only by indexing, `len`
and `for … in` keeps that length (`len(mass)` is 5). The slots are still written, so the tree-walker's constructs
see the same values. The main block is compiled before the functions so they see these constants too.
(2) *Integer trip counts* for `for i from a to b` with whole a, b and step ±1 (the same number as the
floating-point formula below 2⁵³), integer comparisons of integer loop variables, and integer loop variables in
`parallel for` bodies. (3) *Versioned loops* (`hoist.rs versioned_loop`): a small straight-line loop body
(no inner loop, call, integrand or solve) whose indexes are integer loop variables is compiled twice; one test
before the loop checks every such index for the first and last value, then the copy without the checks runs,
else the checked copy (so a program that fails still fails with the same message and line). (4) *Owned lists*: a
list that is bound once to a new list and only ever indexed gets a TBAA type of its own, so a store into `vx`
doesn't make LLVM reload `x` or `mass` (v1's lists were separate globals, which gave LLVM the same fact).
(5) *Compiled fixed-step RK4* (`ode.rs rk4_inline`): a `solve … step h` without `until` takes its steps in the
module with the right side called directly (inlined) and the state in registers, `ode::rk4_plain` operation for
operation; fermium-runtime does the checks, the first derivative, the error estimate and the solution object
(fm_rk4_begin / fm_rk4_end; the solution under construction is boxed in the loop's plan, so a right side that
itself solves an equation is safe). (6) The quadrature's sentinel cache uses a cheap hasher instead of SipHash.
- **Why:** PERF.md showed v2's compiled inner loops 1.2–2.4× slower than v1's on nbody, spring_rk4, forces and
blackbody. Every change keeps the printed results bit for bit (llvm_diff, full conformance).
- **Alternatives:** a Gauss–Kronrod panel compiled into the module with the integrand inlined, as v1 did
(tried: only ~15% fewer instructions per integral on blackbody, the rest being the adaptive bookkeeping and
`exp`, and ~0.3 s more compile time for the 15 inlined copies; rejected); `default<O3>` (no measurable gain on
the benchmarks, 3–30 ms more compile time: only the `FERMIUM_LLVM_PASSES` experiment switch); fast-math flags
(they change results: never).

## D274. v2.0 is tagged locally and marked by the branch claude/v2.0-freeze; the release binaries wait for the owner
- **What:** `v2.0` is an annotated tag on f61b8e6 (the version bump to 2.0.0 after make check passed: legacy 3968 passed, all Rust tests, conformance 3334 + 32 documented; PR #3 green on Linux and macOS). The session's GitHub token refuses tag pushes (HTTP 403, as for v1.5, D263), so the branch `claude/v2.0-freeze` marks the commit. `claude/v2.5`, the Phase C branch, starts there. The release workflow publishes binaries only on a `v*` tag push. A manual run (workflow_dispatch) returns 404, because GitHub dispatches only workflows on the default branch, and release.yml isn't on `main` yet.
- **Why:** the same constraint and remedy as v1.5. A branch keeps the exact commit, and the owner can create the tag from it with `git tag -a v2.0 origin/claude/v2.0-freeze && git push origin v2.0`, which also runs release.yml and attaches the binaries.
- **Alternatives:** none available from this session (tag pushes and dispatch are refused). Until the release exists, Lesson 0's download instructions point at a release that hasn't been published; CHANGES_2.0 and the README say how to build from source meanwhile.

## D275. C and Fortran interop (C3): use python's signatures, a trampoline without libffi, checked at compile time
- **What:** `import c "libphys.so":` and `import fortran "libnuclear.so":` followed by one signature per line, in
the syntax of `use python` (D140): `kinetic_energy(m [kg], v [km/s]) -> [J]`, `twice(n: int) -> int`, plus
`x: list [m]` with `n: len(x)` for arrays of doubles, and `bind(C)` / `bind(C, name="…")` after a Fortran
function's result. The spec's `m: kg` form gets the existing error pointing to `m [kg]`. Functions are called by
their bare name; each argument's dimension is checked at every call (generic functions per call); values cross
as SI ÷ the declared unit's factor, the result is multiplied back. The result must be declared (`-> [unit]`,
`-> number`, `-> int`). Fortran passes everything by reference; the default symbol is lowercase plus a trailing
underscore (gfortran, flang), `bind(C)` is lowercase without it, `bind(C, name=…)` is exact. Greek letters and
subscripts in a name map to ASCII (`δ_e` → `delta_e`, `v₀` → `v_0`) so `fmt --pretty/--ascii` round-trips keep
the symbol. The library is dlopen'd at check time relative to the program's folder (a bare name it doesn't have
is left to the system's search), and every symbol is looked up then: a missing library or symbol is a compile
error with a hint (naming the other Fortran underscore spelling when the library has it). Run time:
`fermium-runtime/src/cffi.rs` classifies the arguments (double / int / pointer) into the platform's registers and
8-byte stack slots and calls the function pointer as an `extern "C" fn` taking every integer register, every
floating-point register and ten stack slots (x86-64 SysV: 6 + 8; AArch64: 8 + 8; up to 16 arguments; Apple's
AArch64 packs small stack arguments, so a spilled `int` is refused there). The tree-walker converts in
`eval_c.rs`; the LLVM JIT calls the function directly when every argument and the result are doubles (≈5 ns per
call), otherwise through the built-in callback (≈0.5 µs); `fermium build` executables use the callback and the
library's path as resolved at build time. A list passed to a number parameter maps elementwise.
- **Why:** one signature language for every foreign function (the user learns it once), with the unit check
where the value crosses the boundary, which is the whole point of C3. Resolving the library at check time makes
a typo a compile error, like a misspelt Python function. Only doubles, ints and pointers are needed for the
physics routines this is for, and for those the C ABI is simple enough to reach from Rust alone, so there is no
libffi dependency (the binary stays one file with no shared-library dependencies beyond libc).
- **Alternatives:** libffi (general, but a C dependency to build and ship on every platform); generating and
  compiling a C shim per program (needs a C compiler at run time, which Fermium 2 has avoided since B7); the
  spec's `m: kg` parameter syntax (rejected: it would give two spellings of the same thing, and `: int` already
  means a type after the colon); passing Fortran's hidden string lengths or `value` arguments (not needed yet).
  Known gaps: output arrays, `float`/`long`, structs, strings, callbacks, void functions, `import c` in modules or
  calls inside `units natural`, the playground.

## D276. Uncertain values through integrals: exact derivatives under the integral sign and by the limits (spec C7)
- **What:** `∫ f dx from a to b` whose integrand reads uncertain values, or whose limits are uncertain, gives an
uncertain value instead of v1's "an integral can't use uncertain values (±) yet". Linear propagation with exact
derivatives: each uncertain value carries its contributions c_k = ∂x/∂z_k (one per independent source k), and the
tree-walker's arithmetic on those values is forward-mode differentiation, so the integrand evaluated at a plain x
gives ∂f/∂z_k there. Then ∂I/∂z_k = ∫ ∂f/∂z_k dx (one extra quadrature per source, rtol 10⁻¹⁰, an absolute
tolerance of 10⁻¹² × the largest |∂f/∂z_k| at a, b and the midpoint × the width) + f(b) ∂b/∂z_k − f(a) ∂a/∂z_k.
Correlations are kept: `∫ x² dx from 0 to a` minus `a³/3` is exactly 0 ± 0. An integrand that doesn't depend on x
with uncertain limits keeps v1's path (the result is linear in b − a). A `±` written inside the integrand (or an
ODE's right side) is one measurement for the whole kernel: its value is kept per IR node while the kernel runs
(otherwise each evaluation would be a new independent source). Code: `fermium-codegen/src/eval_unc_kern.rs`.
- **Why:** exact (no step to choose), cheap (K + 1 quadratures for K sources), and it reuses the propagation every
operator already has. Differentiation under the integral sign needs f and ∂f/∂z continuous in x on the range,
which holds for the integrands the quadrature handles anyway.
- **Alternatives:** central differences in each source with a step (rejected: a step to choose, and the adaptive
quadrature's node changes make the difference noisy); a vector quadrature of (f, ∂f/∂z_1, …) on one set of nodes
(the runtime's quadrature is scalar; left for later); always Monte Carlo (slow, and noisy where linear is exact).

## D277. Uncertain values through ODEs: the sensitivity (variational) equations solved alongside, by the same solver
- **What:** `solve` with uncertain starting values, an uncertain start time, or a right side that reads uncertain
values solves the state y (n components) together with S_k = ∂y/∂z_k for each source k: dS_k/dt = J S_k + ∂f/∂z_k.
The right side is evaluated once per step stage on uncertain numbers whose contributions are the S_k, which gives
f and J S_k + ∂f/∂z_k at once (forward-mode differentiation; no Jacobian is formed). S_k(t₀) = ∂y₀/∂z_k −
f(t₀, y₀) ∂t₀/∂z_k. The augmented system (n(1 + K) components) goes through the chosen solver (RK45, RK4, Radau,
BDF) with its step control over all components; the relative tolerance is divided by √(1 + K) because the error norm
is an RMS over all components, so the nominal part keeps its accuracy. `y(t)` is the interpolated value with the
interpolated S_k as contributions (correlations kept: `x(1 s) / x0` is exactly ± 0 for x' = −x); `y'(t)` evaluates
the augmented right side at the interpolated state; `values(y)` is a list of uncertain values; an uncertain query
time adds the slope times its uncertainty. The event time of `until` stays a plain number (the nominal crossing),
and `max`/`min` of a solution and plots use the nominal solution. An uncertain end time is used at its value.
- **Why:** the textbook method (it is what "linear error propagation through an ODE" means), accurate to the
solver's tolerance, and one solve instead of 2K + 1 finite-difference solves.
- **Alternatives:** finite differences of whole solves (step choice, adaptive-step noise); forming ∂f/∂y by finite
differences (a step again); Monte Carlo only (see D278).

## D278. When linear propagation isn't valid: a ±1σ test per source, then Monte Carlo (integrals and ODEs)
- **What:** after the linear result, each source k is moved to z_k = +1 and −1 (every uncertain input read as its
value ± its contribution c_k, through the Monte Carlo sampler with fixed streams) and the kernel recomputed with
plain numbers. With y₀ the linear result's value, d₁ = (y₊ − y₋)/2 and d₂ = (y₊ + y₋ − 2 y₀)/2, linear
propagation is accepted when |d₂| ≤ 0.1 |d₁| (the second-order change over ±1σ is at most 10 % of the first-order
one) or |d₂| is below the kernel's own accuracy (10³ × the quadrature's error estimate; 100 × rtol × the component's
largest |y| for an ODE, at 8 times across the solution). Otherwise the result comes from Monte Carlo, with a
warning: 10 000 samples for an integral, 2 000 whole solves for an ODE (each sample draws every source from the
program's seeded random numbers, so a run is reproducible and `seed(n)` changes it). The result is linked to the
sources by the same regression as `propagate montecarlo` (value at z = 0, one contribution per source, the
nonlinear rest as a new source; for an ODE the same new source at each (component, t) asked twice). A kernel whose
integrand or right side needs a plain number (an uncertain loop bound, an index) goes to Monte Carlo directly.
Kernels nested inside another kernel's linear pass (an integral inside an ODE's right side) propagate linearly
without their own test.
- **Why:** first-order propagation is wrong exactly where the slope vanishes or the function bends strongly over
±1σ (a peak centred in an integration range, a decay rate known to ±40 %). Testing at ±1σ is the scale the result
describes; it costs 2K extra kernel runs. The test misses cross terms between sources (∂²/∂z_i∂z_j) and nonlinearity
between the 8 test times of an ODE.
- **Alternatives:** always Monte Carlo (slow, noisy); only a warning pointing at `propagate montecarlo` (the user
asked for a result); a second-order (Hessian) correction (needs second derivatives; doesn't cover strongly
non-Gaussian outputs).

## D279. Vectors, matrices and lists of uncertain values print and work in every operation (spec C7)
- **What:** `<1.0 ± 0.1, 2.0 ± 0.2> m` is a vector of uncertain values (v1: "needs a plain number"). Printing: one
unit for the vector or matrix, each uncertain entry rounded as a single uncertain number (σ to 2 figures), plain
entries with up to 6 figures as in lists of uncertain values; a state vector prints each component with its unit;
an entry whose value and σ are both below 10⁻¹⁴ of the largest entry is rounding noise and prints `0 ± 0` (D197).
`norm`/`|v|`, `unit`, `abs`, `≈` (on values), `len`, components, `·`, `×`, `det`, `inverse`, `solve_linear`,
matrix products and transposes propagate with correlations. For lists, `std` (the spread, with its uncertainty
propagated) and `interp` (segment chosen by the values) now work; the other list functions already did.
- **Why:** spec C7. Plain 6-figure entries follow v1's rule for lists of uncertain values, so a list and a vector
look alike.
- **Alternatives:** a common exponent for the whole vector (harder to read when entries differ in size).

## D285. Multiple dispatch (C5): versions chosen at compile time by arity, dimensions and kinds
- **What:** a second top-level definition of a function name with a different signature adds a *version*
  (`energy(m [kg], v [m/s])`, `energy(λ [m])`, `energy(f [Hz])`); a call uses the version its checked argument
  types fit. A parameter can now also name a kind, `r: vector [m]` (`number`, `vector`, `list`, `complex`),
  in the `x: list [m]` syntax of `import c` (D275). Fit: the same number of arguments; a `[unit]` needs that
  dimension (a number, list or complex; a vector only with `: vector`, every component); a kind needs that kind (a
  list also fits `: number`, element by element); an unannotated parameter takes anything; a function argument
  fits only an unannotated parameter the body calls, and a value never fits a parameter the body surely calls.
  *Specificity* per parameter: 0 unannotated, +1 for a kind, +1 for a unit (a list in a `: number` slot scores
  the kind 0). The chosen version must be at least as specific as every other fitting version in every parameter
  and more specific in one; otherwise the call is ambiguous: a compile-time error naming two such versions with
  their lines. No fit: a one-line error (`no version of energy takes (time [s])`, or `energy takes 1 or 2
  arguments`) whose hint lists every version with its line. A *same signature* (arity, kinds, dimensions of the
  units) replaces the earlier version, which is exactly v1's redefinition (`f(x) = 2 x` then `f(x) = 3 x`).
- **Mechanism:** `FuncInfo.versions` (fermium-check `dispatch.rs`): each new definition gets a new list, a
  snapshot of the versions visible there, so a function passed or bound earlier keeps what it meant. `call_user`
  and `instantiate` call `pick_version` first; the chosen version is instantiated exactly like any function
  (one IR function per version and argument types), so the IR, both back ends and `fermium build` are
  unchanged and the choice costs nothing at run time. Inside a generic function the argument types are concrete
  per instance, so each instance chooses again. When a dimension isn't known yet (the generic check of a function
  never called), several fitting versions give "can't tell which version", which that check ignores. `f'`,
  `d/dx f` and `∂/∂x f` of a function with versions are a new function whose versions are the derivatives of
  the versions that have that parameter (or one parameter for `'`); plotting, `∫`, passing to functions and
  `module.name` go through the same calls. Every version is checked when never called (`check_uncalled`) and
  gets its module's name. The LSP records each call's choice (`Checker.dispatch_sites`), so hover over a call
  shows the version used there; `print f` and the REPL's `vars` list all versions.
- **Why:** it is Julia's core idea (spec C5), and in a units language the dimension is the natural thing to
  dispatch on: the photon energy from a wavelength or a frequency is one name, as on paper. Choosing at compile
  time keeps the zero-cost promise and needs no run-time type tags. Before C5, a redefinition silently replaced
  the function; no conformance program depends on that for a *different* signature (the full suite is
  unchanged, 3334 + 32), and keeping the replacement for the same signature keeps every v1 program the same.
- **Alternatives:** Julia's rule of the most specific *type* with a type lattice (Fermium has only a handful of
  kinds, and dimensions are exact, so per-parameter scores suffice); detecting ambiguity when the definitions are
  made (it would reject pairs that no call ever makes ambiguous; the call-time error names both anyway);
  run-time dispatch (not needed: every type is known at compile time); letting a unit annotation outrank a kind
  (arbitrary; equal scores are reported as ambiguous instead). Not yet: differentiating a formula that calls a
  function with versions, versions added to an imported function (still "defined again" as before), the Python
  API's `fermium.compile` (checks arguments against the last version), and Fermium 1.5.

## D280. The LLVM back end frees lists: a mark-and-sweep collector with frame epochs (spec C1, memory)
- **What:** the compiled code's lists, text lists, texts made at run time and Obj values (the tree-walker's values it holds: lists of
complex numbers or vectors, data sets) are registered when made and freed by a collector (`llvm/rt.rs`, memory
section; code generation in `llvm/gc.rs`). Roots are variable slots: fm_main registers the module's list variables
and its own slots, and every compiled function whose loops may make lists registers its list slots on entry
(`fm_gc_enter`: an array of slot addresses on its stack) and unregisters on each return. Collections happen only at
safe points, the top of an iteration of such a loop when the allocator has set a flag (`gc_flag`, one load and a
cold branch), where the function holds no list in a register (a `for … in` keeps its copy of the list in a hidden
registered slot). A collection frees only lists made after the innermost registered frame was entered (its
epoch): a caller in the middle of an expression may hold a temporary (`f(2 xs, g(y))` while g loops), and those
are all older. The main program's frame has epoch 0. A function without such loops is never interrupted by a
collection, so it needs no frame; nothing is collected while a parallel for runs. A collection runs after as many
allocations (or bytes) as were live after the last one, at least 20 000 lists or 64 MB. `FERMIUM_GC_STATS=1`
reports collections and peak memory; `FERMIUM_GC_STRESS=1` collects at every safe point (the whole conformance
suite agrees with the tree-walker under it: `rust/tools/llvm_diff.py --bin` a wrapper that sets it).
- **Why:** the tree-walker's lists were already reference counted (DIVERGENCES "Lists are freed"), but the LLVM
back end kept every list until the program ended, like v1's compiled code: a loop making 3×10⁶ small lists
reached 300 MB (now 52 MB; `memory_loop.fm`, 10⁶ lists of 100 numbers, peaks at ~76 MB instead of ~800 MB).
- **Alternatives:** reference counting in the generated code (retain/release on every store, argument and
temporary: much more code in compile.rs, and a cost on every list operation); a conservative scan of the machine
stack (not portable, and LLVM may keep only derived pointers); an arena per statement (lists stored in variables
outlive statements). Texts made at run time (`"run " + str(i)`) are collected the same way: text-id slots are roots
(kind 4), the ids in surviving text lists are marked, and a freed id is reused (the module's own texts are never
freed); 2×10⁶ labels in a loop stay at ~55 MB.

## D281. Lists of vectors, matrices, complex numbers and text (spec C1)
- **What:** a list may hold vectors (all the same length and units: `[<1, 2> m, <3, 4> m]`, type `VList`), matrices
(same size and units: `[[[1, 0], [0, 1]], [[0, 1], [1, 0]]] N/m`, or `[A, B]`), complex numbers (`[1 + 2i, 3i]`,
the existing `ComplexList`) and text. For each: written out, `push`, `xs[i]`, `xs[end]`, `xs[i] = …` (and `+=`),
`len`, `for x in xs`, `clear`, `print`; a unit after the list applies to every element; a list of vectors or
matrices times or over a number; `sum` and `mean` of a list of vectors or matrices. A variable set to `[]` becomes
the kind of the first value pushed (`vel = []` then `push(vel, <1, 0> m/s)`), checked from then on (units per
element type, sizes). The tree-walker holds these as values (`Value::VList`, `CList`, `TextList`); the LLVM back end
holds lists of vectors and complex numbers as Obj values and hands the statements and expressions that use them to
the tree-walker (mixed mode), so the rest of the program stays compiled (N-body code over lists of vectors runs
compiled apart from those accesses).
- **Why:** reaction networks and N-body problems are written as loops over lists of state vectors; before this,
users kept one list per component.
- **Kept from v1:** a list mixing real and complex numbers (`[1i, 2]`) is still the error "a list element must be
a number, but it is a complex number" (conformance golden ae4d6aeeae8f); write `2 + 0i`. A list of vectors with a
different unit per component is refused (put each component in its own list).
- **Alternatives:** a general `List(Ty)` of any element type (nested lists, lists of lists): more checker
surface than C1 needs; N-dimensional arrays (D283) cover grids. Compiling list-of-vector operations in LLVM
natively (a flat buffer with a stride) is the performance follow-up.

## D282. `solve` with a list of unknowns (spec C1)
- **What:** an unknown whose initial value is a list of numbers or of vectors (one unit for all elements) is a list
unknown: its state variables in the right-hand side are lists (`List`, `VList`), and its size is the initial value's
length when the solve runs. The checker puts list unknowns' state slots last in the layout (`SolveExtra::lists`),
so the other unknowns keep offsets known at compile time; a list unknown's `SolView` holds positions in the layout
(`SolView::list`) and `N(t)` becomes the built-in `__sol_list(solution, slot, t, derivative?, k, format)`, which
finds the offset from the sizes stored with the solution (`SolData::lens`). The tree-walker flattens the initial
values, records each list state variable's size for the right side's evaluations, and checks that the right side
returns as many numbers as the unknowns hold. `absolute` gives one value per unknown's units; a list unknown's value
is repeated over its elements. The LLVM back end hands the whole `solve` statement to the tree-walker (mixed mode)
and gets the solution back as a handle into the tree-walker's table (`from_value` of kind H); `N(t)` is then a
built-in call, so the rest of the program stays compiled.
- **Why:** spec C1: reaction networks and N-body problems written as loops. One equation per species (the radon
chain in §10) doesn't scale to a 12-isotope network or a 10-body problem.
- **Alternatives:** unrolling at compile time (needs the length at compile time, which a list built by `push` in a
loop doesn't have); a compiled right-hand side over lists in LLVM (the performance follow-up: the right side runs at
tree-walker speed now); `values(N)` and `plot N` (a list per step; left out for now, with a clear error; `N[end]` is
`N` at the last time).

## D283. N-dimensional arrays: `fill(value, n1, n2, …)`, `A[i, j, k]`, entry-by-entry arithmetic (spec C1)
- **What:** a new type `Array { rank, dim }` (2 ≤ rank ≤ 4; every entry shares one unit; the shape is a run-time
fact, the rank a compile-time one). `fill(value, n1, …, nr)` makes one (the value's unit is the array's; one size
gives a list), `A[i, j, …]` reads an entry and `A[i, j, …] = x` (or `+=`) sets one, with exactly `rank` indexes;
`+ - * /` work entry by entry with numbers and arrays of the same rank (same shape checked when it runs; `+`/`-` need
the same units, `*`/`/` multiply them); `size(A)`, `size(A, k)`, `sum`, `mean`, `max`, `min`, `abs`, `copy`. `B = A`
shares the array, as lists do (D26). The parser now reads any number of indexes (`A[i, j, k]`, nested Index nodes as
for `M[i, j]`), and `IndexAssign` keeps the ones after the second in a new field `rest` (empty for every program
v1 accepts, so the AST dump and the formatter are unchanged for them). The tree-walker holds `Value::NdArr`
(row-major); the LLVM back end holds arrays as Obj values: `A[i, j]` reads and writes (and the other array
built-ins, `len`/`sum`/`mean` of a list of vectors, and `N(t)` of a list unknown) are calls through `fm_builtin` with
the array as an argument (it is shared, so a write needs no copy back), and the other constructs that touch one are
handed to the tree-walker; the collector counts an Obj's real size so arrays made in a loop are freed in time.
- **Why:** spec C1 asks for N-dimensional arrays with units; physics grids (heat, diffusion, Poisson, lattice
models) need more than 16×16 and more than 2 indexes. `fill(value, dims…)` reads like the notebook ("fill a 50×50
grid with 300 K"), puts the unit in the value where the unit rule already applies, and is Julia's `fill(x, dims…)`.
- **Alternatives:** growing matrices past 16×16 (they are straight-line code for linear algebra, D195, and their
products and inverses mean something an array's don't); `zeros(n1, n2, n3) K` (zeros(r, c) is already a matrix; a
unit after a call isn't the unit rule); `A[i][j][k]` only (kept working, but `A[i, j, k]` is what physicists write);
NumPy-style broadcasting, slices and vectorized functions (next steps; each needs its own unit rules).

## D290. C++ interop (C4): a generated extern "C" wrapper per import, compiled by the system's C++ compiler and cached
- **What:** `import cpp "libphys.so" header "phys.hpp":` followed by `import c` signatures whose names may be
qualified (`phys::Particle::compton_wavelength(m [kg]) -> [m]`) and may end with `as name`; `import cpp header
"cmath":` without a library for header-only code. At check time Fermium writes one C++ file per import: for each
signature an `extern "C"` function `fermium_cpp_k` taking C types (`double`, `int`, `double *`) that converts
`&phys::f` to the function-pointer type of the declared signature (`double (*)(double, int)`, the result `double`
or `int`) by passing it to a helper overloaded on that type (one helper per `const double *`/`double *` spelling
of the lists, up to three lists), calls it inside `try`, and on an exception keeps "the C++ function phys::f threw
an exception: what()" in a thread-local buffer and returns 0. `int fermium_cpp_error(char *, int)` hands the
message over and clears it. The file is compiled by `$CXX`, else the first of c++, g++, clang++ that runs, with
`-std=c++17 -O2 -fPIC -shared -MMD`, `-I` the program's and the header's folders, the library by its path with
an rpath, `-Wl,--no-undefined` on Linux, then `$CXXFLAGS`. The result goes to `$FERMIUM_CACHE_DIR/cpp` (else
`$XDG_CACHE_HOME/fermium/cpp`, `~/.cache/fermium/cpp`, macOS `~/Library/Caches/fermium/cpp`) as `w<FNV-1a 64 of
the source, the named header's text, the library path, $CXX, $CXXFLAGS>.so`, written under a temporary name and
renamed (parallel runs don't see half a file); it is reused while no file in its `-MMD` list and not the library
is newer than it. Each signature then becomes a C3 function of the wrapper (`CFuncRef` with lang "C++"): units
checked at every call, lists, elementwise maps, `fermium build`. `CCallSite.cpp` marks the call sites: after each
call, eval_c.rs asks `fermium_cpp_error` and turns a message into a run-time error at the call's line, so the
LLVM back end sends C++ calls through its callback (≈0.6 µs a call) instead of calling directly. Compiler errors
are translated into one line on the signature they concern (the error's line in the generated file says which):
undeclared name → "the header declares no function", a pointer-to-member in the output → "a member function,
which needs an object", no conversion → "no overload of phys::f has the C++ type double(double, int)", ambiguous,
missing header, undefined reference at link time → "the C++ library has no definition of phys::f(double)" (or,
with no library, "defined nowhere"); anything else gives the first error and the path of a log with the full
output.
- **Why:** spec C4 ("through generated C wrappers"). The function-pointer conversion makes the declared signature
choose among overloads exactly as C++ would for `static_cast<double(*)(double)>(&f)`, deduces template
arguments, works for static member functions, and lets the compiler check the declaration against the header,
which C3 can't. Mangled names are compiler-specific, so calling C++ symbols without a compiler would tie Fermium
to one ABI's mangling and still not handle inline functions or templates. Catching exceptions in the wrapper is
required: unwinding through Rust's frames is undefined behaviour. The cache keeps the check fast (a hit needs no
compiler: ≈40 ms for a program); `-MMD` makes edits to included headers count. The error message crosses as a
buffer through the existing C3 call machinery, so no new runtime entry points or dependencies were needed.
- **Alternatives:** calling the function directly in the wrapper (`return phys::f(a0, a1)`: overload resolution
with implicit conversions would silently pick `f(int)` for a double, or `f(float)`); `static_cast` to one
pointer type (can't accept either `const double *` or `double *` for a list); parsing the header (libclang: a
large native dependency); a C++ compiler at run time instead of check time (errors would come late); the
direct LLVM call with an error flag checked after it (faster, but a new runtime path in the JIT and the AOT
runtime; left for later); a per-program wrapper instead of per import (fewer files, but more recompiles).
Known gaps: non-static member functions and objects, references, `float`/`long`/structs/strings/`std::vector`,
`void` results, explicit template arguments, several headers per import, `import cpp` in modules, calls inside
`units natural`, the playground; macOS linking is written but untested; `fermium build` executables load the
wrapper from the cache by its build-time path.

## D295. Derivatives of multi-line functions by automatic differentiation, on by default in v2.5 (spec C2)
- **What:** `f'`, `f''`, `d/dx f`, `∂/∂v E`, `∇φ` and `∇²φ` of a function written over several lines (and of a
one-line function or formula that calls one) are computed by forward-mode automatic differentiation as a source
transformation (`fermium-sym/src/ad.rs`): each local that depends on the variable gets a tangent, assigned just
before the local by the chain rule with fermium-sym's symbolic partial derivatives; `if`/`else` differentiate
branch by branch, loops run the same iterations, a loop counter is a constant, `print` is dropped. Lists whose
elements depend on the variable, `solve`, `plot`, `fit` and `propagate` inside the function are compile errors
naming the statement; `∇·` and `∇×` still need a one-line vector formula. The groundwork was opt-in
(`FERMIUM_C2=1`); v2.5 is the language change, so it is now always on, like C5 and C7. The two v1 goldens that
pinned "can't differentiate through g: it's defined over several lines" are documented divergences
(rust/DIVERGENCES.md, *v2.5: derivatives of multi-line functions (C2)*), their new outputs checked analytically.
- **Why:** physicists write helper variables and loops; refusing their derivatives pushed them to finite
differences, which lose half the digits. A source transformation keeps the result exact to rounding, reuses the
one-line differentiator and both back ends (the derivative is an ordinary multi-line function), and a second
derivative is the same transformation applied again.
- **Alternatives:** dual numbers at run time (both back ends would need a dual type, and the LLVM back end's
unboxed doubles would lose their speed); symbolic inlining of the body into one expression (blows up through
loops, impossible through `while`); keeping the opt-in (v2.5 is where language changes land).

## D296. `if` conditions on the unknowns of a solve: frozen during each step, switches located on the dense output (spec C2)
- **What:** in an RK45 solve, each comparison `a < b` (`>`, `<=`, `>=`) in the equations that depends on the
unknowns (also inside a one-line function the equation calls with them, which is inlined, and its `where`) is
rewritten to read a branch flag from an extra state slot: 1 true, 0 false (derivative 0, so it is exactly
constant within a step), 2 "evaluate as written". The checker also builds a lambda of each condition's a − b.
The solver sets the flags at the start from the conditions; during a step the right side is the smooth branch
of the flags; after each accepted step it evaluates the conditions on the step's dense output (DOPRI5's 4th-order
interpolant), and where one no longer agrees with its flag it brackets the switch to rounding (Illinois regula
falsi with a bisection fallback), ends the step there (a sample on each side of the switch), flips the flag and
restarts. Sliding modes (Filippov): at a switch, dg/dt along both branches is estimated (a small Euler step of
each); if both point into the surface, the flag becomes 3 and the solver's right side is α f_true + (1 − α)
f_false with α making dg/dt = 0; the slide ends where one side's flow turns away (located the same way on the dense
output), leaving to that side. Guards: a condition that still flips more than 20 times in a row, each within
10⁻⁹ of the range of the last, is evaluated as written from then on; a stage whose frozen branch isn't finite (an
expression undefined past the switch) is evaluated as written. The error control, first step and "step too small"
look only at the user's slots (`OdeOpts.nerr`). Not covered: `step`/rk4, radau and bdf, sensitivity solves (C7),
conditions in multi-line functions, and `abs`/`sign`/`min`/`max` (all evaluated as written, v1's behaviour).
- **Why:** v1 evaluated the condition at every stage, so steps straddling a switch mixed the two branches and the
error control could only shrink them; accuracy near the switch was that of the step it settled on (a piecewise
spring lost 6×10⁻⁸ m over three periods at rtol 10⁻⁹; now 1.5×10⁻⁹) and friction at rest ("sticking") ended in
"the step became too small". Freezing the branch (discontinuity locking) is what makes locating on the dense
output exact: the interpolant is that of a smooth right side. Keeping the flags in the state makes them reach
every place the right side is evaluated, in both back ends, with no new calling convention for compiled right
sides. One golden (a white dwarf) moves by 2 units in its 8th digit towards the converged value (a documented
divergence).
- **Alternatives:** only locating the switch after the fact without freezing (the dense output of a step that
straddles the switch is wrong, so the location is too); asking the user for an explicit event (the physics is
already in the `if`); stiff-style restarts at every sign change of a live condition (chatters on sliding modes).

## D297. `when lhs = rhs: x' = …` events that change the state in a solve (spec C2)
- **What:** a solve clause `when lhs op rhs: target = value, …` (op `=` fires on every crossing of lhs − rhs,
`<`/`<=` only when it falls through 0, `>`/`>=` only when it rises; targets are unknowns and their derivatives below
the highest, values are computed from the state just before the event, units and shapes checked). The checker
builds a g lambda and a new-state lambda with the right side's state; the solver (RK45 only: `step`, rk4, radau
and bdf are a compile error) locates the crossing on the dense output like `until`, ends the step there, applies
the new state (branch flags recomputed) and restarts; the solution keeps a sample on each side, so `y(t)` and plots
show the jump. After an event its sign is taken from a small Euler step of the new state (a ball that bounced moves
up), so the next crossing isn't missed however large the next step is. The same event firing again within 10⁻⁹ of
the range is a Zeno point: an error naming the time. Several `when` and an `until` can be combined (the earliest
wins). With uncertain values (±) a solve with `when` goes to C7's Monte Carlo (D278), with its warning: the
linear sensitivity solve would need the jump of the sensitivities at a moving event time (a saltation matrix).
In the sensitivity solve itself (C7, D277) the conditions of D296 are evaluated as written; a flag starts at 2
("as written") in the program and the RK45 solver sets it from the condition at the start, so any evaluation of
the right side outside the solver (C7's probes) sees the condition itself.
- **Why:** bounces, impacts, resets and thresholds are the most common discontinuities in physics ODEs; the
alternative in v2.0 was a loop of solves with `until`, which is hard to read. Locating on the dense output makes
the impact times exact to rounding (the bouncing-ball test agrees with the analytic bounces to 6×10⁻¹⁴ m).
- **Alternatives:** `if y < 0 m then y' = …` inside the equations (mixes the model with state changes, and an `if` in
the equations already means a piecewise right side, D296); `on`/`event` keywords (`when` reads like physics on
paper and wasn't a keyword); allowing the highest derivative as a target (it follows from the equation).

## D298. Printed derivatives use the canonical tidy form when it is shorter (spec C2, better symbolic simplification)
- **What:** `print f'` (and `f''`, `d/dx f`, `∂/∂x f`) shows the derivative's formula after fermium-sym's `tidy`
(the SymPy-style canonical form, with fractions put together and common factors taken out, v1's `sympy_tidy`)
when that is at least a fifth shorter than the plainly simplified formula; otherwise the plain formula, as in v1. Only the printed
text changes: the derivative is still evaluated from the plainly simplified formula, so no computed number moves.
Examples: `g(x) = x sin(x)` gives `g''(x) = 2cos(x) - x sin(x)` (v1: `cos(x) + (cos(x) - x sin(x))`),
`x exp(x) - exp(x)` gives `x exp(x)` (v1: `(1 + x - 1)·exp(x)`), `√(1 + x²)` gives `1/(x² + 1)^(3/2)` for the
second derivative, and `1/√(x² + y² + z²)` gives `∂V/∂x = -x/(x² + y² + z²)^(3/2)`. Tidy itself gained one case: a sum that, shifted to the lowest power of the sums in it, is a plain number (√Q − x²/√Q with Q = 1 + x² is 1/√Q).
- **Why:** v1's derivative simplifier works node by node, so like terms from the product rule were never
collected and nested fractions never put together; readability of printed formulas is goal (2). v1 already used
`tidy` for ∇ results, so the form is familiar and tested (its answers are checked numerically in fermium-sym's
tests). Printing only keeps every number bit-identical.
- **Alternatives:** a like-term collector inside `simplify` (would also change the evaluated expressions, and so
the last digits of computed derivatives across the conformance suite); always the tidy form when it is shorter at all (in the conformance suite it changed three v1
formulas, two of them only by reordering or 1-2 characters, e.g. `1/(4 √(1 - (x/4)²))` → `1/(4 √(1 - x²/16))`;
the fifth threshold keeps those as v1 printed them and changes only `(1 + x² - 2x²)/(1 + x²)²` →
`(1 - x²)/(1 + x²)²`, a documented divergence).

## D299. Parameter sweeps over solve: a `sweep` loop whose plots collect one curve per value (spec C2)
- **What:** `sweep k in LIST` / `sweep k from a to b [step s]` + a block is parsed as the `for` loop it spells
(AST `Sweep { body: [the loop] }`) and checked and run exactly like it; the difference is in its plots. A plot
directly in the block (same function) gets the loop variable as an extra expression and its print format; at run
time each iteration's curves are labelled `k = <value as print shows it>` (with the curve's own name in front when
the plot has several curves) and kept; a statement after the loop saves each collected figure once. Both back ends
(the LLVM back end hands plots to the tree-walker, which keeps the figures). The loop variable and results are the
loop's, so collecting numbers (`push(periods, …)`) and printing work as in any loop.
- **Why:** a sweep over a parameter of an ODE is a for loop around a solve, which already worked in v2.0 (`for k in
[1, 2, 4] N/m`), but each plot then overwrote the previous figure; comparing curves for several parameter values
is the point of a sweep. A keyword that reads like the physics ("sweep k in …") keeps the loop visible and costs
nothing else to learn.
- **Alternatives:** changing `for` so that plots in loops accumulate (would change v1 programs that save one figure
per iteration on purpose, and their conformance output); a `solve … for k in […]` clause (mixes two ranges on one
statement, and the results — values at times, event times — would need a new list-of-solutions type); a plot
option (`plot x vs t for each k`) (the plot would have to know which loop it is in anyway).

## D300. The ±1σ test also compares the linear prediction with the actual change, and kernels record every source they read (red team 14 #1)
- **What:** D278's test accepted linear propagation when the second-order change d₂ was small next to the
first-order one d₁. A jump at an uncertain value (`∫ (if x < a then 1 else 0) dx`, `sign(a - x)`, `floor(x + a)`,
`solve x' = if t < a then 1 else 0`) has ∂f/∂a = 0 almost everywhere, so the linear contribution c is 0, while
the result moves by d₁ = σ_a; and since the integrand returned a plain number, `a` wasn't even counted as a
source. Now (1) while an integrand or right side is evaluated in the plain or linear pass, every uncertain
variable it reads is recorded (`kernel_seen`, in the tree-walker's variable read, only for uncertain values and
not inside Monte Carlo), so a kernel that reads `a` without returning an uncertain value goes to the uncertain
path; an ODE whose right side reads a source only after t₀ (a branch not taken at the start) is solved again with
that source's sensitivity; (2) linearity needs both one-sided changes to agree with the prediction:
|y₊ − y₀ − c| and |y₀ − y₋ − c| and |d₂| all ≤ 10 % of s plus the kernel's noise, where s = max(|d₁|, |c|,
scale). Otherwise Monte Carlo (both integrals and ODEs have it), with the usual warning. `∫ (if x < a then 1 else
0) dx from 0 to 2`, a = 1.0 ± 0.1, is 1.00 ± 0.10 (NumPy Monte Carlo of clip(a, 0, 2): 1.00 ± 0.10); a ± written
inside a kernel is now sampled as the source its linear pass made during the test (before, the test sampled a
fresh zero stream for it, so the test compared nothing). A kernel that reads an uncertain value which doesn't move
its result (a comparison far from where it switches) and never returns an uncertain value gives a plain number or
a plain solution, as Fermium 1.5 does (`2`, not `2 ± 0`).
- **Why:** the linear rule is only valid if it predicts the ±1σ change; comparing it with the change directly is
the missing half of the test and costs nothing (the ±1σ runs were already made).
- **Alternatives:** detecting discontinuities symbolically (branches, floor, sign, comparisons) and always going
to Monte Carlo (misses user functions that hide them, and a far-away jump needn't matter); an error (the user
asked for a result, and Monte Carlo is available on both paths).

## D301. Foreign functions take plain numbers: ± is an error, as for `use python` in v1 (red team 14 #2)
- **What:** a `use python` function given an uncertain value stops with v1's UncertainUse message ("this operation
needs a plain number, but got an uncertain value (±); write value(x) …"), as `fermium-legacy` does; Fermium 2
had passed the value alone. C, Fortran and C++ functions do the same with their own message (*sq is a C function,
which takes plain numbers, but got an uncertain value (±)*, hint: value(x) or `propagate montecarlo`). Inside
`propagate montecarlo` a C function is called once per sample (before, the first sample was passed for every
sample, so the spread was lost silently); a C function with list parameters is refused there.
- **Why:** unit safety and honesty first: dropping ± silently is the worst outcome, and an error matches v1 and
the rest of the language's "needs a plain number" places.
- **Alternatives:** linear propagation by numerical derivatives of the foreign function (a step to choose, the
function may be noisy or not smooth, and it would differ from `use python`'s v1 behaviour).

## D302. A later definition replaces every earlier version it covers (red team 14 #3)
- **What:** D285 replaced only a version with the same signature, so `force(x [m]) = 3 N/m x` then
`force(x) = 5 N/m x` kept both and `force(1 m)` still chose the annotated one (3 N) where Fermium 1.5 prints 5 N.
Now a new top-level definition replaces every earlier version it *covers*: the same number of parameters, and
each parameter at least as broad: unannotated covers anything; a kind covers the same kind with any unit or the
same dimension; a unit alone covers the same dimension with any kind but `vector` (which needs `: vector`). A
definition that doesn't cover an earlier version adds one, so a more specific definition written later still
adds a version (`describe(x) = 0` then `describe(x [m]) = 1`). The REPL uses the same rule (it goes through
the same checker).
- **Why:** a v1 program only redefines functions to replace them; with this rule every v1 program behaves as
in v1 (a v1 redefinition is always as broad as, or equivalent to, what it replaces unless it adds annotations),
and the documented way to write versions (general first, specific later, or disjoint signatures) keeps working.
- **Alternatives:** replace on the same arity only (would break `f(x: vector)` then `f(xs: list)`); warn instead
of replacing (the v1 program still prints a different number).

## D303. A declared `: list` parameter takes the list whole; `∂/∂y f(1, 3)` sees every version (red team 14 #4, #8)
- **What:** `call_user` applied a function to each element of a list argument unless the body used the parameter
as a list (`takes_lists`), so `s(x: list) = 2` then `s([1, 2])` failed with "s expects x to be a list, but got a
plain number". An argument for a parameter declared `: list` is now never mapped over, and a function with such a
parameter isn't mapped at all. `∂/∂y f(1, 3)` (read as `(∂/∂y f)(1, 3)`, D221) checked for the parameter y only
in the last version of f; it now accepts any version that has it, and the call chooses among the derivatives.
- **Why:** the declared kind is the user's statement of intent; dispatch (D285) already honoured it.
- **Alternatives:** none considered.

## D304. Monte Carlo ODE solutions: the nominal value, and one shared set of samples; turning points (red team 14 #6)
- **What:** (1) a Monte Carlo solution's `x(t)` is the nominal solution's value (as a linear solve reports), with
the spread and the per-source contributions from the regression on the samples; (2) the regression residual
(the nonlinear part) of each value asked for is projected onto the residuals of the values asked for before (kept
orthonormal over the samples, each an error source; up to 200 per solution, later ones independent), so values
at different times, and closed forms built from them, stay linked through the same samples: for x' = −k x with
k = 1.0 ± 0.4, `x(2) - x(1)^2` is 0.000 ± 0.064 (the σ is the linearization of the square; 0.029 ± 0.099 before),
and asking twice gives the same value; (3) the ±1σ test of an ODE judges each component against its typical ±1σ
change (the largest |d₁| or |c| over the 8 test times) instead of the change at that time, so a turning point
(d₁ ≈ 0) no longer forces Monte Carlo. Decay with k = 1.0 ± 0.1 over 0–3 s and a pendulum with g, L known to
0.5–1 % over a few periods stay linear, with no warning, matching the analytic σ.
- **Why:** the value at the measured inputs is what every other path reports; a separate "nonlinear rest" source
per (component, time) made arithmetic across times inconsistent; the per-time scale was the wrong yardstick.
- **Alternatives:** storing the samples on every uncertain value (a different, much heavier representation);
the Monte Carlo mean as the value (what `propagate montecarlo` reports, and still does).

## D305. `fermium check` and the language server don't load foreign libraries (red team 14 #5)
- **What:** `CheckOptions.no_load` (set by `fermium check` and the LSP): an `import c/fortran/cpp` library is
not dlopen'ed; a library named by a path must exist, and each function's symbol is looked up in the file's ELF
`.dynsym` (defined symbols; `cffi::file_has_symbol`, a small reader of 64-bit little-endian ELF). When the file
can't be read that way (Mach-O, a bare name the system would find), the symbol check is left to the run. C++
wrappers are still compiled (so their errors show in the editor) but not loaded. `fermium run` and
`fermium build` load as before.
- **Why:** loading a library runs its constructors, so opening a cloned .fm file in the editor ran code from the
repository. Reading the symbol table keeps the useful "misspelt function" error without running anything.
- **Alternatives:** skipping the symbol checks in check mode (loses the error in the editor); a Mach-O reader
(later); not compiling C++ wrappers in check mode (safer against compiler bugs, but loses every C++ error in the
editor; compilers are meant to take untrusted input).

## D306. Replacing a version with same-dimension, different-meaning units warns (red team 14 #7)
- **What:** when a definition replaces a version (D302) and a parameter's units differ in kind as in adding such
values (Hz vs rad/s, Bq vs Hz or rad/s, Gy vs Sv, J vs N m; `units::unit_kind`), one warning: *E(ω [rad/s])
replaces E(f [Hz]) (line 1): their units have the same dimensions, so they can't be two versions*, hint: another
name, or convert inside one definition. `g(x [m])` then `g(y [km])` stays silent (an ordinary redefinition). No
conformance program changes (the suite passes unchanged).
- **Why:** Fermium treats rad as 1, so Hz and rad/s are one dimension; the new docs invite overloads by unit, and
`E(1 GHz)` silently off by 2π is exactly the mistake the language exists to catch.
- **Alternatives:** dispatching on the unit's spelling (would make units that are equal in SI behave
differently); an error (v1 accepts the program).

## D310. Performance (spec C6): faster compiled loops, bit for bit, and a compile cache
- **What:** C6's changes to the LLVM back end and `fermium run`, each keeping every printed number bit for bit
(conformance at its floor, `rust/tools/llvm_diff.py` without a new disagreement, `rust/c-cases/c6/*.fm` equal to
Fermium 1.5's output with the tree-walker, with the LLVM back end, and with the LLVM back end with each change
switched off in turn, and from the cache): the loop and SLP vectorizers on in-order sums (D311), if-conversion in
compiled loops (D312), no collector safe point in loops that only read lists (D313), no stack check in leaf
functions (D314), the C library's exp/log/sin/cos through LLVM intrinsics (D315), the samples of a compiled RK4
solve mapped in one go (D316), and a compile cache for whole programs with their modules (D317).
- **Measured:** A/B on this shared 4-core machine around 06:50 UTC (load average 2.4–3.1, 5 GB free), the
v2.5 binary before C6 and the C6 binary run alternately, inner (compute-only) times, min / median of 11–15 runs.
Such numbers move by 10–30 % between runs; the coordinator's quiet-machine run in benchmarks/RESULTS.md is the
reference. forces (4 threads) 7.44 / 10.8 ms → 3.46 / 3.89 ms and forces (1 thread) 19.4 / 19.8 ms → 9.17 / 9.29
ms (≈ 2.1–2.8×: the inner loop vectorized, D311–D313); spring_rk4 30.5 / 35.2 ms → 20.9 / 28.1 ms (D316; populate
without huge pages 27.2 / 33.9, off 30.3 / 34.8); nbody 47.1 / 58.2 ms → 45.1 / 57.6 ms; blackbody 2.95 / 3.80 ms →
2.83 / 3.46 ms; unit_loop 4.92 / 5.94 ms → 4.89 / 6.25 ms (unchanged within the noise); spring_adaptive 0.579 /
0.649 ms → 0.669 / 0.750 ms (slower in each of three A/Bs, though the compiled code executes the same
instructions under callgrind and the solver's Rust code is unchanged: code layout, not explained). Instructions
executed by the compiled code (callgrind, deterministic): forces 304 M → 120 M, nbody 789 M → 623 M, blackbody's
integrand 7.8 M → 6.0 M, unit_loop 52.5 M → 56.9 M (an in-order vectorized sum: more instructions, the same add
chain), spring_rk4 and spring_adaptive unchanged. Against Julia's times in the last quiet run (RESULTS.md), these
ratios suggest forces well below Julia on 1 and 4 threads, nbody and unit_loop at parity, spring_rk4 ≈ 1.5×,
blackbody ≈ 1.4×, spring_adaptive ≈ 1.1×: the goal "faster than Julia on half the rows" is not clearly met (two
clear wins, two ties of seven rows); the quiet run decides. Start-up: D317.
- **Why:** spec C6 ("SIMD-friendly codegen and loop vectorization… Cache compiled modules"), within the priority
order: every change is exact (no fast-math, no reassociation), so unit safety and the printed results don't move.
- **Alternatives, and what was left:** D318.

## D311. The vectorizers: SLP on, and in-order floating-point sums vectorized (`force-ordered-reductions`)
- **What:** the pass pipeline (`default<O2>`) runs with SLP vectorization on (a PassBuilderOptions flag that LLVM's
C API leaves off; clang turns it on at -O2) and with LLVM's option `-force-ordered-reductions`, set once per process
through LLVMParseCommandLineOptions (`llvm::LLVM_OPTIONS`). With it the loop vectorizer takes a loop whose
floating-point sum must keep its order (`s += f(j)`): the terms are computed several at a time (4 doubles with AVX2)
and added one by one in the original order (`llvm.vector.reduce.fadd` without `reassoc`), so the sum is bit for bit
the scalar loop's. Only sums made of `fadd` qualify, so where D312 applies a sum `v − e` is emitted as `v + (−e)`
(the same number: IEEE subtraction is the addition of the negation). `FERMIUM_LLVM_ARGS` replaces the option list
and `FERMIUM_LLVM_NOSLP=1` turns SLP off (experiments, and the tests that show each change keeps the results).
- **Why:** the costly part of a loop like forces' (a √ and two divisions per pair) runs in vector registers while
the sums stay exact. Julia doesn't vectorize these loops without `@simd`/`@fastmath` (which reassociate); Fermium
does, exactly.
- **Alternatives:** fast-math or `reassoc` flags (they change results: never); vectorizing only on request.

## D312. If-conversion in compiled loops, and scratch variables
- **What:** in the copy of a loop whose index checks were proven before it (D273's versioned loops), an `if` whose
branches only assign numbers built from constants, numeric variables, + − × ÷, constant powers, a few pure
built-ins (√, abs, floor, ceil, round, exp, sin, cos, tan, atan, sinh, cosh, tanh, asinh, expm1) and proven list
reads is compiled without a branch (hoist::if_converted): both sides are computed whatever the condition, and a
`select` keeps the right values. A sum `v = v + e` becomes `v + (c ? e : −0)` and `v = v − e` becomes
`v + (c ? −e : −0)`: adding −0 gives v back exactly (±0, ±∞ and NaN included), and the sum stays an in-order
reduction (D311). A *scratch* variable (hoist::scratch_vars: one whose every read, anywhere in the program, comes
after an assignment to it earlier in the same run of statements, with no loop or `if` in between that could have
set it) needs no select: nothing reads its value before it is set again. `FERMIUM_NO_IFCONV=1` switches this off.
- **Why:** a branch in a loop body stops the loop vectorizer ("control flow cannot be substituted for a select").
The select form is exact: nothing computed under a false condition is kept, and floating point doesn't trap (√ of
a negative number is NaN in both forms, a division by zero ±∞ or NaN). forces' inner loop (`if j != i …`) is the
case. The language already forbids reading, after an `if`, a variable set only inside it, which makes most such
variables scratch variables.
- **Alternatives:** masked loads (LLVM can't prove the list reads safe to speculate: their bounds are known only at
run time); converting every `if` (slower when the branch is rarely taken and the loop doesn't vectorize: limited to
the proven, check-free copies of loops, whose bodies are small).

## D313. No collector safe point in loops that only read lists
- **What:** gc.rs counted any expression of list type, even reading a list variable (`xs[i]`, `len(xs)`), as
something that may allocate, so every loop over lists in `fm_main` had a safe point (a load, and a call to `fm_gc`
when the collector asks). Reading a variable allocates nothing: an innermost loop that only reads lists now has no
safe point, and a function whose loops only read lists needs no collector frame. Loops that contain other loops or
write list elements keep theirs (a well-predicted branch per pass): without them LLVM turned nbody's loop nest into
code ≈ 15 % slower (with or without vectorization, measured), so they stay until that is understood.
- **Why:** the call in the loop body stopped the loop vectorizer (and register promotion) in forces' serial loop;
no allocation can happen in such a loop, so the collector never needs to run there.

## D314. No stack check in functions that call no function
- **What:** a user function whose body calls no user function, directly or through a lambda (an integrand, sum,
root, sample, a solve's right side), and has no statement the tree-walker runs (hoist::is_leaf), is compiled
without the runaway-recursion check (its frame address compared with a limit).
- **Why:** it can't be part of a recursion, and the check (a load, a compare and a branch to an error block) stayed
in every inlined copy, e.g. in each of blackbody's 215 000 integrand calls. Runaway recursion is still caught in the
recursive functions, and the message can no longer name a leaf as the function that "called itself".

## D315. exp, log, sin and cos through LLVM's intrinsics
- **What:** in the JIT, `exp`, `ln`/`log`, `sin` and `cos` of a number compile to `llvm.exp.f64` etc. instead of
calls to the Rust shims `fm_exp`…; LLVM emits calls to the C library's `exp`/`log`/`sin`/`cos`, the very functions
the shims call (Rust's `f64::exp` is `llvm.exp.f64` too), so the numbers are the same. `fermium build` keeps the
shims. `FERMIUM_NO_MATH_INTRINSICS=1` switches this off.
- **Why:** one call level less per evaluation, and LLVM knows the functions are pure (it computes a repeated one
once and hoists one out of a loop).
- **Alternatives:** vector math libraries (libmvec, SVML: different last bits, so never); an exp compiled into the
module, as Julia does (a different function, with different last bits than the tree-walker's).

## D316. The samples of a compiled RK4 solve are mapped in one go
- **What:** fm_rk4_begin reserves the solution's arrays for all steps + 1 samples (t, y, y′: 40 MB for
spring_rk4's 10⁶ steps of a 2-state system). For an array of 4 MiB or more it now asks Linux for huge pages
(MADV_HUGEPAGE) and maps the pages at once (MADV_POPULATE_WRITE, Linux 5.14+). Advice only: an error, an older
kernel or another system changes nothing. `FERMIUM_PREFAULT=0` (off) / `p` (populate only) / `h` (the default)
for experiments.
- **Why:** spring_rk4's steps take ≈ 15 ms; writing its samples into fresh memory took as long again, almost all
of it page faults (one per 4 KiB page; measured with the stores left out: 15.5 ms against 33 ms). Julia's program
stores nothing, which is most of why the row was 1.89×. The dense solution (x(t) between steps, x′(t), len(x)) is
part of the language, so the samples stay.
- **Caveat:** with the kernel's huge-page `defrag` setting `madvise` (this machine's), a huge-page request may
compact memory first; on this loaded machine about one run in 20 took 100–250 ms longer. benchmarks/run.py
reports medians.
- **Alternatives:** storing fewer samples, or rebuilding them when the solution is first read (checkpointing; bit
for bit possible, but the work would move out of the timed region: rejected as unfair to the comparison); not
storing t (it is t0 + i·h except at the end; the solution object is shared with the tree-walker: a larger change);
populate without huge pages (≈ 20 % of the gain).

## D317. The compile cache: a program's machine code, reused while its text and its modules are unchanged
- **What:** `fermium run` saves the machine code the JIT generated for a program, with what the run time reads
besides (native::blob, the format of `fermium build`'s executables), in `<cache>/jit/<key>.fmc`, beside C4's C++
wrappers (`$FERMIUM_CACHE_DIR`, else `$XDG_CACHE_HOME/fermium`, else `~/.cache/fermium`). The key hashes this
binary (version, size, modification time), LLVM's version, the CPU and its features, the code generator's
switches, the program's file name, folder and text. An entry is used only if it is intact (magic, checksum), holds
exactly the program's text, and every module file and fermium.toml the compilation read, and every one it looked
for and didn't find, is as it was (contents hashed; a module file added where an import looked first would change
what it finds). A hit parses, checks, generates and optimizes nothing: MCJIT loads the saved object through an
`llvm::ObjectCache` (jit_cache.cpp; LLVM's C API has none) with the run time's callbacks mapped by name, and the
program runs with the saved tables. The warnings of the check are saved and printed again. To make machine code
movable between processes, a compilation for the cache reads the context pointer from the global `fm_ctx` (as
`fermium build`'s code does) instead of a constant address (`Gen::new_reloc`). Direct calls to C functions and
constructs the tree-walker runs hold addresses of the process, so programs with them, and programs that use
Python, C or C++ or read data files when checked, are compiled every time. Writes are atomic (a temporary file
renamed into place); a damaged entry is ignored and replaced; at most 400 entries are kept (the oldest go);
`FERMIUM_NO_CACHE=1` turns it off; the LLVM dump switches bypass it.
- **Why:** spec C6 "cache compiled modules". Fermium's modules are checked with the program that imports them
(their functions are generic, instantiated with the caller's units) and compiled into one LLVM module, so the unit
that can be cached is the program with its modules. What a run spends before the program starts is mostly LLVM:
for a program that imports the six standard-library modules, parse 0.5 ms, check 5.5 ms, code generation and
optimization 6 ms, JIT 8 ms (a moderately loaded machine); from the cache it runs in about 1 ms. Whole-process wall
time, min / median of 15 alternating runs (load ≈ 3): benchmarks/fermium/startup.fm 14.2 / 15.8 ms compiled →
7.6 / 8.7 ms cached; the standard-library program 24.9 / 27.5 → 7.8 / 8.9 ms; blackbody.fm 25.6 / 28.8 → 10.8 /
12.1 ms; forces.fm 90.5 / 105 → 19.7 / 23.8 ms. What remains (≈ 7 ms) is starting the 100 MB binary and LLVM's
initialization.
- **Alternatives:** caching each module's checked form (saves only the checking, and the checked form is
instantiated per caller); caching optimized LLVM bitcode (saves optimization, not code generation); ORC's object
layers (not in LLVM 18's C API, and a larger change than MCJIT's ObjectCache); keying on modification times instead
of contents (misses edits within one timestamp tick).

## D318. What C6 tried and left
- **Tried and dropped:** removing every safe point in loops over lists (nbody slower: D313 keeps the outer and
writing loops'); `default<O3>` (D273: no gain).
- **Left:** blackbody (≈ 1.5× Julia) spends its time in the quadrature's bookkeeping around each of its 215 000
integrand calls and in glibc's exp; both programs evaluate about the same number of points (Julia 211 140, Fermium
214 919). A batched integrand (the 15 Gauss–Kronrod nodes of a panel in one compiled call) would remove the call
overhead and let the divisions vectorize; the adaptive algorithm must stay v1's (its results are the oracle's), so
this is a change to fermium-runtime's quad and to the code generator (BACKLOG). spring_adaptive and unit_loop are
at parity with Julia, bound by the ODE solver's bookkeeping and an in-order sum's latency.

## D320. The C++ wrapper cache: a SHA-256 key of every compile input, a manifest checked on reuse, system headers without the program's folder, a private cache folder (red team 15 #1, #2)
- **What:** (1) a header the program's folder has (or an absolute path) is included by its absolute path with
`-I` the program's and the header's folders, as before; any other name (`cmath`, `math.h`, `Eigen/Dense`) is
included as `<name>` with **no** folder of the program's on the include path, so a `cstdio` planted next to a
program can't stand in for the real one, and installed headers are found where the compiler finds them (a
missing one is the compiler's "not found", reported as "can't find the header" with a hint). (2) The cache key
is the SHA-256 (in-tree, `sha256.rs`; the workspace had no hashing dependency) of the wrapper's source, whether
the header is local or a system one, its absolute path and text, the `-I` folders, `CXX`, `CXXFLAGS`, `CPATH`,
`CPLUS_INCLUDE_PATH`, `C_INCLUDE_PATH`, `LIBRARY_PATH`, `GCC_EXEC_PREFIX`, `COMPILER_PATH`, `SDKROOT`,
`MACOSX_DEPLOYMENT_TARGET`, and the library as written and its path; 40 hex digits name the files. (3) Next to
each wrapper a manifest records the compiler's identity (each word of the command resolved on the PATH, links
followed, with its size and time), the SHA-256 of its `--version` output, the wrapper's size and SHA-256, the
library's size and time, and every file of the compiler's `-MD` list (now `-MD`, so system headers count) with
its size and time. A wrapper is reused only when the manifest is complete and all of it still matches; a
compile writes the manifest last, after the wrapper. With no compiler on the PATH a wrapper whose manifest
otherwise matches is still used (nothing else could be built, and the old test "a second run needs no compiler"
holds). (4) The cache folders (`cpp/`, and `jit/` of C6) are made 0700; a folder owned by another user or
writable by group or others is not used: one warning, then a fresh private folder in the temporary folder for
this run (cachedir.rs). (5) The compiler runs in its own process group and is killed after
`$FERMIUM_CXX_TIMEOUT` seconds (120 by default), with a one-line error.
- **Why:** the old key (FNV-1a 64 of the source, the named header's text, the library path, `$CXX`,
`$CXXFLAGS`) left out the program's folder, which was on the include path, so `fermium check` on one program
planted a wrapper that another program with the same import loaded and ran; it also left out the include-path
variables and the compiler, so a stale wrapper silently printed old constants. The compiler's own identity is
checked in the manifest rather than hashed into the name so a cache hit spawns no process (≈25 ms per program).
- **Alternatives:** running `c++ --version` (or the preprocessor) on every run to put its output or the resolved
header list in the key (a process per import per run; the manifest catches the same changes); a content hash
of every dependency on reuse (≈100 system headers per wrapper to read each run; size and time is what make and
ninja trust); keeping `-I <program folder>` for system headers (what made the attack work); `-iquote` for the
program's folders (would break headers that include their neighbours with `<…>`). Known limit: a file newly
added to an include folder that shadows one the wrapper didn't read isn't noticed (the wrapper stays as it was,
which runs nothing new).

## D321. Passing a parameter on to a function's `: list` parameter makes it a list parameter (red team 15 #4)
- **What:** `takes_lists` (is a list argument taken whole, or is the function applied to each element?) also
counts a call in the body that passes a parameter, as is, to a user function whose parameter at that position is
declared `: list` in any of its versions: `s(x: list) = 2`, `f(y) = s(y)`, `f([1, 2])` is 2, not `[2, 2]`.
- **Why:** D303 made the declaration the user's statement of intent; a plain wrapper around such a function should
keep it. Calls with the parameter inside an expression (`s(2 y)`) still count as element-wise.
- **Alternatives:** inferring list-ness through any chain of calls (a fixed point over the call graph: more
machinery for a rare case); requiring the wrapper to declare `: list` too (surprising after D303).

## D322. A Monte Carlo integral shows the nominal value; ± never prints -0.00 (red team 15 #5)
- **What:** an integral that falls back to Monte Carlo (D278, D300) reports the integral at the measured inputs
(every source at z = 0, computed with plain numbers, no random numbers drawn), with the spread and per-source
contributions from the regression on the samples, as D304 does for ODE solutions; before, it showed the
regression's intercept (≈ the sample mean: 0.505 instead of 0.500 for ∫₀² x·[x < a] dx at a = 1.0 ± 0.1). An
uncertain value whose rounded value is zero prints `0.00 ± 0.14`, not `-0.00 ± 0.14` (numfmt::format_pm_sig;
Fermium 1.5 printed the minus sign, but no conformance program has such a value: rust/DIVERGENCES.md). Arithmetic
on Monte Carlo results stays first order, as all uncertain arithmetic is (documented in reference §21).
- **Why:** one rule for every result (the value at the measured inputs); "-0.00" reads as a sign that means
something.
- **Alternatives:** the sample mean everywhere (what `propagate montecarlo` reports, and still does); storing the
samples with the value to make later arithmetic Monte Carlo too (a different, heavier representation).

## D323. C++ usability: installed headers, a compile timeout, one-line exception messages, keyword names (red team 15 #6)
- **What:** (1) a header the program's folder doesn't have is `#include <name>`d and found by the compiler on its
own include path (`math.h`, `Eigen/Dense`); a header nobody has is the compiler's "No such file", reported as
*can't find the header X* with a hint naming both places. Before, any name with a `/` or ending in `.h`/`.hpp`
had to be in the program's folder. (2) The compiler runs in its own process group with a timeout
(`$FERMIUM_CXX_TIMEOUT`, 120 s); on expiry the group is killed and the import is a one-line error, so `check` and
the language server can't hang. (3) The wrapper turns control characters (line breaks, tabs, ESC, DEL) in an
exception's `what()` into spaces before handing it over, so the message is one line and can't drive the
terminal. (4) A name part that is a C++ keyword (`phys::new`, `operator`) is refused at the signature with a
hint, before compiling. Not done: telling `f(const double *)` from `f(double *)` apart when a header has both (a
`: list` fits either; the import stops with *more than one overload has the C++ type*; documented).
- **Why:** the reviewer's list; each was a raw compiler message, a hang, or terminal output from library code.
- **Alternatives:** probing for headers with our own search (would disagree with the compiler's); a `: mutable
list` spelling to choose `double *` (new syntax for a rare case; later if asked).

## D324. The same-dimension warning knows Bq vs 1/s and rad/m vs 1/m, fits its hint to the pair, and has no line number in the REPL (red team 15 #7)
- **What:** D306's classifier gives a bare inverse unit (`1/s`, `1/m`, `s⁻¹`, `s^-1`) the kind "plain", so
`A(r [Bq])` then `A(k [1/s])` and `k(q [rad/m])` then `k(q [1/m])` warn; `Hz` then `1/s` doesn't (a hertz is one
per second). The hint's example is ω = 2π f only for Hz vs rad/s, "an angle in rad counts as a plain number" for
rad vs plain, and none otherwise (J vs N m). In the REPL, where every input is line 1, the message leaves out
"(line N)".
- **Why:** the reviewer's cases; a wrong example in a hint is worse than none.
- **Alternatives:** a table of named pairs (the kinds already are one); numbering REPL inputs (a larger change to
the REPL's diagnostics).
