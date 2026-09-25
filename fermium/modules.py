"""Finding Fermium modules: the search path, the standard library folder and `fermium.toml` (M7, D100-D103).

A module is a `.fm` file whose top level only defines functions and constants.  `import name` looks for
`name.fm` in, in order:

1. the folder of the file doing the import (the program, or the module that imports another module),
2. the program's folder,
3. the folders listed under `[paths] modules = [...]` in the nearest `fermium.toml` (the program's folder
   or a parent), relative to that file,
4. the standard library shipped with Fermium (`fermium/stdlib/`).

`import "path/to/file.fm"` is relative to the importing file's folder.
"""
from __future__ import annotations

import os
from dataclasses import dataclass, field

from .errors import FermiumError

PROJECT_FILE = "fermium.toml"


def stdlib_dir() -> str:
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), "stdlib")


def stdlib_modules() -> list[str]:
    d = stdlib_dir()
    if not os.path.isdir(d):
        return []
    return sorted(f[:-3] for f in os.listdir(d) if f.endswith(".fm"))


@dataclass
class Project:
    root: str                     # folder holding fermium.toml
    name: str | None = None
    version: str | None = None
    module_paths: list = field(default_factory=list)   # absolute folders

    @property
    def file(self):
        return os.path.join(self.root, PROJECT_FILE)


def _load_toml(path):
    try:
        import tomllib
    except ImportError:                 # Python 3.10
        try:
            import tomli as tomllib     # type: ignore
        except ImportError:
            return _mini_toml(path)
    try:
        with open(path, "rb") as fh:
            return tomllib.load(fh)
    except tomllib.TOMLDecodeError as e:
        raise FermiumError(f"{path} isn't a valid fermium.toml: {e}",
                           hint='it should look like  [project] name = "lab"  and  [paths] modules = ["lib"]')


def _mini_toml(path):
    """Enough TOML for fermium.toml when neither tomllib nor tomli is available: [tables], key = "str" or
    ["list", "of", "str"]."""
    import ast
    out, cur = {}, None
    with open(path, encoding="utf-8") as fh:
        for n, raw in enumerate(fh, 1):
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("[") and line.endswith("]"):
                cur = out.setdefault(line[1:-1].strip(), {})
                continue
            if "=" not in line or cur is None:
                raise FermiumError(f"{path} isn't a valid fermium.toml (line {n}: {raw.strip()})")
            k, v = (x.strip() for x in line.split("=", 1))
            try:
                cur[k] = ast.literal_eval(v)
            except (ValueError, SyntaxError):
                raise FermiumError(f"{path} isn't a valid fermium.toml (line {n}: {raw.strip()})")
    return out


_PROJECT_CACHE: dict = {}


def find_project(start_dir: str) -> Project | None:
    """The nearest fermium.toml in start_dir or a parent folder, read; None when there is none."""
    d = os.path.abspath(start_dir or ".")
    while True:
        path = os.path.join(d, PROJECT_FILE)
        if os.path.isfile(path):
            return read_project(path)
        parent = os.path.dirname(d)
        if parent == d:
            return None
        d = parent


def read_project(path: str) -> Project:
    key = (path, os.path.getmtime(path))
    if key in _PROJECT_CACHE:
        return _PROJECT_CACHE[key]
    data = _load_toml(path)
    root = os.path.dirname(path)
    proj = Project(root)
    info = data.get("project", {})
    if not isinstance(info, dict):
        raise FermiumError(f"{path}: [project] must be a table")
    proj.name = info.get("name")
    proj.version = info.get("version")
    paths = data.get("paths", {})
    mods = paths.get("modules", []) if isinstance(paths, dict) else []
    if not isinstance(mods, list) or not all(isinstance(m, str) for m in mods):
        raise FermiumError(f"{path}: modules under [paths] must be a list of folder names",
                           hint='write  modules = ["lib"]')
    proj.module_paths = [os.path.normpath(os.path.join(root, m)) for m in mods]
    _PROJECT_CACHE[key] = proj
    return proj


def module_search_path(program_dir: str, importer_dir: str | None = None) -> list[str]:
    """Folders searched for `import name`, in order (see the module docstring)."""
    out = []
    for d in [importer_dir, os.path.abspath(program_dir or ".")]:
        if d and d not in out:
            out.append(d)
    proj = find_project(program_dir)
    if proj is not None:
        out += [d for d in proj.module_paths if d not in out]
    out.append(stdlib_dir())
    return out


def resolve_module(name: str, is_path: bool, program_dir: str, importer_dir: str | None = None):
    """(absolute file path, searched folders); the path is None when not found."""
    if is_path:
        base = importer_dir or os.path.abspath(program_dir or ".")
        p = os.path.normpath(os.path.join(base, os.path.expanduser(name)))
        return (p if os.path.isfile(p) else None), [base]
    folders = module_search_path(program_dir, importer_dir)
    for d in folders:
        p = os.path.join(d, name + ".fm")
        if os.path.isfile(p):
            return os.path.normpath(p), folders
    return None, folders


def available_modules(folders) -> list[str]:
    out = set()
    for d in folders:
        try:
            out |= {f[:-3] for f in os.listdir(d) if f.endswith(".fm")}
        except OSError:
            pass
    return sorted(out)
