# Fermium for VS Code

- Syntax highlighting for `.fm` files. Units after numbers (`9.81 m/s²`) are coloured separately from variables.
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
