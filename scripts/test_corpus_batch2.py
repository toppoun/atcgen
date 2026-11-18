"""Independent positive/negative inputs, number theory, and batch 2 generation."""

from itertools import combinations
import math
from pathlib import Path
import tempfile
import unittest

import build_abc246_f
import corpus_batch2 as b
import verify_corpus as v


def check(problem, output):
    case = v.Input(output)
    b.CHECKERS[problem](case)
    case.finish()


VALID = {
    ("abc117", "d"): [b"1 0\n1000000000000\n", b"4 1000000000000\n0 0 1 1000000000000\n"],
    ("abc423", "c"): [b"2 0\n0 0\n", b"2 2\n1 1\n", b"4 1\n0 1 0 1\n"],
    ("abc217", "f"): [b"1 0\n", b"2 2\n1 3\n2 4\n"],
    ("abc246", "f"): [b"3 1000000000\na\naz\naz\n", b"1 1\nabcdefghijklmnopqrstuvwxyz\n"],
    ("abc303", "c"): [b"4 2 1 200000\nRLUD\n0 1\n0 2\n", b"1 1 1 1\nD\n-200000 200000\n"],
    ("abc251", "f"): [b"4 4\n2 1\n4 3\n2 3\n1 3\n", b"2 1\n2 1\n"],
    ("abc217", "e"): [b"2\n3\n3\n", b"6\n3\n1 0\n3\n3\n2\n3\n"],
    ("abc298", "c"): [b"2\n5\n1 7 1\n1 7 1\n1 7 2\n3 7\n2 1\n", b"1\n1\n1 200000 1\n"],
    ("abc284", "d"): [b"4\n12\n18\n242\n363\n", b"1\n8589934588\n"],
    ("abc413", "e"): [b"2\n1\n2 1\n3\n1 2 4 3 5 6 7 8\n",
                       b"65\n" + b"1\n2 1\n" * 65],
}


INVALID = {
    ("abc117", "d"): [b"0 0\n", b"100001 0\n", b"1 -1\n0\n", b"1 1000000000001\n0\n",
                       b"1 0\n-1\n", b"1 0\n1000000000001\n", b"2 0\n1\n"],
    ("abc423", "c"): [b"1 0\n0\n", b"200001 0\n", b"2 -1\n0 1\n", b"2 3\n0 1\n",
                       b"2 0\n0 2\n", b"2 0\n-1 0\n", b"2 0\n1\n"],
    ("abc217", "f"): [b"0 0\n", b"201 0\n", b"1 -1\n", b"1 2\n1 2\n1 2\n",
                       b"2 2\n1 2\n1 2\n", b"1 1\n2 1\n", b"1 1\n1 1\n",
                       b"1 1\n0 2\n", b"1 1\n1 3\n", b"1 1\n1 2 3\n"],
    ("abc246", "f"): [b"0 1\n", b"19 1\n", b"1 0\na\n", b"1 1000000001\na\n",
                       b"1 1\n\n", b"1 1\naa\n", b"1 1\nba\n", b"1 1\nA\n",
                       b"1 1\na1\n", "1 1\nａ\n".encode(), b"1 1\n" + b"a" * 27 + b"\n"],
    ("abc303", "c"): [b"0 1 1 1\n", b"1 0 1 1\n", b"1 1 0 1\n", b"1 1 1 0\n",
                       b"1 1 200001 1\nR\n0 0\n", b"2 1 1 1\nR\n0 0\n",
                       b"1 1 1 1\nX\n0 0\n", b"1 2 1 1\nR\n0 0\n0 0\n",
                       b"1 1 1 1\nR\n-200001 0\n", b"1 1 1 1\nR\n0 200001\n"],
    ("abc251", "f"): [b"1 0\n", b"200001 200000\n", b"3 1\n1 2\n", b"2 2\n1 2\n1 2\n",
                       b"3 3\n1 2\n2 1\n1 3\n", b"2 1\n1 1\n", b"2 1\n0 2\n",
                       b"2 1\n1 3\n", b"4 3\n1 2\n2 3\n1 3\n", b"2 1\n1 2 3\n"],
    ("abc217", "e"): [b"0\n", b"200001\n", b"1\n2\n", b"3\n1 1\n2\n2\n",
                       b"1\n4\n", b"1\n1\n", b"1\n1 -1\n", b"1\n1 1000000001\n",
                       b"1\n2 1\n", b"1\n3 1\n"],
    ("abc298", "c"): [b"0\n1\n", b"200001\n1\n", b"1\n0\n", b"1\n200001\n",
                       b"1\n1\n2 1\n", b"1\n1\n3 1\n", b"2\n2\n1 7 1\n2 2\n",
                       b"1\n2\n1 7 1\n3 8\n", b"1\n1\n1 0 1\n", b"1\n1\n1 200001 1\n",
                       b"1\n1\n1 1 2\n", b"1\n1\n1 1\n", b"1\n1\n4 1\n"],
    ("abc284", "d"): [b"0\n", b"11\n", b"1\n0\n", b"1\n9000000000000000001\n",
                       *[f"1\n{n}\n".encode() for n in (1, 4, 8, 16, 27, 30, 36, 72, 100, 49, 4 * 341550071728321)]],
    ("abc413", "e"): [b"0\n", b"100001\n", b"1\n0\n", b"1\n19\n",
                       b"1\n1\n1 1\n", b"1\n1\n0 2\n", b"1\n1\n1 3\n", b"1\n1\n1\n",
                       b"1\n1\n1 2 3\n"],
}


class Batch2CheckerTests(unittest.TestCase):
    def test_accepts_independent_valid_cases_beyond_generator_restrictions(self):
        self.assertEqual(set(VALID), set(b.BATCH2))
        for problem, cases in VALID.items():
            for output in cases:
                with self.subTest(problem=problem, value=output[:80]):
                    check(problem, output)

    def test_rejects_each_type_range_state_and_structure_violation(self):
        self.assertEqual(set(INVALID), set(b.BATCH2))
        for problem, cases in INVALID.items():
            for output in cases:
                with self.subTest(problem=problem, value=output[:80]), self.assertRaises(ValueError):
                    check(problem, output)

    def test_rejects_missing_extra_rows_and_noninteger_headers(self):
        for problem, cases in VALID.items():
            output = cases[0]
            for bad in (b"".join(output.splitlines(keepends=True)[:-1]), output + b"0\n", b"x" + output[1:]):
                with self.subTest(problem=problem), self.assertRaises(ValueError):
                    check(problem, bad)

    def test_cards_count_multiplicity_and_distinct_boxes_separately(self):
        case = v.Input(VALID[("abc298", "c")][0])
        self.assertEqual(b.card_output_count(case), 4)
        case.finish()
        for kind, expected in ((2, 200000), (3, 400)):
            output = ("1\n900\n" + "1 1 1\n" * 500 + f"{kind} 1\n" * 400).encode()
            case = v.Input(output)
            self.assertEqual(b.card_output_count(case), expected)
            case.finish()
        bad = ("1\n901\n" + "1 1 1\n" * 500 + "2 1\n" * 401).encode()
        with self.assertRaisesRegex(ValueError, "answer integer total"):
            check(("abc298", "c"), bad)

    def test_accepts_official_graph_extremes_without_the_128_vertex_limit(self):
        edges = list(combinations(range(1, 634), 2))[:200000]
        output = ("633 200000\n" + "".join(f"{w} {u}\n" for u, w in edges)).encode()
        check(("abc251", "f"), output)
        output = ("200000 199999\n" + "".join(f"{i} {i+1}\n" for i in range(1, 200000))).encode()
        check(("abc251", "f"), output)

    def test_rejects_cross_case_total_overflow(self):
        row = " ".join(map(str, range(1, (1 << 18) + 1)))
        output = (f"2\n18\n{row}\n18\n").encode()
        with self.assertRaisesRegex(ValueError, "sum of permutation lengths"):
            check(("abc413", "e"), output)

    def test_historical_phase7b_report_cannot_be_overwritten(self):
        path = v.ROOT / "corpus/reports/phase7b.json"
        before = path.read_bytes()
        with self.assertRaisesRegex(ValueError, "preserved historical report"):
            v.main(["--report", str(path)])
        self.assertEqual(path.read_bytes(), before)


def naive_prime(n):
    return n >= 2 and all(n % d for d in range(2, math.isqrt(n) + 1))


class PrimeCheckerTests(unittest.TestCase):
    def test_integer_cube_root_is_exact_at_cubes_and_64_bit_boundaries(self):
        for n in (0, 1, 7, 8, 9, 9 * 10**18, 2**64 - 1):
            root = b.integer_cuberoot(n)
            self.assertTrue(root**3 <= n < (root + 1)**3)
        for root in (2, 37, 2_080_083):
            for delta in (-1, 0, 1):
                n = root**3 + delta
                value = b.integer_cuberoot(n)
                self.assertTrue(value**3 <= n < (value + 1)**3)

    def test_deterministic_primality_matches_trial_division_and_rejects_pseudoprimes(self):
        for n in range(10001):
            self.assertEqual(b.is_prime64(n), naive_prime(n), n)
        for n in (561, 1105, 1729, 3215031751, 341550071728321, 3825123056546413051, 2**64 - 1):
            self.assertFalse(b.is_prime64(n), n)
        self.assertTrue(b.is_prime64(2**61 - 1))
        with self.assertRaisesRegex(ValueError, "64-bit"):
            b.is_prime64(2**64)

    def test_p2q_matches_an_independent_small_exhaustive_oracle(self):
        primes = [n for n in range(2, 2501) if naive_prime(n)]
        expected = {p * p * q for p in primes for q in primes if p != q and p * p * q <= 10000}
        for n in range(1, 10001):
            factors = b.p2q_factors(n)
            self.assertEqual(factors is not None, n in expected, n)
            if factors is not None:
                p, q = factors
                self.assertEqual(p * p * q, n)
                self.assertTrue(p != q and naive_prime(p) and naive_prime(q))

    def test_large_factors_are_verified_without_using_the_generator_table(self):
        self.assertTrue(all(naive_prime(n) for n in b.LARGE_PRIMES))
        self.assertLess(max(b.LARGE_PRIMES)**3, 9 * 10**18)
        p, q = b.LARGE_PRIMES[:2]
        for a, c in ((p, q), (q, p)):
            self.assertEqual(b.p2q_factors(a * a * c), (a, c))
        self.assertEqual(b.p2q_factors(4 * (2**31 - 1)), (2, 2**31 - 1))


class Batch2GeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = v.build_binary()

    def source(self, problem):
        return v.CORPUS.joinpath(*problem, "generator.gen").read_text()

    def run_source(self, problem, source, seed=42):
        with tempfile.TemporaryDirectory(prefix="atcgen-batch2-test-") as directory:
            path = Path(directory) / "probe.gen"
            path.write_text(source)
            output = v.generate(self.binary, path, seed)
            self.assertEqual(output, v.generate(self.binary, path, seed))
        check(problem, output)
        b.resources(problem, output, v.Input)
        return output

    def test_normal_generators_reproduce_and_pass_independent_checkers(self):
        for problem in b.BATCH2:
            with self.subTest(problem=problem):
                self.run_source(problem, self.source(problem))

    def test_typewriter_source_is_reproducible_and_supports_independent_lengths(self):
        problem = ("abc246", "f")
        self.assertEqual(self.source(problem), build_abc246_f.render())
        for profile in b.profiles(problem, self.source(problem)):
            if profile.name not in ("N-18-L-1000000000-lengths-t", "N-18-L-1000000000-lengths-26"):
                continue
            rows = self.run_source(problem, profile.source).splitlines()[1:]
            if profile.name.endswith("lengths-t"):
                self.assertEqual(list(map(len, rows)), list(range(1, 19)))
            else:
                self.assertEqual(rows, [b"abcdefghijklmnopqrstuvwxyz"] * 18)
        self.assertLess(sum(bool(line.strip()) for line in self.source(problem).splitlines()), 200)

    def test_pair_candidate_enumeration_and_u1_order(self):
        problem = ("abc217", "f")
        source = b.fixed(self.source(problem), N=4, M=28)
        output = self.run_source(problem, source)
        edges = [tuple(map(int, row.split())) for row in output.splitlines()[1:]]
        self.assertEqual(set(edges), set(combinations(range(1, 9), 2)))
        self.assertNotEqual(edges, sorted(edges))
        self.assertEqual(self.run_source(problem, b.fixed(self.source(problem), N=1, M=0)), b"1 0\n")

    def test_tree_extra_edges_and_dense_graphs(self):
        problem = ("abc251", "f")
        for profile in b.profiles(problem, self.source(problem)):
            if profile.name not in ("N-128-tree", "N-128-one-extra", "N-128-dense"):
                continue
            output = self.run_source(problem, profile.source)
            self.assertEqual(output.splitlines()[0], profile.rows[0][1])

    def test_sort_queries_cover_empty_sort_last_removal_and_consecutive_sorts(self):
        problem = ("abc217", "e")
        for profile in b.profiles(problem, self.source(problem)):
            if profile.name.startswith("official"):
                continue
            output = self.run_source(problem, profile.source)
            kinds = [int(row.split()[0]) for row in output.splitlines()[1:]]
            if profile.name == "consecutive-sorts":
                self.assertEqual(kinds, [3, 3, 1, 3, 3, 2])
            if profile.name == "last-element":
                self.assertEqual(kinds, [1, 2])

    def test_sort_query_work_bound_includes_fixture_branch_expansion(self):
        problem = ("abc217", "e")
        profile = next(p for p in b.profiles(problem, self.source(problem)) if p.name == "consecutive-sorts")
        output = self.run_source(problem, profile.source)
        upper = b.resources(problem, output, v.Input)["generate_work_upper"]
        full, remainder = divmod(10_000_000 - upper, 1_000_001)
        padding = "for work_probe in 1..=1000000 {\n}\n" * full
        if remainder:
            padding += f"for work_probe in 1..={remainder-1} {{\n}}\n"
        generation, input_block = profile.source.split("\ninput {", 1)
        source = generation.rstrip()[:-1] + padding + "}\ninput {" + input_block
        self.assertEqual(self.run_source(problem, source), output)

    def test_cards_duplicate_queries_and_exact_answer_budget(self):
        problem = ("abc298", "c")
        for profile in b.profiles(problem, self.source(problem)):
            if profile.name not in ("duplicates", "answer-limit"):
                continue
            output = self.run_source(problem, profile.source)
            total = b.card_output_count(v.Input(output))
            self.assertEqual(total, 4 if profile.name == "duplicates" else 200000)

    def test_small_prime_profiles_cover_both_factor_orders(self):
        problem = ("abc284", "d")
        observed = set()
        for profile in b.profiles(problem, self.source(problem))[:2]:
            output = self.run_source(problem, profile.source)
            observed.add(b.p2q_factors(int(output.splitlines()[1])))
        self.assertEqual(observed, {(2, 3), (3, 2)})

    def test_case_budgets_and_affine_bijections(self):
        problem = ("abc413", "e")
        for profile in b.profiles(problem, self.source(problem)):
            if profile.name not in ("mixed-N", "total-limit"):
                continue
            output = self.run_source(problem, profile.source)
            lines = output.splitlines()
            total = 0
            for exponent, row in zip(lines[1::2], lines[2::2]):
                length = 1 << int(exponent)
                values = [int(x) - 1 for x in row.split()]
                coefficient = (values[1] - values[0]) % length
                self.assertEqual(coefficient % 2, 1)
                self.assertEqual(values, [(coefficient * i + values[0]) % length for i in range(length)])
                total += length
            self.assertEqual(total, dict(profile.expected)["total_length"])

    def test_known_capacity_failures_remain_failures_with_empty_stdout(self):
        for problem in (("abc251", "f"), ("abc298", "c")):
            for profile in b.profiles(problem, self.source(problem)):
                if profile.expected_error is None:
                    continue
                with tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / "limit.gen"
                    path.write_text(profile.source)
                    with self.assertRaisesRegex(ValueError, profile.expected_error):
                        v.generate(self.binary, path, 0)


if __name__ == "__main__":
    unittest.main()
