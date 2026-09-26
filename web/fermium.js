// Fermium's WebAssembly module (web/fermium.wasm, built from rust/crates/fermium-wasm) behind a small API.
// A classic script with no dependencies, shared by the page's worker (importScripts) and the Node test
// (web/test/run_wasm.js, require):
//
//   const mod = await FermiumWasm.compile(fetch("fermium.wasm"));     // or the bytes
//   const res = FermiumWasm.run(mod, code, "/bootcamp", files);        // files: {"/bootcamp/data/x.csv": text}
//   // res: {stdout, warnings: [..], error: text|null, plots: [{name, mime, base64}], seconds}
//
// Every run gets a fresh instance of the compiled module (instantiating is cheap): nothing a program leaves
// behind (warnings shown once, a trap in the middle of a run) can change the next run, as with `fermium run`.
(function (root) {
  "use strict";
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  const clock = () => (typeof performance !== "undefined" ? performance.now() : Date.now());

  const DEEP = "this program recurses or nests too deeply for the browser playground " +
    "(its stack is much smaller than a desktop's)\n  hint: run it with  fermium run  on your computer";

  async function compile(source) {
    source = await source;
    if (typeof Response !== "undefined" && source instanceof Response) {
      if (!source.ok) throw new Error(`could not fetch fermium.wasm (HTTP ${source.status})`);
      if (WebAssembly.compileStreaming && (source.headers.get("content-type") || "").includes("application/wasm")) {
        return WebAssembly.compileStreaming(source);
      }
      source = await source.arrayBuffer();
    }
    return WebAssembly.compile(source);
  }

  function instantiate(module, files) {
    const inst = new WebAssembly.Instance(module, { env: { fermium_now_ms: clock } });
    const x = inst.exports;
    const put = (bytes) => {
      const p = x.alloc(bytes.length);
      new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
      return [p, bytes.length];
    };
    const read = (p) => {
      const n = new DataView(x.memory.buffer).getUint32(p, true);
      return dec.decode(new Uint8Array(x.memory.buffer, p + 4, n));
    };
    for (const [path, text] of Object.entries(files || {})) {
      const [pp, pn] = put(enc.encode(path));
      const [dp, dn] = put(typeof text === "string" ? enc.encode(text) : text);
      x.put_file(pp, pn, dp, dn);
      x.dealloc(pp, pn);
      x.dealloc(dp, dn);
    }
    return { x, put, read };
  }

  function run(module, code, dir, files) {
    const t0 = clock();
    const { x, put, read } = instantiate(module, files);
    const [sp, sn] = put(enc.encode(code));
    const [dp, dn] = put(enc.encode(dir || "/"));
    let res;
    try {
      res = JSON.parse(read(x.run_program(sp, sn, dp, dn)));
    } catch (e) {
      let error;
      if (e instanceof RangeError || /call stack|stack overflow/i.test(String(e && e.message))) {
        error = DEEP;
      } else {
        let panic = "";
        try { panic = read(x.panic_message()); } catch (e2) { /* the instance is unusable */ }
        error = `internal error in Fermium: ${panic || (e && e.message) || e}\n` +
          "  (this is a bug in Fermium, not in your program)";
      }
      let stdout = "";
      try { stdout = read(x.partial_stdout()); } catch (e2) { /* the instance is unusable */ }
      res = { stdout, warnings: [], error, plots: [] };
    }
    res.seconds = (clock() - t0) / 1000;
    return res;
  }

  const api = { compile, run };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.FermiumWasm = api;
})(typeof self !== "undefined" ? self : globalThis);
