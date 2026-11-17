"""Build atcgen once and check the official input constraints of the corpus."""

from collections import Counter
from collections.abc import Callable
import json
from pathlib import Path
import re
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "corpus" / "atcoder"
SEEDS = (0, 1, 42, 100, 7, 19, 23, 49, 127, 255, 1024, 65535, 2**63, 2**64 - 1)
STATUSES = ("supported", "format-only", "unsupported")
COVERAGES = ("representative", "restricted", "none")
INTEGER = re.compile(rb"-?[0-9]+")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


class Input:
    def __init__(self, output: bytes):
        self.lines = output.splitlines()
        self.index = 0

    def row(self, width: int | None = None) -> list[int]:
        line = self.index + 1
        if self.index == len(self.lines):
            raise ValueError(f"line {line}: missing row")
        tokens = self.lines[self.index].split()
        self.index += 1
        if width is not None and len(tokens) != width:
            raise ValueError(f"line {line}: expected {width} integers, got {len(tokens)}")
        if not tokens or any(INTEGER.fullmatch(token) is None for token in tokens):
            raise ValueError(f"line {line}: expected a row of integers")
        return [int(token) for token in tokens]

    def finish(self) -> None:
        require(self.index == len(self.lines), f"line {self.index + 1}: unexpected extra row")


def in_range(label: str, value: int, lower: int, upper: int) -> None:
    require(lower <= value <= upper, f"{label}={value}: expected {lower}..={upper}")


def check_abc300_a(case: Input) -> None:
    n, a, b = case.row(3)
    in_range("N", n, 1, 300)
    in_range("A", a, 1, 1000)
    in_range("B", b, 1, 1000)
    choices = case.row(n)
    for i, value in enumerate(choices, 1):
        in_range(f"C[{i}]", value, 1, 2000)
    require(len(set(choices)) == n, "the choices must be pairwise distinct")
    require(choices.count(a + b) == 1, "A+B must appear exactly once in the choices")


def check_abc320_a(case: Input) -> None:
    a, b = case.row(2)
    in_range("A", a, 2, 9)
    in_range("B", b, a, 9)


def check_abc355_b(case: Input) -> None:
    n, m = case.row(2)
    in_range("N", n, 1, 100)
    in_range("M", m, 1, 100)
    a = case.row(n)
    b = case.row(m)
    for name, values in (("A", a), ("B", b)):
        for i, value in enumerate(values, 1):
            in_range(f"{name}[{i}]", value, 1, 200)
    require(len(set(a + b)) == n + m, "the combined elements of A and B must be distinct")


def check_abc355_d(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 2, 500_000)
    for i in range(1, n + 1):
        left, right = case.row(2)
        in_range(f"l[{i}]", left, 0, 10**9 - 1)
        in_range(f"r[{i}]", right, left + 1, 10**9)


def check_abc370_b(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 1, 100)
    for i in range(1, n + 1):
        for j, value in enumerate(case.row(i), 1):
            in_range(f"A[{i},{j}]", value, 1, n)


def check_edge(label: str, u: int, v: int, n: int) -> None:
    in_range(f"{label} u", u, 1, n - 1)
    in_range(f"{label} v", v, u + 1, n)


def check_abc385_e(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 3, 300_000)
    parent = list(range(n + 1))
    size = [1] * (n + 1)

    def root(vertex: int) -> int:
        while parent[vertex] != vertex:
            parent[vertex] = parent[parent[vertex]]
            vertex = parent[vertex]
        return vertex

    for i in range(1, n):
        u, v = case.row(2)
        check_edge(f"edge {i}", u, v, n)
        left, right = root(u), root(v)
        require(left != right, f"edge {i} ({u}, {v}) creates a cycle")
        if size[left] < size[right]:
            left, right = right, left
        parent[right] = left
        size[left] += size[right]
    # Each of the N-1 edges joined two different components.
    require(size[root(1)] == n, "the tree must be connected")


def check_abc400_b(case: Input) -> None:
    n, m = case.row(2)
    in_range("N", n, 1, 10**9)
    in_range("M", m, 1, 100)


def check_abc420_e(case: Input) -> None:
    n, q = case.row(2)
    in_range("N", n, 1, 200_000)
    in_range("Q", q, 1, 600_000)
    edges: set[tuple[int, int]] = set()
    for i in range(1, q + 1):
        query = case.row()
        kind = query[0]
        in_range(f"query {i} type", kind, 1, 3)
        width = 3 if kind == 1 else 2
        require(len(query) == width, f"query {i} type {kind}: expected {width} integers")
        if kind == 1:
            _, u, v = query
            check_edge(f"query {i}", u, v, n)
            require((u, v) not in edges, f"query {i}: edge ({u}, {v}) was already added")
            edges.add((u, v))
        else:
            in_range(f"query {i} v", query[1], 1, n)


def check_abc482_g(case: Input) -> None:
    (t,) = case.row(1)
    in_range("T", t, 1, 10)
    total_edges = 0
    total_cubes = 0
    for i in range(1, t + 1):
        n, m = case.row(2)
        in_range(f"case {i} N", n, 2, 500)
        in_range(f"case {i} M", m, 1, n * (n - 1) // 2)
        limits = case.row(n)
        for j, limit in enumerate(limits, 1):
            in_range(f"case {i} A[{j}]", limit, 1, n - 1)
        limit_sum = sum(limits)
        in_range(f"case {i} sum(A)", limit_sum, n, 500)
        edges: set[tuple[int, int]] = set()
        for j in range(1, m + 1):
            u, v = case.row(2)
            check_edge(f"case {i} edge {j}", u, v, n)
            require((u, v) not in edges, f"case {i} edge {j}: duplicate edge ({u}, {v})")
            edges.add((u, v))
        total_edges += m
        total_cubes += limit_sum**3
    in_range("sum of M across cases", total_edges, t, 200_000)
    in_range("sum of sum(A)^3 across cases", total_cubes, 0, 500**3)


CHECKERS: dict[tuple[str, str], Callable[[Input], None]] = {
    ("abc300", "a"): check_abc300_a,
    ("abc320", "a"): check_abc320_a,
    ("abc355", "b"): check_abc355_b,
    ("abc355", "d"): check_abc355_d,
    ("abc370", "b"): check_abc370_b,
    ("abc385", "e"): check_abc385_e,
    ("abc400", "b"): check_abc400_b,
    ("abc420", "e"): check_abc420_e,
    ("abc482", "g"): check_abc482_g,
}


def read_metadata(path: Path) -> tuple[str, str]:
    with path.open("rb") as file:
        data = tomllib.load(file)

    for key in ("contest", "problem", "url", "status", "coverage", "description"):
        value = data.get(key)
        require(isinstance(value, str) and bool(value.strip()), f"{key} must be a nonempty string")
    for key in ("constraints", "limitations", "missing_features"):
        values = data.get(key)
        require(
            isinstance(values, list)
            and all(isinstance(value, str) and value.strip() for value in values),
            f"{key} must be a list of nonempty strings",
        )

    contest, problem = data["contest"], data["problem"]
    require(re.fullmatch(r"[a-z0-9_-]+", contest) is not None, f"invalid contest: {contest!r}")
    require(re.fullmatch(r"[a-z0-9]+", problem) is not None, f"invalid problem: {problem!r}")
    require(path.parent == CORPUS / contest / problem, "contest/problem must match the directory")
    url = f"https://atcoder.jp/contests/{contest}/tasks/{contest}_{problem}"
    require(data["url"] == url, f"url must be {url}")
    require(bool(data["constraints"]), "constraints must summarize the official constraints")

    status, coverage = data["status"], data["coverage"]
    require(status in STATUSES, f"unknown status: {status!r}")
    require(coverage in COVERAGES, f"unknown coverage: {coverage!r}")
    if status == "supported":
        require(coverage != "none", "supported requires representative or restricted coverage")
    else:
        require(coverage == "none", f"{status} requires coverage none")
        require(bool(data["missing_features"]), f"{status} requires missing_features")
    if coverage != "representative":
        require(bool(data["limitations"]), f"{coverage} coverage requires limitations")
    return status, coverage


def build_binary() -> Path:
    result = subprocess.run(
        ["cargo", "build", "--quiet", "--bin", "atcgen", "--message-format=json-render-diagnostics"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        encoding="utf-8",
        errors="replace",
        check=True,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message["reason"] == "compiler-artifact" and message.get("executable"):
            return ROOT / message["executable"]
    raise ValueError("cargo did not report the atcgen executable")


def generate(binary: Path, generator: Path, seed: int) -> bytes:
    result = subprocess.run(
        [str(binary), "generate", str(generator), "--seed", str(seed)],
        cwd=ROOT,
        capture_output=True,
        timeout=30,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(f"atcgen exited with code {result.returncode}: {detail or '(no diagnostic)'}")
    return result.stdout


def verify_problem(binary: Path, path: Path) -> int:
    generator = path.with_name("generator.gen")
    require(generator.is_file(), "missing generator.gen")
    checker = CHECKERS.get((path.parent.parent.name, path.parent.name))
    if checker is None:
        raise ValueError("missing constraint checker")

    outputs = set()
    for seed in SEEDS:
        try:
            first = generate(binary, generator, seed)
            second = generate(binary, generator, seed)
            require(first == second, "same seed produced different bytes")
            case = Input(first)
            checker(case)
            case.finish()
            outputs.add(first)
        except (ValueError, OSError, subprocess.TimeoutExpired) as error:
            raise ValueError(f"seed {seed}: {error}") from error
    return len(outputs)


def main() -> int:
    paths = sorted(CORPUS.rglob("problem.toml"))
    if not paths:
        print(f"FAIL: no problem.toml files under {CORPUS}")
        return 1

    entries = {}
    failures = {}
    statuses = Counter()
    for path in paths:
        try:
            status, coverage = read_metadata(path)
        except (ValueError, OSError) as error:
            failures[path] = f"metadata: {error}"
        else:
            entries[path] = status, coverage
            statuses[status] += 1

    binary = None
    if statuses["supported"]:
        try:
            binary = build_binary()
        except (ValueError, OSError, subprocess.CalledProcessError) as error:
            print(f"FAIL: building atcgen: {error}", flush=True)

    verified = 0
    for path in paths:
        label = path.parent.relative_to(CORPUS).as_posix()
        if path in failures:
            print(f"FAIL {label}: {failures[path]}")
            continue

        status, coverage = entries[path]
        classification = f"{status}/{coverage}"
        if status != "supported":
            print(f"SKIP {label}: {classification}")
            continue

        try:
            if binary is None:
                raise ValueError("atcgen build failed")
            distinct = verify_problem(binary, path)
        except ValueError as error:
            failures[path] = str(error)
            print(f"FAIL {label}: {classification}: {error}")
        else:
            verified += 1
            print(f"PASS {label}: {classification}, {len(SEEDS)} seeds, {distinct} distinct outputs")

    for generator in sorted(CORPUS.rglob("generator.gen")):
        if not generator.with_name("problem.toml").is_file():
            failures[generator] = "missing problem.toml"
            print(f"FAIL {generator.relative_to(CORPUS).as_posix()}: {failures[generator]}")
    counts = ", ".join(f"{status}={statuses[status]}" for status in STATUSES)
    print(f"Corpus: {len(paths)} registered, {counts}, verified={verified}, failures={len(failures)}")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
