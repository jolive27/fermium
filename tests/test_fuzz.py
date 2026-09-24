"""Fuzz the lexer/parser/checker/codegen with mutated real programs: only clean FermiumErrors allowed."""
import pytest

from fuzzlib import fuzz


@pytest.mark.parametrize("seed", range(4))
def test_mutated_programs_never_crash(seed):
    bad = fuzz(150, seed)
    assert not bad, "\n\n".join(f"{type(e).__name__}: {e}\n--- program ---\n{src}" for src, e in bad[:3])
