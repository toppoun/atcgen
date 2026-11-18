"""Tree relabeling, edge ordering, and resource bounds without changing the DSL."""

import subprocess
from pathlib import Path
import tempfile
import unittest

from verify_corpus import (
    ABC385E_SIZES,
    CORPUS,
    SEEDS,
    Input,
    abc385_e_boundary_source,
    abc385_e_resources,
    build_binary,
    check_abc385_e,
    generate,
    read_metadata,
    verify_abc385_e_boundaries,
)


def check(output: bytes) -> None:
    case = Input(output)
    check_abc385_e(case)
    case.finish()


def parse_tree(output: bytes) -> tuple[int, list[tuple[int, int]]]:
    lines = output.splitlines()
    return int(lines[0]), [tuple(map(int, line.split())) for line in lines[1:]]


def increasing_from_one(n: int, edges: list[tuple[int, int]]) -> bool:
    neighbors = [[] for _ in range(n + 1)]
    for u, v in edges:
        neighbors[u].append(v)
        neighbors[v].append(u)
    stack = [(1, 0)]
    while stack:
        vertex, parent = stack.pop()
        for child in neighbors[vertex]:
            if child != parent:
                if child < vertex:
                    return False
                stack.append((child, vertex))
    return True


def work_padding(units: int) -> str:
    """Empty loops cost their header plus their number of iterations."""
    full, remainder = divmod(units, 1_000_001)
    source = "for probe in 1..=1000000 {\n}\n" * full
    if remainder:
        source += f"for probe in 1..={remainder - 1} {{\n}}\n"
    return source


class ABC385ECheckerTests(unittest.TestCase):
    def test_accepts_reordered_trees_with_large_parent_labels(self):
        check(b"3\n2 3\n1 3\n")
        check(b"5\n2 5\n1 4\n3 5\n1 5\n")

    def test_rejects_cycles_duplicates_and_disconnected_inputs(self):
        for output in (
            b"4\n1 2\n2 3\n1 3\n",
            b"4\n1 2\n1 2\n3 4\n",
            b"4\n1 2\n3 4\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output)

    def test_rejects_invalid_sizes_endpoints_and_row_shapes(self):
        for output in (
            b"2\n1 2\n",
            b"300001\n",
            b"3\n1 1\n1 3\n",
            b"3\n2 1\n1 3\n",
            b"3\n0 2\n1 3\n",
            b"3\n1 4\n1 2\n",
            b"3\n1 2 3\n1 3\n",
            b"3\n1 2\n1 3\n2 3\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output)


class ABC385EGeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = build_binary()
        cls.path = CORPUS / "abc385/e/generator.gen"
        cls.source = cls.path.read_text(encoding="utf-8")
        # Maximum cases are confined to this boundary pass and explicit probes;
        # the common 14-seed test continues to use the small corpus profile.
        cls.boundary_outputs = verify_abc385_e_boundaries(cls.binary, cls.path)

    def run_source(self, source: str, seed: int = 42, expected_error: str | None = None) -> bytes:
        with tempfile.TemporaryDirectory(prefix="atcgen-abc385-e-test-") as directory:
            path = Path(directory) / "probe.gen"
            path.write_text(source, encoding="utf-8")
            result = subprocess.run(
                [str(self.binary), "generate", str(path), "--seed", str(seed)],
                capture_output=True,
                timeout=30,
            )
        if expected_error is None:
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertFalse(result.stderr)
        else:
            self.assertEqual(result.returncode, 1)
            self.assertFalse(result.stdout)
            self.assertIn(expected_error, result.stderr.decode())
        return result.stdout

    def generation_probe(self, extra: str) -> str:
        source = abc385_e_boundary_source(self.source, 300_000)
        generation = source.split("\ninput {", 1)[0].rstrip()
        return generation[:-1] + extra + "}\ninput {\n}\n"

    def test_common_seeds_reproduce_valid_trees_and_escape_increasing_labels(self):
        outside_old_shape = False
        outside_old_order = False
        for seed in SEEDS:
            with self.subTest(seed=seed):
                output = generate(self.binary, self.path, seed)
                self.assertEqual(output, generate(self.binary, self.path, seed))
                check(output)
                n, edges = parse_tree(output)
                self.assertTrue(3 <= n <= 128)
                outside_old_shape |= not increasing_from_one(n, edges)
                outside_old_order |= [v for _, v in edges] != list(range(2, n + 1))
        self.assertTrue(outside_old_shape)
        self.assertTrue(outside_old_order)

    def test_forced_sizes_and_actual_output_counts_fit_the_limits(self):
        self.assertEqual({int(output.splitlines()[0]) for output in self.boundary_outputs}, set(ABC385E_SIZES))
        for output in self.boundary_outputs:
            n = int(output.splitlines()[0])
            resources = abc385_e_resources(n)
            self.assertEqual(sum(len(row.split()) for row in output.splitlines()), 2 * n - 1)
            self.assertLessEqual(len(output), resources["output_bytes_upper_bound"])
            self.assertLess(len(output), 8 * 1024 * 1024)
        self.assertEqual(abc385_e_resources(300_000), {
            "array_elements": 899_998,
            "generated_integers": 899_999,
            "output_tokens": 599_999,
            "output_bytes_upper_bound": 4_199_993,
            "generate_work": 1_800_003,
            "input_work": 899_999,
            "unique_width": 300_000,
            "unique_payload_bytes": 2_400_000,
        })

    def test_observed_permutations_and_parent_arrays_reconstruct_the_output(self):
        permutations = set()
        orders = set()
        for n in (3, 64):
            fixed = abc385_e_boundary_source(self.source, n)
            generation = fixed.split("\ninput {", 1)[0]
            inspect = generation + "\ninput {\nN\nP\nParent\nOrder\n}\n"
            for seed in (0, 42):
                with self.subTest(n=n, seed=seed):
                    header, p_row, parent_row, order_row = self.run_source(inspect, seed).splitlines()
                    p = list(map(int, p_row.split()))
                    parents = list(map(int, parent_row.split()))
                    order = list(map(int, order_row.split()))
                    self.assertEqual(int(header), n)
                    self.assertEqual(sorted(p), list(range(1, n + 1)))
                    self.assertEqual(sorted(order), list(range(1, n)))
                    self.assertEqual(len(parents), n - 1)
                    self.assertTrue(all(1 <= parent <= i for i, parent in enumerate(parents, 1)))
                    expected = [tuple(sorted((p[parents[e - 1] - 1], p[e]))) for e in order]
                    output = self.run_source(fixed, seed)
                    self.assertEqual(parse_tree(output)[1], expected)
                    if n == 64:
                        permutations.add(tuple(p))
                        orders.add(tuple(order))
        self.assertEqual(len(permutations), 2)
        self.assertEqual(len(orders), 2)

    def test_order_changes_only_edge_order_when_labels_and_parents_are_fixed(self):
        fixed = abc385_e_boundary_source(self.source, 64)
        original = "Order in unique(1..=N-1)"
        self.assertEqual(fixed.count(original), 1)
        identity = fixed.replace(original, "for i in 1..N {\nOrder[i] in i..=i\n}")
        for seed in (0, 42):
            with self.subTest(seed=seed):
                shuffled = self.run_source(fixed, seed)
                ordered = self.run_source(identity, seed)
                check(shuffled)
                check(ordered)
                shuffled_edges = parse_tree(shuffled)[1]
                ordered_edges = parse_tree(ordered)[1]
                self.assertEqual(set(shuffled_edges), set(ordered_edges))
                self.assertNotEqual(shuffled_edges, ordered_edges)

    def test_maximum_array_and_generated_integer_counts_reach_exactly_the_limits(self):
        resources = abc385_e_resources(300_000)
        remaining = 1_000_000 - resources["generated_integers"]
        # One uninitialized cell fills the scalar/array count difference.
        extra = f"Unused[1]\nPad[{remaining}]\nfor probe in 1..={remaining} {{\nPad[probe] in 0..=0\n}}\n"
        self.assertEqual(self.run_source(self.generation_probe(extra)), b"")
        self.run_source(self.generation_probe(extra.replace("Unused[1]", "Unused[2]")),
                        expected_error="total array capacity limit exceeded")
        self.run_source(self.generation_probe(extra + "Extra in 0..=0\n"),
                        expected_error="generated integer limit exceeded")

    def test_maximum_generate_and_input_work_counts_reach_exactly_the_limits(self):
        resources = abc385_e_resources(300_000)
        one_unit = "for probe in 1..1 {\n}\n"
        generate_padding = work_padding(10_000_000 - resources["generate_work"])
        self.assertEqual(self.run_source(self.generation_probe(generate_padding)), b"")
        self.run_source(self.generation_probe(generate_padding + one_unit),
                        expected_error="execution work limit exceeded")
        input_padding = work_padding(10_000_000 - resources["input_work"])
        fixed = abc385_e_boundary_source(self.source, 300_000).rstrip()
        at_limit = fixed[:-1] + input_padding + "}\n"
        self.assertIn(self.run_source(at_limit), self.boundary_outputs)
        self.run_source(fixed[:-1] + input_padding + one_unit + "}\n",
                        expected_error="execution work limit exceeded")

    def test_metadata_retains_only_the_remaining_size_and_distribution_restrictions(self):
        self.assertEqual(read_metadata(self.path.with_name("problem.toml")), ("supported", "restricted"))


if __name__ == "__main__":
    unittest.main()
