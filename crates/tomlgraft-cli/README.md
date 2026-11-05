# tomlgraft

A local CLI for small TOML, `pyproject.toml`, and PEP 723 metadata edits.

```sh
cargo install tomlgraft --version 0.1.1 --locked
tomlgraft set pyproject.toml project.version '"1.1.0"' --dry-run --diff
tomlgraft dep add pyproject.toml 'pytest>=8' --group dev
tomlgraft check pylock.toml --format json
```

On PowerShell, pass the TOML literal string as `"'1.1.0'"`.
Versioned JSON batches support ordered, atomic edits and stdin. `--check` on
mutation exits 6 when a change is needed. Runtime errors support JSON reports
with explicit `committed` and `durability` fields.

Initial experimental release: local Windows x86-64 NTFS and Linux x86-64 ext4
writes; UTF-8 input. Windows directory durability is uncertain. Unsupported
layouts/metadata and all recognized lockfile mutations are refused.

[Commands and contract](https://github.com/snchimata/tomlgraft#readme) ·
[Examples](https://github.com/snchimata/tomlgraft/tree/main/examples) ·
[Release guide](https://github.com/snchimata/tomlgraft/blob/main/docs/releasing.md)
