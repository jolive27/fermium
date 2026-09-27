#!/usr/bin/env node
// `fermium run` on top of the playground's WebAssembly module, for testing it with Node:
//
//   node web/test/run_wasm.js run [--base-dir DIR] FILE.fm
//
// It behaves like the native `fermium run`: the program's output on stdout; warnings, then the error (with the
// file name), on stderr; exit code 1 after an error. The files under the base folder (the program's folder by
// default; up to 3 levels deep, 2 MB each) are copied into the module's in-memory file system first, so
// `load "data/x.csv"` works, and the plots a program writes are saved back to disk.
//
// web/test/fermium-wasm is a wrapper with the same command line as the binary, so the conformance suite can score
// the WebAssembly build:  conformance/run --impl rust --bin web/test/fermium-wasm --out /tmp/wasm.md
// The module: FERMIUM_WASM, else web/gen/fermium.wasm (python3 web/build.py).
"use strict";
const fs = require("fs");
const path = require("path");
const F = require(path.join(__dirname, "..", "fermium.js"));

function usage() {
  process.stderr.write("usage: run_wasm.js run [--base-dir DIR] FILE.fm\n");
  process.exit(2);
}

function collect(dir, depth, out) {
  let entries = [];
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch (e) { return; }
  for (const e of entries) {
    const full = path.join(dir, e.name);
    if (e.isDirectory() && depth > 0 && !e.name.startsWith(".") && e.name !== "node_modules") collect(full, depth - 1, out);
    else if (e.isFile()) {
      try {
        if (fs.statSync(full).size <= 2 << 20) out[full] = fs.readFileSync(full);
      } catch (err) { /* unreadable: skip */ }
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args[0] !== "run") usage();
  let base = null, file = null;
  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--base-dir") base = args[++i];
    else if (args[i] === "--backend") i++;
    else file = args[i];
  }
  if (!file) usage();
  let src;
  try { src = fs.readFileSync(file, "utf8"); } catch (e) {
    process.stderr.write(`can't find the file '${file}'\n`);
    process.exit(1);
  }
  base = path.resolve(base || path.dirname(file));
  const wasm = process.env.FERMIUM_WASM || path.join(__dirname, "..", "gen", "fermium.wasm");
  const mod = await F.compile(fs.readFileSync(wasm));
  const files = {};
  collect(base, 3, files);
  const res = F.run(mod, src, base, files);
  process.stdout.write(res.stdout);
  for (const w of res.warnings) process.stderr.write(w + "\n");
  for (const p of res.plots) {
    const dest = path.resolve(base, p.name);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, Buffer.from(p.base64, "base64"));
  }
  if (res.error) {
    // `fermium run` names the file: "prog.fm, line 3: ..."
    const name = path.basename(file);
    process.stderr.write((/^line \d+: /.test(res.error) ? `${name}, ` : "") + res.error + "\n");
    process.exitCode = 1;
  }
}

main().catch((e) => {
  process.stderr.write(`run_wasm.js: ${e && e.stack || e}\n`);
  process.exit(101);
});
