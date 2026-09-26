#!/usr/bin/env node
// Differential test of the playground: every example of the page (web/gen/examples.json: the bootcamp lessons'
// programs and examples/*.fm) run by the WebAssembly module (web/gen/fermium.wasm, through web/fermium.js as the
// page runs it) and by the native `fermium run`; stdout, warnings and errors must be the same.
//
//   node web/test/compare_native.js [--bin rust/target/release/fermium] [--backend interp|auto|llvm] [--only TEXT] [-v]
//
// --backend: the native back end (default interp, the one the page runs, so a difference is the browser build's;
// auto compares with what a plain `fermium run` picks).
//
// The native binary: --bin, else FERMIUM_BIN, else rust/target/{release,fast,debug}/fermium. Both run the program
// from a folder holding the example data files (the page's in-memory files; a temporary folder natively).
// Known divergences (rust/DIVERGENCES.md, "The browser playground"): last-digit differences in numbers printed
// to 16-17 digits (the browser's math library isn't the C library), and recursion depth. A program whose
// only difference is in digits beyond the 12th significant digit counts as "digits", not as a failure.
// Exit code 1 if any example differs otherwise.
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const F = require(path.join(__dirname, "..", "fermium.js"));

const WEB = path.join(__dirname, "..");
const ROOT = path.join(WEB, "..");

function args() {
  const a = process.argv.slice(2);
  const o = { bin: process.env.FERMIUM_BIN || null, only: null, verbose: false, backend: "interp" };
  for (let i = 0; i < a.length; i++) {
    if (a[i] === "--bin") o.bin = a[++i];
    else if (a[i] === "--only") o.only = a[++i];
    else if (a[i] === "--backend") o.backend = a[++i];
    else if (a[i] === "-v") o.verbose = true;
  }
  if (!o.bin) {
    for (const p of ["release", "fast", "debug"]) {
      const b = path.join(ROOT, "rust", "target", p, "fermium");
      if (fs.existsSync(b)) { o.bin = b; break; }
    }
  }
  if (!o.bin) throw new Error("no native fermium: pass --bin (cargo build --release in rust/)");
  o.bin = path.resolve(o.bin);
  return o;
}

// "prog.fm, line 3: ..." (native) and "line 3: ..." (the page) are the same error
function normalize(text, tmp) {
  return text.split(tmp).join("").replace(/^program\.fm, (line \d+: )/gm, "$1");
}

// same text except for digits beyond the 12th significant digit of some numbers
function onlyDigits(a, b) {
  const num = /-?\d+\.\d{12,}(e[-+]?\d+)?/g;
  if (a.replace(num, "#") !== b.replace(num, "#")) return false;
  const na = a.match(num) || [], nb = b.match(num) || [];
  return na.every((x, i) => Math.abs(Number(x) - Number(nb[i])) <= 1e-11 * Math.max(Math.abs(Number(x)), 1e-300));
}

function firstDiff(a, b) {
  const la = a.split("\n"), lb = b.split("\n");
  for (let i = 0; i < Math.max(la.length, lb.length); i++) {
    if (la[i] !== lb[i]) return `line ${i + 1}:\n      wasm:   ${JSON.stringify(la[i])}\n      native: ${JSON.stringify(lb[i])}`;
  }
  return "";
}

// Examples where the browser's pure-Rust math library (the libm crate, instead of glibc) changes printed digits or
// plot pixels; see "browser playground" in rust/DIVERGENCES.md. Anything else that differs fails the test.
const LIBM_DIVERGENT = [
  "04 — Kepler orbit: the Earth around the Sun",      // sin/cos last bits, amplified over many orbit steps
  "03 — A damped mass on a spring",                   // the same, in the plotted curve's pixels
];

async function main() {
  const o = args();
  const ex = JSON.parse(fs.readFileSync(path.join(WEB, "gen", "examples.json"), "utf8"));
  const wasm = process.env.FERMIUM_WASM || path.join(WEB, "gen", "fermium.wasm");
  const mod = await F.compile(fs.readFileSync(wasm));
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "fermium-wasm-"));
  const files = {};
  for (const [p, text] of Object.entries(ex.data)) {
    files["/" + p] = text;
    fs.mkdirSync(path.dirname(path.join(tmp, p)), { recursive: true });
    fs.writeFileSync(path.join(tmp, p), text);
  }
  const counts = { same: 0, digits: 0, differ: 0, errors: 0 };
  const differ = [];
  for (const g of ex.groups) {
    for (const it of g.items) {
      const title = `${g.group} / ${it.title}`;
      if (o.only && !title.includes(o.only)) continue;
      const dir = path.join(tmp, it.dir);
      fs.mkdirSync(dir, { recursive: true });
      const file = path.join(dir, "program.fm");
      fs.writeFileSync(file, it.code);
      const n = spawnSync(o.bin, ["run", "--backend", o.backend, "--base-dir", dir, file], { encoding: "utf8", timeout: 120000, cwd: dir });
      if (n.error) throw new Error(`could not run ${o.bin}: ${n.error.message}`);
      const r = F.run(mod, it.code, "/" + it.dir, files);
      let wErr = r.warnings.map((w) => w + "\n").join("") + (r.error ? r.error + "\n" : "");
      const native = { out: normalize(n.stdout, tmp), err: normalize(n.stderr, tmp), exit: n.status };
      const page = { out: r.stdout, err: normalize(wErr, tmp), exit: r.error ? 1 : 0 };
      // plots: same files (the native run wrote them into the folder; the page got them back)
      for (const p of r.plots) {
        const f = path.join(dir, p.name);
        if (!fs.existsSync(f)) page.err += `(the page got a plot ${p.name} that the native run didn't write)\n`;
        else if (Buffer.compare(fs.readFileSync(f), Buffer.from(p.base64, "base64")) !== 0) {
          page.err += `(the plot ${p.name} differs from the native one)\n`;
        }
      }
      const a = page.out + "\u0000" + page.err + page.exit, b = native.out + "\u0000" + native.err + native.exit;
      let kind = a === b ? "same" : onlyDigits(a, b) ? "digits" : "differ";
      if (kind === "differ" && LIBM_DIVERGENT.some((t) => title.includes(t))) kind = "digits";
      counts[kind]++;
      if (page.exit) counts.errors++;
      if (kind !== "same" && (o.verbose || kind === "differ")) {
        const where = page.out !== native.out ? "stdout " + firstDiff(page.out, native.out)
          : page.err !== native.err ? "stderr " + firstDiff(page.err, native.err) : `exit ${page.exit} vs ${native.exit}`;
        const msg = `  ${kind}: ${title}\n    ${where}`;
        if (kind === "differ") differ.push(msg); else console.log(msg);
      }
      if (o.verbose && kind === "same") console.log(`  same: ${title}`);
    }
  }
  for (const m of differ) console.log(m);
  fs.rmSync(tmp, { recursive: true, force: true });
  const total = counts.same + counts.digits + counts.differ;
  console.log(`${total} examples: ${counts.same} identical, ${counts.digits} differ only in digits beyond the 12th or in the listed math-library cases, ` +
              `${counts.differ} differ (wasm ${path.relative(ROOT, wasm)} vs ${o.bin}); ${counts.errors} end with an error`);
  process.exitCode = counts.differ ? 1 : 0;
}

main().catch((e) => {
  console.error(e && e.stack || e);
  process.exit(2);
});
