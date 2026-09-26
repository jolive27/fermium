// Runs Fermium (gen/fermium.wasm, the Rust compiler's tree-walker as WebAssembly) off the main thread, so a
// long (or endless) program never freezes the page; the Stop button just terminates this worker and starts a
// fresh one.
//
// Messages in:  {type: "init", manifest, data}      data: {"bootcamp/data/pendulum.csv": text, ...}
//               {type: "run", id, code, dir}
// Messages out: {type: "status", text}  {type: "ready", version, bytes}  {type: "fatal", text}
//               {type: "result", id, stdout, warnings, error, plots, seconds}
"use strict";
importScripts("fermium.js");

let compiled = null;
let files = {};

async function init(msg) {
  const m = msg.manifest;
  self.postMessage({ type: "status", text: `loading Fermium ${m.version} (${((m.wasm_bytes || 0) / 1e6).toFixed(1)} MB)…` });
  compiled = await FermiumWasm.compile(fetch(new URL("gen/" + m.wasm, self.location)));
  // the lab data files the bootcamp and examples read, at the same relative paths as in the repository
  files = {};
  for (const [path, text] of Object.entries(msg.data || {})) files["/" + path] = text;
  FermiumWasm.run(compiled, "x = 1\n", "/bootcamp", {});      // warm up (and check the module works)
  self.postMessage({ type: "ready", version: m.version, bytes: m.wasm_bytes });
}

function run(msg) {
  const res = FermiumWasm.run(compiled, msg.code, "/" + (msg.dir || "bootcamp"), files);
  self.postMessage({ type: "result", id: msg.id, ...res });
}

self.onmessage = async (ev) => {
  const msg = ev.data;
  try {
    if (msg.type === "init") await init(msg);
    else if (msg.type === "run") run(msg);
  } catch (e) {
    const text = String((e && e.message) || e);
    if (msg.type === "init") self.postMessage({ type: "fatal", text });
    else self.postMessage({ type: "result", id: msg.id, stdout: "", warnings: [], plots: [],
                            error: "internal error in the playground: " + text });
  }
};
