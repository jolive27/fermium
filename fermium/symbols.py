"""The `\\name<TAB>` symbol table shared by the REPL, the VS Code extension and the cheat sheet."""

LATEX = {
    # Greek lowercase
    "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ε", "varepsilon": "ε", "zeta": "ζ",
    "eta": "η", "theta": "θ", "vartheta": "θ", "iota": "ι", "kappa": "κ", "lambda": "λ", "mu": "μ", "nu": "ν",
    "xi": "ξ", "pi": "π", "rho": "ρ", "sigma": "σ", "tau": "τ", "upsilon": "υ", "phi": "φ", "varphi": "φ",
    "chi": "χ", "psi": "ψ", "omega": "ω",
    # Greek uppercase
    "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ", "Pi": "Π", "Sigma": "Σ",
    "Upsilon": "Υ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
    # physics & math
    "hbar": "ħ", "int": "∫", "integral": "∫", "sqrt": "√", "cbrt": "∛", "partial": "∂", "nabla": "∇", "grad": "∇", "pm": "±",
    "infty": "∞", "inf": "∞", "deg": "°", "degree": "°", "celsius": "°C", "degC": "°C",
    "cdot": "·", "times": "×", "le": "≤", "leq": "≤", "ge": "≥", "geq": "≥", "ne": "≠", "neq": "≠",
    "approx": "≈", "AA": "Å", "angstrom": "Å", "sun": "☉", "odot": "☉", "Msun": "M☉", "half": "½",
    "micro": "μ", "prime": "′", "transpose": "ᵀ",
    # superscripts and subscripts
    "^0": "⁰", "^1": "¹", "^2": "²", "^3": "³", "^4": "⁴", "^5": "⁵", "^6": "⁶", "^7": "⁷", "^8": "⁸",
    "^9": "⁹", "^-": "⁻", "^-1": "⁻¹", "^-2": "⁻²", "^-3": "⁻³",
    "_0": "₀", "_1": "₁", "_2": "₂", "_3": "₃", "_4": "₄", "_5": "₅", "_6": "₆", "_7": "₇", "_8": "₈", "_9": "₉",
}


def complete(fragment: str):
    """Given text ending in \\name, return candidate replacements for the whole fragment."""
    idx = fragment.rfind("\\")
    if idx < 0:
        return []
    prefix, name = fragment[:idx], fragment[idx + 1:]
    if name in LATEX:
        return [prefix + LATEX[name]]
    return [prefix + LATEX[k] for k in sorted(LATEX) if k.startswith(name)] if name else []


def expand_all(text: str) -> str:
    """Replace every \\name in a line by its symbol (used when TAB completion isn't available)."""
    import re

    def rep(m):
        return LATEX.get(m.group(1), m.group(0))
    return re.sub(r"\\(\^-?\d|_\d|\^-|[A-Za-z]+)", rep, text)
