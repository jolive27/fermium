"""The reference interpreter must work without llvmlite: that is how the browser playground (Pyodide,
where llvmlite doesn't exist) runs programs.  Each check runs in a subprocess with llvmlite blocked."""
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

BLOCK = """
import sys
for m in ("llvmlite", "llvmlite.ir", "llvmlite.binding"):
    sys.modules[m] = None          # any `import llvmlite...` now raises ImportError
sys.path.insert(0, ROOT + "/legacy")
"""

PROGRAMS = {
    "pendulum": ("print 4π² (1.20 m) / (2.21 s)²\n", "9.70 m/s²"),
    "integral": ("print ∫ x² dx from 0 m to 2 m\n", "2.67 m³"),
    "function+loop": ("KE(M, v) = ½ M v²\nfor v in [1 m/s, 2 m/s]\n    print KE(2 kg, v)\n", "1 J\n4 J"),
    "list": ("L = [1.0 m, 1.2 m, 1.4 m]\nprint mean(L)\n", "1.2 m"),
    "ode": ("k = 1 N/m\nmass = 1 kg\nsolve mass x'' = -k x\n  with x(0) = 1 m, x'(0) = 0 m/s\n  for t from 0 s to 10 s\n"
            "print x(π * 1 s)\n", "-1.00 m"),
}


def _run(script):
    code = f"ROOT = {ROOT!r}\n" + BLOCK + script
    p = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=120)
    assert p.returncode == 0, p.stderr
    return p.stdout


def test_interpreter_runs_programs_without_llvmlite():
    out = _run(f"""
import io, json
from fermium.interp import run_interpreted
res = {{}}
for name, (src, _) in {json.dumps({k: v for k, v in PROGRAMS.items()}, ensure_ascii=False)}.items():
    buf = io.StringIO()
    run_interpreted(src, out=buf)
    res[name] = buf.getvalue().strip()
res["llvmlite_loaded"] = any(m.startswith("llvmlite") and sys.modules[m] is not None for m in sys.modules)
res["codegen_loaded"] = "fermium.codegen_llvm" in sys.modules
print(json.dumps(res))
""")
    res = json.loads(out.strip().split("\n")[-1])
    for name, (_, want) in PROGRAMS.items():
        assert res[name] == want, (name, res[name])
    assert not res["llvmlite_loaded"] and not res["codegen_loaded"]


def test_unit_errors_without_llvmlite():
    out = _run("""
import io
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
src = "L = 1.20 m\\nT = 2.21 s\\nprint 4π² L / T + 9.8 m/s²\\n"
try:
    run_interpreted(src, out=io.StringIO())
except FermiumError as e:
    print(e.format(src).split("\\n")[0])
""")
    assert "line 3: can't add speed [m/s] to acceleration [m/s²]" in out


def test_moved_numerics_are_shared():
    # the Gauss-Kronrod tables and odd_root_numerator live in the pure fermium.numerics now
    from fermium import codegen_llvm, interp, numerics
    assert codegen_llvm.XGK is numerics.XGK and interp.WGK is numerics.WGK and interp.WG is numerics.WG
    assert numerics.odd_root_numerator(2 / 3) == 2 and numerics.odd_root_numerator(0.5) is None
    assert abs(sum(numerics.WGK[:7]) * 2 + numerics.WGK[7] - 2) < 1e-14
