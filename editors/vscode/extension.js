// Fermium for VS Code: \name completion (\theta -> θ, \hbar -> ħ, \int -> ∫, \^2 -> ²).
// Type a backslash and a name; pick the symbol from the list (Tab or Enter).
const vscode = require('vscode');
const symbols = require('./symbols.json');

function activate(context) {
  const provider = vscode.languages.registerCompletionItemProvider('fermium', {
    provideCompletionItems(document, position) {
      const line = document.lineAt(position).text.slice(0, position.character);
      const m = line.match(/\\([A-Za-z]*|\^-?\d?|_\d?)$/);
      if (!m) return undefined;
      const start = position.translate(0, -m[0].length);
      const range = new vscode.Range(start, position);
      return Object.entries(symbols)
        .filter(([name]) => name.startsWith(m[1]))
        .map(([name, sym]) => {
          const item = new vscode.CompletionItem(`\\${name}`, vscode.CompletionItemKind.Text);
          item.insertText = sym;
          item.detail = sym;
          item.filterText = `\\${name}`;
          item.range = range;
          item.sortText = (name === m[1] ? '0' : '1') + name;
          return item;
        });
    }
  }, '\\');
  context.subscriptions.push(provider);
}

function deactivate() {}
module.exports = { activate, deactivate };
