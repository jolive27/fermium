"""Spec A8.2 (D261): an ODE solution captured by an integrand or equation inside a function (D48) is passed in
the environment as a pointer, never as the bits of a double.  A pointer such as 0x00007f3a12345678 read as a
double is a subnormal, and flush-to-zero / denormals-are-zero (fast-math) would turn it into 0."""
import os
import re
import subprocess
import sys

import pytest

from conftest import run
from fermium.driver import Program

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# the D48 programs of tests/test_friction_numerics2.py (#48, Q16) and a nested integrand (gauntlet E9)
PROGRAMS = {
    "root": ("f(k) =\n    solve y' = k y\n      with y(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
             "    solve y(T) = 2 for T from 0 s to 1 s\n    return T\nprint f(1 / (1 s)) to 9 digits",
             "0.693147181 s"),
    "integral": ("g(k) =\n    solve u' = -k u\n      with u(0 s) = 1\n      for t from 0 s to 2 s tolerance 1e-12\n"
                 "    return ∫ u(t) dt from 0 s to 2 s\nprint g(1 / (1 s)) to 9 digits", "0.864664717 s"),
    "nested": ("h(k) =\n    solve u' = -k u\n      with u(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
               "    return ∫ (∫ u(t) u(τ) dτ from 0 s to t) dt from 0 s to 1 s\nprint h(1 / (1 s)) to 9 digits",
               "0.199788200 s²"),
    "ratio": ("expect(E) =\n    solve u'' = -E u / (1 m²)\n      with u(0 m) = 0, u'(0 m) = 1 / (1 m)\n"
              "      for r from 0 m to 1 m\n"
              "    return ∫ r u(r)² dr from 0 m to 1 m / ∫ u(r)² dr from 0 m to 1 m\nprint expect(π²) to 8 digits",
              "0.50000000 m"),
    # two solutions and a number captured together, so the slots after the pointers must line up
    "two": ("f(a) =\n    solve x' = -a x\n      with x(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
            "    solve y' = a y\n      with y(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
            "    k2 = 2\n    return ∫ k2 x(t) y(t) dt from 0 s to 1 s\nprint f(1 / (1 s)) to 9 digits",
            "2.00000000 s"),
}


def _functions(ir_text):
    """The bodies of the module's functions (SSA names are local to a function)."""
    return re.findall(r"^define .*?^}", ir_text, re.M | re.S) or [ir_text]


def pointer_through_double(ir_text):
    """Instructions that move a pointer's bits into a double or back (ptrtoint → bitcast to double, or
    bitcast double → i64 → inttoptr)."""
    bad = []
    for body in _functions(ir_text):
        defs = {m.group(1): m.group(2) for m in re.finditer(r"^\s*(%\S+) = (.*)$", body, re.M)}
        for ins in defs.values():
            m = re.match(r"bitcast i64 (%\S+) to double", ins)
            if m and defs.get(m.group(1), "").startswith("ptrtoint"):
                bad.append(ins)
            m = re.match(r"inttoptr i64 (%\S+) to", ins)
            if m and defs.get(m.group(1), "").startswith("bitcast double"):
                bad.append(ins)
    return bad


@pytest.mark.parametrize("name", sorted(PROGRAMS))
def test_d48_programs_still_work(name):
    src, want = PROGRAMS[name]
    assert run(src) == want


@pytest.mark.parametrize("name", sorted(PROGRAMS))
def test_no_pointer_is_stored_as_a_double(name):
    ir_text = Program(PROGRAMS[name][0], "<test>").llvm_ir
    assert "ptrtoint" in ir_text or "bitcast" in ir_text      # the check below looks at real IR
    assert pointer_through_double(ir_text) == []


def test_the_ir_check_catches_the_old_code():
    old = ('%".5" = load double, double* %".4"\n%".6" = bitcast double %".5" to i64\n'
           '%".7" = inttoptr i64 %".6" to {i64, i64}*\n%".8" = ptrtoint {i64, i64}* %".7" to i64\n'
           '%".9" = bitcast i64 %".8" to double\n')
    assert len(pointer_through_double(old)) == 2


def test_emit_llvm_has_no_pointer_in_a_double(tmp_path):
    f = tmp_path / "p.fm"
    f.write_text(PROGRAMS["integral"][0] + "\n", encoding="utf-8")
    r = subprocess.run([sys.executable, "-m", "fermium", "run", "--emit-llvm", str(f)], capture_output=True,
                       text=True, cwd=ROOT, timeout=120)
    assert r.returncode == 0, r.stderr
    assert "define" in r.stdout and pointer_through_double(r.stdout) == []


# Flush-to-zero, simulated: every double loaded from memory goes through a function that turns subnormals into
# 0, as a CPU with FTZ/DAZ (or a fast-math build) may.  With D48's pointer-in-a-double, the solution's address
# (a subnormal) became a null pointer; now it is loaded as a pointer and the programs print the same.
FTZ_RUNNER = r'''
import re, sys
import llvmlite.binding as llvm
FTZ = """
define internal double @fm_test_ftz(double %x) alwaysinline {
  %b = bitcast double %x to i64
  %e = and i64 %b, 9218868437227405312
  %z = icmp eq i64 %e, 0
  %r = select i1 %z, double 0.0, double %x
  ret double %r
}
"""
count = [0]
def ftz(text):
    def rep(m):
        count[0] += 1
        raw = f"%ftz.raw.{count[0]}"
        return f"{m.group(1)}{raw} = load double, double* {m.group(3)}\n{m.group(1)}{m.group(2)} = call double @fm_test_ftz(double {raw})"
    return re.sub(r"^(\s*)(%\S+) = load double, double\* (.+)$", rep, text, flags=re.M) + FTZ
orig = llvm.parse_assembly
llvm.parse_assembly = lambda text, *a, **k: orig(ftz(text), *a, **k)
from fermium.driver import run_source
run_source(open(sys.argv[1], encoding="utf-8").read(), "<ftz>")
assert count[0] > 0, "no double loads were rewritten"
'''


@pytest.mark.parametrize("name", sorted(PROGRAMS))
def test_solution_pointer_survives_flush_to_zero(name, tmp_path):
    src, want = PROGRAMS[name]
    (tmp_path / "p.fm").write_text(src + "\n", encoding="utf-8")
    (tmp_path / "ftz.py").write_text(FTZ_RUNNER, encoding="utf-8")
    env = dict(os.environ, PYTHONPATH=os.path.join(ROOT, "legacy") + os.pathsep + os.environ.get("PYTHONPATH", ""))
    r = subprocess.run([sys.executable, str(tmp_path / "ftz.py"), str(tmp_path / "p.fm")], capture_output=True,
                       text=True, cwd=ROOT, env=env, timeout=120)
    assert r.returncode == 0, r.stderr[-2000:]
    assert r.stdout.strip() == want
