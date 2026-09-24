"""The VS Code extension: grammar is valid JSON, symbols match the REPL's, and \\name completion works."""
import json
import os
import shutil
import subprocess

import pytest

from fermium.symbols import LATEX

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
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
