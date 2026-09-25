"""The Fermium Jupyter kernel.

Each cell is run by one ReplSession, so variables and functions carry over from cell to cell, as in
the REPL.  Printed output is streamed back, plots are shown inline (and still saved as PNG files),
errors come back in the usual one-line form, and TAB completes `\\name` symbols (`\\omega` → ω).
"""
from __future__ import annotations

import base64
import os

from ipykernel.kernelbase import Kernel

from .. import __version__
from ..errors import FermiumError
from ..symbols import LATEX


class _Sink:
    """File-like object that collects what the session writes during one cell."""

    def __init__(self):
        self.parts = []

    def write(self, s):
        self.parts.append(s)
        return len(s)

    def flush(self):
        pass

    def take(self):
        s = "".join(self.parts)
        self.parts = []
        return s


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
        self.fm = ReplSession(out=self.sink, base_dir=os.getcwd())

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

    def do_execute(self, code, silent, store_history=True, user_expressions=None, allow_stdin=False):
        self.silent = silent
        if not code.strip():
            return {"status": "ok", "execution_count": self.execution_count, "payload": [], "user_expressions": {}}
        before = len(self.fm.runtime.plots_saved)
        try:
            self.fm.execute(code)
        except FermiumError as e:
            out = self.sink.take()
            # plot lines ("plot saved to ...") are replaced by the inline picture
            self._stream("stdout", "".join(ln for ln in out.splitlines(True) if not ln.startswith("plot saved to ")))
            self._show_new_plots(before)
            msg = e.format(code)
            self._stream("stderr", msg + "\n")
            return {"status": "error", "execution_count": self.execution_count,
                    "ename": type(e).__name__, "evalue": msg, "traceback": [msg]}
        out = self.sink.take()
        self._stream("stdout", "".join(ln for ln in out.splitlines(True) if not ln.startswith("plot saved to ")))
        self._show_new_plots(before)
        return {"status": "ok", "execution_count": self.execution_count, "payload": [], "user_expressions": {}}

    def do_complete(self, code, cursor_pos):
        text = code[:cursor_pos]
        i = text.rfind("\\")
        if i < 0 or any(c.isspace() for c in text[i:]):
            return {"status": "ok", "matches": [], "cursor_start": cursor_pos, "cursor_end": cursor_pos,
                    "metadata": {}}
        name = text[i + 1:]
        matches = [LATEX[name]] if name in LATEX else sorted({LATEX[k] for k in LATEX if k.startswith(name)})
        return {"status": "ok", "matches": matches, "cursor_start": i, "cursor_end": cursor_pos, "metadata": {}}

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
