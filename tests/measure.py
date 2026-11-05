"""Descriptive local baseline: fresh CLI processes with an uncontrolled warm cache."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

import tomlgraft
import tomlgraft._native as native


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--samples", type=int, default=30)
    args = parser.parse_args()
    assert args.samples >= 20, "use at least 20 samples for this descriptive baseline"
    cli = args.cli.resolve()
    manifest = ("[project]\nversion = '1'\n" + "".join(f"# fixture line {n:04d}\n" for n in range(498))).encode()
    script = (b"# /// script\n# dependencies = []\n# [tool.example]\n# enabled = true\n# ///\n" + b"# fixture\n" * 494 + b"print('never executed')\n")
    lock = ("lock-version = '1.0'\ncreated-by = 'fixture'\n" + "".join(f"[[packages]]\nname = 'package-{n}'\n" for n in range(4999))).encode()
    fixtures = {"manifest": manifest, "script": script, "lock": lock}
    result = {
        "schema_version": 1,
        "date_utc": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "cpu": platform.processor(),
        "python": platform.python_version(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "target": subprocess.check_output(["rustc", "-vV"], text=True).split("host: ")[1].splitlines()[0],
        "flags": "cargo build --release --locked; profile.release.strip=true; default dynamic platform runtime",
        "cache": "warm/uncontrolled filesystem cache; CLI starts a fresh process per sample; no cold-storage claim",
        "qualification": "descriptive local baseline; no thresholds, p99, or superiority claim",
        "fixtures": {name: {"bytes": len(data), "lines": len(data.splitlines()), "sha256": hashlib.sha256(data).hexdigest()} for name, data in fixtures.items()},
        "binary_bytes": cli.stat().st_size, "binary_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(),
        "extension_bytes": Path(native.__file__).stat().st_size,
        "extension_sha256": hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
        "peak_memory": "not measured", "rows": [],
    }
    if result["cpu"] in ("", "x86_64", "aarch64") and Path("/proc/cpuinfo").exists():
        result["cpu"] = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown")
    def sample(name, action):
        values = []
        for _ in range(args.samples):
            start = time.perf_counter_ns()
            action()
            values.append((time.perf_counter_ns() - start) / 1_000_000)
        assert len(values) == args.samples and all(v > 0 for v in values)
        result["rows"].append({"name": name, "count": len(values), "median_ms": statistics.median(values), "p95_ms": sorted(values)[math.ceil(.95 * len(values)) - 1], "samples_ms": values})

    with tempfile.TemporaryDirectory() as temp:
        directory = Path(temp)
        result["temporary_filesystem_path"] = str(directory)
        for name, data in fixtures.items():
            path = directory / {"manifest": "pyproject.toml", "script": "example.py", "lock": "pylock.toml"}[name]
            path.write_bytes(data)
            def command(words, expected_commit=None):
                def call():
                    completed = subprocess.run([str(cli), *words, "--format", "json"], capture_output=True, check=True)
                    report = json.loads(completed.stdout)
                    assert report["status"] != "error"
                    if expected_commit is not None:
                        assert report["committed"] == expected_commit
                return call
            if name == "lock":
                sample("cli.lock.check", command(["check", str(path)]))
                assert path.read_bytes() == data
                continue
            key = "project.version" if name == "manifest" else "tool.example.enabled"
            value = "'2'" if name == "manifest" else "false"
            no_op = ["set", str(path), key, "'1'" if name == "manifest" else "true"]
            sample(f"cli.{name}.noop", command(no_op))
            sample(f"cli.{name}.preview", command(["set", str(path), key, value, "--dry-run"]))
            sample(f"cli.{name}.diff", command(["set", str(path), key, value, "--dry-run", "--diff"]))
            assert path.read_bytes() == data
            counter = [0]
            def write():
                counter[0] += 1
                next_value = f"'{counter[0]+1}'" if name == "manifest" else str(counter[0])
                command(["set", str(path), key, next_value], expected_commit=True)()
            sample(f"cli.{name}.file_write", write)
            kind = "pyproject" if name == "manifest" else "script"
            op = {"op": "set", "path": key.split("."), "value_toml": value}
            sample(f"python.{name}.prepare_owned_boundary", lambda: tomlgraft.patch(data, kind=kind, operations=[op]))
        # The CLI includes process start, I/O and JSON; binding timings include owned Python conversion.
        result["filesystem"] = "local fixed NTFS" if platform.system() == "Windows" else "local ext4 (verify temporary path on other hosts)"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("\n".join(f"{r['name']}: median {r['median_ms']:.3f} ms, p95 {r['p95_ms']:.3f} ms" for r in result["rows"]))


if __name__ == "__main__":
    main()
