"""Official constraints, independent star recognition, metadata, and DSL profiles."""

import heapq
from itertools import product
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import verify_corpus as v


def check(problem, output):
    case = v.Input(output)
    v.CHECKERS[problem](case)
    case.finish()


def tree_input(n, edges):
    return (str(n) + "\n" + "".join(f"{u} {w}\n" for u, w in edges)).encode()


BRANCHING_STARS = [(1, 2), (1, 3), (1, 4), (5, 6), (5, 7), (8, 9),
                   (8, 10), (11, 12), (11, 13), (2, 6), (3, 9), (4, 12)]

VALID = {
    ("abc095", "c"): [b"1 1 1 1 1\n", b"5000 5000 5000 100000 100000\n",
                       b"100 300 200 1 100000\n", b"1 1 5000 100000 1\n"],
    ("abc192", "d"): [b"1\n1\n", b"9" * 60 + b"\n1000000000000000000\n"],
    ("abc150", "c"): [b"2\n1 2\n1 2\n", b"8\n8 7 6 5 4 3 2 1\n1 2 3 4 5 6 7 8\n"],
    ("abc302", "f"): [b"3 5\n2\n1 3\n2\n2 3\n0\n\n", b"1 2\n1\n2\n"],
    ("abc390", "c"): [b"1 1\n#\n", b"1 3\n#.#\n", b"2 3\n?.#\n.#?\n"],
    ("abc146", "d"): [b"2\n1 2\n", b"4\n3 4\n1 4\n2 4\n"],
    ("abc303", "e"): [b"3\n3 2\n1 2\n", tree_input(6, [(i + 1, i) for i in range(1, 6)]),
                       tree_input(13, BRANCHING_STARS)],
    ("abc241", "d"): [b"1\n2 1000000000000000000 5\n",
                       b"6\n3 1 1\n2 1 5\n1 7\n3 7 5\n1 7\n2 7 1\n"],
    ("abc216", "d"): [b"1 2\n1\n1\n1\n1\n", b"2 2\n2\n1 2\n2\n2 1\n",
                       b"2 3\n1\n1\n2\n1 2\n1\n2\n"],
    ("abc339", "f"): [b"4\n1\n22\n22\n" + b"9" * 1000 + b"\n", b"2\n2\n7\n"],
}


INVALID = {
    ("abc095", "c"): [b"0 1 1 1 1\n", b"1 5001 1 1 1\n", b"1 1 0 1 1\n",
                       b"1 1 1 0 1\n", b"1 1 1 100001 1\n", b"1 1 1 1 0\n",
                       b"1 1 1 1 100001\n", b"1 1 1 1\n", b"1 1 1 1 1 1\n"],
    ("abc192", "d"): [b"0\n1\n", b"01\n1\n", b"-1\n1\n", b"1a\n1\n",
                       b"9" * 61 + b"\n1\n", "１\n1\n".encode(), b"1\n0\n",
                       b"1\n1000000000000000001\n", b"1 2\n1\n", b"1\n1.0\n"],
    ("abc150", "c"): [b"1\n1\n1\n", b"9\n", b"2\n1 1\n1 2\n",
                       b"2\n0 2\n1 2\n", b"2\n1 3\n1 2\n", b"2\n1 2\n2 2\n",
                       b"2\n1\n1 2\n", b"2\n1 2\n1 2 3\n"],
    ("abc302", "f"): [b"0 2\n", b"200001 2\n", b"1 1\n1\n1\n", b"1 200001\n",
                       b"1 2\n-1\n", b"1 2\n0\n\n", b"1 2\n2\n1 1\n",
                       b"1 2\n1\n0\n", b"1 2\n1\n3\n", b"1 2\n2\n1\n",
                       b"1 2\n1\n1 2\n"],
    ("abc390", "c"): [b"0 1\n", b"1001 1\n", b"1 0\n", b"1 1001\n",
                       b"1 1\n?\n", b"1 1\n.\n", b"1 2\n#x\n",
                       b"1 2\n#\n", b"1 2\n#..\n", b"1 3\n# #\n"],
    ("abc146", "d"): [b"1\n", b"100001\n", b"2\n2 1\n", b"2\n1 1\n",
                       b"2\n0 2\n", b"2\n1 3\n", b"2\n1 2 3\n",
                       b"4\n1 2\n2 3\n1 3\n", b"4\n1 2\n3 4\n3 4\n"],
    ("abc303", "e"): [b"2\n1 2\n", b"200001\n", tree_input(4, [(1, 2), (2, 3), (3, 4)]),
                       tree_input(5, [(1, 2), (2, 3), (3, 4), (4, 5)]),
                       tree_input(6, [(1, 2), (1, 3), (4, 5), (4, 6), (1, 4)]),
                       tree_input(9, [(1, 2), (1, 3), (4, 5), (4, 6), (7, 8), (7, 9), (2, 5), (2, 8)]),
                       b"3\n1 2\n1 2\n", b"3\n1 1\n1 2\n", b"3\n1 4\n1 2\n"],
    ("abc241", "d"): [b"0\n", b"200001\n", b"1\n0 1\n", b"1\n4 1 1\n",
                       b"1\n1\n", b"1\n1 1 1\n", b"1\n2 1\n", b"1\n3 1 0\n",
                       b"1\n2 1 6\n", b"1\n1 0\n", b"1\n1 1000000000000000001\n"],
    ("abc216", "d"): [b"0 2\n", b"200001 2\n", b"1 1\n2\n1 1\n", b"1 200001\n",
                       b"1 2\n0\n\n1\n1\n", b"2 2\n2\n1 1\n2\n1 2\n",
                       b"1 2\n1\n0\n1\n1\n", b"1 2\n1\n2\n1\n1\n",
                       b"2 2\n1\n1\n1\n2\n", b"1 2\n2\n1 1\n1\n1\n",
                       b"1 2\n1\n1 1\n1\n1\n"],
    ("abc339", "f"): [b"0\n", b"1001\n", b"1\n0\n", b"1\n01\n", b"1\n-1\n",
                       b"1\n+1\n", b"1\n1x\n", b"1\n1 2\n", b"1\n" + b"1" + b"0" * 1000 + b"\n"],
}


class Batch1CheckerTests(unittest.TestCase):
    def test_accepts_independent_inputs_including_solutions_that_do_not_exist(self):
        self.assertEqual(set(VALID), set(v.BATCH1))
        for problem, cases in VALID.items():
            for output in cases:
                with self.subTest(problem=problem, output=output[:80]):
                    check(problem, output)

    def test_rejects_each_constraint_and_row_shape_violation(self):
        self.assertEqual(set(INVALID), set(v.BATCH1))
        for problem, cases in INVALID.items():
            for output in cases:
                with self.subTest(problem=problem, output=output[:80]), self.assertRaises(ValueError):
                    check(problem, output)

    def test_rejects_missing_extra_rows_and_noninteger_headers(self):
        for problem, cases in VALID.items():
            output = cases[0]
            for bad in (b"".join(output.splitlines(keepends=True)[:-1]), output + b"0\n", b"x" + output[1:]):
                with self.subTest(problem=problem, output=bad[:80]), self.assertRaises(ValueError):
                    check(problem, bad)

    def test_checks_total_set_size_independently_of_normal_profile(self):
        full = " ".join(map(str, range(1, 200_001)))
        output = ("3 200000\n" + ("200000\n" + full + "\n") * 3).encode()
        with self.assertRaisesRegex(ValueError, r"sum\(A\)"):
            check(("abc302", "f"), output)

    def test_accepts_each_color_twice_without_requiring_clearable_tubes(self):
        # The two tops differ, so no first move exists; this is still valid input.
        check(("abc216", "d"), b"2 2\n2\n1 2\n2\n2 1\n")

    def test_decimal_tokens_are_not_limited_by_i64_or_a_common_width(self):
        check(("abc192", "d"), b"9" * 60 + b"\n1\n")
        check(("abc339", "f"), b"3\n1\n12345\n" + b"9" * 1000 + b"\n")


def prufer_tree(n, code):
    degree = [0] + [1] * n
    for vertex in code:
        degree[vertex] += 1
    leaves = [vertex for vertex in range(1, n + 1) if degree[vertex] == 1]
    heapq.heapify(leaves)
    edges = []
    for vertex in code:
        leaf = heapq.heappop(leaves)
        edges.append((leaf, vertex))
        degree[vertex] -= 1
        if degree[vertex] == 1:
            heapq.heappush(leaves, vertex)
    edges.append(tuple(leaves))
    return edges


def star_cut_oracle(n, edges):
    """Exhaust every set of joining edges, rather than use depths or labels."""
    for removed in range(1 << len(edges)):
        neighbors = [[] for _ in range(n + 1)]
        used = [0] * (n + 1)
        for i, (u, w) in enumerate(edges):
            if removed >> i & 1:
                used[u] += 1
                used[w] += 1
            else:
                neighbors[u].append(w)
                neighbors[w].append(u)
        if any(used[v] and (used[v] != 1 or len(neighbors[v]) != 1) for v in range(1, n + 1)):
            continue
        unseen = set(range(1, n + 1))
        valid = True
        while unseen:
            root = unseen.pop()
            component = [root]
            for vertex in component:
                for child in neighbors[vertex]:
                    if child in unseen:
                        unseen.remove(child)
                        component.append(child)
            centers = [v for v in component if len(neighbors[v]) == len(component) - 1]
            if len(component) < 3 or len(centers) != 1:
                valid = False
                break
        if valid:
            return True
    return False


class StarRecognitionTests(unittest.TestCase):
    def test_matches_independent_edge_cut_oracle_on_every_labeled_tree_through_six_vertices(self):
        count = 0
        for n in range(3, 7):
            for code in product(range(1, n + 1), repeat=n - 2):
                edges = prufer_tree(n, code)
                try:
                    check(("abc303", "e"), tree_input(n, edges))
                    actual = True
                except ValueError:
                    actual = False
                self.assertEqual(actual, star_cut_oracle(n, edges), (n, code))
                count += 1
        self.assertEqual(count, 1440)

    def test_accepts_branched_star_connections_with_relabeling_and_reversed_edges(self):
        labels = [0, 8, 2, 13, 4, 9, 6, 1, 5, 7, 3, 10, 12, 11]
        edges = [(labels[w], labels[u]) for u, w in reversed(BRANCHING_STARS)]
        check(("abc303", "e"), tree_input(13, edges))


class MetadataTaskIdTests(unittest.TestCase):
    def metadata(self, contest, problem, text):
        with tempfile.TemporaryDirectory(prefix="atcgen-meta-") as directory:
            root = Path(directory)
            path = root / contest / problem / "problem.toml"
            path.parent.mkdir(parents=True)
            path.write_text(text, encoding="utf-8")
            with patch.object(v, "CORPUS", root):
                return v.read_metadata(path)

    def test_legacy_and_explicit_exception_are_accepted(self):
        for problem in (("abc300", "a"), ("abc095", "c")):
            source = v.CORPUS.joinpath(*problem, "problem.toml").read_text()
            self.assertEqual(self.metadata(*problem, source)[0], "supported")
        source = v.CORPUS.joinpath("abc300/a/problem.toml").read_text()
        self.metadata("abc300", "a", source + '\ntask_id = "abc300_a"\n')

    def test_exception_cannot_override_or_weaken_legacy_url_validation(self):
        legacy = v.CORPUS.joinpath("abc300/a/problem.toml").read_text()
        for changed in (
            legacy.replace("/tasks/abc300_a", "/tasks/arc096_a") + '\ntask_id = "arc096_a"\n',
            legacy + '\ntask_id = "abc300_b"\n',
            legacy.replace("https://atcoder.jp", "http://atcoder.jp"),
            legacy.replace("/contests/abc300/", "/contests/abc095/"),
        ):
            with self.subTest(source=changed), self.assertRaises(ValueError):
                self.metadata("abc300", "a", changed)
        special = v.CORPUS.joinpath("abc095/c/problem.toml").read_text()
        for changed in (special.replace('task_id = "arc096_a"\n', ""),
                        special.replace('task_id = "arc096_a"', 'task_id = "abc095_c"'),
                        special.replace("/tasks/arc096_a", "/tasks/abc095_c")):
            with self.subTest(source=changed), self.assertRaises(ValueError):
                self.metadata("abc095", "c", changed)


class Batch1GeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = v.build_binary()

    def run_source(self, problem, source, seed=42):
        with tempfile.TemporaryDirectory(prefix="atcgen-batch1-test-") as directory:
            path = Path(directory) / "probe.gen"
            path.write_text(source, encoding="utf-8")
            first = v.generate(self.binary, path, seed)
            self.assertEqual(first, v.generate(self.binary, path, seed))
        check(problem, first)
        v.batch1_resources(problem, first)
        return first

    def test_normal_generators_reproduce_and_satisfy_official_constraints(self):
        for problem in v.BATCH1:
            path = v.CORPUS.joinpath(*problem, "generator.gen")
            with self.subTest(problem=problem):
                self.run_source(problem, path.read_text())

    def test_independent_u1s_allow_equal_permutations_and_both_orders(self):
        problem = ("abc150", "c")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        self.assertIn("P in unique(1..=N)", source)
        self.assertIn("Q in unique(1..=N)", source)
        observed = set()
        for seed in (0, 1, 7, 42):
            output = self.run_source(problem, v.fixed(source, N=2), seed)
            observed.add(tuple(output.splitlines()[1:]))
        self.assertTrue(any(p == q for p, q in observed))
        self.assertEqual({p for p, _ in observed}, {b"1 2", b"2 1"})
        self.assertEqual({q for _, q in observed}, {b"1 2", b"2 1"})

    def test_variable_rows_and_empty_set_use_the_actual_generator_body(self):
        problem = ("abc302", "f")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        for profile in v.batch1_profiles(problem, source):
            if profile.name not in ("empty-set", "variable-rows"):
                continue
            output = self.run_source(problem, profile.source)
            lengths = [int(row) for row in output.splitlines()[1::2]]
            self.assertEqual(len(set(lengths)), len(lengths))
            self.assertEqual(sum(lengths), profile.total)
            sets = [row.split() for row in output.splitlines()[2::2]]
            longest = max(sets, key=len)
            self.assertTrue(all(row == longest[:len(row)] for row in sets))
            if profile.name == "empty-set":
                self.assertIn(b"\n0\n\n", output)

    def test_grid_anchor_survives_single_rows_and_columns(self):
        problem = ("abc390", "c")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        profiles = v.batch1_profiles(problem, source)
        row_profile = next(p for p in profiles if p.name == "1x1000-anchor-1000")
        row = self.run_source(problem, row_profile.source).splitlines()[1]
        self.assertEqual(row, b"." * 999 + b"#")
        column_profile = next(p for p in profiles if p.name == "1000x1-anchor-1")
        column = self.run_source(problem, column_profile.source).splitlines()[1:]
        self.assertEqual(column, [b"#", *([b"?"] * 999)])

    def test_tree_output_uses_both_permutations_and_parent_references(self):
        problem = ("abc146", "d")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        source = v.fixed(source, N=16)
        output = self.run_source(problem, source)
        probe = source.split("\ninput {", 1)[0] + "\ninput {\n    P\n    Parent\n    Order\n}\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "internal.gen"
            path.write_text(probe)
            rows = v.generate(self.binary, path, 42).splitlines()
        p, parents, order = ([int(x) for x in row.split()] for row in rows)
        expected = [tuple(sorted((p[parents[i - 1] - 1], p[i]))) for i in order]
        actual = [tuple(map(int, row.split())) for row in output.splitlines()[1:]]
        self.assertEqual(actual, expected)
        self.assertNotEqual(p, list(range(1, 17)))
        self.assertNotEqual(order, list(range(1, 16)))

    def test_query_types_can_interleave_and_be_generated_without_insertions(self):
        problem = ("abc241", "d")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        for profile in v.batch1_profiles(problem, source):
            if profile.name not in ("interleaved", "singleton-query2-empty", "singleton-query3-empty"):
                continue
            output = self.run_source(problem, profile.source)
            kinds = [int(row.split()[0]) for row in output.splitlines()[1:]]
            if profile.name == "interleaved":
                self.assertEqual(kinds, [2, 3, 1, 2, 3, 1])
            else:
                self.assertNotIn(1, kinds)

    def test_expected_capacity_failure_is_not_a_successful_generation(self):
        problem = ("abc216", "d")
        source = v.CORPUS.joinpath(*problem, "generator.gen").read_text()
        profile = next(p for p in v.batch1_profiles(problem, source) if p.expected_error)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "limit.gen"
            path.write_text(profile.source)
            for seed in profile.seeds:
                with self.assertRaisesRegex(ValueError, "total array capacity limit exceeded"):
                    v.generate(self.binary, path, seed)


if __name__ == "__main__":
    unittest.main()
