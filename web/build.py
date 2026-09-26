#!/usr/bin/env python3
"""Build the files the browser playground (web/index.html) loads, from the repository:

    web/gen/fermium.wasm    Fermium itself: the Rust compiler's tree-walking back end compiled to WebAssembly
                            (rust/crates/fermium-wasm, `cargo build --profile wasm --target wasm32-unknown-unknown`)
    web/gen/examples.json   every ```fermium block of the bootcamp lessons, the examples/*.fm programs,
                            and the CSV data files they read
    web/gen/symbols.json    the \\name -> symbol table (fermium/symbols.py LATEX) for Tab completion
    web/gen/manifest.json   the module's file name, size and version

    python3 web/build.py                   # the above (the first wasm build takes a few minutes)
    python3 web/build.py --wasm FILE.wasm  # use a module built elsewhere
    python3 web/build.py --no-wasm         # only the JSON files (keeps a web/gen/fermium.wasm already there)

Needs: Rust with the wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`). Nothing else:
no wasm-bindgen, no npm. Then serve it:  python3 -m http.server -d web 8000   and open http://localhost:8000/
"""
import argparse
import ast
import glob
import json
import os
import re
import shutil
import subprocess
import sys

WEB = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(WEB)
GEN = os.path.join(WEB, "gen")
RUST = os.path.join(ROOT, "rust")
TARGET = "wasm32-unknown-unknown"
PROFILE = "wasm"          # rust/Cargo.toml [profile.wasm]: release + fat LTO + stripped

WELCOME = """\
# Welcome to the Fermium playground!
# Edit this program and press Run (or Ctrl+Enter / Cmd+Enter).
# Type \\pi then Tab to get π, \\^2 then Tab to get ².

# Measure g with a pendulum: L is its length, T one full swing
L = 1.20 m
T = 2.21 s
g = 4π² L / T²
print g

# Units are checked before the program runs.  Try changing T² to T above.
print "in feet:", g in ft/s²
"""

UNIT_ERROR = """\
# A unit mistake: Fermium stops before running and says what is wrong.
L = 1.20 m
T = 2.21 s
g = 4π² L / T
print g + 9.8 m/s²
"""


def lesson_blocks(path):
    """Yield (lesson title, block title, code) for each ```fermium block of a bootcamp lesson."""
    lesson, section, in_fence, fence_lang, buf = None, None, False, None, []
    for line in open(path, encoding="utf-8").read().split("\n"):
        if in_fence:
            if line.startswith("```"):
                in_fence = False
                if fence_lang == "fermium":
                    code = "\n".join(buf).rstrip() + "\n"
                    first = buf[0].strip() if buf else ""
                    title = first[1:].strip() if first.startswith("#") else (section or lesson)
                    yield lesson, title.replace("`", ""), code
            else:
                buf.append(line)
            continue
        if line.startswith("```"):
            in_fence, fence_lang, buf = True, line[3:].strip(), []
        elif line.startswith("# ") and lesson is None:
            lesson = line[2:].strip()
        elif re.match(r"#{2,4} ", line):
            section = line.lstrip("#").strip()


def collect_examples():
    groups = [{"group": "Start here", "items": [
        {"title": "Welcome: measure g with a pendulum", "code": WELCOME, "dir": "bootcamp"},
        {"title": "A unit error", "code": UNIT_ERROR, "dir": "bootcamp"},
    ]}]
    for path in sorted(glob.glob(os.path.join(ROOT, "bootcamp", "lesson*.md"))):
        items, seen = [], {}
        lesson = None
        for lesson, title, code in lesson_blocks(path):
            seen[title] = seen.get(title, 0) + 1
            items.append({"title": title if seen[title] == 1 else f"{title} ({seen[title]})",
                          "code": code, "dir": "bootcamp"})
        if items:
            groups.append({"group": lesson or os.path.basename(path), "items": items})
    items = []
    for path in sorted(glob.glob(os.path.join(ROOT, "examples", "*.fm"))):
        code = open(path, encoding="utf-8").read()
        first = code.split("\n", 1)[0]
        title = first.lstrip("#").strip() if first.startswith("#") else os.path.basename(path)
        items.append({"title": title, "code": code, "dir": "examples", "file": os.path.basename(path)})
    groups.append({"group": "Example programs", "items": items})
    data = {}
    for d in ("bootcamp", "examples"):
        for path in sorted(glob.glob(os.path.join(ROOT, d, "data", "*"))):
            if os.path.isfile(path):
                data[f"{d}/data/{os.path.basename(path)}"] = open(path, encoding="utf-8").read()
    return {"groups": groups, "data": data}


def load_symbols():
    """The LATEX table of fermium/symbols.py (legacy/fermium/ after the cutover), read without importing it."""
    for rel in ("fermium/symbols.py", "legacy/fermium/symbols.py"):
        path = os.path.join(ROOT, rel)
        if os.path.exists(path):
            tree = ast.parse(open(path, encoding="utf-8").read())
            for node in tree.body:
                if isinstance(node, ast.Assign) and any(getattr(t, "id", None) == "LATEX" for t in node.targets):
                    return ast.literal_eval(node.value)
    raise SystemExit("fermium/symbols.py (the \\name table) not found")


def rust_version():
    m = re.search(r'^version\s*=\s*"([^"]+)"', open(os.path.join(RUST, "Cargo.toml"), encoding="utf-8").read(), re.M)
    return m.group(1) if m else "?"


def build_wasm():
    """cargo build the fermium-wasm crate for the browser; the path of the module, or None (with the reason
    printed) when Rust or its wasm32 target isn't installed."""
    cargo = shutil.which("cargo") or os.path.expanduser("~/.cargo/bin/cargo")
    if not os.path.exists(cargo):
        print("build.py: cargo not found: can't build fermium.wasm (install Rust: https://rustup.rs)", file=sys.stderr)
        return None
    rustup = shutil.which("rustup")
    if rustup:
        installed = subprocess.run([rustup, "target", "list", "--installed"], capture_output=True, text=True).stdout
        if TARGET not in installed.split():
            print(f"build.py: the {TARGET} target isn't installed: rustup target add {TARGET}", file=sys.stderr)
            return None
    cmd = [cargo, "build", "--profile", PROFILE, "--target", TARGET, "-p", "fermium-wasm"]
    print("  " + " ".join(["cargo"] + cmd[1:]), "(in rust/)")
    p = subprocess.run(cmd, cwd=RUST)
    if p.returncode != 0:
        raise SystemExit("build.py: building fermium.wasm failed")
    target_dir = os.environ.get("CARGO_TARGET_DIR") or os.path.join(RUST, "target")
    return os.path.join(target_dir, TARGET, PROFILE, "fermium_wasm.wasm")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--wasm", metavar="FILE", help="use this fermium.wasm instead of building it")
    ap.add_argument("--no-wasm", action="store_true", help="don't build fermium.wasm")
    args = ap.parse_args(argv)
    os.makedirs(GEN, exist_ok=True)
    for old in glob.glob(os.path.join(GEN, "fermium-*.whl")):      # the Pyodide playground's wheel (v1.5)
        os.remove(old)
    ex = collect_examples()
    symbols = load_symbols()
    with open(os.path.join(GEN, "examples.json"), "w", encoding="utf-8") as fh:
        json.dump(ex, fh, ensure_ascii=False, indent=1)
    with open(os.path.join(GEN, "symbols.json"), "w", encoding="utf-8") as fh:
        json.dump(symbols, fh, ensure_ascii=False, indent=1)
    dest = os.path.join(GEN, "fermium.wasm")
    src = args.wasm if args.wasm else (None if args.no_wasm else build_wasm())
    if src:
        shutil.copyfile(src, dest)
    manifest = {"wasm": "fermium.wasm", "version": rust_version()}
    if os.path.exists(dest):
        manifest["wasm_bytes"] = os.path.getsize(dest)
    with open(os.path.join(GEN, "manifest.json"), "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=1)
    n = sum(len(g["items"]) for g in ex["groups"])
    size = f", fermium.wasm {manifest['wasm_bytes'] / 1e6:.1f} MB" if "wasm_bytes" in manifest else ", no fermium.wasm"
    print(f"web/gen: {n} examples, {len(symbols)} symbols{size}")


if __name__ == "__main__":
    main()
