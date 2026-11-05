import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location("release", Path(__file__).resolve().parents[1] / "scripts/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_readme_diagrams_have_accessible_svg_text(self):
        namespace = {"svg": "http://www.w3.org/2000/svg"}
        for name in ("architecture", "transaction"):
            root = ET.parse(release.ROOT / "docs" / "diagrams" / f"{name}.svg")
            self.assertTrue(root.find("svg:title", namespace).text)
            self.assertTrue(root.find("svg:desc", namespace).text)
            self.assertTrue(root.findall(".//svg:text", namespace))
            self.assertFalse(root.findall(".//svg:foreignObject", namespace))

    def test_versions_and_tag_mismatch(self):
        self.assertEqual(release.version(), "0.1.0")
        with self.assertRaises(ValueError):
            release.check_tag("../../main", release.version())

    def test_tampering_missing_and_extra_files_fail_checksums(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            binary = directory / "artifact.zip"
            binary.write_bytes(b"original")
            release.write_checksums(directory)
            release.verify_checksums(directory)
            binary.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                release.verify_checksums(directory)
            binary.unlink()
            with self.assertRaises(ValueError):
                release.verify_checksums(directory)
            binary.write_bytes(b"original")
            (directory / "unexpected").write_bytes(b"extra")
            with self.assertRaises(ValueError):
                release.verify_checksums(directory)

    def test_stale_or_incomplete_artifact_matrix_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                release.validate_assets(Path(temp), release.version())

    def test_arm_host_cannot_emit_an_x86_archive(self):
        with patch.object(release.platform, "machine", return_value="aarch64"):
            with self.assertRaises(ValueError):
                release.bundle(Path("unused"), Path("unused"), release.version())


if __name__ == "__main__":
    unittest.main()
