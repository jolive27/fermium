"""Parse numbers printed by Fermium (e.g. '6.674×10⁻¹¹ N m²/kg²')."""
import re

SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")


def num(text):
    t = text.strip().split()[0]
    t = re.sub(r"×10([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)", lambda m: "e" + m.group(1).translate(SUP), t)
    if t == "∞":
        return float("inf")
    if t == "-∞":
        return float("-inf")
    return float(t)


def nums(text):
    return [num(line) for line in text.strip().split("\n") if line.strip()]
