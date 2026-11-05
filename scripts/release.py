"""Validate release versions, build a CLI archive, and verify release checksums."""
import argparse
import hashlib
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def version():
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    value = workspace["workspace"]["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", value) or project["version"] != value:
        raise ValueError("Cargo and Python release versions must match")
    if workspace["workspace"]["dependencies"]["tomlgraft-core"]["version"] != f"={value}":
        raise ValueError("core dependency must pin the release version")
    for member in workspace["workspace"]["members"]:
        package = tomllib.loads((ROOT / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        if package["version"] != {"workspace": True}:
            raise ValueError("all crates must inherit the workspace version")
    if f"## [{value}]" not in (ROOT / "CHANGELOG.md").read_text(encoding="utf-8"):
        raise ValueError("release version needs a changelog entry")
    if not (ROOT / "docs" / "releases" / f"v{value}.md").is_file():
        raise ValueError("release notes are missing")
    return value


def check_tag(tag, value):
    if tag != f"v{value}":
        raise ValueError("tag must match the package version")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tagged = subprocess.check_output(["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"], cwd=ROOT, text=True).strip()
    if tagged != head:
        raise ValueError("checkout must be the exact release tag")


def bundle(binary, output, value):
    targets = {"win32": "x86_64-pc-windows-msvc", "linux": "x86_64-unknown-linux-gnu"}
    if sys.platform not in targets or platform.machine().lower() not in {"amd64", "x86_64"}:
        raise ValueError("release archives are limited to Windows/Linux x86-64")
    name = f"tomlgraft-{value}-{targets[sys.platform]}"
    executable = "tomlgraft.exe" if sys.platform == "win32" else "tomlgraft"
    payload = [(binary, executable), (ROOT / "LICENSE", "LICENSE"),
               (ROOT / "CHANGELOG.md", "CHANGELOG.md"),
               (ROOT / "crates/tomlgraft-cli/README.md", "README.md")]
    if not binary.is_file():
        raise ValueError("build the release CLI before bundling")
    output.mkdir(parents=True, exist_ok=True)
    path = output / (name + (".zip" if sys.platform == "win32" else ".tar.gz"))
    if sys.platform == "win32":
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for source, target in payload:
                archive.write(source, f"{name}/{target}")
    else:
        with tarfile.open(path, "w:gz") as archive:
            for source, target in payload:
                archive.add(source, arcname=f"{name}/{target}", recursive=False)
    return path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def files(directory):
    entries = list(directory.iterdir())
    if any(path.is_symlink() or not path.is_file() for path in entries):
        raise ValueError("release directory must contain only regular files")
    return sorted(path for path in entries if path.name != "SHA256SUMS")


def validate_assets(directory, value):
    expected = {
        f"tomlgraft-{value}-cp312-cp312-win_amd64.whl",
        f"tomlgraft-{value}-cp312-cp312-manylinux_2_35_x86_64.whl",
        f"tomlgraft-{value}.tar.gz",
        f"tomlgraft-{value}.crate", f"tomlgraft-core-{value}.crate",
        f"tomlgraft-{value}-x86_64-pc-windows-msvc.zip",
        f"tomlgraft-{value}-x86_64-unknown-linux-gnu.tar.gz",
    }
    if {path.name for path in files(directory)} != expected:
        raise ValueError("release assets do not match the supported version/platform matrix")


def write_checksums(directory):
    (directory / "SHA256SUMS").write_text(
        "".join(f"{digest(path)}  {path.name}\n" for path in files(directory)), encoding="utf-8"
    )


def verify_checksums(directory):
    expected = {path.name: digest(path) for path in files(directory)}
    recorded = {}
    for line in (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        checksum, name = line.split("  ", 1)
        if name in recorded:
            raise ValueError("duplicate checksum entry")
        recorded[name] = checksum
    if recorded != expected:
        raise ValueError("release checksum mismatch or unexpected file")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--bundle", type=Path)
    action.add_argument("--assemble", type=Path)
    action.add_argument("--verify", type=Path)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    value = version()
    if args.tag:
        check_tag(args.tag, value)
    if args.bundle:
        if args.binary is None:
            parser.error("--bundle requires --binary")
        print(bundle(args.binary, args.bundle, value))
    elif args.assemble:
        validate_assets(args.assemble, value)
        write_checksums(args.assemble)
        verify_checksums(args.assemble)
    elif args.verify:
        validate_assets(args.verify, value)
        verify_checksums(args.verify)
    else:
        print(value)


if __name__ == "__main__":
    main()
