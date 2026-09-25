#!/usr/bin/env python3
"""Build the files the browser playground (web/index.html) loads, from the repository:

    web/gen/examples.json   every ```fermium block of the bootcamp lessons, the examples/*.fm programs,
                            and the CSV data files they read
    web/gen/symbols.json    the \\name -> symbol table (fermium/symbols.py LATEX) for Tab completion
    web/gen/fermium-*.whl   a pure-Python wheel of the fermium package (written directly, see build_wheel),
                            unpacked into Pyodide's site-packages by web/worker.js
    web/gen/manifest.json   the wheel's file name

    python3 web/build.py                 # the above (a few seconds)
    python3 web/build.py --local-pyodide # also download Pyodide itself into web/pyodide/, so the page
                                         # (and tests/test_playground.py) needs no CDN

Then serve it:  python3 -m http.server -d web 8000   and open http://localhost:8000/
"""
import argparse
import glob
import io
import json
import os
import re
import shutil
import sys
import tarfile
import urllib.request

WEB = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(WEB)
GEN = os.path.join(WEB, "gen")
PYODIDE_VERSION = "0.29.5"
PYODIDE_CDN = f"https://cdn.jsdelivr.net/pyodide/v{PYODIDE_VERSION}/full/"
# numpy is always loaded; matplotlib for plots, scipy for fits, sympy for integrals without limits
PYODIDE_PACKAGES = ["numpy", "matplotlib", "scipy", "sympy"]
LOCAL_MARKER = "fermium-local.json"     # in web/pyodide/, written when the local copy is complete

sys.path.insert(0, ROOT)

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


def build_wheel():
    """Write a pure-Python wheel of the fermium package with zipfile.

    `pip wheel . --no-deps` does the same job, but it needs a working setuptools/wheel pair (Debian's
    system setuptools breaks it) or network access for build isolation; the wheel format for a pure
    package is just a zip with a .dist-info folder, so writing it directly is simpler and always works.
    The wheel is valid: `pip install web/gen/fermium-*.whl` accepts it."""
    import base64
    import hashlib
    import zipfile
    for old in glob.glob(os.path.join(GEN, "fermium-*.whl")):
        os.remove(old)
    pyproject = open(os.path.join(ROOT, "pyproject.toml"), encoding="utf-8").read()
    version = re.search(r'^version\s*=\s*"([^"]+)"', pyproject, re.M).group(1)
    summary = re.search(r'^description\s*=\s*"([^"]*)"', pyproject, re.M).group(1)
    dist = f"fermium-{version}.dist-info"
    name = f"fermium-{version}-py3-none-any.whl"
    files = []
    pkg = os.path.join(ROOT, "fermium")
    for d, dirs, fs in os.walk(pkg):
        dirs[:] = sorted(x for x in dirs if x != "__pycache__")
        for f in sorted(fs):
            if f.endswith((".py", ".c")):
                full = os.path.join(d, f)
                files.append((os.path.relpath(full, ROOT).replace(os.sep, "/"), open(full, "rb").read()))
    files.append((f"{dist}/METADATA", (f"Metadata-Version: 2.1\nName: fermium\nVersion: {version}\n"
                                        f"Summary: {summary}\nRequires-Python: >=3.10\n").encode()))
    files.append((f"{dist}/WHEEL", b"Wheel-Version: 1.0\nGenerator: fermium web/build.py\n"
                                   b"Root-Is-Purelib: true\nTag: py3-none-any\n"))
    files.append((f"{dist}/entry_points.txt", b"[console_scripts]\nfermium = fermium.cli:entry\n"))
    files.append((f"{dist}/top_level.txt", b"fermium\n"))
    record = []
    for path, data in files:
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
        record.append(f"{path},sha256={digest},{len(data)}")
    record.append(f"{dist}/RECORD,,")
    files.append((f"{dist}/RECORD", ("\n".join(record) + "\n").encode()))
    with zipfile.ZipFile(os.path.join(GEN, name), "w", zipfile.ZIP_DEFLATED) as z:
        for path, data in files:
            info = zipfile.ZipInfo(path, date_time=(2020, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, data)
    return name


def _sha256(path):
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _download(url, dest, sha256):
    if os.path.exists(dest) and _sha256(dest) == sha256:
        return
    tmp = dest + ".part"
    with urllib.request.urlopen(url, timeout=120) as r, open(tmp, "wb") as fh:
        shutil.copyfileobj(r, fh)
    if _sha256(tmp) != sha256:
        os.remove(tmp)
        raise SystemExit(f"checksum mismatch for {url}")
    os.replace(tmp, dest)


def local_pyodide():
    """Download the Pyodide core (npm package) and the wheels of the packages the playground uses
    into web/pyodide/ (about 50 MB); web/worker.js prefers it over the CDN when it is there."""
    dest = os.path.join(WEB, "pyodide")
    marker = os.path.join(dest, LOCAL_MARKER)
    if os.path.exists(marker) and json.load(open(marker)).get("version") == PYODIDE_VERSION:
        return
    if os.path.exists(marker):           # another Pyodide version: start again
        shutil.rmtree(dest)
    os.makedirs(dest, exist_ok=True)
    if not os.path.exists(os.path.join(dest, "pyodide-lock.json")):
        url = f"https://registry.npmjs.org/pyodide/-/pyodide-{PYODIDE_VERSION}.tgz"
        print(f"downloading {url}")
        with urllib.request.urlopen(url, timeout=120) as r:
            blob = r.read()
        with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as tf:
            for m in tf.getmembers():
                if m.isfile() and m.name.startswith("package/"):
                    name = m.name[len("package/"):]
                    if "/" in name:
                        continue
                    with open(os.path.join(dest, name), "wb") as fh:
                        fh.write(tf.extractfile(m).read())
    lock = json.load(open(os.path.join(dest, "pyodide-lock.json"), encoding="utf-8"))["packages"]
    need = set()

    def add(n):
        if n not in need:
            need.add(n)
            for d in lock[n]["depends"]:
                add(d)
    for p in PYODIDE_PACKAGES:
        add(p)
    for n in sorted(need):
        f = lock[n]["file_name"]
        print(f"  {f}")
        _download(PYODIDE_CDN + f, os.path.join(dest, f), lock[n]["sha256"])
    with open(marker, "w") as fh:        # written last: the page only uses a complete copy
        json.dump({"version": PYODIDE_VERSION, "packages": sorted(need)}, fh)
    print(f"local Pyodide {PYODIDE_VERSION} in {os.path.relpath(dest, ROOT)}/ ({len(need)} packages)")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--local-pyodide", action="store_true", help="also download Pyodide into web/pyodide/")
    ap.add_argument("--no-wheel", action="store_true", help="skip building the wheel")
    args = ap.parse_args(argv)
    from fermium.symbols import LATEX
    os.makedirs(GEN, exist_ok=True)
    ex = collect_examples()
    with open(os.path.join(GEN, "examples.json"), "w", encoding="utf-8") as fh:
        json.dump(ex, fh, ensure_ascii=False, indent=1)
    with open(os.path.join(GEN, "symbols.json"), "w", encoding="utf-8") as fh:
        json.dump(LATEX, fh, ensure_ascii=False, indent=1)
    manifest = {"pyodide_version": PYODIDE_VERSION, "pyodide_cdn": PYODIDE_CDN}
    if not args.no_wheel:
        manifest["wheel"] = build_wheel()
    elif os.path.exists(os.path.join(GEN, "manifest.json")):
        manifest["wheel"] = json.load(open(os.path.join(GEN, "manifest.json"))).get("wheel")
    with open(os.path.join(GEN, "manifest.json"), "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=1)
    n = sum(len(g["items"]) for g in ex["groups"])
    print(f"web/gen: {n} examples, {len(LATEX)} symbols" + (f", {manifest['wheel']}" if manifest.get("wheel") else ""))
    if args.local_pyodide:
        local_pyodide()


if __name__ == "__main__":
    main()
