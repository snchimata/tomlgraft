# Runnable examples

The fixtures are safe to copy; all examples below edit a disposable copy.
The Python and Rust programs contain assertions and run in CI.

## CLI and ordered batch

From the repository root, with `tomlgraft` on PATH:

```sh
example_dir=$(mktemp -d)
cp examples/pyproject.toml "$example_dir/pyproject.toml"
tomlgraft batch "$example_dir/pyproject.toml" --patch examples/changes.json --dry-run --diff
tomlgraft batch "$example_dir/pyproject.toml" --patch examples/changes.json --format json
tomlgraft dep add "$example_dir/pyproject.toml" 'httpx>=0.28' --optional test
tomlgraft dep rm "$example_dir/pyproject.toml" requests
tomlgraft check "$example_dir/pyproject.toml"
```

PowerShell equivalents:

```powershell
$exampleDir = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $exampleDir | Out-Null
$manifest = Join-Path $exampleDir 'pyproject.toml'
Copy-Item -LiteralPath examples/pyproject.toml -Destination $manifest
tomlgraft batch $manifest --patch examples/changes.json --dry-run --diff
tomlgraft batch $manifest --patch examples/changes.json --format json
tomlgraft set $manifest project.version "'1.2.0'"
```

`--check` on a mutation returns 6 if the batch would change the file; it writes
nothing. Running the same upserts again returns `unchanged`. A failed operation
rolls back the whole batch. File writes can leave a stable sibling lock sidecar.

## PEP 723 scripts

```sh
cp examples/script.py "$example_dir/demo.py"
tomlgraft dep add "$example_dir/demo.py" 'rich>=14' --dry-run --diff
tomlgraft dep add "$example_dir/demo.py" 'rich>=14'
```

The Python code is never executed by tomlgraft. To add a metadata block to a
script that lacks one, pass `--create-script-block` explicitly.

## Python and Rust

```sh
python examples/patch.py
cargo run --locked -p tomlgraft-core --example patch
```

Python requires the installed CPython 3.12 wheel. File examples use supported
local NTFS/ext4 targets; pure `patch` / `patch_bytes` needs no writable file.
See the [preservation contract](../README.md#preservation-and-metadata) and
[release guide](../docs/releasing.md) for the supported scope.
