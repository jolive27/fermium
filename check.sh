#!/usr/bin/env bash
# Keep the build green: lint, full test suite (incl. bootcamp/doc snippets), and all examples.
set -euo pipefail
cd "$(dirname "$0")"
echo "== lint (ruff)"
ruff check fermium tests
echo "== tests (pytest; includes docs + bootcamp code blocks and examples)"
# Parallel when pytest-xdist is installed (the dev extra has it); a file's tests stay on one worker,
# which the Jupyter kernel fixture needs.
par=()
if python3 -c "import xdist" 2>/dev/null; then par=(-n auto --dist loadfile); fi
python3 -m pytest -q -x -p no:cacheprovider "${par[@]}" "$@"
echo "== all good"
