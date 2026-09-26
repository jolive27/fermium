"""The conformance suite (spec §B3) agrees with its oracle: the Python implementation passes John's Appendix 1
cases (the whole suite runs with `conformance/run --impl legacy`; its last result is conformance/LEGACY.md)."""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_legacy_passes_appendix1_cases(tmp_path):
    p = subprocess.run([sys.executable, os.path.join(ROOT, "conformance", "run"), "--impl", "legacy", "--area",
                        "appendix1", "-j", "1", "--out", str(tmp_path / "c.md")], capture_output=True, text=True,
                       timeout=300, cwd=ROOT)
    assert p.returncode == 0, p.stdout + p.stderr
    assert "5 of 5 programs pass" in (tmp_path / "c.md").read_text()
