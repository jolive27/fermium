"""The Jupyter kernel: output, errors, inline plots, completion, and the example notebook."""
import os

import pytest

pytest.importorskip("ipykernel")
jupyter_client = pytest.importorskip("jupyter_client")

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


@pytest.fixture(scope="module")
def jupyter_path(tmp_path_factory):
    """Install the kernel spec into a temporary prefix and point Jupyter at it."""
    from fermium.jupyter.kernel import install
    prefix = tmp_path_factory.mktemp("jup")
    install(prefix=str(prefix))
    old = os.environ.get("JUPYTER_PATH")
    os.environ["JUPYTER_PATH"] = str(prefix / "share" / "jupyter")
    yield
    if old is None:
        os.environ.pop("JUPYTER_PATH", None)
    else:
        os.environ["JUPYTER_PATH"] = old


@pytest.fixture(scope="module")
def kernel(jupyter_path, tmp_path_factory):
    from jupyter_client.manager import start_new_kernel
    cwd = tmp_path_factory.mktemp("work")
    km, kc = start_new_kernel(kernel_name="fermium", cwd=str(cwd))
    yield kc, cwd
    kc.stop_channels()
    km.shutdown_kernel(now=True)


def execute(kc, code):
    msg_id = kc.execute(code)
    outs = []
    while True:
        m = kc.get_iopub_msg(timeout=60)
        if m["parent_header"].get("msg_id") != msg_id:
            continue
        t = m["msg_type"]
        if t == "stream":
            outs.append((m["content"]["name"], m["content"]["text"]))
        elif t == "display_data":
            outs.append(("display", m["content"]["data"]))
        elif t == "status" and m["content"]["execution_state"] == "idle":
            break
    return reply_to(kc, msg_id)["status"], outs


def reply_to(kc, msg_id, timeout=60):
    """The shell reply to this request. Under load, start_new_kernel may resend its kernel_info_request, and the
    extra kernel_info_reply (status "ok") waits in the shell queue: never take it for this request's reply."""
    while True:
        r = kc.get_shell_msg(timeout=timeout)
        if r["parent_header"].get("msg_id") == msg_id:
            return r["content"]


def test_values_carry_over_between_cells(kernel):
    kc, _ = kernel
    assert execute(kc, "L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g") == ("ok", [("stdout", "9.70 m/s²\n")])
    assert execute(kc, "print g in ft/s²") == ("ok", [("stdout", "31.8 ft/s²\n")])


def test_errors_are_one_line_physics(kernel):
    kc, _ = kernel
    # self-contained: under pytest-xdist this test may not run right after the one that defines L and T
    assert execute(kc, "L = 1.20 m\nT = 2.21 s")[0] == "ok"
    status, outs = execute(kc, "y = L + T")
    assert status == "error"
    assert outs[0][0] == "stderr" and "can't add length [m] to time [s]" in outs[0][1]


def test_plots_are_inline(kernel):
    kc, cwd = kernel
    status, outs = execute(kc, "solve x'' = -(9/s²) x with x(0) = 1 cm, x'(0) = 0 cm/s for t from 0 s to 5 s\n"
                               "plot x vs t to \"k.png\"")
    assert status == "ok"
    kinds = [o[0] for o in outs]
    assert "display" in kinds and "image/png" in outs[kinds.index("display")][1]
    assert not any("plot saved" in o[1] for o in outs if o[0] == "stdout")
    assert (cwd / "k.png").exists()


def test_backslash_completion(kernel):
    kc, _ = kernel
    reply = reply_to(kc, kc.complete("x = \\ome", 8), timeout=10)
    assert reply["matches"] == ["ω"] and reply["cursor_start"] == 4


def test_is_complete(kernel):
    kc, _ = kernel
    assert reply_to(kc, kc.is_complete("if 1 > 0"), timeout=10)["status"] == "incomplete"


def test_example_notebook_runs(jupyter_path, tmp_path):
    nbformat = pytest.importorskip("nbformat")
    nbclient = pytest.importorskip("nbclient")
    nb = nbformat.read(os.path.join(ROOT, "examples", "notebook.ipynb"), as_version=4)
    client = nbclient.NotebookClient(nb, kernel_name="fermium", allow_errors=True,
                                     resources={"metadata": {"path": str(tmp_path)}}, timeout=120)
    client.execute()
    code = [c for c in nb.cells if c.cell_type == "code"]
    text = lambda c: "".join(o.get("text", "") for o in c.outputs if o.output_type == "stream")  # noqa: E731
    assert text(code[0]) == "9.70 m/s²\n31.8 ft/s²\n"
    assert "can't add" in text(code[1])
    assert text(code[2]) == "10 rad/s\n"
    assert "3.52 cm" in text(code[3])
    assert any(o.output_type == "display_data" and "image/png" in o.data for o in code[3].outputs)
    assert text(code[4]).splitlines()[1] == "1 fm"
