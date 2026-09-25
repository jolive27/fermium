"""Seeded random numbers (DECISIONS D80): xoshiro256** seeded through splitmix64.

This is the reference (Python) half; codegen_llvm.py emits the same generator in LLVM IR, so a program
gives the same random numbers under `fermium run` (the JIT), `fermium build` and the reference
interpreter.  The state is four 64-bit words.

- rand()           uniform in [0, 1): the top 53 bits of the next output, times 2⁻⁵³
- randn()          a standard normal: Box–Muller from two rand() values, sqrt(-2 ln(1 - u₁)) cos(2π u₂)
- seed(s)          s is truncated to a whole number (|s| < 2⁶³, else 0) and expanded to the state by splitmix64
- a program that never calls seed starts as if it had called seed(0)
"""
from __future__ import annotations

import math

MASK = (1 << 64) - 1
GOLDEN = 0x9E3779B97F4A7C15
SM1 = 0xBF58476D1CE4E5B9
SM2 = 0x94D049BB133111EB
TWO_M53 = 2.0 ** -53
TWO_PI = 2.0 * math.pi


def _splitmix(x):
    x = (x + GOLDEN) & MASK
    z = x
    z = ((z ^ (z >> 30)) * SM1) & MASK
    z = ((z ^ (z >> 27)) * SM2) & MASK
    return x, z ^ (z >> 31)


def seed_int(s: float) -> int:
    """The whole number a seed value stands for (mirrors fptosi in the compiled code)."""
    if not (s == s) or abs(s) >= 9.2e18:
        return 0
    return int(s) & MASK


def state_for(s: float) -> list:
    x = seed_int(s)
    out = []
    for _ in range(4):
        x, z = _splitmix(x)
        out.append(z)
    return out


def _rotl(x, k):
    return ((x << k) | (x >> (64 - k))) & MASK


def next_u64(st) -> int:
    """Advance the state (a mutable sequence of four ints, e.g. a ctypes c_uint64 array) and return the output."""
    s0, s1, s2, s3 = int(st[0]), int(st[1]), int(st[2]), int(st[3])
    result = (_rotl((s1 * 5) & MASK, 7) * 9) & MASK
    t = (s1 << 17) & MASK
    s2 ^= s0
    s3 ^= s1
    s1 ^= s2
    s0 ^= s3
    s2 ^= t
    s3 = _rotl(s3, 45)
    st[0], st[1], st[2], st[3] = s0, s1, s2, s3
    return result


def rand(st) -> float:
    return (next_u64(st) >> 11) * TWO_M53


def randn(st) -> float:
    u1 = rand(st)
    u2 = rand(st)
    return math.sqrt(-2.0 * math.log(1.0 - u1)) * math.cos(TWO_PI * u2)


def seed(st, s: float):
    for i, w in enumerate(state_for(s)):
        st[i] = w


DEFAULT_STATE = state_for(0.0)
