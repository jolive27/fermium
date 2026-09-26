"""The VS Code extension: grammar is valid JSON, symbols match the REPL's, and \\name completion works."""
import json
import os
import shutil
import subprocess

import pytest

from fermium.symbols import LATEX

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
VS = os.path.join(ROOT, "editors", "vscode")


def test_symbols_json_matches_repl():
    assert json.load(open(os.path.join(VS, "symbols.json"), encoding="utf-8")) == LATEX


def test_manifest_and_grammar_are_valid():
    pkg = json.load(open(os.path.join(VS, "package.json")))
    assert pkg["contributes"]["languages"][0]["extensions"] == [".fm"]
    g = json.load(open(os.path.join(VS, "syntaxes", "fermium.tmLanguage.json"), encoding="utf-8"))
    assert g["scopeName"] == "source.fermium"


MOCK = r"""
const Module = require('module');
const orig = Module._load;
let provider = null;
Module._load = function (req, ...rest) {
  if (req === 'vscode-languageclient/node') throw new Error('not installed');   // the fallback path
  if (req === 'vscode') return {
    languages: { registerCompletionItemProvider: (lang, p) => { provider = p; return {}; } },
    CompletionItem: function (label, kind) { this.label = label; },
    CompletionItemKind: { Text: 0 },
    Range: function (a, b) { this.a = a; this.b = b; },
  };
  return orig.call(this, req, ...rest);
};
const ext = require(process.argv[2] + '/extension.js');
ext.activate({ subscriptions: [] });
const out = {};
for (const text of process.argv.slice(3)) {
  const doc = { lineAt: () => ({ text }) };
  const pos = { character: text.length, translate: (l, c) => ({ c }) };
  const items = provider.provideCompletionItems(doc, pos) || [];
  out[text] = items.map(i => i.insertText);
}
console.log(JSON.stringify(out));
"""


@pytest.mark.skipif(shutil.which("node") is None, reason="node not installed")
def test_completion_provider(tmp_path):
    script = tmp_path / "t.js"
    script.write_text(MOCK)
    r = subprocess.run(["node", str(script), VS, "x = \\theta", "\\hba", "a\\^2", "\\int"], capture_output=True,
                       text=True, check=True)
    got = json.loads(r.stdout)
    assert got["x = \\theta"][0] == "θ"
    assert "ħ" in got["\\hba"]
    assert got["a\\^2"][0] == "²"
    assert got["\\int"][0] == "∫"


LSP_MOCK = r"""
const Module = require('module');
const orig = Module._load;
const seen = { provider: false };
Module._load = function (req, ...rest) {
  if (req === 'vscode-languageclient/node') return {
    LanguageClient: function (id, name, server, clientOpts) {
      seen.id = id; seen.server = server; seen.selector = clientOpts.documentSelector;
      this.start = () => { seen.started = true; return Promise.resolve(); };
      this.stop = () => Promise.resolve();
    },
  };
  if (req === 'vscode') return {
    workspace: { getConfiguration: () => ({ get: (k, d) => d }) },
    window: { showWarningMessage: () => {} },
    languages: { registerCompletionItemProvider: () => { seen.provider = true; return {}; } },
  };
  return orig.call(this, req, ...rest);
};
const ext = require(process.argv[2] + '/extension.js');
ext.activate({ subscriptions: [] });
setTimeout(() => console.log(JSON.stringify(seen)), 10);
"""


@pytest.mark.skipif(shutil.which("node") is None, reason="node not installed")
def test_extension_starts_the_language_server(tmp_path):
    script = tmp_path / "l.js"
    script.write_text(LSP_MOCK)
    r = subprocess.run(["node", str(script), VS], capture_output=True, text=True, check=True)
    seen = json.loads(r.stdout)
    assert seen["server"] == {"command": "fermium", "args": ["lsp"]}
    assert seen["started"] and seen["id"] == "fermium"
    assert {"scheme": "file", "language": "fermium"} in seen["selector"]
    assert seen["provider"] is False          # \name completion comes from the server


def test_manifest_declares_language_server_settings():
    pkg = json.load(open(os.path.join(VS, "package.json")))
    props = pkg["contributes"]["configuration"]["properties"]
    assert props["fermium.languageServer.command"]["default"] == "fermium"
    assert props["fermium.languageServer.args"]["default"] == ["lsp"]
    assert "vscode-languageclient" in pkg["dependencies"]
