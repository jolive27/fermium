"""Re-run every example in the bootcamp and paste its real output under it.

Usage (from the repository root):  python3 bootcamp/update_outputs.py [files...]
With no arguments, updates bootcamp/*.md and bootcamp/solutions/*.md.

Two patterns are recognised:

    ```fermium                      <!-- run as oops.fm -->
    CODE                            ```
    ```                             CODE (may fail on purpose)
                                    ```
    <!-- output -->
    ```                             <!-- output -->
    (replaced with the real output) ```
    ```                             (replaced with the real output)
                                    ```

```fermium blocks run in the markdown file's folder (so data/ and plot files resolve). "run as" blocks
run in a temporary folder under the given file name, so error messages show a friendly file name.
"""
import glob
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
PAT = re.compile(
    r"(?P<head>(?:<!-- run as (?P<name>[\w.\-]+) -->\n```\n|```fermium\n))(?P<code>(?:(?!```).)*?)```\n\n"
    r"<!-- output -->\n```\n(?P<old>(?:(?!```).)*?)```", re.S)
SHOWN_DIR = "/Users/ada/fermium/bootcamp"   # what a temp folder is shown as in messages


def run(code, name, mddir):
    if name:
        cwd = tempfile.mkdtemp()
        path = os.path.join(cwd, name)
    else:
        cwd = mddir
        path = os.path.join(cwd, "_block_tmp.fm")
    with open(path, "w", encoding="utf-8") as f:
        f.write(code)
    try:
        p = subprocess.run(["fermium", "run", os.path.basename(path)], cwd=cwd, capture_output=True, text=True)
    finally:
        os.remove(path)
    out, err = p.stdout, p.stderr
    if name:
        out, err = out.replace(cwd, SHOWN_DIR), err.replace(cwd, SHOWN_DIR)
    elif p.returncode != 0:
        print(f"  !! a ```fermium block failed in {mddir}:\n{code}\n{err}", file=sys.stderr)
    # warnings are printed before running, runtime errors after the output
    text = err + out if err.startswith("warning") else out + err
    return text if text.endswith("\n") or not text else text + "\n"


def update(md):
    with open(md, encoding="utf-8") as f:
        text = f.read()
    mddir = os.path.dirname(os.path.abspath(md))

    def rep(m):
        out = run(m.group("code"), m.group("name"), mddir)
        return m.group("head") + m.group("code") + "```\n\n<!-- output -->\n```\n" + out + "```"
    new = PAT.sub(rep, text)
    if new != text:
        with open(md, "w", encoding="utf-8") as f:
            f.write(new)
        print("updated", os.path.relpath(md))


if __name__ == "__main__":
    files = sys.argv[1:] or sorted(glob.glob(os.path.join(HERE, "*.md")) + glob.glob(os.path.join(HERE, "solutions", "*.md")))
    for md in files:
        update(md)
