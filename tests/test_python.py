"""Installed-wheel checks with independent TOML and packaging readers."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest

from packaging.requirements import Requirement
from packaging.specifiers import SpecifierSet
import tomlgraft


class BindingTests(unittest.TestCase):
    def test_documented_cli_batch_and_script_examples(self):
        cli = os.environ.get("TOMLGRAFT_CLI")
        if not cli:
            self.skipTest("TOMLGRAFT_CLI is required for CLI examples")
        examples = Path(__file__).resolve().parents[1] / "examples"
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "pyproject.toml"
            source = (examples / "pyproject.toml").read_bytes()
            path.write_bytes(source)
            command = [cli, "batch", str(path), "--patch", str(examples / "changes.json"), "--format", "json"]
            preview = subprocess.run([*command, "--dry-run", "--diff"], capture_output=True, check=True)
            self.assertEqual(json.loads(preview.stdout)["status"], "would_change")
            self.assertEqual(path.read_bytes(), source)
            self.assertEqual(len(list(Path(temp).iterdir())), 1)
            subprocess.run(command, capture_output=True, check=True)
            metadata = tomllib.loads(path.read_text())
            self.assertEqual(metadata["project"]["version"], "1.1.0")
            self.assertEqual(metadata["dependency-groups"]["dev"], ["pytest>=8"])
            self.assertFalse(metadata["tool"]["demo"]["enabled"])
            self.assertIn(b"# Keep this release comment.", path.read_bytes())
            subprocess.run([*command, "--check"], capture_output=True, check=True)
            subprocess.run([cli, "dep", "add", str(path), "httpx>=0.28", "--optional", "test"], capture_output=True, check=True)
            subprocess.run([cli, "dep", "rm", str(path), "requests"], capture_output=True, check=True)
            self.assertEqual(tomllib.loads(path.read_text())["project"]["dependencies"], [])
            script = Path(temp) / "demo.py"
            original = (examples / "script.py").read_bytes()
            script.write_bytes(original)
            subprocess.run([cli, "dep", "add", str(script), "rich>=14"], capture_output=True, check=True)
            self.assertEqual(script.read_bytes(), original.replace(b"rich>=13", b"rich>=14"))

    def test_bytes_noop_and_independent_parser(self):
        source = b"# keep\r\n[project]\r\nversion = '1' # tail"
        op = {"op": "set", "path": ["project", "version"], "value_toml": "'2'"}
        result = tomlgraft.patch(source, kind="pyproject", operations=[op])
        self.assertEqual(result["output"], source.replace(b"'1'", b"'2'"))
        self.assertEqual(tomllib.loads(result["output"].decode())["project"]["version"], "2")
        self.assertEqual(tomlgraft.patch(result["output"], kind="pyproject", operations=[op])["output"], result["output"])

    def test_reference_requirement_identity_and_script_reader(self):
        source = b"#!/usr/bin/python\n# /// script\n# dependencies = ['My_Pkg[b,a]>=1']\n# requires-python = '>=3.12'\n# ///\nprint('untouched')\n"
        result = tomlgraft.patch(source, kind="script", operations=[{"op": "dep_upsert", "scope": {"kind": "script"}, "requirement": "my-pkg[a,b]>=2"}])["output"]
        pattern = r"(?m)^# /// (?P<type>[a-zA-Z0-9-]+)$\s(?P<content>(^#(| .*)$\s)+)^# ///$"
        match = re.search(pattern, result.decode())
        self.assertEqual(match["type"], "script")
        data = tomllib.loads("".join(line[2:] if line.startswith("# ") else line[1:] for line in match["content"].splitlines(keepends=True)))
        req = Requirement(data["dependencies"][0])
        self.assertEqual(req.extras, {"a", "b"})
        self.assertEqual(str(req.specifier), ">=2")
        self.assertIn("3.12", SpecifierSet(data["requires-python"]))
        self.assertTrue(result.endswith(b"print('untouched')\n"))

    def test_typed_error_and_batch_rollback(self):
        with self.assertRaises(tomlgraft.ConflictError) as caught:
            tomlgraft.patch(b"x=1", kind="toml", operations=[{"op": "set", "path": ["x"], "value_toml": "2"}, {"op": "remove", "path": ["missing"]}])
        e = caught.exception
        self.assertEqual((e.code, e.operation, e.committed, e.report["operations"]), ("missing_path", 1, False, 0))
        with self.assertRaises(tomlgraft.ArgumentError):
            tomlgraft.patch(b"x=1", kind="toml", operations=[{"op": "remove", "path": ["x"], "unknown": True}])
        with self.assertRaises(tomlgraft.UnsupportedError):
            tomlgraft.patch(b"lock-version='1.0'\ncreated-by='test'\npackages=[]", kind="toml", operations=[{"op": "set", "path": ["created-by"], "value_toml": "'other'"}])

    def test_file_preview_noop_parity_and_write(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "a.toml"
            source = b"x=1 # keep\n"
            path.write_bytes(source)
            mtime = path.stat().st_mtime_ns
            ops = [{"op": "set", "path": ["x"], "value_toml": "2"}]
            report = tomlgraft.patch_file(path, ops, dry_run=True)
            self.assertEqual((report["status"], report["committed"]), ("would_change", False))
            self.assertEqual(len(list(Path(temp).iterdir())), 1)
            tomlgraft.patch_file(path, [])
            self.assertEqual(path.stat().st_mtime_ns, mtime)
            report = tomlgraft.patch_file(path, ops)
            self.assertTrue(report["committed"])
            self.assertEqual(path.read_bytes(), tomlgraft.patch(source, kind="toml", operations=ops)["output"])
            cli = os.environ.get("TOMLGRAFT_CLI")
            if cli:
                path.write_bytes(source)
                batch = Path(temp) / "batch.json"
                batch.write_text(json.dumps({"schema_version": 1, "operations": ops}), encoding="utf-8")
                out = subprocess.run([cli, "batch", str(path), "--patch", str(batch), "--format", "json"], capture_output=True, check=True)
                self.assertTrue(json.loads(out.stdout)["committed"])
                self.assertEqual(path.read_bytes(), b"x=2 # keep\n")


if __name__ == "__main__":
    unittest.main()
