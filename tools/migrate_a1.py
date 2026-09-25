"""One-off migration for the A1 unit-name rule (D235): run `fermium fmt --fix` over every tracked .fm program and
every ```fermium block in the Markdown files, and log each edit to dev-notes/A1_MIGRATION.md.

Usage:  python3 tools/migrate_a1.py [--dry-run]
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from fermium.fmt import fix_source          # noqa: E402
from fermium.errors import FermiumError     # noqa: E402

SKIP = ("tests/programs/john/", "dev-notes/")   # John's programs and the specs/logs stay exactly as written


def tracked(pattern):
    out = subprocess.run(["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True).stdout.split()
    return [f for f in out if not f.startswith(SKIP)]


def diff_lines(old, new):
    o, n = old.split("\n"), new.split("\n")
    return [(i + 1, a, b) for i, (a, b) in enumerate(zip(o, n)) if a != b]


def main():
    dry = "--dry-run" in sys.argv
    log = []
    failed = []
    for f in tracked("*.fm"):
        path = os.path.join(ROOT, f)
        src = open(path, encoding="utf-8").read()
        try:
            new, n = fix_source(src)
        except FermiumError as e:
            failed.append((f, e.message))
            continue
        if n:
            log.append((f, diff_lines(src, new)))
            if not dry:
                open(path, "w", encoding="utf-8").write(new)
    for f in tracked("*.md"):
        path = os.path.join(ROOT, f)
        text = open(path, encoding="utf-8").read()
        if "```fermium" not in text:
            continue
        changes = []

        def fix_block(m):
            code = m.group(1)
            try:
                new, n = fix_source(code)
            except FermiumError:
                return m.group(0)
            if n:
                changes.extend(diff_lines(code, new))
            return "```fermium\n" + new + "```"
        new_text = re.sub(r"```fermium\n(.*?)```", fix_block, text, flags=re.S)
        if new_text != text:
            log.append((f, changes))
            if not dry:
                open(path, "w", encoding="utf-8").write(new_text)
    lines = ["# A1 migration log (D235)", "",
             "Every edit `fermium fmt --fix` made when the unit-name rule changed. Each keeps what Fermium 1 did.", ""]
    for f, ch in log:
        lines.append(f"## {f}")
        for ln, a, b in ch:
            lines.append(f"- line {ln}: `{a.strip()}` → `{b.strip()}`")
        lines.append("")
    if failed:
        lines.append("## Not parsed (left alone)")
        lines += [f"- {f}: {m}" for f, m in failed]
    report = "\n".join(lines) + "\n"
    if not dry:
        open(os.path.join(ROOT, "dev-notes", "A1_MIGRATION.md"), "w", encoding="utf-8").write(report)
    print(report)


if __name__ == "__main__":
    main()
