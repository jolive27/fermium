#!/usr/bin/env bash
# Keep the build green: lint, full test suite (incl. bootcamp/doc snippets), and all examples.
set -euo pipefail
cd "$(dirname "$0")"
echo "== lint (ruff)"
ruff check fermium tests
echo "== tests (pytest; includes docs + bootcamp code blocks and examples)"
python3 -m pytest -q -x -p no:cacheprovider "$@"
echo "== all good"
