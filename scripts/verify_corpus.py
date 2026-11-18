"""Build atcgen once and check the official input constraints of the corpus."""

from collections import Counter
from collections.abc import Callable
import argparse
from dataclasses import dataclass
from itertools import product
import hashlib
import json
import math
from pathlib import Path
import platform
import re
import statistics
import subprocess
import tempfile
from time import perf_counter
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "corpus" / "atcoder"
SEEDS = (0, 1, 42, 100, 7, 19, 23, 49, 127, 255, 1024, 65535, 2**63, 2**64 - 1)
# Additional seeds and their expected first-row integer (N or T).
BOUNDARY_SEEDS = {("abc370", "b"): {29: 1, 217: 100}, ("abc482", "g"): {2: 1, 17: 10}}
ABC400D_DIMENSIONS = ((1, 2), (2, 1), (1, 1000), (1000, 1), (1000, 1000))
ABC355B_DIMENSIONS = ((1, 1), (1, 100), (100, 1), (100, 100))
ABC385E_SIZES = (3, 64, 300_000)
ABC300A_PROFILES = (
    (1, 1, 1, 1),
    (1, 1000, 1000, 1),
    (300, 1, 1, 1),
    (300, 1, 1, 150),
    (300, 1, 1, 300),
    (300, 1000, 1000, 1),
    (300, 1000, 1000, 150),
    (300, 1000, 1000, 300),
)
STATUSES = ("supported", "format-only", "unsupported")
COVERAGES = ("representative", "restricted", "none")
INTEGER = re.compile(rb"-?[0-9]+")
TASK_ID_EXCEPTIONS = {("abc095", "c"): "arc096_a"}
BATCH1 = (
    ("abc095", "c"), ("abc192", "d"), ("abc150", "c"), ("abc302", "f"),
    ("abc390", "c"), ("abc146", "d"), ("abc303", "e"), ("abc241", "d"),
    ("abc216", "d"), ("abc339", "f"),
)
BATCH1_RESULTS: dict[str, dict] = {}


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
        if (not tokens and width != 0) or any(INTEGER.fullmatch(token) is None for token in tokens):
            raise ValueError(f"line {line}: expected a row of integers")
        return [int(token) for token in tokens]

    def token(self) -> bytes:
        line = self.index + 1
        require(self.index < len(self.lines), f"line {line}: missing row")
        tokens = self.lines[self.index].split()
        self.index += 1
        require(len(tokens) == 1, f"line {line}: expected one token")
        return tokens[0]

    def finish(self) -> None:
        require(self.index == len(self.lines), f"line {self.index + 1}: unexpected extra row")

    def string_row(self, width: int) -> bytes:
        line = self.index + 1
        require(self.index < len(self.lines), f"line {line}: missing string row")
        value = self.lines[self.index]
        self.index += 1
        require(len(value) == width, f"line {line}: expected string length {width}, got {len(value)}")
        require(all(byte in b".#" for byte in value), f"line {line}: expected only '.' and '#'")
        return value


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


def check_abc400_d(case: Input) -> None:
    h, w = case.row(2)
    in_range("H", h, 1, 1000)
    in_range("W", w, 1, 1000)
    grid = [case.string_row(w) for _ in range(h)]
    a, b, c, d = case.row(4)
    for label, value, limit in (("A", a, h), ("B", b, w), ("C", c, h), ("D", d, w)):
        in_range(label, value, 1, limit)
    require((a, b) != (c, d), "start and goal must be different cells")
    require(grid[a - 1][b - 1] == ord("."), "start cell must be a road")
    require(grid[c - 1][d - 1] == ord("."), "goal cell must be a road")


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


def check_abc095_c(case: Input) -> None:
    a, b, c, x, y = case.row(5)
    for name, value in (("A", a), ("B", b), ("C", c)):
        in_range(name, value, 1, 5000)
    for name, value in (("X", x), ("Y", y)):
        in_range(name, value, 1, 100_000)


def decimal_token(case: Input, max_digits: int) -> bytes:
    value = case.token()
    in_range("decimal length", len(value), 1, max_digits)
    require(re.fullmatch(rb"[1-9][0-9]*", value) is not None, "expected positive decimal without leading zero")
    return value


def check_abc192_d(case: Input) -> None:
    decimal_token(case, 60)
    (m,) = case.row(1)
    in_range("M", m, 1, 10**18)


def check_abc150_c(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 2, 8)
    for name in ("P", "Q"):
        values = case.row(n)
        require(sorted(values) == list(range(1, n + 1)), f"{name} must be a permutation of 1..N")


def check_abc302_f(case: Input) -> None:
    n, m = case.row(2)
    in_range("N", n, 1, 200_000)
    in_range("M", m, 2, 200_000)
    total = 0
    for i in range(1, n + 1):
        (length,) = case.row(1)
        # No positive lower bound for an individual set is stated officially.
        in_range(f"A[{i}]", length, 0, m)
        values = case.row(length)
        for value in values:
            in_range(f"set {i} value", value, 1, m)
        require(len(set(values)) == length, f"set {i}: elements must be distinct")
        total += length
    in_range("sum(A)", total, 1, 500_000)


def check_abc390_c(case: Input) -> None:
    h, w = case.row(2)
    in_range("H", h, 1, 1000)
    in_range("W", w, 1, 1000)
    black = False
    for i in range(1, h + 1):
        row = case.token()
        require(len(row) == w, f"row {i}: expected width {w}")
        require(all(value in b"#.?" for value in row), f"row {i}: expected only '#', '.', '?'")
        black |= b"#" in row
    require(black, "at least one '#' is required")


def read_tree(case: Input, n: int, ordered: bool) -> list[list[int]]:
    neighbors: list[list[int]] = [[] for _ in range(n + 1)]
    for i in range(1, n):
        u, v = case.row(2)
        in_range(f"edge {i} u", u, 1, n)
        in_range(f"edge {i} v", v, 1, n)
        require(u != v, f"edge {i}: self loop")
        if ordered:
            require(u < v, f"edge {i}: expected u < v")
        neighbors[u].append(v)
        neighbors[v].append(u)
    seen = bytearray(n + 1)
    seen[1] = 1
    stack = [(1, 0)]
    visited = 0
    while stack:
        vertex, parent = stack.pop()
        visited += 1
        for child in neighbors[vertex]:
            if child == parent:
                continue
            require(not seen[child], "tree contains a cycle or duplicate edge")
            seen[child] = 1
            stack.append((child, vertex))
    require(visited == n, "tree must be connected")
    return neighbors


def check_abc146_d(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 2, 100_000)
    read_tree(case, n, ordered=True)


def check_abc303_e(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 3, 200_000)
    neighbors = read_tree(case, n, ordered=False)
    # Any final leaf must be an unused original leaf, forcing its neighbor to
    # be a star center. Centers are then exactly the vertices at depth 0 mod 3.
    leaf = next(v for v in range(1, n + 1) if len(neighbors[v]) == 1)
    root = neighbors[leaf][0]
    depth = [-1] * (n + 1)
    depth[root] = 0
    stack = [root]
    while stack:
        vertex = stack.pop()
        for child in neighbors[vertex]:
            if depth[child] == -1:
                depth[child] = depth[vertex] + 1
                stack.append(child)
    for vertex in range(1, n + 1):
        if depth[vertex] % 3 == 0:
            require(len(neighbors[vertex]) >= 2, "star center must have level >= 2")
        else:
            require(len(neighbors[vertex]) <= 2, "original star leaf was used more than once")
            centers = sum(depth[other] % 3 == 0 for other in neighbors[vertex])
            require(centers == 1, "each original leaf must have exactly one adjacent star center")
    # These disjoint stars cover every vertex. Remaining leaf-leaf edges form
    # a matching, and contracting the stars in a tree gives a tree, so those
    # edges can be added between different components in the official order.


def check_abc241_d(case: Input) -> None:
    (q,) = case.row(1)
    in_range("Q", q, 1, 200_000)
    for i in range(1, q + 1):
        query = case.row()
        kind = query[0]
        in_range(f"query {i} kind", kind, 1, 3)
        require(len(query) == (2 if kind == 1 else 3), f"query {i}: incorrect row width")
        in_range(f"query {i} x", query[1], 1, 10**18)
        if kind != 1:
            in_range(f"query {i} k", query[2], 1, 5)


def check_abc216_d(case: Input) -> None:
    n, m = case.row(2)
    in_range("N", n, 1, 200_000)
    in_range("M", m, 2, 200_000)
    counts = [0] * (n + 1)
    total = 0
    for i in range(1, m + 1):
        (length,) = case.row(1)
        in_range(f"k[{i}]", length, 1, 2 * n)
        total += length
        require(total <= 2 * n, "sum(k) exceeds 2N")
        for value in case.row(length):
            in_range(f"tube {i} color", value, 1, n)
            counts[value] += 1
    require(total == 2 * n, "sum(k) must equal 2N")
    require(all(count == 2 for count in counts[1:]), "each color must occur exactly twice")


def check_abc339_f(case: Input) -> None:
    (n,) = case.row(1)
    in_range("N", n, 1, 1000)
    for _ in range(n):
        decimal_token(case, 1000)


CHECKERS: dict[tuple[str, str], Callable[[Input], None]] = {
    ("abc095", "c"): check_abc095_c,
    ("abc192", "d"): check_abc192_d,
    ("abc150", "c"): check_abc150_c,
    ("abc302", "f"): check_abc302_f,
    ("abc390", "c"): check_abc390_c,
    ("abc146", "d"): check_abc146_d,
    ("abc303", "e"): check_abc303_e,
    ("abc241", "d"): check_abc241_d,
    ("abc216", "d"): check_abc216_d,
    ("abc339", "f"): check_abc339_f,
    ("abc300", "a"): check_abc300_a,
    ("abc320", "a"): check_abc320_a,
    ("abc355", "b"): check_abc355_b,
    ("abc355", "d"): check_abc355_d,
    ("abc370", "b"): check_abc370_b,
    ("abc385", "e"): check_abc385_e,
    ("abc400", "b"): check_abc400_b,
    ("abc400", "d"): check_abc400_d,
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
    default_task = f"{contest}_{problem}"
    expected_task = TASK_ID_EXCEPTIONS.get((contest, problem), default_task)
    task = data.get("task_id", default_task)
    require(task == expected_task, f"task_id must be {expected_task}")
    url = f"https://atcoder.jp/contests/{contest}/tasks/{expected_task}"
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
        require(not result.stdout, "generation failed after writing partial stdout")
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(f"atcgen exited with code {result.returncode}: {detail or '(no diagnostic)'}")
    return result.stdout


def _fix_declarations(source: str, replacements: dict[str, str]) -> str:
    for original, fixed in replacements.items():
        require(source.count(original) == 1, f"missing or repeated declaration: {original}")
        source = source.replace(original, fixed)
    return source


def abc355_b_boundary_source(source: str, n: int, m: int) -> str:
    """Fix lengths while retaining the actual corpus generator's U1 operations."""
    in_range("fixed N", n, 1, 100)
    in_range("fixed M", m, 1, 100)
    return _fix_declarations(source, {
        "N in 1..=100": f"N in {n}..={n}",
        "M in 1..=100": f"M in {m}..={m}",
    })


def abc300_a_boundary_source(
    source: str, n: int, a: int, b: int, answer_index: int | None = None,
) -> str:
    """Keep Distractors/C generation intact; optionally leave the answer random."""
    in_range("fixed N", n, 1, 300)
    in_range("fixed A", a, 1, 1000)
    in_range("fixed B", b, 1, 1000)
    replacements = {
        "N in 1..=300": f"N in {n}..={n}",
        "A in 1..=1000": f"A in {a}..={a}",
        "B in 1..=1000": f"B in {b}..={b}",
    }
    if answer_index is not None:
        in_range("fixed AnswerIndex", answer_index, 1, n)
        replacements["AnswerIndex in 1..=N"] = f"AnswerIndex in {answer_index}..={answer_index}"
    return _fix_declarations(source, replacements)


def _checked_generation(
    binary: Path, generator: Path, seed: int, checker: Callable[[Input], None], context: str,
) -> bytes:
    try:
        output = generate(binary, generator, seed)
        require(output == generate(binary, generator, seed), "same seed produced different bytes")
        case = Input(output)
        checker(case)
        case.finish()
        return output
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"{context}, seed {seed}: {error}") from error


def verify_abc355_b_boundaries(binary: Path, generator: Path) -> set[bytes]:
    outputs = set()
    nonlegacy_order = False
    source = generator.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="atcgen-abc355-b-") as directory:
        fixed = Path(directory) / "boundary.gen"
        for n, m in ABC355B_DIMENSIONS:
            fixed.write_text(abc355_b_boundary_source(source, n, m), encoding="utf-8")
            for seed in (0, 42):
                context = f"N={n}, M={m}"
                output = _checked_generation(binary, fixed, seed, check_abc355_b, context)
                require(output.splitlines()[0] == f"{n} {m}".encode(), f"{context}: incorrect header")
                values = [int(value) for row in output.splitlines()[1:] for value in row.split()]
                if n + m == 200:
                    require(set(values) == set(range(1, 201)), f"{context}: missing domain values")
                nonlegacy_order |= any(
                    (right - left) % 200 != 37 for left, right in zip(values, values[1:])
                )
                outputs.add(output)
    require(nonlegacy_order, "ABC355-B outputs still follow the old step-37 sequence")
    return outputs


def verify_abc300_a_boundaries(binary: Path, generator: Path) -> set[bytes]:
    outputs = set()
    source = generator.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="atcgen-abc300-a-") as directory:
        fixed = Path(directory) / "boundary.gen"
        for n, a, b, answer in ABC300A_PROFILES:
            fixed.write_text(abc300_a_boundary_source(source, n, a, b, answer), encoding="utf-8")
            for seed in (0, 42):
                context = f"N={n}, A={a}, B={b}, AnswerIndex={answer}"
                output = _checked_generation(binary, fixed, seed, check_abc300_a, context)
                lines = output.splitlines()
                require(lines[0] == f"{n} {a} {b}".encode(), f"{context}: incorrect header")
                choices = list(map(int, lines[1].split()))
                require(choices[answer - 1] == a + b, f"{context}: incorrect answer position")
                outputs.add(output)
    return outputs


def verify_abc300_a_diversity(binary: Path, generator: Path) -> set[bytes]:
    """Observe structural diversity at fixed N/A/B, without a uniformity claim."""
    outputs = set()
    distractors_by_answer: dict[int, set[frozenset[int]]] = {}
    nonlegacy_order = False
    source = abc300_a_boundary_source(generator.read_text(encoding="utf-8"), 9, 7, 11)
    with tempfile.TemporaryDirectory(prefix="atcgen-abc300-a-diversity-") as directory:
        fixed = Path(directory) / "fixed-header.gen"
        fixed.write_text(source, encoding="utf-8")
        for seed in SEEDS:
            output = _checked_generation(binary, fixed, seed, check_abc300_a, "fixed N=9,A=7,B=11")
            require(output.splitlines()[0] == b"9 7 11", "incorrect diversity header")
            choices = list(map(int, output.splitlines()[1].split()))
            answer = choices.index(18)
            distractors = frozenset(value for value in choices if value != 18)
            distractors_by_answer.setdefault(answer, set()).add(distractors)
            legacy = [(18 - 1 + i - answer) % 2000 + 1 for i in range(9)]
            nonlegacy_order |= choices != legacy
            outputs.add(output)
    require(len(distractors_by_answer) > 1, "ABC300-A answer position did not vary")
    require(
        any(len(sets) > 1 for sets in distractors_by_answer.values()),
        "ABC300-A distractor sets did not vary at the same answer position",
    )
    require(nonlegacy_order, "ABC300-A outputs still follow the old cyclic sequence")
    return outputs


def abc385_e_boundary_source(source: str, n: int) -> str:
    """Keep both permutations, the parent construction, and output unchanged."""
    in_range("fixed N", n, 3, 300_000)
    return _fix_declarations(source, {"N in 3..=128": f"N in {n}..={n}"})


def abc385_e_resources(n: int) -> dict[str, int]:
    """Exact counts for the corpus construction; bytes are a worst-case bound."""
    in_range("resource N", n, 3, 300_000)
    digits = len(str(n))
    return {
        "array_elements": 3 * n - 2,
        "generated_integers": 3 * n - 1,
        "output_tokens": 2 * n - 1,
        "output_bytes_upper_bound": digits + 1 + (n - 1) * (2 * digits + 2),
        "generate_work": 6 * n + 3,
        "input_work": 3 * n - 1,
        "unique_width": n,
        "unique_payload_bytes": 8 * n,
    }


def verify_abc385_e_boundaries(binary: Path, generator: Path) -> set[bytes]:
    outputs = set()
    source = generator.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="atcgen-abc385-e-") as directory:
        fixed = Path(directory) / "boundary.gen"
        for n in ABC385E_SIZES:
            resources = abc385_e_resources(n)
            for name in ("array_elements", "generated_integers", "output_tokens", "unique_width"):
                require(resources[name] <= 1_000_000, f"N={n}: {name} exceeds limit")
            for name in ("generate_work", "input_work"):
                require(resources[name] <= 10_000_000, f"N={n}: {name} exceeds limit")
            require(resources["unique_payload_bytes"] <= 16_000_000, f"N={n}: U1 workspace exceeds limit")
            require(resources["output_bytes_upper_bound"] <= 8 * 1024 * 1024, f"N={n}: byte bound exceeds limit")
            fixed.write_text(abc385_e_boundary_source(source, n), encoding="utf-8")
            for seed in (0, 42):
                context = f"N={n}, seed {seed}"
                output = _checked_generation(binary, fixed, seed, check_abc385_e, f"N={n}")
                lines = output.splitlines()
                require(lines[0] == str(n).encode(), f"{context}: incorrect header")
                require(len(lines) == n, f"{context}: incorrect row count")
                require(
                    sum(len(row.split()) for row in lines) == resources["output_tokens"],
                    f"{context}: incorrect output token count",
                )
                require(len(output) <= resources["output_bytes_upper_bound"], f"{context}: byte bound exceeded")
                outputs.add(output)
    return outputs


def abc400_d_boundary_source(source: str, h: int, w: int) -> str:
    """Fix dimensions while exercising the actual corpus generator's body."""
    old_h, old_w = "H in 1..=1000", "W in (1+1/H)..=1000"
    require(source.count(old_h) == source.count(old_w) == 1, "missing ABC400-D dimension declarations")
    return source.replace(old_h, f"H in {h}..={h}").replace(old_w, f"W in {w}..={w}")


def verify_abc400_d_boundaries(binary: Path, generator: Path) -> set[bytes]:
    outputs = set()
    source = generator.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="atcgen-abc400-d-") as directory:
        fixed = Path(directory) / "boundary.gen"
        for h, w in ABC400D_DIMENSIONS:
            fixed.write_text(abc400_d_boundary_source(source, h, w), encoding="utf-8")
            for seed in (0, 42):
                try:
                    output = generate(binary, fixed, seed)
                    require(output == generate(binary, fixed, seed), "same seed produced different bytes")
                    case = Input(output)
                    check_abc400_d(case)
                    case.finish()
                    require(output.splitlines()[0] == f"{h} {w}".encode(), "incorrect fixed dimensions")
                    require(len(output) <= 8 * 1024 * 1024, "output exceeds 8 MiB")
                    require(len(output.split()) == h + 6, "incorrect output token count")
                    outputs.add(output)
                except (ValueError, OSError, subprocess.TimeoutExpired) as error:
                    raise ValueError(f"dimensions {h}x{w}, seed {seed}: {error}") from error
    return outputs


@dataclass(frozen=True)
class Batch1Profile:
    name: str
    source: str
    rows: tuple[tuple[int, bytes], ...] = ()
    seeds: tuple[int, ...] = (0, 42)
    digits: tuple[int, int, int] | None = None
    total: int | None = None
    expected_error: str | None = None


def fixed(source: str, **values: int) -> str:
    """Pin existing scalar declarations; never silently miss a renamed field."""
    for name, value in values.items():
        pattern = rf"(?m)^(    {re.escape(name)} in )[^\n]+$"
        source, count = re.subn(pattern, lambda m: f"{m[1]}{value}..={value}", source)
        require(count == 1, f"expected one scalar declaration for {name}")
    return source


def batch1_profiles(problem: tuple[str, str], source: str) -> list[Batch1Profile]:
    profiles = []
    if problem == ("abc095", "c"):
        corners = list(product((1, 5000), (1, 5000), (1, 5000), (1, 100_000), (1, 100_000)))
        for values in [*corners, (100, 300, 200, 1, 100_000)]:
            text = fixed(source, **dict(zip(("A", "B", "C", "X", "Y"), values)))
            header = " ".join(map(str, values)).encode()
            profiles.append(Batch1Profile("prices-" + header.decode().replace(" ", "-"), text, ((0, header),)))
    elif problem == ("abc192", "d"):
        for length, m, first, rest in ((1, 1, "1", "0"), (1, 10**18, "9", "9"),
                                        (60, 1, "1", "0"), (60, 10**18, "9", "9")):
            text = fixed(source, Length=length, M=m)
            text = _fix_declarations(text, {'"123456789"': f'"{first}"', '"0123456789"': f'"{rest}"'})
            value = (first + rest * (length - 1)).encode()
            profiles.append(Batch1Profile(f"length-{length}-M-{m}", text, ((0, value), (1, str(m).encode()))))
    elif problem == ("abc150", "c"):
        for n, seeds in ((2, (0, 1, 7, 42)), (8, (0, 42))):
            profiles.append(Batch1Profile(f"N-{n}", fixed(source, N=n), ((0, str(n).encode()),), seeds))
    elif problem == ("abc302", "f"):
        for name, n, m, length, total in (
            ("minimum", 1, 2, "1", 1),
            ("usual-maximum", 128, 128, "128", 16384),
            ("variable-rows", 4, 7, "i", 10),
            ("empty-set", 2, 2, "i-1", 1),
            ("official-N-M", 200_000, 200_000, "1", 200_000),
            ("official-total", 125_000, 200_000, "4", 500_000),
        ):
            text = fixed(source, N=n, M=m)
            text = _fix_declarations(text, {"A[i] in 1..=M": f"A[i] in ({length})..=({length})"})
            profiles.append(Batch1Profile(name, text, ((0, f"{n} {m}".encode()),), total=total))
    elif problem == ("abc390", "c"):
        for h, w, anchor, alphabet in (
            (1, 1, 1, "#.?"), (1, 1000, 1000, "."), (1000, 1, 1, "?"),
            (1000, 1000, 1_000_000, "#.?"), (2, 3, 1, "#.?"), (2, 3, 6, "#"),
        ):
            text = fixed(source, H=h, W=w, Anchor=anchor)
            text = _fix_declarations(text, {'"#.?"': f'"{alphabet}"'})
            profiles.append(Batch1Profile(f"{h}x{w}-anchor-{anchor}", text, ((0, f"{h} {w}".encode()),)))
    elif problem == ("abc146", "d"):
        for n, shape in ((2, "random"), (100_000, "random"), (64, "path"), (64, "star")):
            text = fixed(source, N=n)
            if shape != "random":
                parent = "i" if shape == "path" else "1"
                text = _fix_declarations(text, {"Parent[i] in 1..=i": f"Parent[i] in {parent}..={parent}"})
            profiles.append(Batch1Profile(f"N-{n}-{shape}", text, ((0, str(n).encode()),)))
    elif problem == ("abc303", "e"):
        for stars, level, n in ((1, "2", 3), (1, "8", 9), (64, "8", 576),
                                (3, "t+1", 12), (1, "199999", 200_000), (64, "3124", 200_000)):
            text = fixed(source, Stars=stars)
            text = _fix_declarations(text, {"Level[t] in 2..=8": f"Level[t] in ({level})..=({level})"})
            profiles.append(Batch1Profile(f"stars-{stars}-level-{level}", text, ((0, str(n).encode()),)))
    elif problem == ("abc241", "d"):
        for name, q, kind, x, k in (
            ("singleton-add", 1, "1", "1", "1"),
            ("singleton-query2-empty", 1, "2", str(10**18), "5"),
            ("singleton-query3-empty", 1, "3", "1", "1"),
            ("interleaved", 6, "i%3+1", str(10**18), "5"),
            ("official-Q", 200_000, None, None, None),
            ("official-Q-all-query2", 200_000, "2", str(10**18), "5"),
        ):
            text = fixed(source, Q=q)
            replacements = {}
            for old, value, target in (("1..=3", kind, "Kind[i]"),
                                       ("1..=1000000000000000000", x, "X[i]"),
                                       ("1..=5", k, "K[i]")):
                if value is not None:
                    replacements[f"{target} in {old}"] = f"{target} in ({value})..=({value})"
            text = _fix_declarations(text, replacements)
            profiles.append(Batch1Profile(name, text, ((0, str(q).encode()),)))
    elif problem == ("abc216", "d"):
        for n, m in ((1, 2), (4096, 2), (4096, 256), (128, 256),
                     (200_000, 2), (100_000, 200_000), (200_000, 200_000)):
            text = fixed(source, N=n, M=m)
            error = "total array capacity limit exceeded" if n == m == 200_000 else None
            profiles.append(Batch1Profile(f"N-{n}-M-{m}", text, ((0, f"{n} {m}".encode()),), expected_error=error))
    elif problem == ("abc339", "f"):
        for n, digits, first, rest in ((1, 1, "1", "0"), (1, 1000, "1", "0"),
                                        (1000, 1, None, None), (1000, 1000, "9", "9")):
            text = fixed(source, N=n, Digits=digits)
            if first is not None:
                text = _fix_declarations(text, {'"123456789"': f'"{first}"', '"0123456789"': f'"{rest}"'})
            profiles.append(Batch1Profile(f"N-{n}-digits-{digits}", text, ((0, str(n).encode()),), digits=(1, n + 1, digits)))
    else:
        raise ValueError(f"unknown batch 1 problem: {problem}")
    return profiles


def batch1_resources(problem: tuple[str, str], output: bytes) -> dict[str, int]:
    """Bounds for these generators, separate from the official checkers."""
    lines = output.splitlines()
    integers = list(map(int, lines[0].split())) if problem != ("abc192", "d") else []
    arrays = chars = generated = width = 0
    if problem == ("abc095", "c"):
        tokens, byte_bound, generated, work, input_work = 5, 29, 5, 5, 1
    elif problem == ("abc192", "d"):
        chars = len(lines[0])
        tokens, byte_bound, generated, work, input_work = 2, chars + 21, 2, 2 * chars + 3, 2
    elif problem == ("abc150", "c"):
        n = integers[0]
        arrays, generated, width = 2 * n, 2 * n + 1, n
        tokens, byte_bound, work, input_work = 2 * n + 1, 4 * n + 2, 4 * n + 5, 3
    elif problem == ("abc302", "f"):
        n, m = integers
        lengths = [int(row) for row in lines[1::2]]
        total = sum(lengths)
        arrays, generated, width = n + m, n + m + 2, m
        tokens = n + total + 2
        # An empty set still emits its own LF, unlike a nonempty row where
        # the final LF replaces a token separator in the per-element bound.
        byte_bound = len(str(n)) + len(str(m)) + 2 + (n + total) * (len(str(m)) + 1) + lengths.count(0)
        work, input_work = 6 + 2 * (n + m), 2 + 4 * n + 2 * total
    elif problem == ("abc390", "c"):
        h, w = integers
        chars, generated = h * w, 3
        tokens, byte_bound = h + 2, len(str(h)) + len(str(w)) + 2 + h * (w + 1)
        work, input_work = 5 + 2 * h + 3 * h * w, 2 + 2 * h
    elif problem == ("abc146", "d"):
        n = integers[0]
        arrays, generated, width = 3 * n - 2, 3 * n - 1, n
        tokens, byte_bound = 2 * n - 1, len(str(n)) + 1 + (n - 1) * (2 * len(str(n)) + 2)
        work, input_work = 6 * n + 3, 3 * n - 1
    elif problem == ("abc303", "e"):
        n = integers[0]
        # All normal and fixed profiles have at most 64 stars. The checker
        # itself has no such restriction and does not use this bound.
        arrays, generated, width = 4 * n + 126, 4 * n + 128, n
        tokens, byte_bound = 2 * n - 1, len(str(n)) + 1 + (n - 1) * (2 * len(str(n)) + 2)
        work, input_work = 7 * n + 329, 2 * n
    elif problem == ("abc241", "d"):
        q = integers[0]
        arrays, generated = 3 * q, 3 * q + 1
        tokens = 1 + sum(2 if row.split()[0] == b"1" else 3 for row in lines[1:])
        byte_bound, work, input_work = len(str(q)) + 1 + 24 * q, 5 + 5 * q, 2 + 3 * q
    elif problem == ("abc216", "d"):
        n, m = integers
        arrays, generated, width = 3 * n + 2 * m + 2, 3 * n + 2 * m + 4, 2 * n
        tokens = 2 + m + 2 * n
        byte_bound = len(str(n)) + len(str(m)) + 2 + m * (len(str(2 * n)) + 1) + 2 * n * (len(str(n)) + 1)
        work, input_work = 13 + 6 * n + 4 * m, 2 + 4 * m + 4 * n
    elif problem == ("abc339", "f"):
        n, digits = integers[0], len(lines[1])
        require(all(len(row) == digits for row in lines[1:]), "generator lost its common-width structure")
        chars, generated = n * digits, 2
        tokens, byte_bound = n + 1, len(str(n)) + 1 + n * (digits + 1)
        work, input_work = 4 + n + 2 * n * digits, 2 + 2 * n
    else:
        raise ValueError(f"unknown batch 1 problem: {problem}")
    bounds = {
        "array_cells": arrays, "character_cells": chars, "generated_integers_upper": generated,
        "output_tokens": tokens, "output_bytes_upper": byte_bound, "unique_width": width,
        "unique_payload_bytes": 8 * width, "generate_work_upper": work, "input_work_upper": input_work,
    }
    for key in ("array_cells", "character_cells", "generated_integers_upper", "output_tokens", "unique_width"):
        require(bounds[key] <= 1_000_000, f"{key} exceeds DSL limit")
    for key in ("generate_work_upper", "input_work_upper"):
        require(bounds[key] <= 10_000_000, f"{key} exceeds DSL work limit")
    require(8 * width <= 16_000_000, "unique workspace exceeds limit")
    require(len(output.split()) == tokens, "output token count disagrees with resource bound")
    require(len(output) <= byte_bound <= 8 * 1024 * 1024, "output byte bound exceeds limit")
    return bounds


def verify_batch1_problem(binary: Path, path: Path) -> tuple[int, int]:
    problem = (path.parent.parent.name, path.parent.name)
    generator = path.with_name("generator.gen")
    source = generator.read_text(encoding="utf-8")
    records: list[dict] = []
    label = "/".join(problem)
    BATCH1_RESULTS[label] = {
        "conditions": records,
        "dsl_nonblank_lines": sum(bool(line.strip()) for line in source.splitlines()),
        "dsl_physical_lines": len(source.splitlines()),
        "dsl_bytes": len(source.encode()),
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
    }
    outputs = set()
    profiles = [Batch1Profile("normal", source, seeds=SEEDS), *batch1_profiles(problem, source)]
    BATCH1_RESULTS[label]["planned_conditions"] = sum(len(profile.seeds) for profile in profiles)
    with tempfile.TemporaryDirectory(prefix="atcgen-batch1-") as directory:
        probe = Path(directory) / "profile.gen"
        for profile in profiles:
            probe.write_text(profile.source, encoding="utf-8")
            for seed in profile.seeds:
                record = {"profile": profile.name, "seed": seed, "status": "failed",
                          "source_sha256": hashlib.sha256(profile.source.encode()).hexdigest()}
                records.append(record)
                try:
                    started = perf_counter()
                    if profile.expected_error is not None:
                        errors = []
                        for _ in range(2):
                            try:
                                generate(binary, probe, seed)
                            except ValueError as error:
                                require(profile.expected_error in str(error), f"unexpected limit diagnostic: {error}")
                                errors.append(str(error))
                            else:
                                raise ValueError("expected capacity limitation was not observed")
                        require(errors[0] == errors[1], "same seed produced different failures")
                        record.update(status="limited", reason=errors[0], probe_seconds=perf_counter() - started,
                                      empty_stdout=True, repeatable_failure=True)
                        continue
                    first = generate(binary, probe, seed)
                    record["generate_seconds"] = perf_counter() - started
                    started = perf_counter()
                    second = generate(binary, probe, seed)
                    record["replay_seconds"] = perf_counter() - started
                    require(first == second, "same seed produced different bytes")
                    record["reproducible"] = True
                    started = perf_counter()
                    case = Input(first)
                    CHECKERS[problem](case)
                    case.finish()
                    record["check_seconds"] = perf_counter() - started
                    lines = first.splitlines()
                    for index, expected in profile.rows:
                        require(lines[index] == expected, f"profile row {index + 1} mismatch")
                    if profile.digits is not None:
                        start, stop, digits = profile.digits
                        require(all(len(row) == digits for row in lines[start:stop]), "profile digit length mismatch")
                    if profile.total is not None:
                        require(sum(int(row) for row in lines[1::2]) == profile.total, "profile element total mismatch")
                    record["resources"] = batch1_resources(problem, first)
                    record.update(status="passed", output_bytes=len(first), output_tokens=len(first.split()),
                                  output_sha256=hashlib.sha256(first).hexdigest())
                    outputs.add(first)
                except (ValueError, OSError, subprocess.TimeoutExpired) as error:
                    record["reason"] = str(error)
                    raise ValueError(f"{profile.name}, seed {seed}: {error}") from error
    return len(SEEDS), len(outputs)


def verify_problem(binary: Path, path: Path) -> tuple[int, int]:
    generator = path.with_name("generator.gen")
    require(generator.is_file(), "missing generator.gen")
    problem = (path.parent.parent.name, path.parent.name)
    checker = CHECKERS.get(problem)
    if checker is None:
        raise ValueError("missing constraint checker")
    if problem in BATCH1:
        return verify_batch1_problem(binary, path)

    outputs = set()
    boundaries = BOUNDARY_SEEDS.get(problem, {})
    seeds = (*SEEDS, *boundaries)
    for seed in seeds:
        try:
            first = generate(binary, generator, seed)
            second = generate(binary, generator, seed)
            require(first == second, "same seed produced different bytes")
            case = Input(first)
            checker(case)
            case.finish()
            if seed in boundaries:
                count = int(first.split(b"\n", 1)[0])
                require(count == boundaries[seed], f"expected first row {boundaries[seed]}, got {count}")
            outputs.add(first)
        except (ValueError, OSError, subprocess.TimeoutExpired) as error:
            raise ValueError(f"seed {seed}: {error}") from error
    if problem == ("abc400", "d"):
        outputs.update(verify_abc400_d_boundaries(binary, generator))
    elif problem == ("abc355", "b"):
        outputs.update(verify_abc355_b_boundaries(binary, generator))
    elif problem == ("abc300", "a"):
        outputs.update(verify_abc300_a_boundaries(binary, generator))
        outputs.update(verify_abc300_a_diversity(binary, generator))
    elif problem == ("abc385", "e"):
        outputs.update(verify_abc385_e_boundaries(binary, generator))
    return len(seeds), len(outputs)


def timing_summary(records: list[dict], field: str) -> dict[str, float | int]:
    values = sorted(record[field] for record in records if record["status"] == "passed")
    if not values:
        return {"count": 0}
    return {"count": len(values), "median_seconds": statistics.median(values),
            "p95_seconds": values[math.ceil(0.95 * len(values)) - 1], "max_seconds": max(values)}


def batch1_summary(result: dict) -> dict:
    records = result["conditions"]
    groups = {}
    for name, selected in (("normal", [r for r in records if r["profile"] == "normal"]),
                           ("boundary", [r for r in records if r["profile"] != "normal"])):
        groups[name] = {
            "counts": dict(Counter(record["status"] for record in selected)),
            "generate": timing_summary(selected, "generate_seconds"),
            "replay": timing_summary(selected, "replay_seconds"),
            "check": timing_summary(selected, "check_seconds"),
        }
    groups["reproducible_conditions"] = sum(r.get("reproducible", False) for r in records)
    return groups


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="write batch 1 conditions, timings, and resource bounds as JSON")
    args = parser.parse_args(argv)
    BATCH1_RESULTS.clear()
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
            seed_count, distinct = verify_problem(binary, path)
        except ValueError as error:
            failures[path] = str(error)
            print(f"FAIL {label}: {classification}: {error}")
        else:
            verified += 1
            boundaries = {
                "abc400/d": ", 5 fixed dimensions x 2 seeds",
                "abc355/b": ", 4 fixed sizes x 2 seeds",
                "abc300/a": ", 8 fixed profiles x 2 seeds, 14 fixed-header diversity seeds",
                "abc385/e": ", 3 fixed sizes x 2 seeds (including N=300000)",
            }.get(label, "")
            if label in BATCH1_RESULTS:
                result = BATCH1_RESULTS[label]
                result.update(status=status, coverage=coverage)
                summary = batch1_summary(result)
                result["summary"] = summary
                counts = summary["boundary"]["counts"]
                normal = summary["normal"]
                boundaries = (f", fixed passed={counts.get('passed', 0)}, limited={counts.get('limited', 0)}"
                              f", normal median generation={normal['generate']['median_seconds'] * 1000:.3f}ms"
                              f", checker={normal['check']['median_seconds'] * 1000:.3f}ms")
            print(f"PASS {label}: {classification}, {seed_count} seeds{boundaries}, {distinct} distinct outputs")

    for generator in sorted(CORPUS.rglob("generator.gen")):
        if not generator.with_name("problem.toml").is_file():
            failures[generator] = "missing problem.toml"
            print(f"FAIL {generator.relative_to(CORPUS).as_posix()}: {failures[generator]}")
    counts = ", ".join(f"{status}={statuses[status]}" for status in STATUSES)
    print(f"Corpus: {len(paths)} registered, {counts}, verified={verified}, failures={len(failures)}")
    if args.report is not None:
        report = {"report_version": 1, "environment": {"python": platform.python_version(),
                  "platform": platform.platform(), "binary_profile": "debug", "timeout_seconds": 30},
                  "registered": len(paths), "verified": verified, "failures": len(failures),
                  "statuses": dict(statuses), "batch1": BATCH1_RESULTS}
        args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
