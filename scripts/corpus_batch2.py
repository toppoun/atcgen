"""Phase 7-C: independent official constraints and fixed generator profiles.

No import of verify_corpus: its Input/generate APIs are passed to the runner so
the existing twenty checkers and their generation paths remain unchanged.
"""

from collections import Counter
from dataclasses import dataclass
from functools import lru_cache
import hashlib
import math
from pathlib import Path
import re
import subprocess
import tempfile
from time import perf_counter


BATCH2 = (
    ("abc117", "d"), ("abc423", "c"), ("abc217", "f"), ("abc246", "f"),
    ("abc303", "c"), ("abc251", "f"), ("abc217", "e"), ("abc298", "c"),
    ("abc284", "d"), ("abc413", "e"),
)
RESULTS: dict[str, dict] = {}
MR_BASES = (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)
LARGE_PRIMES = (2_000_003, 2_000_029, 2_000_039, 2_000_081)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def in_range(name, value, lower, upper):
    require(lower <= value <= upper, f"{name}={value}: expected {lower}..={upper}")


def check_abc117_d(case):
    n, k = case.row(2)
    in_range("N", n, 1, 100_000)
    in_range("K", k, 0, 10**12)
    for value in case.row(n):
        in_range("A_i", value, 0, 10**12)


def check_abc423_c(case):
    n, r = case.row(2)
    in_range("N", n, 2, 200_000)
    in_range("R", r, 0, n)
    for value in case.row(n):
        in_range("L_i", value, 0, 1)


def check_abc217_f(case):
    n, m = case.row(2)
    in_range("N", n, 1, 200)
    in_range("M", m, 0, n * (2 * n - 1))
    seen = set()
    for _ in range(m):
        a, b = case.row(2)
        require(1 <= a < b <= 2 * n, "expected 1 <= A_i < B_i <= 2N")
        require((a, b) not in seen, "duplicate unordered pair")
        seen.add((a, b))


def check_abc246_f(case):
    n, length = case.row(2)
    in_range("N", n, 1, 18)
    in_range("L", length, 1, 10**9)
    for _ in range(n):
        value = case.token()
        in_range("string length", len(value), 1, 26)
        require(all(97 <= char <= 122 for char in value), "expected lowercase ASCII")
        require(all(a < b for a, b in zip(value, value[1:])), "letters must be strictly increasing")


def check_abc303_c(case):
    n, m, health, threshold = case.row(4)
    for name, value in (("N", n), ("M", m), ("H", health), ("K", threshold)):
        in_range(name, value, 1, 200_000)
    moves = case.token()
    require(len(moves) == n, "movement length must equal N")
    require(all(char in b"RLUD" for char in moves), "invalid movement character")
    points = set()
    for _ in range(m):
        x, y = case.row(2)
        in_range("x", x, -200_000, 200_000)
        in_range("y", y, -200_000, 200_000)
        require((x, y) not in points, "duplicate coordinate pair")
        points.add((x, y))


def check_abc251_f(case):
    n, m = case.row(2)
    in_range("N", n, 2, 200_000)
    in_range("M", m, n - 1, min(200_000, n * (n - 1) // 2))
    parent = list(range(n + 1))
    size = [1] * (n + 1)
    components = n

    def root(vertex):
        while parent[vertex] != vertex:
            parent[vertex] = parent[parent[vertex]]
            vertex = parent[vertex]
        return vertex

    seen = set()
    for _ in range(m):
        u, v = case.row(2)
        in_range("u", u, 1, n)
        in_range("v", v, 1, n)
        require(u != v, "self loop")
        edge = (min(u, v), max(u, v))
        require(edge not in seen, "duplicate undirected edge")
        seen.add(edge)
        a, b = root(u), root(v)
        if a != b:
            if size[a] < size[b]:
                a, b = b, a
            parent[b] = a
            size[a] += size[b]
            components -= 1
    require(components == 1, "graph must be connected")


def check_abc217_e(case):
    (q,) = case.row(1)
    in_range("Q", q, 1, 200_000)
    count = 0
    for _ in range(q):
        query = case.row()
        kind = query[0]
        in_range("kind", kind, 1, 3)
        require(len(query) == (2 if kind == 1 else 1), "incorrect query width")
        if kind == 1:
            in_range("x", query[1], 0, 10**9)
            count += 1
        elif kind == 2:
            require(count > 0, "cannot remove from empty sequence")
            count -= 1


def card_output_count(case):
    (n,) = case.row(1)
    (q,) = case.row(1)
    in_range("N", n, 1, 200_000)
    in_range("Q", q, 1, 200_000)
    boxes: dict[int, Counter] = {}
    membership: dict[int, set[int]] = {}
    total = 0
    for _ in range(q):
        query = case.row()
        kind = query[0]
        in_range("kind", kind, 1, 3)
        require(len(query) == (3 if kind == 1 else 2), "incorrect query width")
        value = query[1]
        if kind == 1:
            box = query[2]
            in_range("card", value, 1, 200_000)
            in_range("box", box, 1, n)
            boxes.setdefault(box, Counter())[value] += 1
            membership.setdefault(value, set()).add(box)
        elif kind == 2:
            in_range("box", value, 1, n)
            require(value in boxes, "query references empty box")
            total += sum(boxes[value].values())
        else:
            in_range("card", value, 1, 200_000)
            require(value in membership, "query references nonexistent card")
            total += len(membership[value])
        require(total <= 200_000, "answer integer total exceeds 200000")
    return total


def check_abc298_c(case):
    card_output_count(case)


def integer_cuberoot(n):
    require(n >= 0, "negative cube root input")
    lower, upper = 0, 1 << ((n.bit_length() + 2) // 3)
    while lower + 1 < upper:
        middle = (lower + upper) // 2
        if middle**3 <= n:
            lower = middle
        else:
            upper = middle
    return lower


def is_prime64(n):
    """Deterministic below 2^64 using the first twelve prime bases.

    Sorenson/Webster, Theorem 1.1: the first common strong pseudoprime is
    318665857834031151167461 > 2^64. https://arxiv.org/abs/1509.00864
    """
    if n < 2:
        return False
    require(n < 2**64, "primality bound is limited to unsigned 64-bit integers")
    for prime in MR_BASES:
        if n % prime == 0:
            return n == prime
    odd, power = n - 1, 0
    while odd % 2 == 0:
        odd //= 2
        power += 1
    for base in MR_BASES:
        value = pow(base, odd, n)
        if value in (1, n - 1):
            continue
        for _ in range(power - 1):
            value = value * value % n
            if value == n - 1:
                break
        else:
            return False
    return True


@lru_cache(maxsize=1)
def trial_primes():
    limit = integer_cuberoot(9 * 10**18)
    flags = bytearray(b"\x01") * (limit + 1)
    flags[:2] = b"\x00\x00"
    for prime in range(2, math.isqrt(limit) + 1):
        if flags[prime]:
            start = prime * prime
            flags[start::prime] = b"\x00" * ((limit - start) // prime + 1)
    return tuple(i for i in range(2, limit + 1) if flags[i])


def p2q_factors(n):
    if n < 12 or n > 9 * 10**18 or is_prime64(n):
        return None
    bound = integer_cuberoot(n)
    for divisor in trial_primes():
        if divisor > bound:
            break
        if n % divisor:
            continue
        if n % (divisor * divisor) == 0:
            p, q = divisor, n // (divisor * divisor)
        else:
            q = divisor
            p = math.isqrt(n // q)
            if p * p * q != n:
                return None
        if p != q and is_prime64(p) and is_prime64(q) and p * p * q == n:
            return p, q
        return None
    return None


def check_abc284_d(case):
    (t,) = case.row(1)
    in_range("T", t, 1, 10)
    for _ in range(t):
        (n,) = case.row(1)
        in_range("N", n, 1, 9 * 10**18)
        require(p2q_factors(n) is not None, "N must be p^2*q for distinct primes")


def check_abc413_e(case):
    (t,) = case.row(1)
    in_range("T", t, 1, 100_000)
    total = 0
    for _ in range(t):
        (n,) = case.row(1)
        in_range("N", n, 1, 18)
        length = 1 << n
        total += length
        require(total <= 300_000, "sum of permutation lengths exceeds 300000")
        values = case.row(length)
        require(all(1 <= v <= length for v in values), "permutation value out of range")
        require(len(set(values)) == length, "permutation contains duplicate or missing values")


CHECKERS = dict(zip(BATCH2, (check_abc117_d, check_abc423_c, check_abc217_f, check_abc246_f,
                           check_abc303_c, check_abc251_f, check_abc217_e, check_abc298_c,
                           check_abc284_d, check_abc413_e)))


@dataclass(frozen=True)
class Profile:
    name: str
    source: str
    rows: tuple[tuple[int, bytes], ...] = ()
    expected: tuple[tuple[str, int], ...] = ()
    expected_error: str | None = None
    seeds: tuple[int, ...] = (0, 42)


def replace_once(source, old, new):
    require(source.count(old) == 1, f"missing or repeated source fragment: {old}")
    return source.replace(old, new)


def fixed(source, **values):
    for name, value in values.items():
        pattern = rf"(?m)^(    {re.escape(name)} in )[^\n]+$"
        source, count = re.subn(pattern, lambda m: f"{m[1]}{value}..={value}", source)
        require(count == 1, f"expected one scalar declaration: {name}")
    return source


def exact_assignment(target, expression):
    return f"{target} in ({expression})..=({expression})"


def sequence_assignment(target, index, values):
    return "\n".join(f"if {index} == {i} {{\n    {exact_assignment(target, str(value))}\n}}"
                     for i, value in enumerate(values, 1))


def profiles(problem, source):
    result = []
    if problem == ("abc117", "d"):
        for n in (1, 100_000):
            for k in (0, 10**12):
                for value in (0, 10**12):
                    text = fixed(source, N=n, K=k)
                    text = replace_once(text, "A[i] in 0..=1000000000000", exact_assignment("A[i]", str(value)))
                    result.append(Profile(f"N-{n}-K-{k}-A-{value}", text, ((0, f"{n} {k}".encode()),)))
    elif problem == ("abc423", "c"):
        for n in (2, 200_000):
            for r in (0, n):
                for pattern in ("0", "1", "i%2"):
                    text = fixed(source, N=n, R=r)
                    text = replace_once(text, "L[i] in 0..=1", exact_assignment("L[i]", pattern))
                    result.append(Profile(f"N-{n}-R-{r}-pattern-{pattern}", text, ((0, f"{n} {r}".encode()),)))
    elif problem == ("abc217", "f"):
        for n, m in ((1, 0), (1, 1), (200, 0), (200, 1), (200, 79800)):
            result.append(Profile(f"N-{n}-M-{m}", fixed(source, N=n, M=m), ((0, f"{n} {m}".encode()),)))
    elif problem == ("abc246", "f"):
        for n, length, count in ((1, 1, "1"), (1, 10**9, "26"), (18, 1, "1"),
                                 (18, 10**9, "26"), (18, 10**9, "t")):
            text = fixed(source, N=n, L=length)
            text = replace_once(text, "Length[t] in 1..=26", exact_assignment("Length[t]", count))
            result.append(Profile(f"N-{n}-L-{length}-lengths-{count}", text, ((0, f"{n} {length}".encode()),)))
    elif problem == ("abc303", "c"):
        for coordinate in (-200_000, 200_000):
            text = fixed(source, N=1, M=1, H=1, K=200_000)
            text = replace_once(text, "X in unique(-200000..=200000)", f"X in unique({coordinate}..={coordinate})")
            text = replace_once(text, "Y[i] in -200000..=200000", exact_assignment("Y[i]", str(coordinate)))
            result.append(Profile(f"coordinate-{coordinate}", text, ((0, b"1 1 1 200000"),)))
        result.append(Profile("official-N-M", fixed(source, N=200_000, M=200_000, H=200_000, K=1),
                              ((0, b"200000 200000 200000 1"),)))
        for move in "RLUD":
            text = fixed(source, N=16, M=4, H=1, K=200_000)
            text = replace_once(text, '"RLUD"', f'"{move}"')
            result.append(Profile(f"moves-{move}", text, ((1, (move * 16).encode()),)))
    elif problem == ("abc251", "f"):
        flag = "Keep[(v-1)*(v-2)/2+u]"
        for n, shape, m, error in (
            (2, "tree", 1, None), (128, "tree", 127, None),
            (128, "one-extra", 128, None), (128, "dense", 8128, None),
            (632, "dense", 199396, None),
            (633, "M-200000", 200000, "total array capacity limit exceeded"),
            (200000, "tree", 199999, "array length limit exceeded"),
        ):
            text = fixed(source, N=n)
            text = replace_once(text, "Parent[i] in 1..=i", "Parent[i] in 1..=1")
            if shape == "one-extra":
                assignment = (f"if v == 3 {{\n    if u == 2 {{\n        {flag} in 1..=1\n"
                              f"    }} else {{\n        {flag} in 0..=0\n    }}\n}} else {{\n    {flag} in 0..=0\n}}")
            elif shape == "M-200000":
                assignment = (f"if (v-1)*(v-2)/2+u <= 200000 {{\n    {flag} in 1..=1\n"
                              f"}} else {{\n    {flag} in 0..=0\n}}")
            else:
                value = 1 if shape == "dense" else 0
                assignment = exact_assignment(flag, str(value))
            text = replace_once(text, f"{flag} in 0..=1", assignment)
            result.append(Profile(f"N-{n}-{shape}", text, ((0, f"{n} {m}".encode()),), expected_error=error))
    elif problem == ("abc217", "e"):
        for name, q, expression, values, x in (
            ("singleton-add", 1, "1", None, 0), ("singleton-empty-sort", 1, "3", None, 0),
            ("last-element", 2, "2-i%2", None, 10**9),
            ("consecutive-sorts", 6, None, (3, 3, 1, 3, 3, 2), 0),
            ("official-Q-adds", 200000, "1", None, 10**9),
            ("official-Q-alternating", 200000, "2-i%2", None, 0),
        ):
            text = fixed(source, Q=q)
            assignment = exact_assignment("Kind[i]", expression) if expression is not None else sequence_assignment("Kind[i]", "i", values)
            text = replace_once(text, "Kind[i] in (2*Pick[i]-1)..=(2*Pick[i]-1)", assignment)
            text = replace_once(text, "Kind[i] in Pick[i]..=Pick[i]", assignment)
            text = replace_once(text, "X[i] in 0..=1000000000", exact_assignment("X[i]", str(x)))
            result.append(Profile(name, text, ((0, str(q).encode()),)))
    elif problem == ("abc298", "c"):
        for name, n, q in (("minimum", 1, 1), ("max-identifiers", 200000, 1),
                           ("duplicates", 1, 6), ("usual-Q", 3, 256),
                           ("answer-limit", 1, 900), ("official-Q", 1, 200000)):
            text = fixed(source, N=n, Q=q)
            card, box = (200000, "N") if name == "max-identifiers" else (1, "1")
            text = replace_once(text, "Card[i] in 1..=200000", exact_assignment("Card[i]", str(card)))
            if name != "usual-Q":
                text = replace_once(text, "Box[i] in 1..=N", exact_assignment("Box[i]", box))
            if name == "duplicates":
                assignment = sequence_assignment("Kind[i]", "i", (1, 1, 3, 2, 1, 3))
                text = replace_once(text, "Kind[i] in 1..=3", assignment)
            elif name == "answer-limit":
                text = replace_once(text, "Kind[i] in 1..=3",
                                    "if i <= 500 {\n    Kind[i] in 1..=1\n} else {\n    Kind[i] in 2..=2\n}")
            elif name == "official-Q":
                text = replace_once(text, "Kind[i] in 1..=3", "Kind[i] in 1..=1")
            error = "total array capacity limit exceeded" if name == "official-Q" else None
            expected = (("answer_integers", 200000),) if name == "answer-limit" else ()
            result.append(Profile(name, text, ((0, str(n).encode()), (1, str(q).encode())), expected, error))
    elif problem == ("abc284", "d"):
        for name, t, p, distance, large in (
            ("p-less-q", 1, "1", "1", False), ("p-greater-q", 1, "2", "3", False),
            ("max-T", 10, None, None, False), ("small-cycle", 10, "(t-1)%4+1", "1", False),
            ("large-cycle", 10, "(t-1)%4+1", "1", True), ("large-singleton", 1, "4", "1", True),
        ):
            text = fixed(source, T=t)
            if p is not None:
                text = replace_once(text, "P[t] in 1..=4", exact_assignment("P[t]", p))
                text = replace_once(text, "Distance[t] in 1..=3", exact_assignment("Distance[t]", distance))
            if large:
                for i, (old, prime) in enumerate(zip((2, 3, 5, 7), LARGE_PRIMES), 1):
                    text = replace_once(text, f"Prime[{i}] in {old}..={old}", exact_assignment(f"Prime[{i}]", str(prime)))
            result.append(Profile(name, text, ((0, str(t).encode()),)))
    elif problem == ("abc413", "e"):
        for name, t, assignment, total in (
            ("min-N", 1, exact_assignment("N[t]", "1"), 2),
            ("max-N", 1, exact_assignment("N[t]", "18"), 262144),
            ("mixed-N", 3, exact_assignment("N[t]", "t"), 14),
            ("total-limit", 8, "if t < 4 {\n    N[t] in (21-3*t)..=(21-3*t)\n} else {\n    N[t] in (13-t)..=(13-t)\n}", 300000),
            ("usual-T", 64, exact_assignment("N[t]", "1"), 128),
            ("official-T-total", 100000, "if t <= T/2 {\n    N[t] in 1..=1\n} else {\n    N[t] in 2..=2\n}", 300000),
        ):
            text = fixed(source, T=t)
            text = replace_once(text, "N[t] in 1..=Cap[t]", assignment)
            result.append(Profile(name, text, ((0, str(t).encode()),), (("total_length", total),)))
    else:
        raise ValueError(f"unknown batch2 problem: {problem}")
    return result


def resources(problem, output, input_type):
    """Conservative resource bounds for these sources; not official constraints."""
    lines = output.splitlines()
    header = list(map(int, lines[0].split()))
    arrays = chars = generated = width = 0
    extra = {}
    if problem == ("abc117", "d"):
        n, _ = header
        arrays, generated = n, n + 2
        tokens, byte_bound, work, out_work = n + 2, 14 * n + 21, 4 + 2 * n, 2
    elif problem == ("abc423", "c"):
        n, _ = header
        arrays, generated = n, n + 2
        tokens, byte_bound, work, out_work = n + 2, 2 * n + 14, 4 + 2 * n, 2
    elif problem == ("abc217", "f"):
        n, m = header
        candidates = n * (2 * n - 1)
        arrays, generated, width = 3 * candidates, 3 * candidates + 3, candidates
        tokens = 2 + 2 * m
        byte_bound = len(str(n)) + len(str(m)) + 2 + m * (2 * len(str(2 * n)) + 2)
        work, out_work = 6 + 4 * n + 5 * candidates, 2 + 2 * m
    elif problem == ("abc246", "f"):
        n, _ = header
        arrays, generated, chars = 504, 506, 494
        tokens, byte_bound, work, out_work = n + 2, 500, 2200, 2 + 20 * n
    elif problem == ("abc303", "c"):
        n, m, _, _ = header
        arrays, generated, chars, width = 2 * m, 2 * m + 4, n, 400001
        tokens, byte_bound = 2 * m + 5, 29 + n + 16 * m
        work, out_work = 400011 + 3 * m + 2 * n, 3 + 2 * m
    elif problem == ("abc251", "f"):
        n, m = header
        candidates = n * (n - 1) // 2
        arrays, generated, width = 5 * candidates + 2 * n, 5 * candidates + 2 * n + 3, candidates
        tokens = 2 + 2 * m
        byte_bound = len(str(n)) + len(str(m)) + 2 + m * (2 * len(str(n)) + 2)
        work, out_work = 20 + 8 * n + 12 * candidates, 2 + 2 * candidates + m
    elif problem == ("abc217", "e"):
        q = header[0]
        arrays, generated = 4 * q + 1, 4 * q + 2
        tokens = 1 + sum(2 if row.split()[0] == b"1" else 1 for row in lines[1:])
        # The six-query fixture expands each Kind assignment into six ifs.
        # Include those profile-only headers as well as the normal body.
        byte_bound, work, out_work = len(str(q)) + 1 + 13 * q, 20 + 14 * q, 2 + 3 * q
    elif problem == ("abc298", "c"):
        n, q = header[0], int(lines[1])
        arrays, generated = 6 * q + 1, 6 * q + 3
        tokens = 2 + sum(len(row.split()) for row in lines[2:])
        byte_bound = len(str(n)) + len(str(q)) + 2 + 16 * q
        work, out_work = 20 + 20 * q, 3 + 4 * q
        case = input_type(output)
        extra["answer_integers"] = card_output_count(case)
        case.finish()
    elif problem == ("abc284", "d"):
        t = header[0]
        arrays, generated = 4 + 4 * t, 5 + 4 * t
        tokens, byte_bound, work, out_work = t + 1, len(str(t)) + 1 + 20 * t, 11 + 5 * t, 2 + 2 * t
    elif problem == ("abc413", "e"):
        t = header[0]
        total = sum(1 << int(row) for row in lines[1::2])
        arrays, generated = total + 6 * t + 21, total + 6 * t + 23
        tokens, byte_bound = 1 + t + total, 7 + 3 * t + 7 * total
        work, out_work = 100 + 66 * t + 2 * total, 2 + 4 * t + 2 * total
        extra["total_length"] = total
    else:
        raise ValueError(f"unknown batch2 problem: {problem}")
    bounds = {"array_cells_upper": arrays, "character_cells_upper": chars,
              "generated_integers_upper": generated, "output_tokens": tokens,
              "output_bytes_upper": byte_bound, "unique_width_upper": width,
              "unique_payload_bytes_upper": 8 * width, "generate_work_upper": work,
              "input_work_upper": out_work, **extra}
    for name in ("array_cells_upper", "character_cells_upper", "generated_integers_upper",
                 "output_tokens", "unique_width_upper"):
        require(bounds[name] <= 1_000_000, f"{name} exceeds DSL limit")
    require(work <= 10_000_000 and out_work <= 10_000_000, "DSL work bound exceeded")
    require(8 * width <= 16_000_000, "U1 workspace exceeded")
    require(len(output.split()) == tokens, "output token count mismatch")
    require(len(output) <= byte_bound <= 8 * 1024 * 1024, "output byte bound exceeded")
    return bounds


def verify_problem(binary, path, generate, input_type, seeds):
    problem = (path.parent.parent.name, path.parent.name)
    source = path.with_name("generator.gen").read_text(encoding="utf-8")
    conditions = []
    result = {"conditions": conditions,
              "dsl_nonblank_lines": sum(bool(line.strip()) for line in source.splitlines()),
              "dsl_physical_lines": len(source.splitlines()), "dsl_bytes": len(source.encode()),
              "source_sha256": hashlib.sha256(source.encode()).hexdigest()}
    RESULTS["/".join(problem)] = result
    selected = [Profile("normal", source, seeds=seeds), *profiles(problem, source)]
    result["planned_conditions"] = sum(len(p.seeds) for p in selected)
    outputs = set()
    with tempfile.TemporaryDirectory(prefix="atcgen-batch2-") as directory:
        probe = Path(directory) / "profile.gen"
        for profile in selected:
            probe.write_text(profile.source, encoding="utf-8")
            for seed in profile.seeds:
                record = {"profile": profile.name, "seed": seed, "status": "failed",
                          "source_sha256": hashlib.sha256(profile.source.encode()).hexdigest()}
                conditions.append(record)
                try:
                    if profile.expected_error:
                        started = perf_counter()
                        errors = []
                        for _ in range(2):
                            try:
                                generate(binary, probe, seed)
                            except ValueError as error:
                                require(profile.expected_error in str(error), f"unexpected limit: {error}")
                                errors.append(str(error))
                            else:
                                raise ValueError("expected limitation was not observed")
                        require(errors[0] == errors[1], "failure did not reproduce")
                        record.update(status="limited", reason=errors[0], empty_stdout=True,
                                      repeatable_failure=True, probe_seconds=perf_counter() - started)
                        continue
                    started = perf_counter()
                    output = generate(binary, probe, seed)
                    record["generate_seconds"] = perf_counter() - started
                    started = perf_counter()
                    repeated = generate(binary, probe, seed)
                    record["replay_seconds"] = perf_counter() - started
                    require(output == repeated, "same seed produced different bytes")
                    record["reproducible"] = True
                    started = perf_counter()
                    case = input_type(output)
                    CHECKERS[problem](case)
                    case.finish()
                    record["check_seconds"] = perf_counter() - started
                    lines = output.splitlines()
                    for index, expected in profile.rows:
                        require(lines[index] == expected, "fixed profile row mismatch")
                    bounds = resources(problem, output, input_type)
                    for name, expected in profile.expected:
                        require(bounds[name] == expected, f"fixed profile {name} mismatch")
                    record.update(status="passed", output_bytes=len(output), output_tokens=len(output.split()),
                                  output_sha256=hashlib.sha256(output).hexdigest(), resources=bounds)
                    outputs.add(output)
                except (ValueError, OSError, subprocess.TimeoutExpired) as error:
                    record["reason"] = str(error)
                    raise ValueError(f"{profile.name}, seed {seed}: {error}") from error
    return len(seeds), len(outputs)
