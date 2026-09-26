"""`python -m fermium` runs the command-line tool (the same as `fermium`)."""
import sys

from .cli import entry

sys.exit(entry())
