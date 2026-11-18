"""Phase 7-D: official-constraint checkers and measured fixed profiles.

The checkers consume only serialized input. Generation-specific bounds and
profile assertions are deliberately separate from the official constraints.
"""

from dataclasses import dataclass
from collections import Counter
import hashlib
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
from time import perf_counter

from corpus_batch2 import exact_assignment, fixed, replace_once, sequence_assignment


BATCH3 = (("abc301", "d"), ("abc205", "d"), ("abc425", "b"), ("xmascon18", "j"),
          ("abc326", "d"), ("abc383", "e"), ("abc401", "f"), ("abc253", "f"),
          ("abc420", "c"), ("abc353", "e"))
RESULTS: dict[str, dict] = {}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def in_range(name, value, lower, upper):
    require(lower <= value <= upper, f"{name}={value}: expected {lower}..={upper}")


def raw_row(case, width):
    require(case.index < len(case.lines), "missing row")
    tokens = case.lines[case.index].split()
    case.index += 1
    require(len(tokens) == width, f"expected {width} tokens, got {len(tokens)}")
    return tokens


def integer(token):
    require(re.fullmatch(rb"-?[0-9]+", token) is not None, "expected integer token")
    return int(token)


def check_abc301_d(case):
    value = case.token()
    in_range("length", len(value), 1, 60)
    require(all(c in b"01?" for c in value), "invalid bitmask character")
    (n,) = case.row(1)
    in_range("N", n, 1, 10**18)


def check_abc205_d(case):
    n, q = case.row(2)
    in_range("N", n, 1, 100_000)
    in_range("Q", q, 1, 100_000)
    values = case.row(n)
    previous = 0
    for value in values:
        require(previous < value <= 10**18, "A must be strictly increasing within 1..10^18")
        previous = value
    for _ in range(q):
        (k,) = case.row(1)
        in_range("K", k, 1, 10**18)


def check_abc425_b(case):
    (n,) = case.row(1)
    in_range("N", n, 1, 10)
    for value in case.row(n):
        require(value == -1 or 1 <= value <= n, "A must be -1 or 1..N")


def check_abc326_d(case):
    (n,) = case.row(1)
    in_range("N", n, 3, 5)
    for _ in range(2):
        value = case.token()
        require(len(value) == n and all(c in b"ABC" for c in value), "invalid R/C")


class Components:
    """DSU on output vertices; no parent data from the generator is used."""

    def __init__(self, n):
        self.parent = list(range(n + 1))
        self.size = [1] * (n + 1)
        self.count = n

    def root(self, x):
        while self.parent[x] != x:
            self.parent[x] = self.parent[self.parent[x]]
            x = self.parent[x]
        return x

    def join(self, u, v):
        a, b = self.root(u), self.root(v)
        if a == b:
            return False
        if self.size[a] < self.size[b]:
            a, b = b, a
        self.parent[b] = a
        self.size[a] += self.size[b]
        self.count -= 1
        return True


def check_abc383_e(case):
    n, m, k = case.row(3)
    in_range("N", n, 2, 200_000)
    in_range("M", m, n - 1, min(n * (n - 1) // 2, 200_000))
    in_range("K", k, 1, n)
    components = Components(n)
    seen = set()
    for _ in range(m):
        u, v, weight = case.row(3)
        require(1 <= u < v <= n, "expected 1 <= u < v <= N")
        require((u, v) not in seen, "duplicate undirected edge")
        seen.add((u, v))
        in_range("weight", weight, 1, 10**9)
        components.join(u, v)
    require(components.count == 1, "graph must be connected")
    a, b = case.row(k), case.row(k)
    for value in (*a, *b):
        in_range("A/B vertex", value, 1, n)
    require(set(a).isdisjoint(b), "A/B supports must be disjoint")


def check_abc401_f(case):
    for _ in range(2):
        (n,) = case.row(1)
        in_range("N", n, 1, 200_000)
        components = Components(n)
        for _ in range(n - 1):
            u, v = case.row(2)
            in_range("u", u, 1, n)
            in_range("v", v, 1, n)
            require(u != v, "self loop")
            require(components.join(u, v), "cycle or duplicate undirected edge")
        require(components.count == 1, "tree must be connected")


def check_abc253_f(case):
    n, m, q = case.row(3)
    for name, value in (("N", n), ("M", m), ("Q", q)):
        in_range(name, value, 1, 200_000)
    has_three = False
    for _ in range(q):
        row = case.row()
        kind = row[0]
        in_range("kind", kind, 1, 3)
        require(len(row) == (4 if kind == 1 else 3), "incorrect query width")
        if kind == 1:
            _, left, right, value = row
            require(1 <= left <= right <= m, "invalid interval")
            in_range("x", value, 1, 10**9)
        elif kind == 2:
            _, i, value = row
            in_range("i", i, 1, n)
            in_range("x", value, 1, 10**9)
        else:
            _, i, j = row
            in_range("i", i, 1, n)
            in_range("j", j, 1, m)
            has_three = True
    require(has_three, "at least one type 3 query is required")


def check_abc420_c(case):
    n, q = case.row(2)
    in_range("N", n, 1, 200_000)
    in_range("Q", q, 1, 200_000)
    for _ in range(2):
        for value in case.row(n):
            in_range("A/B value", value, 1, 10**9)
    for _ in range(q):
        kind, x, value = raw_row(case, 3)
        require(kind in (b"A", b"B"), "query character must be A/B")
        in_range("X", integer(x), 1, n)
        in_range("V", integer(value), 1, 10**9)


def check_abc353_e(case):
    (n,) = case.row(1)
    in_range("N", n, 2, 300_000)
    values = raw_row(case, n)
    total = 0
    for value in values:
        require(value and all(97 <= c <= 122 for c in value), "expected nonempty lowercase string")
        total += len(value)
    require(total <= 300_000, "total string length exceeds 300000")


SMALL_NUMERAL = re.compile(r"(?:[二三四五六七八九]?千)?(?:[二三四五六七八九]?百)?"
                           r"(?:[二三四五六七八九]?十)?[一二三四五六七八九]?")
NUMERAL_CHARS = frozenset("〇一二三四五六七八九十百千万億")


def canonical_numeral(value):
    if value == "〇":
        return True
    if not value or "〇" in value:
        return False
    rest = value
    for unit in ("億", "万"):
        if unit in rest:
            coefficient, rest = rest.split(unit, 1)
            if not coefficient or SMALL_NUMERAL.fullmatch(coefficient) is None:
                return False
    return not rest or SMALL_NUMERAL.fullmatch(rest) is not None


def check_xmascon18_j(case):
    """Recognize E := numeral | E の E 乗 without evaluating huge powers.

    The explicit stack depth supports 100000-character nested expressions.
    Canonical numerals have nonzero 1..9999 groups in descending 億/万 units;
    zero is a separate atom. No Unicode normalization is performed.
    """
    try:
        text = case.token().decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("expected UTF-8 without BOM") from error
    in_range("codepoints", len(text), 1, 100_000)
    require(set(text) <= NUMERAL_CHARS | {"の", "乗"}, "invalid Unicode character or BOM")
    depth = 0
    operand = True
    index = 0
    while index < len(text):
        char = text[index]
        if char in NUMERAL_CHARS:
            end = index + 1
            while end < len(text) and text[end] in NUMERAL_CHARS:
                end += 1
            require(operand and canonical_numeral(text[index:end]), "noncanonical numeral or missing operator")
            operand = False
            index = end
        else:
            require(not operand, "operator without operand")
            if char == "の":
                depth += 1
                operand = True
            else:
                require(depth > 0, "unmatched 乗")
                depth -= 1
            index += 1
    require(not operand and depth == 0, "incomplete exponentiation expression")


CHECKERS = {problem: globals()[f"check_{problem[0]}_{problem[1]}"] for problem in BATCH3}


@dataclass(frozen=True)
class Profile:
    name: str
    source: str
    rows: tuple = ()
    expected_error: str | None = None
    limit_category: str | None = None
    seeds: tuple = (0, 42)


def token_limit_graph():
    """Independent low-storage fixture isolating the intrinsic output limit."""
    return """generate {
    N in 200000..=200000
    M in N..=N
    K in N..=N
    One in 1..=1
    Two in 2..=2
    Last in N..=N
    W in 1..=1
    I[N]
    for i in 1..=N {
        I[i] in i..=i
    }
}
input {
    N M K
    for i in 1..N {
        I[i] I[i+1] W
    }
    One Last W
    row {
        for i in 1..=K {
            One
        }
    }
    row {
        for i in 1..=K {
            Two
        }
    }
}
"""


def profiles(problem, source):
    result = []
    if problem == ("abc301", "d"):
        for length in (1, 60):
            for n in (1, 10**18):
                for char in "01?":
                    text = fixed(source, Length=length, N=n)
                    text = replace_once(text, '"01?"', f'"{char}"')
                    result.append(Profile(f"length-{length}-N-{n}-char-{char}", text,
                                          ((0, (char * length).encode()), (1, str(n).encode()))))
    elif problem == ("abc205", "d"):
        for n, q in ((1, 1), (100000, 100000)):
            for shape in ("low", "high", "gaps"):
                if n == 1 and shape == "gaps":
                    continue
                expr = {"low": "i", "high": "1000000000000000000-N+i",
                        "gaps": "1+(i-1)*((1000000000000000000-1)/(N-1))"}[shape]
                for k in (1, 10**18):
                    text = fixed(source, N=n, Q=q)
                    text = replace_once(text, "A[i+1] in (A[i]+1)..=(1000000000000000000-(N-i))",
                                        exact_assignment("A[i+1]", expr))
                    text = replace_once(text, "K[i] in 1..=1000000000000000000", exact_assignment("K[i]", str(k)))
                    result.append(Profile(f"N-{n}-Q-{q}-{shape}-K-{k}", text, ((0, f"{n} {q}".encode()),)))
    elif problem == ("abc425", "b"):
        for n in (1, 10):
            for shape in ("unspecified", "duplicates", "distinct"):
                text = fixed(source, N=n)
                choice = 0 if shape == "unspecified" else 1
                text = replace_once(text, "Choice[i] in 0..=1", exact_assignment("Choice[i]", str(choice)))
                text = replace_once(text, "A[i] in 1..=N", exact_assignment("A[i]", "i" if shape == "distinct" else "1"))
                result.append(Profile(f"N-{n}-{shape}", text, ((0, str(n).encode()),)))
    elif problem == ("abc326", "d"):
        for n in (3, 5):
            for char in "ABC":
                text = replace_once(fixed(source, N=n), 'R[i] in "ABC"', f'R[i] in "{char}"')
                text = replace_once(text, 'C[i] in "ABC"', f'C[i] in "{char}"')
                result.append(Profile(f"N-{n}-repeat-{char}", text,
                                      ((1, (char * n).encode()), (2, (char * n).encode()))))
    elif problem == ("abc383", "e"):
        flag = "Keep[(v-1)*(v-2)/2+u]"
        for n, shape in ((2, "tree"), (128, "tree"), (128, "extra"), (128, "dense")):
            for weight in (1, 10**9):
                text = fixed(source, N=n, K=n, Cut=1)
                text = replace_once(text, "Parent[i] in 1..=i", "Parent[i] in 1..=1")
                if shape == "extra":
                    assignment = (f"if v == 3 {{\n    {flag} in 1..=1\n}} else {{\n    {flag} in 0..=0\n}}")
                else:
                    assignment = exact_assignment(flag, "1" if shape == "dense" else "0")
                text = replace_once(text, f"{flag} in 0..=1", assignment)
                text = replace_once(text, "W[e] in 1..=1000000000", exact_assignment("W[e]", str(weight)))
                m = n * (n - 1) // 2 if shape == "dense" else n - 1 + (shape == "extra")
                result.append(Profile(f"N-{n}-{shape}-W-{weight}", text, ((0, f"{n} {m} {n}".encode()),)))
        result.append(Profile("official-N-M-K-token-limit", token_limit_graph(),
                              expected_error="output token limit exceeded", limit_category="output-specification"))
    elif problem == ("abc401", "f"):
        for n1, n2 in ((1, 1), (1, 200000), (200000, 1), (128, 128), (200000, 200000)):
            text = fixed(source, N1=n1, N2=n2)
            error = "total array capacity limit exceeded" if n1 == n2 == 200000 else None
            result.append(Profile(f"N1-{n1}-N2-{n2}", text, expected_error=error,
                                  limit_category="construction-storage" if error else None))
        for shape, expr in (("paths", "i"), ("stars", "1")):
            text = fixed(source, N1=32, N2=64)
            for t in (1, 2):
                text = replace_once(text, f"Parent{t}[i] in 1..=i", exact_assignment(f"Parent{t}[i]", expr))
            result.append(Profile(shape, text))
    elif problem == ("abc253", "f"):
        for name, q, kind, interval in (("singleton-three", 1, "3", "single"),
                                         ("mixed-single-column", 12, "(i-1)%3+1", "single"),
                                         ("mixed-full-interval", 12, "(i-1)%3+1", "full"),
                                         ("official-Q", 200000, "(i-1)%3+1", "full")):
            dimension = 1 if q == 1 else 200000
            text = fixed(source, N=dimension, M=dimension, Q=q, Anchor=q)
            text = replace_once(text, "Kind[i] in 1..=3", exact_assignment("Kind[i]", kind))
            text = replace_once(text, "A[i] in 1..=M", "A[i] in 1..=1")
            text = replace_once(text, "B[i] in A[i]..=M", exact_assignment("B[i]", "A[i]" if interval == "single" else "M"))
            result.append(Profile(name, text, ((0, f"{dimension} {dimension} {q}".encode()),)))
    elif problem == ("abc420", "c"):
        for name, n, q, value, index, error in (
            ("minimum-A", 1, 1, 1, "1", None), ("minimum-B", 1, 1, 10**9, "1", None),
            ("usual-maximum", 100000, 100000, 10**9, "N", None),
            ("token-limit-only", 200000, 200000, 1, "1", "output token limit exceeded"),
            ("byte-limit-only", 200000, 199999, 10**9, "N", "output size limit exceeded")):
            text = fixed(source, N=n, Q=q)
            for target in ("A[i]", "B[i]", "V[i]"):
                text = replace_once(text, f"{target} in 1..=1000000000", exact_assignment(target, str(value)))
            text = replace_once(text, "X[i] in 1..=N", exact_assignment("X[i]", index))
            text = replace_once(text, '"AB"', '"B"' if name.endswith("B") or "byte" in name else '"A"')
            result.append(Profile(name, text, ((0, f"{n} {q}".encode()),), error,
                                  "output-specification" if error else None))
    elif problem == ("abc353", "e"):
        for name, n, length, prefix in (("minimum", 2, 1, 0), ("max-N-total", 300000, 1, 0),
                                         ("max-width-total", 2, 150000, 0),
                                         ("identical", 4, 10, 10), ("shared-prefix-mixed", 3, 4, 3)):
            text = fixed(source, N=n, Length=length, PrefixLength=prefix)
            if name == "shared-prefix-mixed":
                text = replace_once(text, 'Prefix[j] in "abcdefghijklmnopqrstuvwxyz"', 'Prefix[j] in "a"')
                text = replace_once(text, 'S[i][j] in "abcdefghijklmnopqrstuvwxyz"',
                                    'if i < 3 {\n    S[i][j] in "a"\n} else {\n    S[i][j] in "b"\n}')
            result.append(Profile(name, text, ((1, b"aaaa aaaa aaab"),) if name == "shared-prefix-mixed" else ()))
    else:
        raise ValueError(f"no generator profiles for {problem}")
    return result


def resources(problem, output):
    """Conservative analytical bounds, separate from the official checker."""
    lines = output.splitlines()
    arrays = chars = generated = width = 0
    if problem == ("abc301", "d"):
        chars, generated = len(lines[0]), 2
        tokens, byte_bound, work, out_work = 2, chars + 21, 5 + 2 * chars, 2
    elif problem == ("abc205", "d"):
        n, q = map(int, lines[0].split())
        arrays, generated = n + q + 1, n + q + 3
        tokens, byte_bound = n + q + 2, 14 + 20 * (n + q)
        work, out_work = 10 + 2 * (n + q), 5 + 2 * (n + q)
    elif problem == ("abc425", "b"):
        n = int(lines[0])
        arrays, generated = 2 * n, 2 * n + 1
        tokens, byte_bound, work, out_work = n + 1, 3 + 3 * n, 5 + 5 * n, 2
    elif problem == ("abc326", "d"):
        n = int(lines[0])
        chars, generated = 2 * n, 1
        tokens, byte_bound, work, out_work = 3, 4 + 2 * n, 5 + 3 * n, 3
    elif problem == ("abc383", "e"):
        n, m, k = map(int, lines[0].split())
        candidates = n * (n - 1) // 2
        arrays, generated, width = 6 * candidates + 2 * n + 2 * k, 6 * candidates + 2 * n + 2 * k + 5, candidates
        tokens, byte_bound = 3 + 3 * m + 2 * k, 30 + 25 * m + 14 * k
        work, out_work = 30 + 10 * n + 16 * candidates + 3 * k, 10 + 3 * candidates + 2 * m + 4 * k
    elif problem == ("abc401", "f"):
        n1 = int(lines[0])
        n2 = int(lines[n1])
        cap = n1 - 1 + 1 // n1 + n2 - 1 + 1 // n2
        arrays, generated, width = n1 + n2 + 2 * cap, n1 + n2 + 2 * cap + 4, max(n1, n2)
        tokens, byte_bound = 2 * (n1 + n2) - 2, 14 * (n1 + n2) + 14
        work, out_work = 20 + 10 * (n1 + n2), 4 + 2 * (n1 + n2)
    elif problem == ("abc253", "f"):
        _, _, q = map(int, lines[0].split())
        arrays, generated = 4 * q, 4 * q + 4
        tokens = 3 + sum(len(row.split()) for row in lines[1:])
        byte_bound, work, out_work = 21 + 27 * q, 20 + 15 * q, 4 + 5 * q
    elif problem == ("abc420", "c"):
        n, q = map(int, lines[0].split())
        arrays, generated, chars = 2 * (n + q), 2 * (n + q) + 2, q
        tokens, byte_bound = 2 + 2 * n + 3 * q, 14 + 22 * n + (14 + len(str(n))) * q
        work, out_work = 20 + 3 * n + 4 * q, 4 + 2 * q
    elif problem == ("abc353", "e"):
        n = int(lines[0])
        length = len(lines[1].split()[0])
        chars, generated = (n + 1) * length, 3
        tokens, byte_bound = n + 1, len(str(n)) + 1 + n * (length + 1)
        work, out_work = 20 + 2 * length + 2 * n + 4 * n * length, 2
    else:
        raise ValueError(f"no resources for {problem}")
    bounds = dict(array_cells_upper=arrays, character_cells_upper=chars,
                  generated_integers_upper=generated, output_tokens=tokens,
                  output_bytes_upper=byte_bound, unique_width_upper=width,
                  unique_payload_bytes_upper=8 * width, generate_work_upper=work,
                  input_work_upper=out_work)
    for key in ("array_cells_upper", "character_cells_upper", "generated_integers_upper", "output_tokens", "unique_width_upper"):
        require(bounds[key] <= 1_000_000, f"{key} exceeds limit")
    require(work <= 10_000_000 and out_work <= 10_000_000, "work bound exceeds limit")
    require(len(output.split()) == tokens, "token count mismatch")
    # Some small-value profiles have a loose worst-case byte bound above the
    # limit. Only actual serialized bytes determine the output-spec limit.
    require(len(output) <= byte_bound, "analytical byte bound too small")
    require(len(output) <= 8 * 1024 * 1024, "output byte limit exceeded")
    return bounds


def verify_problem(binary, path, generate, input_type, seeds):
    problem = (path.parent.parent.name, path.parent.name)
    source = path.with_name("generator.gen").read_text(encoding="utf-8")
    conditions = []
    result = {"conditions": conditions, "dsl_nonblank_lines": sum(bool(x.strip()) for x in source.splitlines()),
              "dsl_physical_lines": len(source.splitlines()), "dsl_bytes": len(source.encode()),
              "source_sha256": hashlib.sha256(source.encode()).hexdigest()}
    RESULTS["/".join(problem)] = result
    selected = [Profile("normal", source, seeds=seeds), *profiles(problem, source)]
    result["planned_conditions"] = sum(len(profile.seeds) for profile in selected)
    outputs = set()
    with tempfile.TemporaryDirectory(prefix="atcgen-batch3-") as directory:
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
                                require(profile.expected_error in str(error), f"unexpected limitation: {error}")
                                require(str(error).startswith("atcgen exited with code 1:"), "unexpected limit exit code")
                                errors.append(str(error))
                            else:
                                raise ValueError("expected limitation was not observed")
                        require(errors[0] == errors[1], "failure not repeatable")
                        record.update(status="limited", reason=errors[0], exit_code=1, empty_stdout=True,
                                      repeatable_failure=True, limit_category=profile.limit_category,
                                      probe_seconds=perf_counter() - started)
                        continue
                    started = perf_counter()
                    output = generate(binary, probe, seed)
                    record["generate_seconds"] = perf_counter() - started
                    started = perf_counter()
                    replay = generate(binary, probe, seed)
                    record["replay_seconds"] = perf_counter() - started
                    require(output == replay, "same seed produced different bytes")
                    record["reproducible"] = True
                    started = perf_counter()
                    case = input_type(output)
                    CHECKERS[problem](case)
                    case.finish()
                    record["check_seconds"] = perf_counter() - started
                    for row, expected in profile.rows:
                        require(output.splitlines()[row] == expected, "fixed row mismatch")
                    record.update(status="passed", output_bytes=len(output), output_tokens=len(output.split()),
                                  output_sha256=hashlib.sha256(output).hexdigest(), resources=resources(problem, output))
                    outputs.add(output)
                except (ValueError, OSError, subprocess.TimeoutExpired) as error:
                    record["reason"] = str(error)
                    raise ValueError(f"{profile.name}, seed {seed}: {error}") from error
    return len(seeds), len(outputs)


def verify_unsupported(input_type):
    """Checker-only evidence; these are never generation/replay conditions."""
    result = {"status": "unsupported", "coverage": "none", "conditions": [], "planned_conditions": 0,
              "dsl_nonblank_lines": 0, "dsl_physical_lines": 0, "dsl_bytes": 0,
              "generator_present": False, "skip_reason": "DSL character type/output is ASCII-only",
              "checker_scope": "UTF-8 without BOM; codepoints; canonical Japanese numerals; complete exponentiation grammar",
              "checker_only": []}
    RESULTS["xmascon18/j"] = result
    positives = ("〇", "六", "〇の〇乗", "四の三の二乗乗", "四の三乗の二乗", "千億千万千",
                 "一億一万一", "一の" * 33333 + "一" + "乗" * 33333)
    negatives = ("一千", "万", "億", "〇一", "百千", "一万一億", "の一乗", "一の乗",
                 "一乗", "一の一", "一の一乗二", "\ufeff六", "六", "1")
    for expected, values in ((True, positives), (False, negatives)):
        for index, value in enumerate(values):
            output = (value + "\n").encode()
            started = perf_counter()
            accepted = True
            try:
                case = input_type(output)
                check_xmascon18_j(case)
                case.finish()
            except ValueError:
                accepted = False
            require(accepted == expected, "unsupported checker-only fixture mismatch")
            result["checker_only"].append({"expected_valid": expected, "fixture": index,
                                            "codepoints": len(value), "input_sha256": hashlib.sha256(output).hexdigest(),
                                            "passed": True, "check_seconds": perf_counter() - started})


FEATURES = (
    ("variable-width-strings", "生成集合の拡大", "異長文字列と1トークンでの区間出力", ("abc339/f", "abc353/e")),
    ("segment-unique", "生成集合の拡大", "可変区画・ループ内の独立U1", ("abc302/f", "abc413/e", "abc482/g")),
    ("sparse-pair-sampling", "生成集合・規模の拡大", "広い範囲・組の相異抽選と全候補走査の回避",
     ("abc302/f", "abc303/c", "abc251/f", "abc383/e", "abc420/e")),
    ("output-specification", "出力仕様の上限", "公式最大入力と出力トークン・バイト上限の衝突",
     ("abc355/d", "abc420/e", "abc383/e", "abc420/c")),
    ("construction-storage", "今回の保存方式", "補助配列容量・領域再利用・省容量構成",
     ("abc251/f", "abc401/f", "abc298/c", "abc216/d")),
    ("component-structure", "生成集合の拡大", "次数と接続先の容量を保つ成分接続", ("abc303/e",)),
    ("state-answer-budget", "生成集合・規模の拡大", "多重集合状態と解答出力量の予算", ("abc298/c",)),
    ("prime-construction", "生成集合の拡大", "固定表に依存しない素数と安全な積の構成", ("abc284/d",)),
    ("unicode-and-grammar", "必須型・形式", "Unicodeと標準漢数字・再帰的な式の生成", ("xmascon18/j",)),
    ("pure-computation", "主に記述短縮", "幅1抽選を使わない純粋計算・累積計算の補助",
     ("abc205/d", "abc302/f", "abc246/f", "abc303/c", "abc251/f", "abc303/e", "abc383/e", "abc217/e",
      "abc298/c", "abc216/d", "abc284/d", "abc413/e")),
    ("bulk-initialization", "主に記述短縮", "配列一括初期化と固定展開の共通化",
     ("abc117/d", "abc423/c", "abc425/b", "abc246/f", "abc241/d", "abc253/f", "abc420/c")),
)

ARGUMENTS = {
    "abc301/d": "長さと許可文字集合、Nの独立値域だけを生成する。解の存在条件は追加しない。",
    "abc205/d": "初期A[1]=0。i番目の出力を前値+1..10^18-(N-i)で抽選。前段の上界により下限≤上限が帰納的に成り立ち、残り要素に1ずつ予約する。全中間値は0..10^18。クエリは独立。固定隙間プロファイルも除算を先に行い積≤10^18-1。",
    "abc425/b": "各位置を-1または1..Nから抽選するため重複指定も有効。順列補完の判定を課さない。",
    "abc326/d": "N=3..5と長さNのABC文字列2本のみを構成する。盤面の存在は入力制約でない。",
    "abc383/e": "各子vをu<vへ接続する木を必ず含め、他の組を最大1回だけ採用。頂点置換・辺順列は単純性と連結性を保存する。重みは公式値域。置換前の互いに素な2区画から復元抽選し、同じ置換をA/Bへ適用するので両支持集合は非交差。配列内部重複は許可。通常N≤128。",
    "abc401/f": "各子i+1の親を1..iから選ぶため連結で非巡回。全域U1の頂点置換と辺順置換で木を保持する。N=1の反復は空で辺0本。3配列×2木の合計容量を共有し、両木最大では1199996セル。形の一様分布は主張しない。",
    "abc253/f": "アンカー1..Qを種別3に固定し、他の種別を自由抽選。分岐ごとに公式範囲と行幅を保証。種別1のrをl..Mで抽選する。行列の現在値は不要。",
    "abc420/c": "A/B/Vは1..10^9、Xは1..N、種別文字はA/B。通常N,Q≤100000で保存・出力上限内。公式最大のトークン超過と別のバイト超過は入力制約ではなくDSL出力仕様の限界。",
    "abc353/e": "共通Lを1..floor(300000/N)で選ぶのでNL≤300000。各文字を英小文字で初期化し、先頭の共通区間だけコピー。S全体の参照はN個の1トークン文字列を1行に展開する。異長や真の接頭辞関係を通常生成しない。",
    "xmascon18/j": "DSLはASCIIしか扱えず必須文字を出せない。生成器なし。検査は標準漢数字の下降単位とE:=数|EのE乗の文法を反復的に認識し、巨大な値の評価はしない。",
}


def report_catalog(paths, measured, root):
    """Include all forty, with separate populations for generation and SKIP."""
    predictions = {}
    for line in (root / "docs/CORPUS_EXPANSION_PLAN.md").read_text().splitlines():
        parts = [p.strip() for p in line.split("|")[1:-1]]
        if len(parts) == 7 and re.fullmatch(r"P[0-9]{2}", parts[0]) and parts[6] in ("S/R", "S/L", "U/N"):
            label = parts[2] + "/" + parts[3].split("/")[0].strip().lower()
            predictions[label] = {"plan_id": parts[0], "status": "unsupported" if parts[6] == "U/N" else "supported",
                                  "coverage": {"S/R": "representative", "S/L": "restricted", "U/N": "none"}[parts[6]]}
    catalog = {}
    for path in paths:
        data = tomllib.loads(path.read_text())
        label = data["contest"] + "/" + data["problem"]
        generator = path.with_name("generator.gen")
        source = generator.read_bytes() if generator.exists() else b""
        prediction = predictions.get(label)
        entry = {**data, "task_id": data.get("task_id", data["contest"] + "_" + data["problem"]),
                 "generator_present": generator.exists(), "dsl_nonblank_lines": sum(bool(x.strip()) for x in source.splitlines()),
                 "dsl_bytes": len(source), "source_sha256": hashlib.sha256(source).hexdigest() if source else None,
                 "phase7a_prediction": prediction, "constraint_argument": ARGUMENTS.get(label, data["description"])}
        entry["changed_from_prediction"] = prediction is not None and any(data[k] != prediction[k] for k in ("status", "coverage"))
        catalog[label] = entry

    def aggregate(selected):
        conditions = [record for label in selected for record in measured.get(label, {}).get("conditions", [])]
        passed = [r for r in conditions if r["status"] == "passed"]
        groups = {}
        for group, records in (("normal", [r for r in conditions if r["profile"] == "normal"]),
                               ("boundary", [r for r in conditions if r["profile"] != "normal"])):
            groups[group] = {"planned": len(records), "counts": dict(Counter(r["status"] for r in records))}
        return {"evaluation_problems": len(selected), "generators": sum(catalog[x]["generator_present"] for x in selected),
                "skipped_unsupported_problems": sum(catalog[x]["status"] == "unsupported" for x in selected),
                "statuses": {s: sum(catalog[x]["status"] == s for x in selected) for s in ("supported", "format-only", "unsupported")},
                "coverage": {c: sum(catalog[x]["coverage"] == c for x in selected) for c in ("representative", "restricted", "none")},
                "generation_conditions": len(conditions), "generation_counts": dict(Counter(r["status"] for r in conditions)),
                "unmeasured_conditions": sum(measured.get(x, {}).get("planned_conditions", len(measured.get(x, {}).get("conditions", [])))
                                              - len(measured.get(x, {}).get("conditions", [])) for x in selected),
                "constraint_checks": {"passed": len(passed), "failed": sum(r["status"] == "failed" for r in conditions)},
                "byte_replay": {"matched": sum(r.get("reproducible", False) for r in passed), "denominator": len(passed)},
                "groups": groups}

    new = ["/".join(p) for p in BATCH3]
    old = [label for label in catalog if label not in new]
    features = [{"id": name, "effect": effect, "description": desc, "problems": list(labels), "affected_problems": len(labels)}
                for name, effect, desc, labels in FEATURES]
    return {"catalog": catalog, "populations": {"all40": aggregate(list(catalog)), "existing30": aggregate(old), "new10": aggregate(new)},
            "prediction_comparison": {"denominator_new30": len(predictions),
                                      "changed": sum(e["changed_from_prediction"] for e in catalog.values()),
                                      "new_batch_changed": sum(catalog[x]["changed_from_prediction"] for x in new)},
            "missing_features": features,
            "denominator_policy": "40 evaluation problems; unsupported remains in catalog, has no generator or generation conditions; known limits are not successes; checker-only cases are separate; finite seed observations are not arbitrary-seed proofs."}
