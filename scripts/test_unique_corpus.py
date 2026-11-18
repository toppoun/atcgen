"""U1 corpus constraints, forced boundaries, structural diversity, and replay."""

import unittest

from verify_corpus import (
    ABC300A_PROFILES,
    ABC355B_DIMENSIONS,
    CORPUS,
    SEEDS,
    Input,
    build_binary,
    check_abc300_a,
    check_abc355_b,
    generate,
    read_metadata,
    verify_abc300_a_boundaries,
    verify_abc300_a_diversity,
    verify_abc355_b_boundaries,
)


def check(output: bytes, checker) -> None:
    case = Input(output)
    checker(case)
    case.finish()


class ABC355BCheckerTests(unittest.TestCase):
    def test_accepts_smallest_case_and_full_candidate_domain(self):
        check(b"1 1\n200\n1\n", check_abc355_b)
        a = " ".join(map(str, range(1, 101)))
        b = " ".join(map(str, range(101, 201)))
        check(f"100 100\n{a}\n{b}\n".encode(), check_abc355_b)

    def test_rejects_duplicates_within_either_array_or_across_arrays(self):
        for output in (
            b"2 1\n1 1\n2\n",
            b"1 2\n1\n2 2\n",
            b"1 1\n1\n1\n",
        ):
            with self.subTest(output=output), self.assertRaisesRegex(ValueError, "distinct"):
                check(output, check_abc355_b)

    def test_rejects_invalid_lengths_values_and_row_shapes(self):
        for output in (
            b"0 1\n\n1\n",
            b"101 1\n",
            b"1 0\n1\n\n",
            b"1 101\n",
            b"1 1\n0\n1\n",
            b"1 1\n1\n201\n",
            b"2 1\n1\n2\n",
            b"1 1\n1 2\n3\n",
            b"1 1\n1\n",
            b"1 1\n1\n2\n3\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output, check_abc355_b)


class ABC300ACheckerTests(unittest.TestCase):
    def test_accepts_singleton_and_maximum_length_at_both_answer_value_extremes(self):
        for output in (b"1 1 1\n2\n", b"1 1000 1000\n2000\n"):
            check(output, check_abc300_a)
        for a, b, choices in (
            (1, 1, [2, *range(3, 302)]),
            (1000, 1000, [*range(1, 300), 2000]),
        ):
            row = " ".join(map(str, choices))
            check(f"300 {a} {b}\n{row}\n".encode(), check_abc300_a)

    def test_rejects_missing_answer_duplicate_distractors_and_repeated_answer(self):
        for output, diagnostic in (
            (b"2 1 1\n3 4\n", "A\\+B must appear exactly once"),
            (b"3 1 1\n2 3 3\n", "pairwise distinct"),
            (b"2 1 1\n2 2\n", "pairwise distinct"),
        ):
            with self.subTest(output=output), self.assertRaisesRegex(ValueError, diagnostic):
                check(output, check_abc300_a)

    def test_rejects_invalid_ranges_and_row_shapes(self):
        for output in (
            b"0 1 1\n\n",
            b"301 1 1\n",
            b"1 0 1\n1\n",
            b"1 1001 1\n1002\n",
            b"1 1 0\n1\n",
            b"1 1 1001\n1002\n",
            b"2 1 1\n2 0\n",
            b"2 1 1\n2 2001\n",
            b"2 1 1\n2\n",
            b"1 1 1\n2 3\n",
            b"1 1 1\n",
            b"1 1 1\n2\n3\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output, check_abc300_a)


class UniqueCorpusGeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = build_binary()
        cls.abc355_b = CORPUS / "abc355/b/generator.gen"
        cls.abc300_a = CORPUS / "abc300/a/generator.gen"

    def test_all_existing_seeds_reproduce_valid_cases_for_both_problems(self):
        for source, checker in (
            (self.abc355_b, check_abc355_b),
            (self.abc300_a, check_abc300_a),
        ):
            outputs = set()
            for seed in SEEDS:
                with self.subTest(source=source, seed=seed):
                    output = generate(self.binary, source, seed)
                    self.assertEqual(output, generate(self.binary, source, seed))
                    check(output, checker)
                    outputs.add(output)
            self.assertGreater(len(outputs), 1)

    def test_abc355_b_forced_sizes_cover_the_domain_and_escape_the_old_order(self):
        outputs = verify_abc355_b_boundaries(self.binary, self.abc355_b)
        headers = {tuple(map(int, output.splitlines()[0].split())) for output in outputs}
        self.assertEqual(headers, set(ABC355B_DIMENSIONS))
        full = [output for output in outputs if output.splitlines()[0] == b"100 100"]
        self.assertEqual(len(full), 2)
        for output in full:
            values = [int(value) for row in output.splitlines()[1:] for value in row.split()]
            self.assertEqual(set(values), set(range(1, 201)))
            self.assertTrue(any((right - left) % 200 != 37 for left, right in zip(values, values[1:])))

    def test_abc300_a_forced_extremes_and_first_middle_last_answers(self):
        outputs = verify_abc300_a_boundaries(self.binary, self.abc300_a)
        observed = set()
        for output in outputs:
            header, row = output.splitlines()
            n, a, b = map(int, header.split())
            choices = list(map(int, row.split()))
            observed.add((n, a, b, choices.index(a + b) + 1))
        self.assertEqual(observed, set(ABC300A_PROFILES))

    def test_abc300_a_random_answer_and_distractor_sets_vary_at_fixed_header(self):
        outputs = verify_abc300_a_diversity(self.binary, self.abc300_a)
        self.assertEqual({output.splitlines()[0] for output in outputs}, {b"9 7 11"})
        self.assertGreater(len(outputs), 1)

    def test_metadata_records_representative_coverage_for_both_problems(self):
        for source in (self.abc355_b, self.abc300_a):
            with self.subTest(source=source):
                self.assertEqual(read_metadata(source.with_name("problem.toml")), ("supported", "representative"))


if __name__ == "__main__":
    unittest.main()
