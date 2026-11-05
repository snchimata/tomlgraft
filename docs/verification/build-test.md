# Local build and test evidence

Date: 2026-11-05. Scope: implement and locally verify
`docs/sol-design/tomlgraft_plan.md`; no remote publication or release.

The Rust core, CLI, file transaction, CPython binding/stubs, strict JSON decoder,
dependency/group helpers, PEP 723 support, and read-only lock guards are built.
The README states supported layouts and platform restrictions. The original
`.gitignore` changes are retained; the design plan remains locally ignored by
the existing `*sol-design/` rule.

| Local environment | Evidence |
| --- | --- |
| Windows x86-64, local NTFS; Rust 1.97 | 24 Rust tests plus 1 doctest pass; formatting and Clippy with warnings denied pass |
| Windows x86-64, Rust 1.89 minimum | Workspace tests, formatting and Clippy pass |
| Ubuntu 24.04 under WSL2, x86-64, local ext4; Rust 1.97 | 24 Rust tests plus 1 doctest pass; formatting and Clippy pass |
| Linux x86-64, Rust 1.89 minimum | Workspace tests pass |
| CPython 3.12, Windows and Linux wheels | Four installed-binding tests pass per platform, including CLI file parity and independent `tomllib`/`packaging` parsing |
| Windows source distribution | Clean isolated build/install and installed-binding tests pass |
| PowerShell | String-value, dependency, and JSON batch-preview examples pass |
| Cargo packages | Both crates package and verify, using Cargo's temporary local registry for the unpublished core dependency |
| Local CLI installation | `cargo install --path crates/tomlgraft-cli --locked` into an isolated install root; installed executable used by binding/PowerShell smokes |
| Artifact contents | Two wheels, one sdist, two crates checked; licenses and type stubs present; runtime test/fixture payload absent |

The interruption helper is one of the Rust test functions; its parent test
actually spawns/kills that helper process after staging output, checks the
unchanged original, and verifies lock recovery. Fault injection covers write,
file-sync, replacement, and directory-sync boundaries. Post-commit failure
reports retain `committed=true`, uncertain durability, and the prepared count.
External edits and replacement identity changes cause conflicts. OS-specific
checks cover protected Windows DACL/owner preservation, alternate-stream
refusal, Linux permissions, xattr refusal, and link refusal. The generated
property covers byte-identical empty/equal operations with comments/endings.

Fixtures/specification snapshots: primary sources listed in the design plan
retrieved on 2026-11-05. Pins: toml_edit 0.25.18 (TOML 1.1), pep508_rs 0.9.2,
clap 4.6.7, tempfile 3.27.0, PyO3 0.30.1, maturin 1.13.1, packaging 26.3.
The independent TOML reader covers the compatible TOML 1.0 subset.

The crates.io APIs for `tomlgraft` and `tomlgraft-core`, and the PyPI JSON API
for `tomlgraft`, returned 404 during read-only availability checks. This does
not reserve names or authorize publication.

## Reproduce

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
cargo build --release --locked -p tomlgraft
python -m maturin build --release --locked --out dist
# Install the matching wheel in a clean CPython 3.12 environment.
python -m unittest discover -s tests -v
cargo package --workspace --exclude tomlgraft-py --allow-dirty --locked
python -m maturin sdist --out dist
python tests/check_artifacts.py
```

Use `TOMLGRAFT_CLI` with the absolute CLI path for Python parity. Linux workspace
tests require a C linker and Python development libraries. Local Linux testing
uses ext4 temporary files, with build artifacts in a separate Linux cache.
Linux wheels were built with `--compatibility linux`, without a manylinux claim.
The checked-in CI workflow repeats the checks on Windows/Ubuntu and Rust
1.89/1.97; no hosted workflow was executed in this local scope.

## Descriptive measurements

Raw samples, fixture hashes, artifact hashes, compiler/target/OS, and sample
counts are in [Windows baseline](baseline-windows.json) and
[Linux baseline](baseline-linux.json). The host CPU is an AMD Ryzen 7 9800X3D.
Package sizes/hashes and the uncommitted source inventory are recorded in
[artifact provenance](artifacts.json); the recorded Git head is the baseline,
not a claim that these changes have been committed.
Each row has 30 observations; manifests/scripts have 500 lines and the read-only
lock has 10,000 lines. `tests/measure.py` checks successful reports and unchanged
preview inputs. CLI rows include fresh process creation, I/O, and JSON output;
Python rows include the owned binding conversion and preparation.

All times below are milliseconds. Python preparation excludes file I/O.

| Workload | Windows median | Windows p95 | Linux / WSL2 median | Linux / WSL2 p95 |
| --- | ---: | ---: | ---: | ---: |
| CLI · manifest · no-op | 4.835 | 5.504 | 1.109 | 1.286 |
| CLI · manifest · preview | 5.630 | 7.193 | 1.098 | 1.223 |
| CLI · manifest · preview + diff | 4.893 | 5.707 | 1.154 | 1.313 |
| CLI · manifest · file write | 8.008 | 9.333 | 3.897 | 4.700 |
| Python · manifest · prepare | 0.087 | 0.098 | 0.083 | 0.192 |
| CLI · script · no-op | 4.792 | 5.506 | 1.014 | 1.059 |
| CLI · script · preview | 4.637 | 4.949 | 1.075 | 1.279 |
| CLI · script · preview + diff | 4.707 | 5.373 | 1.073 | 1.213 |
| CLI · script · file write | 7.757 | 8.403 | 3.980 | 4.500 |
| Python · script · prepare | 0.030 | 0.045 | 0.027 | 0.060 |
| CLI · lock · structural check | 14.756 | 18.901 | 11.669 | 13.035 |

Filesystem cache is warm/uncontrolled. New processes do not imply cold storage.
Windows write rows report uncertain directory durability; Linux write rows
sync the parent. These local rows are not interchangeable clocks, portability
evidence, p99 estimates, superiority claims, or release thresholds. Rust-only
preparation and peak memory were not measured. Initial performance hypotheses
remain unqualified.

## Remaining release gates

No hosted CI, physical power-loss test, macOS/ARM build, Python versions other
than 3.12, manylinux/static portability, or registry upload was performed.
The bounded writer contract excludes network/hostile directories and retains
the noncooperating-editor recheck/rename race. Windows directory sync and audit
policy preservation remain unqualified. Other security/access policy metadata
that cannot be preserved is refused. No general metadata-preservation or
crash-durability claim is authorized by these tests.
