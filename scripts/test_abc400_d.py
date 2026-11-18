"""Official ABC400-D constraints, forced boundaries, and seed reproducibility."""

import unittest

from verify_corpus import (
    CORPUS,
    Input,
    build_binary,
    check_abc400_d,
    generate,
    verify_abc400_d_boundaries,
)


def check(output: bytes) -> None:
    case = Input(output)
    check_abc400_d(case)
    case.finish()


class ABC400DCheckerTests(unittest.TestCase):
    def test_accepts_horizontal_vertical_and_mixed_grids(self):
        for output in (
            b"1 2\n..\n1 1 1 2\n",
            b"2 1\n.\n.\n1 1 2 1\n",
            b"2 3\n.##\n##.\n1 1 2 3\n",
        ):
            with self.subTest(output=output):
                check(output)

    def test_rejects_wrong_dimensions_rows_widths_and_alphabets(self):
        for output in (
            b"0 2\n1 1 1 2\n",
            b"1001 1\n",
            b"1 0\n\n1 1 1 2\n",
            b"1 1001\n",
            b"2 2\n..\n",
            b"1 2\n.\n1 1 1 2\n",
            b"1 2\n...\n1 1 1 2\n",
            b"1 2\n.x\n1 1 1 2\n",
            b"1 2\n\xc3\xa9\n1 1 1 2\n",
            b"1 2\n .\n1 1 1 2\n",
            b"1 2\n..\n1 1 1 2\nextra\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output)

    def test_rejects_out_of_bounds_identical_or_wall_endpoints(self):
        for output in (
            b"1 2\n..\n0 1 1 2\n",
            b"1 2\n..\n1 0 1 2\n",
            b"1 2\n..\n1 1 2 2\n",
            b"1 2\n..\n1 1 1 3\n",
            b"1 2\n..\n1 1 1 1\n",
            b"1 1\n.\n1 1 1 1\n",
            b"1 2\n#.\n1 1 1 2\n",
            b"1 2\n.#\n1 1 1 2\n",
            b"1 2\n..\n1 1 1\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output)


class ABC400DGeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = build_binary()
        cls.source = CORPUS / "abc400/d/generator.gen"

    def test_multiple_seeds_reproduce_valid_random_grids(self):
        outputs = set()
        endpoints = set()
        for seed in (0, 1, 42, 2**64 - 1):
            output = generate(self.binary, self.source, seed)
            self.assertEqual(output, generate(self.binary, self.source, seed))
            check(output)
            self.assertIn(b"#", output)
            outputs.add(output)
            endpoints.add(output.splitlines()[-1])
        self.assertEqual(len(outputs), 4)
        self.assertEqual(len(endpoints), 4)

    def test_minimum_thin_and_maximum_dimensions_fit_dsl_limits(self):
        outputs = verify_abc400_d_boundaries(self.binary, self.source)
        self.assertEqual(
            {output.splitlines()[0] for output in outputs},
            {b"1 2", b"2 1", b"1 1000", b"1000 1", b"1000 1000"},
        )


if __name__ == "__main__":
    unittest.main()
