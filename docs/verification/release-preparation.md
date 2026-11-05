# v0.1.0 release preparation

Date: 2026-11-05. This prepares the initial experimental release; it does not
record package publication or create a release tag.

Added runnable CLI/JSON/PEP 723/Python/Rust examples, rendered README diagrams,
package-specific documentation/metadata, changelog/release notes, version and
checksum guards, platform CLI bundles, and manual GitHub/crates.io/PyPI jobs.
The workflow defaults to build-only; destinations require explicit selection.

Local checks:

- Rust workspace tests, doctest, examples, formatting, and Clippy pass on
  Windows/Linux; the original Rust 1.89 minimum remains covered by CI.
- Ten Python/binding/release checks pass on Windows, including the documented
  CLI batch/script fixtures and checksum tampering/incomplete-matrix refusal.
- The Python example's preview/commit/idempotence/rollback assertions pass.
- Windows wheel/sdist metadata passes `twine check --strict`; source installation
  and PowerShell smoke checks pass.
- The Linux wheel passes enforced manylinux 2.35 auditing and installation,
  binding/example checks, and strict metadata validation. It is deliberately
  tagged 2.35 even though the local audit permits the narrower 2.34 symbol set.
- Both Cargo packages verify. The core package includes its runnable example;
  runtime test fixtures remain excluded.
- Both README SVGs were rendered with mermaid-cli 12.2.0 and visually inspected.
  SVG XML parses, accessibility titles/descriptions are included, and labels
  use SVG text instead of foreign HTML.
- Actionlint 1.7.12 accepts both workflows. Cargo audit 0.22.2 reports no
  vulnerabilities or warnings; no advisory waiver was added.
- A local seven-artifact release directory passes the exact matrix and checksum
  checks. Tagged CI regenerates the authoritative clean artifacts and hashes.

Performance baselines are reused unchanged; these documentation/package changes
did not trigger another timing benchmark. The older build-test/provenance files
describe their original local snapshot rather than this release commit.

Release account prerequisites remain external: the repository check found no
release environments/secrets. Configure the crates.io token and verify the PyPI
pending trusted publisher before enabling package destinations. No credential
value was read, copied, or committed. Hosted CI is recorded separately from
these local checks; a local pass alone does not mark the release published.

The release retains Windows directory-durability uncertainty, bounded layout
and metadata support, and the noncooperating-editor race. Physical power-loss,
macOS/ARM, other Python versions, and universal preservation remain outside
this release. See the [release guide](../releasing.md) for the exact procedure.

The first hosted run exposed Cargo 1.89's inability to package the CLI against
the still-unpublished core. Source tests and wheels passed on that toolchain.
CI now performs only release packaging with Cargo 1.97, retaining the 1.89
compiler checks. Official action pins use their current Node 24 releases.
