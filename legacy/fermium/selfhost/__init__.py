"""Self-hosting (moonshot M8): parts of Fermium written in Fermium.

`units_db.fm` defines every non-SI unit from SI base units and exact defining numbers; Fermium's own unit
checker verifies each definition's dimension.  `python3 -m fermium.selfhost` compiles and runs it with the
existing compiler and writes `fermium/units_selfhosted.py`, the factor table that `fermium/units.py` uses.
"""
