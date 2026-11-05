"""Assert local distributable contents without importing from the checkout."""
from pathlib import Path
import tarfile
import zipfile
import tomllib

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
wheels = list((root / "dist").glob("*.whl"))
assert wheels, "build a wheel first"
for wheel in wheels:
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        assert "tomlgraft/__init__.py" in names
        assert "tomlgraft/__init__.pyi" in names
        assert "tomlgraft/py.typed" in names
        assert any(name.endswith((".pyd", ".so")) for name in names)
        assert any(name.endswith("/LICENSE") for name in names)
        assert not any("/tests/" in name or "/fixtures/" in name for name in names)
sdists = list((root / "dist").glob("*.tar.gz"))
assert sdists, "build an sdist first"
for source in sdists:
    with tarfile.open(source) as archive:
        names = archive.getnames()
        assert any(name.endswith("/Cargo.lock") for name in names)
        assert any(name.endswith("/LICENSE") for name in names)
        assert not any("/.codex/" in name or "/sol-design/" in name or "/tests/" in name for name in names)
for name in ("tomlgraft-core", "tomlgraft"):
    package = root / "target" / "package" / f"{name}-{version}.crate"
    with tarfile.open(package) as archive:
        names = archive.getnames()
        assert any(name.endswith("/LICENSE") for name in names)
        assert not any("/tests/" in name or "/fixtures/" in name for name in names)
print(f"Artifact contents passed ({len(wheels)} wheel(s), {len(sdists)} sdist(s), 2 crates)")
