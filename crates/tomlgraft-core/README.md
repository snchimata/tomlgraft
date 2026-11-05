# tomlgraft-core

Small, transactional TOML edits with source-span preservation. This is the
shared Rust library used by the `tomlgraft` CLI and Python extension.

```rust
use tomlgraft_core::{patch_bytes, Kind, Operation, Options};

let operation = Operation::Set {
    path: vec!["project".into(), "version".into()],
    value_toml: "'1.1.0'".into(),
};
let patch = patch_bytes(
    b"[project]\nversion = '1.0.0' # keep\n",
    Kind::Pyproject,
    &[operation],
    &Options::default(),
)?;
assert_eq!(patch.output, b"[project]\nversion = '1.1.0' # keep\n");
# Ok::<(), tomlgraft_core::PatchError>(())
```

`patch_bytes` performs owned, in-memory preparation. `patch_file` uses the same
editor and a locked, staged file transaction. Ordered operations either all
succeed or return an error. Recognized lockfiles are read-only. No resolver,
Python execution, backend, or network request is invoked.

Initial experimental release: Rust 1.89+, TOML 1.1; local Windows x86-64 NTFS
and Linux x86-64 ext4 writes. Windows directory durability is uncertain.
Unsupported layouts and security metadata are refused. Other editors can race
the final recheck/rename. No universal losslessness or durability claim.

[Full contract and CLI](https://github.com/snchimata/tomlgraft#readme) ·
[Runnable examples](https://github.com/snchimata/tomlgraft/tree/main/examples) ·
[API reference](https://docs.rs/tomlgraft-core)
