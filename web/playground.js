// Fermium playground page: editor, examples, \name completion, and a Web Worker running Fermium's
// WebAssembly module (worker.js, fermium.js).
"use strict";

const $ = (id) => document.getElementById(id);
const code = $("code");
const output = $("output");
const statusEl = $("status");
const runBtn = $("run");
const stopBtn = $("stop");
const select = $("examples");
const completeEl = $("complete");
const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
$("runkey").textContent = isMac ? "⌘↵" : "Ctrl+Enter";

let manifest = null, examples = null, symbols = {};
let worker = null, runId = 0, currentDir = "bootcamp";
const params = new URLSearchParams(location.search);

function setState(state, text) {
  document.body.dataset.state = state;
  if (text !== undefined) statusEl.textContent = text;
  runBtn.disabled = state !== "ready";
  stopBtn.hidden = state !== "running";
}

function block(cls, text) {
  const pre = document.createElement("pre");
  pre.className = cls;
  pre.textContent = text;
  output.appendChild(pre);
  return pre;
}

function saveDraft() {
  try { localStorage.setItem("fermium-playground-draft", JSON.stringify({ code: code.value, dir: currentDir, ex: select.value })); }
  catch (e) { /* storage may be unavailable */ }
}

function loadDraft() {
  try { return JSON.parse(localStorage.getItem("fermium-playground-draft") || "null"); }
  catch (e) { return null; }
}

// ------------------------------------------------------------------ worker
function startWorker() {
  if (worker) worker.terminate();
  worker = new Worker("worker.js");
  setState("loading", "starting Fermium…");
  worker.onmessage = (ev) => {
    const m = ev.data;
    if (m.type === "status") {
      statusEl.textContent = m.text;
    } else if (m.type === "ready") {
      setState("ready", `ready · Fermium ${m.version} (WebAssembly)`);
    } else if (m.type === "fatal") {
      setState("fatal", "could not start");
      output.textContent = "";
      block("error", "The playground could not start Fermium in this browser:\n" + m.text);
    } else if (m.type === "result" && m.id === runId) {
      showResult(m);
    }
  };
  worker.onerror = (ev) => {
    setState("fatal", "could not start");
    block("error", "The playground's worker failed: " + (ev.message || "unknown error"));
  };
  worker.postMessage({ type: "init", manifest, data: examples.data });
}

function run() {
  if (document.body.dataset.state !== "ready") return;
  output.textContent = "";
  hideCompletions();
  runId += 1;
  setState("running", "running…");
  worker.postMessage({ type: "run", id: runId, code: code.value, dir: currentDir });
}

function stop() {
  block("error", "stopped");
  startWorker();       // the only way to interrupt WebAssembly in a worker: throw it away and start again
}

function showResult(r) {
  output.textContent = "";
  for (const w of r.warnings || []) block("warning", w);
  if (r.stdout) block("stdout", r.stdout.replace(/\n$/, ""));
  if (r.error) block("error", r.error);
  for (const p of r.plots || []) {
    const fig = document.createElement("figure");
    const img = document.createElement("img");
    img.src = `data:${p.mime || "image/png"};base64,${p.base64}`;
    img.alt = "plot " + p.name;
    const cap = document.createElement("figcaption");
    cap.textContent = p.name;
    fig.append(img, cap);
    output.appendChild(fig);
  }
  if (!r.stdout && !r.error && !(r.plots || []).length && !(r.warnings || []).length) {
    block("note", "(the program printed nothing)");
  }
  output.dataset.done = String(runId);
  setState("ready", r.error ? "finished with an error" : `finished in ${(r.seconds ?? 0).toFixed(2)} s`);
}

// ------------------------------------------------------------------ examples
function fillExamples() {
  select.textContent = "";
  examples.groups.forEach((g, gi) => {
    const og = document.createElement("optgroup");
    og.label = g.group;
    g.items.forEach((it, ii) => {
      const o = document.createElement("option");
      o.value = `${gi}:${ii}`;
      o.textContent = it.title;
      og.appendChild(o);
    });
    select.appendChild(og);
  });
}

function pickExample(value) {
  const [gi, ii] = value.split(":").map(Number);
  const it = examples.groups[gi] && examples.groups[gi].items[ii];
  if (!it) return;
  select.value = value;
  code.value = it.code;
  currentDir = it.dir;
  output.textContent = "";
  saveDraft();
}

// ------------------------------------------------------------------ \name + Tab completion
// Same rule as fermium/symbols.py complete(): an exact name wins, otherwise complete a unique prefix.
const NAME_RE = /\\(\^-?\d?|_\d?|[A-Za-z]*)$/;

function hideCompletions() {
  completeEl.textContent = "";
  completeEl.classList.remove("open");
}

function replaceBefore(n, text) {
  const s = code.selectionStart;
  code.setRangeText(text, s - n, s, "end");
  code.dispatchEvent(new Event("input"));
}

function tryComplete() {
  const s = code.selectionStart;
  if (s !== code.selectionEnd) return false;
  const before = code.value.slice(Math.max(0, s - 12), s);
  const m = before.match(NAME_RE);
  if (!m) return false;
  const name = m[1];
  if (name && Object.prototype.hasOwnProperty.call(symbols, name)) {
    replaceBefore(m[0].length, symbols[name]);
    hideCompletions();
    return true;
  }
  const cands = Object.keys(symbols).filter((k) => k.startsWith(name)).sort();
  if (!name || !cands.length) {
    hideCompletions();
    return name.length > 0 || cands.length > 0;   // a lone "\" + Tab: swallow it, don't indent
  }
  const distinct = [...new Set(cands.map((k) => symbols[k]))];
  if (distinct.length === 1) {
    replaceBefore(m[0].length, distinct[0]);
    hideCompletions();
    return true;
  }
  let common = cands[0];
  for (const c of cands) while (!c.startsWith(common)) common = common.slice(0, -1);
  if (common.length > name.length) replaceBefore(name.length, common);
  completeEl.textContent = "";
  for (const k of cands.slice(0, 24)) {
    const b = document.createElement("button");
    b.type = "button";
    b.append(`\\${k} `);
    const sym = document.createElement("b");
    sym.textContent = symbols[k];
    b.appendChild(sym);
    b.onmousedown = (e) => {
      e.preventDefault();
      const mm = code.value.slice(0, code.selectionStart).match(NAME_RE);
      if (mm) replaceBefore(mm[0].length, symbols[k]);
      hideCompletions();
      code.focus();
    };
    completeEl.appendChild(b);
  }
  completeEl.classList.add("open");
  return true;
}

code.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
    e.preventDefault();
    run();
  } else if (e.key === "Tab" && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
    e.preventDefault();
    if (!tryComplete()) replaceBefore(0, "    ");
  } else if (e.key === "Escape") {
    hideCompletions();
  } else if (e.key === "Enter") {
    // keep the indentation of the current line
    const s = code.selectionStart;
    const lineStart = code.value.lastIndexOf("\n", s - 1) + 1;
    const indent = code.value.slice(lineStart, s).match(/^[ \t]*/)[0];
    if (indent) {
      e.preventDefault();
      replaceBefore(0, "\n" + indent);
    }
  }
});
code.addEventListener("input", () => { saveDraft(); if (completeEl.classList.contains("open") && !code.value.slice(0, code.selectionStart).match(NAME_RE)) hideCompletions(); });
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && e.target !== code) { e.preventDefault(); run(); }
});
runBtn.addEventListener("click", run);
stopBtn.addEventListener("click", stop);
select.addEventListener("change", () => pickExample(select.value));

// ------------------------------------------------------------------ start
async function getJSON(path) {
  const r = await fetch(path, { cache: "no-cache" });
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
  return r.json();
}

(async () => {
  try {
    [manifest, examples, symbols] = await Promise.all(
      ["gen/manifest.json", "gen/examples.json", "gen/symbols.json"].map(getJSON));
  } catch (e) {
    setState("fatal", "not built");
    block("error", "The playground files are missing (" + e.message + ").\n" +
      "Build them first:  python3 web/build.py\nthen serve:        python3 -m http.server -d web 8000");
    return;
  }
  if (!manifest.wasm) {
    setState("fatal", "not built");
    block("error", "gen/fermium.wasm is missing.\nBuild it first:  python3 web/build.py   (needs Rust and " +
      "rustup target add wasm32-unknown-unknown)");
    return;
  }
  fillExamples();
  const draft = params.has("example") ? null : loadDraft();
  if (params.has("example")) pickExample(params.get("example"));
  else if (draft && draft.code) {
    code.value = draft.code;
    currentDir = draft.dir || "bootcamp";
    if (draft.ex) select.value = draft.ex;
  } else pickExample("0:0");
  startWorker();
})();
