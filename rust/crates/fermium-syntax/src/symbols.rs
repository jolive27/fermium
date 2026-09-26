//! The `\name` + Tab symbol table shared by the REPL, the language server and the Jupyter kernel
//! (a port of `fermium/symbols.py`; the VS Code extension and the cheat sheet carry the same table).

/// `\name` → symbol, in the order of `fermium/symbols.py` (sorted lookups use [`sorted_names`]).
pub static LATEX: &[(&str, &str)] = &[
    // Greek lowercase
    ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"), ("epsilon", "ε"), ("varepsilon", "ε"),
    ("zeta", "ζ"), ("eta", "η"), ("theta", "θ"), ("vartheta", "θ"), ("iota", "ι"), ("kappa", "κ"),
    ("lambda", "λ"), ("mu", "μ"), ("nu", "ν"), ("xi", "ξ"), ("pi", "π"), ("rho", "ρ"), ("sigma", "σ"),
    ("tau", "τ"), ("upsilon", "υ"), ("phi", "φ"), ("varphi", "φ"), ("chi", "χ"), ("psi", "ψ"), ("omega", "ω"),
    // Greek uppercase
    ("Gamma", "Γ"), ("Delta", "Δ"), ("Theta", "Θ"), ("Lambda", "Λ"), ("Xi", "Ξ"), ("Pi", "Π"), ("Sigma", "Σ"),
    ("Upsilon", "Υ"), ("Phi", "Φ"), ("Psi", "Ψ"), ("Omega", "Ω"),
    // physics & math
    ("hbar", "ħ"), ("int", "∫"), ("integral", "∫"), ("sqrt", "√"), ("cbrt", "∛"), ("partial", "∂"),
    ("nabla", "∇"), ("grad", "∇"), ("pm", "±"), ("infty", "∞"), ("inf", "∞"), ("deg", "°"), ("degree", "°"),
    ("celsius", "°C"), ("degC", "°C"), ("cdot", "·"), ("times", "×"), ("le", "≤"), ("leq", "≤"), ("ge", "≥"),
    ("geq", "≥"), ("ne", "≠"), ("neq", "≠"), ("approx", "≈"), ("AA", "Å"), ("angstrom", "Å"), ("sun", "☉"),
    ("odot", "☉"), ("Msun", "M☉"), ("half", "½"), ("micro", "μ"), ("prime", "′"), ("transpose", "ᵀ"),
    ("imag", "𝑖"),
    // superscripts and subscripts
    ("^0", "⁰"), ("^1", "¹"), ("^2", "²"), ("^3", "³"), ("^4", "⁴"), ("^5", "⁵"), ("^6", "⁶"), ("^7", "⁷"),
    ("^8", "⁸"), ("^9", "⁹"), ("^-", "⁻"), ("^-1", "⁻¹"), ("^-2", "⁻²"), ("^-3", "⁻³"),
    ("_0", "₀"), ("_1", "₁"), ("_2", "₂"), ("_3", "₃"), ("_4", "₄"), ("_5", "₅"), ("_6", "₆"), ("_7", "₇"),
    ("_8", "₈"), ("_9", "₉"),
];

/// The symbol for `\name`, if there is one.
pub fn lookup(name: &str) -> Option<&'static str> {
    LATEX.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Every (name, symbol), sorted by name as Python's `sorted(LATEX)` sorts them (by code point).
pub fn sorted_names() -> Vec<(&'static str, &'static str)> {
    let mut v = LATEX.to_vec();
    v.sort_by(|a, b| a.0.cmp(b.0));
    v
}

/// Given text ending in `\name`, the candidate replacements for the whole fragment (symbols.py `complete`).
pub fn complete(fragment: &str) -> Vec<String> {
    let Some(idx) = fragment.rfind('\\') else { return vec![] };
    let (prefix, name) = (&fragment[..idx], &fragment[idx + 1..]);
    if let Some(s) = lookup(name) {
        return vec![format!("{prefix}{s}")];
    }
    if name.is_empty() {
        return vec![];
    }
    sorted_names().into_iter().filter(|(k, _)| k.starts_with(name)).map(|(_, v)| format!("{prefix}{v}")).collect()
}

/// Replace every `\name` in a line by its symbol (symbols.py `expand_all`: the regex
/// `\\(\^-?\d|_\d|\^-|[A-Za-z]+)`, unknown names left as they are).
pub fn expand_all(text: &str) -> String {
    let cs: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '\\' {
            out.push(cs[i]);
            i += 1;
            continue;
        }
        let rest = &cs[i + 1..];
        let digit = |c: Option<&char>| c.map(|c| c.is_ascii_digit()).unwrap_or(false);
        let n = if rest.first() == Some(&'^') {
            if rest.get(1) == Some(&'-') && digit(rest.get(2)) {
                3
            } else if digit(rest.get(1)) {
                2
            } else if rest.get(1) == Some(&'-') {
                2
            } else {
                0
            }
        } else if rest.first() == Some(&'_') {
            if digit(rest.get(1)) {
                2
            } else {
                0
            }
        } else {
            rest.iter().take_while(|c| c.is_ascii_alphabetic()).count()
        };
        if n == 0 {
            out.push('\\');
            i += 1;
            continue;
        }
        let name: String = rest[..n].iter().collect();
        match lookup(&name) {
            Some(s) => out.push_str(s),
            None => {
                out.push('\\');
                out.push_str(&name);
            }
        }
        i += 1 + n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_like_v1() {
        assert_eq!(complete("x = \\omega"), vec!["x = ω"]);
        assert_eq!(complete("\\var"), vec!["ε", "φ", "θ"]);
        assert_eq!(complete("\\"), Vec::<String>::new());
        assert_eq!(complete("abc"), Vec::<String>::new());
    }

    #[test]
    fn expands_like_v1() {
        assert_eq!(expand_all("\\theta = 2\\pi r\\^2"), "θ = 2π r²");
        assert_eq!(expand_all("x\\^-1 \\^-"), "x⁻¹ ⁻");
        assert_eq!(expand_all("\\nope \\_3 \\"), "\\nope ₃ \\");
        assert_eq!(expand_all("\\^12"), "¹2");
    }
}
