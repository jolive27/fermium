# Fermium for VS Code

- **Live errors**: unit mistakes and other errors are underlined as you type, with the same one-line message and hint as `fermium run`. Warnings get a yellow underline.
- **Hover shows units**: point at a variable to see e.g. `g: acceleration [m/s²]` or `d: length [m], shown in cm`. Hovering over a function shows its formula and units, and hovering over an ODE solution, a constant (`h`, `G`, `m_e`) or a unit (`km`) describes it too.
- **Symbol completion**: type `\theta` and press Tab (or pick from the list) to get `θ`. The same names work in the Fermium REPL and in Jupyter:
  - `\hbar` → ħ, `\int` → ∫, `\sqrt` → √, `\partial` → ∂, `\pm` → ±
  - `\^2` → ², `\_0` → ₀
  - `\deg` → °, `\AA` → Å, `\Msun` → M☉
- **Syntax highlighting** for `.fm` files: numbers, keywords, constants, strings and comments. Any name right after a number is coloured as a unit (the grammar can't tell `2 m` the unit from `2 m` the variable; the language server's warning can).

Live errors and hover come from the Fermium language server (`fermium lsp`). Without it, the extension still highlights code and completes `\name` symbols.

## Install (from this folder)

1. Install Fermium, including the language server's library:
   `python3 -m pip install -e "../..[full]"` (check with `fermium doctor`).
2. In this folder, run `npm install`. This fetches `vscode-languageclient`.
3. Copy or symlink this folder into your VS Code extensions directory:
   - macOS/Linux: `ln -s "$(pwd)" ~/.vscode/extensions/fermium`
   - Windows: copy the folder to `%USERPROFILE%\.vscode\extensions\fermium`
4. Restart VS Code and open a `.fm` file.

If `fermium` isn't on the PATH VS Code sees (for example, it lives in a virtual environment), set **Fermium › Language Server: Command** to its full path, such as `/path/to/venv/bin/fermium`. You can turn the server off with **Fermium › Language Server: Enable**.

(Alternatively, `npx @vscode/vsce package` builds a `.vsix` you can install with "Install from VSIX…".)
