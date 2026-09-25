__version__ = "0.1.0"

# The Python API (DECISIONS D142) is imported lazily, so `import fermium` (and the CLI) stay light.
_API = ("Module", "Quantity", "Q", "QuantityArray")


def compile(source, filename="<python>", base_dir=None, out=None, warnings=True):  # noqa: A001
    """Check and compile a Fermium program: `mod = fermium.compile(src)`, then `mod.f(2.0)`, `mod["x"]`."""
    from .api import compile as _compile
    return _compile(source, filename, base_dir=base_dir, out=out, warnings=warnings)


def load(path, out=None, warnings=True):
    """Check and compile the Fermium program in a file: `mod = fermium.load("orbit.fm")`."""
    from .api import load as _load
    return _load(path, out=out, warnings=warnings)


def __getattr__(name):
    if name in _API:
        from . import api
        return getattr(api, name)
    raise AttributeError(f"module 'fermium' has no attribute {name!r}")
