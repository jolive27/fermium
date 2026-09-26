"""The GitHub Actions workflows (spec §B7) parse, have no duplicated keys, and keep the minutes rules (CLAUDE.md
rule 11): macOS never runs on plain pushes, and every workflow cancels its old runs.

PyYAML silently keeps the last of two equal keys, so a merge that left a second `jobs:` block went unnoticed until
GitHub rejected the file; the loader here refuses duplicates.
"""
import glob
import os

import pytest

yaml = pytest.importorskip("yaml")

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FILES = sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml")))


class NoDuplicateKeys(yaml.SafeLoader):
    pass


def _mapping(loader, node, deep=False):
    loader.flatten_mapping(node)
    seen = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise yaml.constructor.ConstructorError(None, None, f"duplicate key {key!r}", key_node.start_mark)
        seen.add(key)
    return yaml.SafeLoader.construct_mapping(loader, node, deep)


NoDuplicateKeys.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _mapping)


def load(path):
    return yaml.load(open(path, encoding="utf-8"), Loader=NoDuplicateKeys)


def triggers(wf):
    return wf.get("on", wf.get(True))        # YAML 1.1 reads a bare `on` key as True


def test_the_workflows_exist():
    names = {os.path.basename(f) for f in FILES}
    assert {"ci.yml", "release.yml"} <= names


def test_duplicate_keys_are_refused():
    with pytest.raises(yaml.constructor.ConstructorError):
        yaml.load("jobs:\n  a: 1\njobs:\n  b: 2\n", Loader=NoDuplicateKeys)


@pytest.mark.parametrize("path", FILES, ids=os.path.basename)
def test_workflow_parses_and_cancels_old_runs(path):
    wf = load(path)
    assert wf["jobs"] and triggers(wf)
    assert wf["concurrency"]["cancel-in-progress"] is True


@pytest.mark.parametrize("path", FILES, ids=os.path.basename)
def test_macos_never_runs_on_a_plain_push(path):
    wf = load(path)
    on = triggers(wf)
    for name, job in wf["jobs"].items():
        runs_on = str(job.get("runs-on", ""))
        oses = [str(e.get("os", "")) for e in job.get("strategy", {}).get("matrix", {}).get("include", [])]
        if "macos" not in runs_on and not any("macos" in o for o in oses):
            continue
        push = on.get("push") or {}
        if push.get("tags") and not push.get("branches") and set(push) == {"tags"}:
            continue                     # pushes of version tags only (the release), never plain pushes
        cond = job.get("if", "")
        assert "pull_request" in cond or "workflow_dispatch" in cond, f"{name} would run on every push"
        assert "push" not in cond.replace("pull_request", ""), f"{name} would run on pushes"


def test_ci_builds_and_runs_the_conformance_suite_on_both_platforms():
    ci = load(os.path.join(ROOT, ".github", "workflows", "ci.yml"))
    for job in ("rust-linux", "rust-macos"):
        steps = " ".join(str(s.get("run", "")) for s in ci["jobs"][job]["steps"])
        assert "cargo test" in steps and "conformance/run --impl rust" in steps and "RUST_FLOOR" in steps, job
    assert ci["jobs"]["rust-linux"]["runs-on"] == "ubuntu-latest"
    assert ci["jobs"]["rust-macos"]["runs-on"] == "macos-14"


def test_release_builds_one_binary_per_platform_on_tags_and_by_hand():
    rel = load(os.path.join(ROOT, ".github", "workflows", "release.yml"))
    on = triggers(rel)
    assert set(on) == {"push", "workflow_dispatch"} and on["push"] == {"tags": ["v*"]}
    assets = {e["asset"] for e in rel["jobs"]["build"]["strategy"]["matrix"]["include"]}
    assert assets == {"fermium-linux-x86_64", "fermium-macos-arm64"}
    steps = " ".join(str(s.get("run", "")) for s in rel["jobs"]["build"]["steps"])
    assert "cargo build --release -p fermium-cli" in steps and "strip" in steps
