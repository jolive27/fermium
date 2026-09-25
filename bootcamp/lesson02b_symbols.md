# Lesson 2b — Symbols: π, θ, √ and friends

Physics is written with symbols: π, θ, ω, √, ∫, x². Fermium understands all of them. But your keyboard doesn't have a π key, so Fermium gives you **three ways** to get them. You never *have* to use symbols: everything has a plain-keyboard spelling that means exactly the same thing.

## Step 1: the plain-keyboard (ASCII) spellings

"ASCII" (say *ask-ee*) just means the ordinary characters on an English keyboard. Every symbol in Fermium has an ASCII spelling:

| You want | Type | Example |
|---|---|---|
| π | `pi` | `2 pi r` |
| θ, ω, λ, … | the Greek letter's name: `theta`, `omega`, `lambda`, … | `omega = 2 pi f` |
| ω₀ (subscript) | `omega_0` | `omega_0 = sqrt(k / m)` |
| √x | `sqrt(x)` | `sqrt(g / L)` |
| x² | `x^2` | `v^2`, `r^-2`, `x^(1/3)` |
| · or × (multiply) | `*` | `2 * pi` |
| ≤ ≥ ≠ ≈ | `<=` `>=` `!=` `~=` | `if x <= 0 m` |
| ° (degrees) | `deg` | `30 deg` |
| ∫ | `integral` | `integral x^2 dx from 0 to 1` |
| ∂ | `partial` | `partial/partial x f` |
| ∞ | `inf` | `from 0 to inf` |
| ħ | `hbar` | `hbar omega` |

Here's a pendulum program written only with keys you already have:

```fermium
# Pendulum, plain ASCII
L = 1.20 m
g = 9.81 m/s^2
theta_0 = 10 deg
omega = sqrt(g / L)
T = 2 * pi / omega
print "omega =", omega in rad/s
print "T =", T
if theta_0 <= 15 deg
    print "small angle: the formula is accurate"
```

<!-- output -->
```
omega = 2.86 rad/s
T = 2.20 s
small angle: the formula is accurate
```

(`if` is new; it's explained properly in Lesson 4. It runs the indented line only when the condition is true.)

Greek names are converted *piece by piece* between underscores, so `omega_0`, `ω_0` and `ω₀` are all the **same** variable. That's handy: you can type `omega_0` today and use `ω₀` tomorrow.

## Step 2: the upgrade — symbols

The same program with symbols looks like a page from your textbook:

```fermium
# Pendulum, with symbols
L = 1.20 m
g = 9.81 m/s²
θ₀ = 10°
ω = √(g / L)
T = 2π / ω
print "omega =", ω in rad/s
print "T =", T
if θ₀ ≤ 15°
    print "small angle: the formula is accurate"
```

<!-- output -->
```
omega = 2.86 rad/s
T = 2.20 s
small angle: the formula is accurate
```

Same output. It's purely a matter of taste. Here are three ways to type the symbols.

### Way 1: `\name` then Tab (REPL and VS Code)

In the Fermium REPL, type a backslash, the symbol's name, and press **Tab**:

```
fm> \theta          ← now press Tab
fm> θ
```

It works for everything: `\omega` → ω, `\pi` → π, `\sqrt` → √, `\int` → ∫, `\hbar` → ħ, `\^2` → ², `\_0` → ₀, `\le` → ≤, `\deg` → °, `\half` → ½, `\infty` → ∞. Here's a real REPL session:

```
fm> \theta = 30 \deg
fm> print sin(\theta)
0.5
```

(Pressing Return also converts any `\name` left in the line.) If you type the start of a name and press Tab twice, you'll see all the matches.

The **VS Code extension** in `editors/vscode/` does the same thing while you edit `.fm` files: type `\theta`, press Tab, get `θ`. The names are the same as in LaTeX, so if you've written a physics report in LaTeX, you already know them. All of them are listed on the [cheat sheet](CHEATSHEET.md).

### Way 2: write ASCII, then convert with `fermium fmt --pretty`

Write your program in plain ASCII, then let Fermium rewrite it with symbols. Suppose `pendulum.fm` contains the first (ASCII) program above. In the Terminal:

```
fermium fmt pendulum.fm --pretty
```

prints the converted program:

```
# Pendulum, plain ASCII
L = 1.20 m
g = 9.81 m/s²
θ₀ = 10 °
ω = √(g / L)
T = 2 · π / ω
print "omega =", ω in rad/s
print "T =", T
if θ₀ ≤ 15 °
    print "small angle: the formula is accurate"
```

Notice what changed (`pi` → `π`, `sqrt` → `√`, `^2` → `²`, `theta_0` → `θ₀`, `<=` → `≤`, `deg` → `°`, `*` → `·`) and what didn't: comments and text in quotes stay as you wrote them, and so does the meaning. To actually change the file rather than just print the new version, add `-w` ("write"):

```
fermium fmt pendulum.fm --pretty -w
```

And to go back to plain ASCII, for example to email code to someone or paste it into a system that dislikes symbols:

```
fermium fmt pendulum.fm --ascii
```

Converting back and forth never changes what the program does.

### Way 3: Mac Option-key shortcuts

On a Mac, holding **Option** (⌥) while pressing a key types a special character. These work in *any* app, including VS Code and the Terminal (with a US or British keyboard layout):

| Keys | Symbol |
|---|---|
| Option + P | π |
| Option + V | √ |
| Option + B | ∫ |
| Option + D | ∂ |
| Option + 5 | ∞ |
| Option + Shift + 8 | ° |
| Option + , (comma) | ≤ |
| Option + . (period) | ≥ |
| Option + = | ≠ |
| Option + X | ≈ |
| Option + Z | Ω |
| Option + M | µ (micro) |
| Option + Shift + = | ± |

π, √ and ∫ come up constantly, so these three are worth memorising.

> ± is for **uncertainties**: `L = 5.0 ± 0.2 m` is a measurement with its uncertainty, and formulas that use it propagate the uncertainty. [Lesson 12](lesson12_lab_report.md) shows how.

> The micro sign µ (Option + M) and the Greek letter μ look identical. Fermium treats them as the same letter, so `5 µm` and `5 μm` both work. (You can also just type `um`.)

## Things to watch out for with symbols

- **Superscripts are exponents:** `x²` is `x^2`, `r⁻¹` is `r^-1`.
- **Put a space between single-letter names:** `ω t` is ω × t, but `ωt` is a single name "ωt" (just like `LT` in Lesson 2). The exception is π, which is always on its own: `2πf` is 2 × π × f.
- **Look-alikes:** Latin `v` and Greek `ν` (nu) look almost the same, as do `p` and `ρ` (rho). If a program uses both, Fermium warns you, because it's very likely a mistake.
- **Some names have no ASCII spelling**, e.g. `ΔE` (it's one name that starts with Δ). `fermium fmt --ascii` leaves those alone and tells you so. If you want pure ASCII, call it `dE` or `delta_E`.

## Summary

- Everything can be written in plain ASCII: `pi`, `theta`, `sqrt(x)`, `x^2`, `integral`, `<=`, `deg`.
- Upgrade 1: `\theta` + Tab → θ in the REPL and VS Code.
- Upgrade 2: `fermium fmt file.fm --pretty` converts a whole file to symbols (`--ascii` converts back; `-w` saves it).
- Upgrade 3: Option + P (π), Option + V (√), Option + B (∫), Option + D (∂), Option + Shift + = (±).

## Exercises

1. **Translate to ASCII.** Rewrite `E = ½ m v² where m = 2 kg, v = 3 m/s` using only ASCII characters, and run it. (Remember the gotcha from Lesson 2!)
2. **Translate to symbols.** Rewrite `omega_0 = sqrt(k / m) where k = 50 N/m, m = 0.5 kg` with symbols (ω₀, √), and print `omega_0 in rad/s`.
3. **Use the formatter.** Save the ASCII program from exercise 1 as `ke.fm`, run `fermium fmt ke.fm --pretty`, then `fermium fmt ke.fm --pretty -w`, then `fermium fmt ke.fm --ascii`. Run the program after each step: the answer should never change.
4. **Tab completion.** Start the REPL and use `\name` + Tab to type `print 2π √(1.0 m / 9.81 m/s²)`. Which physical quantity is this?
5. **Two names, one variable.** Make a program with the line `theta = 30 deg` and then `print sin(θ)`. Does it work? Why?

Solutions: [solutions/lesson02b.md](solutions/lesson02b.md)

**Next:** [Lesson 3 — Functions](lesson03_functions.md)
