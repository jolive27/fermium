"""Re-run every example in the bootcamp and paste its real output under it.

Usage (from the repository root):  python3 bootcamp/update_outputs.py [files...]
With no arguments, updates bootcamp/*.md and bootcamp/solutions/*.md.
`python3 bootcamp/update_outputs.py --check` changes nothing and lists the boxes that are out of date
(tests/test_bootcamp_outputs.py does the same check in the test suite).

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

The programs run in this process with the Fermium next to this file (not whatever `fermium` is on the
PATH), and the output is what `fermium run` prints: warnings first, then the program's output, then a
runtime error if there is one.
"""
import glob
import io
import os
import re
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

PAT = re.compile(
    r"(?P<head>(?:<!-- run as (?P<name>[\w.\-]+) -->\n```\n|```fermium\n))(?P<code>(?:(?!```).)*?)```\n\n"
    r"<!-- output -->\n```\n(?P<old>(?:(?!```).)*?)```", re.S)
SHOWN_DIR = "/Users/ada/fermium/bootcamp"   # what a temp folder is shown as in messages


def default_files():
    return sorted(glob.glob(os.path.join(HERE, "*.md")) + glob.glob(os.path.join(HERE, "solutions", "*.md")))


def run(code, name, mddir):
    """Run one example the way `fermium run` would and return what it prints."""
    from fermium.driver import run_source
    from fermium.errors import FermiumError
    tmp = None
    if name:
        tmp = tempfile.mkdtemp()
        cwd, shown = tmp, name
    else:
        cwd, shown = mddir, "_block_tmp.fm"
    out, err = io.StringIO(), io.StringIO()
    failed = False
    old = os.getcwd()
    try:
        os.chdir(cwd)
        run_source(code, shown, out=out, base_dir=cwd, err=err)
    except FermiumError as e:
        failed = True
        err.write(e.format(code, shown) + "\n")
    finally:
        os.chdir(old)
        if tmp:
            shutil.rmtree(tmp, ignore_errors=True)
    out, err = out.getvalue(), err.getvalue()
    if name:
        out, err = out.replace(cwd, SHOWN_DIR), err.replace(cwd, SHOWN_DIR)
    elif failed:
        print(f"  !! a ```fermium block failed in {mddir}:\n{code}\n{err}", file=sys.stderr)
    # warnings are printed before running, runtime errors after the output
    text = err + out if err.startswith("warning") else out + err
    return text if text.endswith("\n") or not text else text + "\n"


def boxes(md):
    """(line number, code, run-as name, box text) for every output box in a markdown file."""
    with open(md, encoding="utf-8") as f:
        text = f.read()
    for m in PAT.finditer(text):
        line = text.count("\n", 0, m.start("old")) + 1
        yield line, m.group("code"), m.group("name"), m.group("old")


def update(md, check=False):
    with open(md, encoding="utf-8") as f:
        text = f.read()
    mddir = os.path.dirname(os.path.abspath(md))
    stale = []

    def rep(m):
        out = run(m.group("code"), m.group("name"), mddir)
        if out != m.group("old"):
            stale.append(text.count("\n", 0, m.start("old")) + 1)
        return m.group("head") + m.group("code") + "```\n\n<!-- output -->\n```\n" + out + "```"
    new = PAT.sub(rep, text)
    if check:
        for line in stale:
            print(f"out of date: {os.path.relpath(md)}:{line}")
    elif new != text:
        with open(md, "w", encoding="utf-8") as f:
            f.write(new)
        print("updated", os.path.relpath(md))
    return stale


if __name__ == "__main__":
    args = sys.argv[1:]
    check = "--check" in args
    files = [a for a in args if a != "--check"] or default_files()
    n = sum(len(update(md, check)) for md in files)
    sys.stdout.flush()
    sys.stderr.flush()
    # os._exit: tearing down JIT engines at interpreter shutdown can crash (see fermium/cli.py entry)
    os._exit(1 if check and n else 0)
