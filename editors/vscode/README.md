# Fermium for VS Code

- Syntax highlighting for `.fm` files: keywords, comments, text, numbers, built-in functions and constants. A name right after a number (`9.81 m/s²`) is coloured as a unit, following Fermium's rule that a name right after a number is a unit. The highlighter doesn't know which unit names exist, so a variable in that position (the `x` in `2 x`) is coloured as a unit too.
- **Symbol completion**: type `\theta` and press Tab (or pick from the list) to get `θ`. The same names work in the Fermium REPL:
  - `\hbar` → ħ, `\int` → ∫, `\sqrt` → √, `\partial` → ∂, `\pm` → ±
  - `\^2` → ², `\_0` → ₀
  - `\deg` → °, `\AA` → Å, `\Msun` → M☉

## Install (from this folder)

1. Copy or symlink this folder into your VS Code extensions directory:
   - macOS/Linux: `ln -s "$(pwd)" ~/.vscode/extensions/fermium`
   - Windows: copy the folder to `%USERPROFILE%\.vscode\extensions\fermium`
2. Restart VS Code and open a `.fm` file.

(Alternatively, `npx @vscode/vsce package` builds a `.vsix` you can install with "Install from VSIX…".)

## Status

`tests/test_vscode.py` checks that the manifest and grammar are valid JSON, that `symbols.json` is exactly the REPL's symbol table (91 names), and that the completion provider returns the right symbols (run under Node with a stand-in for the VS Code API). The extension hasn't been tried in a running VS Code yet. It has no hover, error underlines or other language-server features.
