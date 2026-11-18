"""Replay a saved pre-change corpus manifest and compare actual raw bytes.

The snapshot has manifest.json, sources/<sha256>.gen and outputs/<record-key>.
Failed records save the exit-code diagnostic instead of an output file. Only
the temporary source filename is omitted from diagnostic comparisons.
"""

from collections import Counter
import argparse
import hashlib
import inspect
import json
from pathlib import Path
from time import perf_counter

import verify_corpus as v


def digest(data):
    return hashlib.sha256(data).hexdigest()


def diagnostic_without_filename(value):
    code, location = value.split(": ", 1)
    return code + ": " + location.split(".gen:", 1)[1]


def compare(directory, binary):
    manifest_path = directory / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    records = []
    started = perf_counter()
    for key, saved in manifest.items():
        source = directory / "sources" / (saved["source"] + ".gen")
        v.require(digest(source.read_bytes()) == saved["source"], "baseline source hash mismatch")
        record = {"key": key, "problem": saved["label"], "seed": saved["seed"],
                  "source_sha256": saved["source"], "baseline_status": saved["status"]}
        if saved["status"] == "passed":
            expected = (directory / "outputs" / key).read_bytes()
            v.require(digest(expected) == saved["sha256"], "baseline raw output hash mismatch")
            actual = v.generate(binary, source, saved["seed"])
            v.require(actual == expected, f"raw bytes differ for {key}")
            record.update(byte_equal=True, output_sha256=digest(actual), output_bytes=len(actual))
        else:
            try:
                v.generate(binary, source, saved["seed"])
            except ValueError as error:
                expected = diagnostic_without_filename(saved["diagnostic"])
                actual = diagnostic_without_filename(str(error))
                v.require(expected == actual, f"failure diagnostic differs for {key}")
                record.update(diagnostic_equal=True, empty_stdout=True,
                              diagnostic_without_source_filename=actual)
            else:
                raise ValueError(f"previously limited condition succeeded: {key}")
        records.append(record)
    protected = json.loads((directory / "protected.json").read_text())
    for path, saved_hash in protected.items():
        v.require(digest((v.ROOT / path).read_bytes()) == saved_hash, f"protected file changed: {path}")
    checkers = json.loads((directory / "checkers.json").read_text())
    for label, saved_hash in checkers.items():
        checker = v.CHECKERS[tuple(label.split("/"))]
        v.require(digest(inspect.getsource(checker).encode()) == saved_hash, f"legacy checker changed: {label}")
    # Historical predictions and measured sections are retained verbatim.
    import subprocess
    original_plan = subprocess.check_output(["git", "show", "HEAD:docs/CORPUS_EXPANSION_PLAN.md"], cwd=v.ROOT)
    v.require((v.ROOT / "docs/CORPUS_EXPANSION_PLAN.md").read_bytes().startswith(original_plan),
              "historical plan sections were modified")
    return {"snapshot_directory": str(directory.resolve()), "manifest_sha256": digest(manifest_path.read_bytes()),
            "sources_and_raw_outputs_saved": True, "source_seed_conditions": len(records),
            "successful_byte_matches": sum(r.get("byte_equal", False) for r in records),
            "known_limited_diagnostic_matches": sum(r.get("diagnostic_equal", False) for r in records),
            "protected_files_unchanged": len(protected), "legacy_checker_functions_unchanged": len(checkers),
            "by_problem": {label: dict(Counter(r["baseline_status"] for r in records if r["problem"] == label))
                           for label in sorted(checkers)},
            "seconds": perf_counter() - started, "conditions": records}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    for name in ("phase7b.json", "phase7c.json"):
        v.require(args.report.resolve() != (v.ROOT / "corpus/reports" / name).resolve(), "historical report is protected")
    result = compare(args.baseline, v.build_binary())
    report = json.loads(args.report.read_text())
    report["legacy_regression"] = result
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"Legacy: {result['successful_byte_matches']} byte matches; "
          f"{result['known_limited_diagnostic_matches']} matching known failures; "
          f"{result['protected_files_unchanged']} protected files unchanged")


if __name__ == "__main__":
    main()
