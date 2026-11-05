"""Run with an installed tomlgraft wheel; file edits use a disposable directory."""
import json
from pathlib import Path
import tempfile
import tomllib

import tomlgraft

examples = Path(__file__).resolve().parent
source = (examples / "pyproject.toml").read_bytes()
operations = json.loads((examples / "changes.json").read_text(encoding="utf-8"))["operations"]
result = tomlgraft.patch(source, kind="pyproject", operations=operations)
metadata = tomllib.loads(result["output"].decode())
assert metadata["project"]["version"] == "1.1.0"
assert metadata["dependency-groups"]["dev"] == ["pytest>=8"]
assert b"# Keep this release comment." in result["output"]
assert tomlgraft.patch(result["output"], kind="pyproject", operations=operations)["output"] == result["output"]

with tempfile.TemporaryDirectory() as directory:
    target = Path(directory) / "pyproject.toml"
    target.write_bytes(source)
    preview = tomlgraft.patch_file(target, operations, dry_run=True)
    assert preview["status"] == "would_change" and not preview["committed"]
    assert target.read_bytes() == source
    assert len(list(Path(directory).iterdir())) == 1
    report = tomlgraft.patch_file(target, operations)
    assert report["committed"] and target.read_bytes() == result["output"]
    try:
        tomlgraft.patch_file(target, [operations[0], {"op": "remove", "path": ["missing"]}])
    except tomlgraft.ConflictError as error:
        assert error.operation == 1 and not error.committed
    else:
        raise AssertionError("expected atomic rollback")
    assert target.read_bytes() == result["output"]

print("Python example passed: preparation, preview, commit, idempotence, rollback")
