# Changelog

## [0.1.0] - Unreleased

Initial experimental release, pending publication.

- Shared Rust core, CLI, and CPython 3.12 bindings.
- Source-span leaf edits, typed no-op equality, ordered JSON batches and rollback.
- Previews, diffs, stable exits, and explicit commit/durability reports.
- Dependency helpers for projects, optional extras, groups, and PEP 723 scripts.
- Read-only Python lockfiles with bounded structural validation.
- Locked, staged transactions with metadata checks on local NTFS/ext4.
- Runnable examples, README diagrams, and manual release automation.

Windows directory durability is uncertain. Unsupported layouts/metadata are
refused. Other writable platforms, Python versions outside 3.12, lock mutation,
resolver execution, and universal preservation are outside this release.

[0.1.0]: https://github.com/snchimata/tomlgraft/releases/tag/v0.1.0
