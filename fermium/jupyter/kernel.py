"""The Fermium Jupyter kernel.

Each cell is run by one ReplSession, so variables and functions carry over from cell to cell, as in
the REPL.  Printed output is streamed back, plots are shown inline (and still saved as PNG files),
errors come back in the usual one-line form, and TAB completes `\\name` symbols (`\\omega` → ω), the names
defined so far, keywords, and a module's members (`mechanics.` → spring_period, …).  Warnings go to stderr.
"""
from __future__ import annotations

import base64
import os

from ipykernel.kernelbase import Kernel

from .. import __version__
from ..errors import FermiumError
from ..symbols import LATEX


class _Sink:
    """File-like object that collects what the session writes during one cell.  Two sinks (stdout and
    stderr) can share one list of parts, so the cell's output keeps its order."""

    def __init__(self, name="stdout", parts=None):
        self.name = name
        self.parts = [] if parts is None else parts

    def write(self, s):
        self.parts.append((self.name, s))
        return len(s)

    def flush(self):
        pass

    def take(self):
        """[(stream name, text)] in order, with neighbouring pieces of the same stream joined."""
        out = []
        for name, s in self.parts:
            if out and out[-1][0] == name:
                out[-1] = (name, out[-1][1] + s)
            else:
                out.append((name, s))
        self.parts.clear()
        return out


class FermiumKernel(Kernel):
    implementation = "fermium"
    implementation_version = __version__
    language = "fermium"
    language_version = __version__
    language_info = {"name": "fermium", "mimetype": "text/x-fermium", "file_extension": ".fm",
                     "codemirror_mode": "python", "pygments_lexer": "python"}
    banner = f"Fermium {__version__}: physics code that reads like physics on paper"

    def __init__(self, **kw):
        super().__init__(**kw)
        from ..driver import ReplSession
        self.sink = _Sink()
        self.errsink = _Sink("stderr", self.sink.parts)
        # warnings (compile-time and run-time) go to stderr, as `fermium run` shows them (red team 5 #3)
        self.fm = ReplSession(out=self.sink, base_dir=os.getcwd(), err=self.errsink)

    def _stream(self, name, text):
        if text and not self.silent:
            self.send_response(self.iopub_socket, "stream", {"name": name, "text": text})

    def _show_new_plots(self, before):
        for path in self.fm.runtime.plots_saved[before:]:
            try:
                with open(path, "rb") as fh:
                    data = base64.b64encode(fh.read()).decode("ascii")
            except OSError:
                continue
            if not self.silent:
                self.send_response(self.iopub_socket, "display_data",
                                   {"data": {"image/png": data, "text/plain": f"<plot {os.path.basename(path)}>"},
                                    "metadata": {}})

    def _flush(self, before):
        for name, text in self.sink.take():
            if name == "stdout":
                # plot lines ("plot saved to ...") are replaced by the inline picture
                text = "".join(ln for ln in text.splitlines(True) if not ln.startswith("plot saved to "))
            self._stream(name, text)
        self._show_new_plots(before)

    def _error(self, msg):
        self._stream("stderr", msg + "\n")
        return {"status": "error", "execution_count": self.execution_count,
                "ename": "FermiumError", "evalue": msg, "traceback": [msg]}

    def do_execute(self, code, silent, store_history=True, user_expressions=None, allow_stdin=False):
        self.silent = silent
        if not code.strip():
            return {"status": "ok", "execution_count": self.execution_count, "payload": [], "user_expressions": {}}
        before = len(self.fm.runtime.plots_saved)
        try:
            self.fm.execute(code)
        except FermiumError as e:
            self._flush(before)
            return self._error(e.format(code))
        except RecursionError:
            self._flush(before)
            return self._error("this cell is nested too deeply for Fermium to compile")
        except Exception as e:      # a bug in Fermium: still answer the cell, never leave it hanging (red team 5 #2)
            self._flush(before)
            return self._error(f"internal error in Fermium: {type(e).__name__}: {e}\n"
                               f"  (this is a bug in Fermium, not in your program)")
        self._flush(before)
        return {"status": "ok", "execution_count": self.execution_count, "payload": [], "user_expressions": {}}

    def do_complete(self, code, cursor_pos):
        text = code[:cursor_pos]
        i = text.rfind("\\")
        if i < 0 or any(c.isspace() for c in text[i:]):
            return self._complete_names(text, cursor_pos)
        name = text[i + 1:]
        matches = [LATEX[name]] if name in LATEX else sorted({LATEX[k] for k in LATEX if k.startswith(name)})
        return {"status": "ok", "matches": matches, "cursor_start": i, "cursor_end": cursor_pos, "metadata": {}}

    def _complete_names(self, text, cursor_pos):
        """Names defined so far, keywords, and a module's members after `mechanics.` (as the language server
        completes them, red team 5 #13)."""
        from types import SimpleNamespace
        from ..lsp import completions
        line = text.split("\n")[-1]
        items = completions(SimpleNamespace(checker=self.fm.checker), line, 0, len(line))
        start = items[0][2] if items else len(line)
        if not line[start:] and not line[:start].endswith("."):
            items = []                          # nothing typed yet: don't list every name
        return {"status": "ok", "matches": [label for label, *_ in items],
                "cursor_start": cursor_pos - (len(line) - start), "cursor_end": cursor_pos, "metadata": {}}

    def do_is_complete(self, code):
        from ..repl import needs_more
        try:
            more = needs_more(code, self.fm)
        except Exception:
            more = False
        return {"status": "incomplete", "indent": "    "} if more else {"status": "complete"}


def install(user=True, prefix=None):
    """Register the kernel with Jupyter (`fermium jupyter install`)."""
    import json
    import sys
    import tempfile
    from jupyter_client.kernelspec import KernelSpecManager
    spec = {"argv": [sys.executable, "-m", "fermium.jupyter", "-f", "{connection_file}"],
            "display_name": "Fermium", "language": "fermium"}
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "kernel.json"), "w") as fh:
            json.dump(spec, fh, indent=1)
        return KernelSpecManager().install_kernel_spec(d, "fermium", user=user and prefix is None, prefix=prefix)
