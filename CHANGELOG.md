# Changelog

## [0.1.1] - Unreleased

Documentation and package metadata update; editing behavior is unchanged.

- Expand the PyPI README with Python API, dependency, and PEP 723 examples.
- Clarify installation, CLI separation, preservation limits, and commit reports.
- Use absolute image/documentation links and qualify recorded performance data.
- Refresh package versions and installation references across distributions.

## [0.1.0] - 2026-11-05

Initial experimental release.

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

[0.1.1]: https://github.com/snchimata/tomlgraft/releases/tag/v0.1.1
[0.1.0]: https://github.com/snchimata/tomlgraft/releases/tag/v0.1.0
