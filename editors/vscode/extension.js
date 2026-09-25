// Fermium for VS Code.
// - With the language server (`fermium lsp`, needs `pip install pygls`): errors are underlined as
//   you type, hovering over a name shows its units, and \name completes to a symbol.
// - Without it, \name completion still works (\theta -> θ, \hbar -> ħ, \int -> ∫, \^2 -> ²).
const vscode = require('vscode');
const symbols = require('./symbols.json');

let client = null;

function startLanguageServer(context) {
  let lc;
  try {
    lc = require('vscode-languageclient/node');
  } catch (e) {
    return null;      // `npm install` wasn't run in this folder: fall back to plain completion
  }
  const cfg = vscode.workspace.getConfiguration('fermium');
  if (!cfg.get('languageServer.enable', true)) return null;
  const serverOptions = {
    command: cfg.get('languageServer.command', 'fermium'),
    args: cfg.get('languageServer.args', ['lsp']),
  };
  const clientOptions = { documentSelector: [{ scheme: 'file', language: 'fermium' }, { scheme: 'untitled', language: 'fermium' }] };
  client = new lc.LanguageClient('fermium', 'Fermium', serverOptions, clientOptions);
  client.start().catch((err) => {
    vscode.window.showWarningMessage(
      `The Fermium language server didn't start (${err && err.message}). ` +
      "Check that 'fermium' is on your PATH and pygls is installed (pip install pygls), " +
      'or set fermium.languageServer.command.');
  });
  context.subscriptions.push({ dispose: () => client && client.stop() });
  return client;
}

function registerSymbolCompletion(context) {
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

function activate(context) {
  if (!startLanguageServer(context)) registerSymbolCompletion(context);   // the server does \name itself
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
