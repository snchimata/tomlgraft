# tomlgraft

Small TOML edits from Python, backed by the same Rust core as the CLI.

```sh
python -m pip install tomlgraft==0.1.0
```

Registry installation becomes available after v0.1.0 is published. The initial
wheels target CPython 3.12 on Windows/Linux x86-64. Source builds require Rust
1.89+ and a platform linker.

```python
import tomlgraft

result = tomlgraft.patch(
    b"[project]\nversion = '1.0.0' # keep\n",
    kind="pyproject",
    operations=[{"op": "set", "path": ["project", "version"], "value_toml": "'1.1.0'"}],
)
assert result["output"] == b"[project]\nversion = '1.1.0' # keep\n"
```

`patch_file(path, operations, dry_run=True)` previews through the shared file
writer. Both functions default `create_script_block=False`. File writes default
`dry_run=False`; inspect `committed` and `durability` in reports/exceptions.
The Rust boundary owns inputs before releasing the GIL; no universal ABI or
free-threaded Python support is advertised.

Initial experimental release: local NTFS/ext4 file writes, bounded preservation,
and read-only lockfiles. Windows directory durability is uncertain. Other
editors can race the final recheck/rename. No resolver, Python script execution,
backend, or network access is invoked. Unsupported layouts/security metadata
are refused.

[Full API and contract](https://github.com/snchimata/tomlgraft#readme) ·
[Runnable examples](https://github.com/snchimata/tomlgraft/tree/main/examples) ·
[Source and releases](https://github.com/snchimata/tomlgraft)
