// Runs Pyodide + Fermium off the main thread, so a long (or endless) program never freezes the page;
// the Stop button just terminates this worker and starts a fresh one.
//
// Messages in:  {type: "init", pyodide: "auto"|"local"|"cdn", manifest, data}
//               {type: "run", id, code, dir}
// Messages out: {type: "status", text}  {type: "ready", source, version}  {type: "fatal", text}
//               {type: "result", id, stdout, warnings, error, plots, seconds}
"use strict";

const HOME = "/home/pyodide";
let pyodide = null;
let glue = null;

function status(text) {
  self.postMessage({ type: "status", text });
}

async function exists(url) {
  try {
    const r = await fetch(url, { method: "HEAD", cache: "no-store" });
    return r.ok;
  } catch (e) {
    return false;
  }
}

async function init(msg) {
  const m = msg.manifest;
  const local = new URL("pyodide/", self.location).href;
  let indexURL = m.pyodide_cdn;
  let source = "cdn";
  if (msg.pyodide === "local" || (msg.pyodide !== "cdn" && (await exists(local + "fermium-local.json")))) {
    indexURL = local;
    source = "local";
  }
  status(`loading Python (Pyodide ${m.pyodide_version}, ${source === "local" ? "local copy" : "from the CDN"})…`);
  importScripts(indexURL + "pyodide.js");
  pyodide = await loadPyodide({ indexURL });
  status("loading numpy…");
  await pyodide.loadPackage(["numpy"]);
  status("installing Fermium…");
  const whl = await fetch(new URL("gen/" + m.wheel, self.location));
  if (!whl.ok) throw new Error(`could not fetch gen/${m.wheel} (${whl.status}); run  python3 web/build.py`);
  pyodide.unpackArchive(await whl.arrayBuffer(), "wheel");
  // the lab data files the bootcamp and examples read, at the same relative paths as in the repo
  for (const [path, text] of Object.entries(msg.data || {})) {
    const full = `${HOME}/${path}`;
    pyodide.FS.mkdirTree(full.slice(0, full.lastIndexOf("/")));
    pyodide.FS.writeFile(full, text);
  }
  const py = await fetch(new URL("playground.py", self.location));
  pyodide.FS.writeFile(`${HOME}/fermium_playground.py`, await py.text());
  pyodide.runPython(`import sys\nif "${HOME}" not in sys.path: sys.path.insert(0, "${HOME}")`);
  glue = pyodide.pyimport("fermium_playground");
  pyodide.runPython("import fermium.interp");    // warm up: parse + import everything once
  self.postMessage({ type: "ready", source, version: m.pyodide_version });
}

async function run(msg) {
  const pkgs = glue.packages_needed(msg.code).toJs();
  if (pkgs.length) {
    status(`loading ${pkgs.join(" and ")} (first time only)…`);
    await pyodide.loadPackage(pkgs);
  }
  let res = null;
  for (let attempt = 0; attempt < 4; attempt++) {
    status("running…");
    res = JSON.parse(glue.run(msg.code, `${HOME}/${msg.dir || "bootcamp"}`));
    if (!res.missing) break;
    status(`loading ${res.missing} (first time only)…`);    // e.g. sympy, for integrals without limits
    await pyodide.loadPackage([res.missing]);
  }
  if (res.missing) res = { stdout: "", warnings: [], plots: [], error: `could not load ${res.missing} in the browser` };
  self.postMessage({ type: "result", id: msg.id, ...res });
}

self.onmessage = async (ev) => {
  const msg = ev.data;
  try {
    if (msg.type === "init") await init(msg);
    else if (msg.type === "run") await run(msg);
  } catch (e) {
    const text = String((e && e.message) || e);
    if (msg.type === "init") self.postMessage({ type: "fatal", text });
    else self.postMessage({ type: "result", id: msg.id, stdout: "", warnings: [], plots: [],
                            error: "internal error in the playground: " + text });
  }
};
