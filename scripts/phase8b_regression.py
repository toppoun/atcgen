"""Freeze/replay every Phase 7-D condition; raw bytes live outside Git.

capture runs the unchanged corpus verifier and observes its CLI calls. The
tracked manifest embeds the exact 270 sources, while outputs are gzip files in
--data-dir. rebuild recreates those files using the recorded Git revision,
without depending on today's profile builders or an old temporary directory.
"""

import argparse
from collections import Counter
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
from time import perf_counter

import verify_corpus as v


MANIFEST = v.ROOT / "corpus/baselines/phase8b-output-buffer.json"
HISTORICAL = v.ROOT / "corpus/reports/phase7d.json"
DATA = v.ROOT / "target/phase8b-baseline"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def diagnostic(stderr):
    """Remove only the source path, retaining line, column and all message bytes."""
    match = re.fullmatch(rb"[^\r\n]*?:(\d+):(\d+): ([\s\S]*)", stderr)
    v.require(match is not None, f"unexpected diagnostic: {stderr!r}")
    return {"line": int(match[1]), "column": int(match[2]),
            "message": match[3].decode("utf-8"),
            "without_source_path": b":".join(match.groups()).decode("utf-8")}


def run(binary, source, seed):
    return subprocess.run([str(binary), "generate", str(source), "--seed", str(seed)],
                          capture_output=True, timeout=30, cwd=v.ROOT)


def historical_conditions():
    report = json.loads(HISTORICAL.read_text())
    return [{"problem": problem, **condition}
            for group in ("initial", "batch1", "batch2", "batch3")
            for problem, result in report[group].items()
            for condition in result["conditions"]]


def capture(manifest_path, data_dir):
    v.require(not manifest_path.exists(), "refusing to replace an existing baseline manifest")
    expected = {(c["source_sha256"], c["seed"]): c for c in historical_conditions()}
    v.require(len(expected) == 1026, "expected all 1026 distinct Phase 7-D conditions")
    data_dir.mkdir(parents=True, exist_ok=True)
    binary = v.build_binary()
    sources, captured = {}, {}
    original = v.generate

    def observe(binary, generator, seed):
        source = generator.read_bytes()
        sha = digest(source)
        started = perf_counter()
        result = run(binary, generator, seed)
        elapsed = perf_counter() - started
        key = sha, seed
        if key in expected:
            old = expected[key]
            record = {k: old[k] for k in ("problem", "profile", "source_sha256", "seed", "status")}
            record.update(exit_code=result.returncode, stderr=result.stderr.decode("utf-8"),
                          stdout_bytes=len(result.stdout), stdout_sha256=digest(result.stdout))
            if old["status"] == "passed":
                v.require(result.returncode == 0 and not result.stderr, "baseline success failed")
                v.require(digest(result.stdout) == old["output_sha256"], "Phase 7-D output hash mismatch")
                record["output_sha256"] = digest(result.stdout)
            else:
                v.require(result.returncode == 1 and not result.stdout, "baseline limit changed")
                record["diagnostic"] = diagnostic(result.stderr)
                historical = old["reason"].split(": ", 1)[1].encode() + b"\n"
                v.require(record["diagnostic"] == diagnostic(historical), "Phase 7-D limit diagnostic changed")
            output_name = f"{sha}-{seed}.gz"
            record["output_file"] = output_name
            if key not in captured:
                (data_dir / output_name).write_bytes(gzip.compress(result.stdout, mtime=0))
                sources[sha] = source.decode("utf-8")
                captured[key] = record
            else:
                v.require(captured[key] == record, "baseline replay changed")
        if result.returncode:
            v.require(not result.stdout, "generation failed after writing partial stdout")
            raise ValueError(f"atcgen exited with code {result.returncode}: "
                             f"{result.stderr.decode('utf-8', errors='replace').strip()}")
        if v.ACTIVE_INITIAL is not None:
            v.record_initial_generation(generator, seed, result.stdout, elapsed)
        return result.stdout

    v.generate = observe
    try:
        v.require(v.main([]) == 0, "baseline corpus verification failed")
    finally:
        v.generate = original
    v.require(captured.keys() == expected.keys(), "baseline is missing historical conditions")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=v.ROOT).decode().strip()
    manifest = {"version": 1, "baseline_commit": revision, "binary_profile": "debug",
                "historical_report_sha256": digest(HISTORICAL.read_bytes()),
                "conditions": list(captured.values()), "sources": sources,
                "counts": dict(Counter(r["status"] for r in captured.values())),
                "unsupported": ["xmascon18/j"],
                "rust_source_sha256": {str(p.relative_to(v.ROOT)): digest(p.read_bytes())
                                       for p in sorted((v.ROOT / "src").glob("*.rs"))}}
    write_json(manifest_path, manifest)
    print(f"Frozen {len(captured)} conditions; {len(sources)} exact sources; {manifest['counts']}")


def compare(manifest_path, data_dir, binary):
    manifest = json.loads(manifest_path.read_text())
    v.require(digest(HISTORICAL.read_bytes()) == manifest["historical_report_sha256"],
              "historical report changed")
    records = []
    with tempfile.TemporaryDirectory(prefix="atcgen-phase8b-replay-") as directory:
        source = Path(directory) / "case.gen"
        for saved in manifest["conditions"]:
            raw_source = manifest["sources"][saved["source_sha256"]].encode()
            v.require(digest(raw_source) == saved["source_sha256"], "source hash mismatch")
            source.write_bytes(raw_source)
            expected = gzip.decompress((data_dir / saved["output_file"]).read_bytes())
            v.require(digest(expected) == saved["stdout_sha256"], "baseline bytes damaged")
            started = perf_counter()
            result = run(binary, source, saved["seed"])
            v.require(result.returncode == saved["exit_code"], f"exit code changed: {saved}")
            v.require(result.stdout == expected, f"raw stdout changed: {saved}")
            if saved["status"] == "passed":
                v.require(result.stderr == saved["stderr"].encode(), "success stderr changed")
                case = v.Input(result.stdout)
                v.CHECKERS[tuple(saved["problem"].split("/"))](case)
                case.finish()
            else:
                v.require(diagnostic(result.stderr) == saved["diagnostic"], "limit diagnostic changed")
            second = run(binary, source, saved["seed"])
            v.require((second.returncode, second.stdout, second.stderr) ==
                      (result.returncode, result.stdout, result.stderr), "same-seed replay changed")
            records.append({k: saved[k] for k in ("problem", "profile", "source_sha256", "seed", "status")})
            records[-1].update(raw_bytes_equal=True, stderr_equal_except_source_path=True,
                               same_seed_equal=True, exit_code=result.returncode,
                               stdout_bytes=len(result.stdout), seconds=perf_counter() - started)
    return {"baseline_commit": manifest["baseline_commit"],
            "manifest_sha256": digest(manifest_path.read_bytes()), "raw_data_directory": str(data_dir.resolve()),
            "successful_byte_matches": sum(r["status"] == "passed" for r in records),
            "known_limited_diagnostic_matches": sum(r["status"] == "limited" for r in records),
            "conditions": records, "unsupported": manifest["unsupported"]}


def rebuild(manifest_path, data_dir):
    manifest = json.loads(manifest_path.read_text())
    revision = manifest["baseline_commit"]
    for path, expected in manifest["rust_source_sha256"].items():
        raw = subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=v.ROOT)
        v.require(digest(raw) == expected, "recorded revision does not match baseline Rust sources")
    with tempfile.TemporaryDirectory(prefix="atcgen-phase8b-reference-") as directory:
        root = Path(directory)
        archive = subprocess.check_output(["git", "archive", revision], cwd=v.ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as archive_file:
            archive_file.extractall(root, filter="data")
        subprocess.run(["cargo", "build", "--release", "--offline", "--locked"], cwd=root, check=True)
        binary = root / "target/release" / ("atcgen.exe" if __import__("os").name == "nt" else "atcgen")
        data_dir.mkdir(parents=True, exist_ok=True)
        source = root / "replay.gen"
        for saved in manifest["conditions"]:
            source.write_text(manifest["sources"][saved["source_sha256"]], encoding="utf-8", newline="")
            result = run(binary, source, saved["seed"])
            v.require(result.returncode == saved["exit_code"] and digest(result.stdout) == saved["stdout_sha256"],
                      "Git reference differs from manifest")
            if saved["status"] == "limited":
                v.require(diagnostic(result.stderr) == saved["diagnostic"], "reference diagnostic differs")
            else:
                v.require(result.stderr == saved["stderr"].encode(), "reference stderr differs")
            (data_dir / saved["output_file"]).write_bytes(gzip.compress(result.stdout, mtime=0))
    print(f"Rebuilt all {len(manifest['conditions'])} baseline outputs from {revision}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("capture", "compare", "rebuild"))
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--data-dir", type=Path, default=DATA)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if args.action == "capture":
        capture(args.manifest, args.data_dir)
    elif args.action == "rebuild":
        rebuild(args.manifest, args.data_dir)
    else:
        v.require(args.report is not None, "compare requires a new --report path")
        historical = {p.resolve() for p in (v.ROOT / "corpus/reports").glob("phase7*.json")}
        v.require(args.report.resolve() not in historical, "refusing to overwrite a historical report")
        result = compare(args.manifest, args.data_dir, args.binary or v.build_binary())
        write_json(args.report, {"phase": "8-B1", "regression": result})
        print(f"Regression: {result['successful_byte_matches']}/1010 raw byte matches; "
              f"{result['known_limited_diagnostic_matches']}/16 limits unchanged")


if __name__ == "__main__":
    main()
