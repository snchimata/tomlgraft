# Releasing tomlgraft

Prepared version: **0.1.1**, documentation patch release. No package or GitHub
release is published by opening a PR, merging it, or pushing a tag. The release
workflow is manual, and each destination is off by default.

## Supported release artifacts

| Destination | Artifacts |
| --- | --- |
| crates.io | `tomlgraft-core`, then `tomlgraft` (CLI) |
| PyPI | `tomlgraft`: CPython 3.12 Windows x86-64 and manylinux glibc 2.35+ x86-64 wheels; source distribution |
| GitHub prerelease | Both CLI archives, both wheels, the sdist, both crates, and `SHA256SUMS` |

The Linux release builder is Ubuntu 22.04 and enforces manylinux 2.35 wheel
auditing. Linux CLI binaries require glibc 2.35+. This scope does not promise
static linkage, macOS/ARM support, other Python versions, or power-loss safety.
See the [file contract](../README.md#file-transactions).

Rust 1.89 remains the build/test minimum. Release packaging uses Cargo 1.97:
older Cargo cannot stage the initial unpublished core dependency while
packaging the CLI. The CI minimum-version builds, tests, examples, and wheels
still run with Rust 1.89 before packaging switches toolchains.

## One-time account setup

The repository must have Actions enabled. The workflow requests write access
only in publishing jobs; build/check jobs have read-only repository access.

For crates.io, verify the maintainer account's email and configure a publishing
token for the two crate names as the `CARGO_REGISTRY_TOKEN` secret in the
`crates-io` GitHub environment. Do not put tokens in files, commands, or PRs.
See [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html).

For PyPI, configure a trusted publisher for the project:

| Setting | Value |
| --- | --- |
| PyPI project | `tomlgraft` |
| GitHub owner | `snchimata` |
| Repository | `tomlgraft` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

The PyPI job uses OIDC, with no stored PyPI token. Configure that publisher in
the intended maintainer's PyPI account before enabling `publish_pypi`. See
[PyPI's trusted-publisher setup](https://docs.pypi.org/trusted-publishers/adding-a-publisher/).
Environment reviewer rules are a repository-owner choice; a new environment
name alone does not add an approval rule.

Repository account setup is separate from checked-in workflow code. The local
readiness check found no release environments/secrets configured; do not treat
their presence in YAML as evidence that publisher credentials are ready.

## Build and review

1. Merge the reviewed release PR only after its exact commit passes CI. The
   workflow tests Rust 1.89/1.97, Windows/Linux, examples, package contents, and
   Python bindings. Dependency advisories fail the designated Linux job.
2. Check `CHANGELOG.md`, `docs/releases/v0.1.1.md`, the supported matrix, and
   current registry-name availability. A 404 check does not reserve a name.
3. Create an annotated tag on that reviewed `main` commit:

   ```sh
   git switch main
   git pull --ff-only
   git tag -a v0.1.1 -m "tomlgraft v0.1.1"
   git push origin v0.1.1
   ```

4. Run the manual workflow with all publishing switches left false:

   ```sh
   gh workflow run release.yml --ref main -f tag=v0.1.1
   ```

The workflow verifies version/tag equality and main ancestry, repeats CI for
that exact tag, builds artifacts, installs/tests wheels and the sdist, checks
package metadata, and assembles an exact artifact list with checksums. Download
the `verified-release` Actions artifact and inspect it before publication:
Run verification from the matching tagged checkout.

```sh
python scripts/release.py --verify path/to/extracted/verified-release
```

`scripts/release.py` checks synchronized versions/changelog/notes by default.
It rejects an incomplete/stale asset matrix, duplicate checksum entries,
unexpected files, symlinks, and changed or missing checksummed bytes. CLI
archives contain the executable, license, changelog, and a focused README.
Cargo examples are included; runtime test fixtures are excluded.

## Publish the reviewed tag

After account setup and artifact review, explicitly choose the destinations:

```sh
gh workflow run release.yml --ref main -f tag=v0.1.1 \
  -f publish_crates=true -f publish_pypi=true -f publish_github=true
```

Core publishes before the CLI. PyPI receives only the two verified wheels and
the Python sdist. The GitHub prerelease publishes only after all requested
package destinations succeed. The workflow never creates/moves tags, merges
PRs, disables validation, overwrites release assets, or skips existing versions.

Package publication is irreversible for a version. If a destination fails after
another succeeds, inspect the registry versions/hashes before proceeding. Rerun
with only still-missing destinations selected; do not retry a successful upload
or change the tag/version's bytes. Record any partial publication explicitly.
If only the core crate published, verify its registry checksum against the
reviewed `.crate`, then publish just the missing CLI from the same clean tag
with `cargo publish --locked -p tomlgraft`. Leave `publish_crates` false when
resuming other destinations; the combined crates job does not skip versions.

After success, verify registry download/install behavior outside the checkout,
the GitHub asset names/checksums, and the release's commit. Mark the changelog
entry released only when publication actually completes.

## Diagram maintenance

README diagrams use committed SVGs, with `.mmd` sources alongside them. Render
with the pinned CLI and configuration, then inspect the resulting images:

```sh
npx --yes --package @mermaid-js/mermaid-cli@12.2.0 mmdc \
  -i docs/diagrams/architecture.mmd -o docs/diagrams/architecture.svg \
  -c docs/diagrams/config.json -b transparent
npx --yes --package @mermaid-js/mermaid-cli@12.2.0 mmdc \
  -i docs/diagrams/transaction.mmd -o docs/diagrams/transaction.svg \
  -c docs/diagrams/config.json -b transparent
```
