"""Regression checks for the variable-width ABC420-E queries and their checker."""

import unittest

from verify_corpus import CORPUS, Input, build_binary, check_abc420_e, generate


def check(output: bytes) -> None:
    case = Input(output)
    check_abc420_e(case)
    case.finish()


class ABC420ECheckerTests(unittest.TestCase):
    def test_accepts_mixed_queries_and_queries_without_edges(self):
        check(b"4 6\n2 1\n1 1 2\n3 2\n1 2 4\n2 4\n3 1\n")
        check(b"1 2\n2 1\n3 1\n")

    def test_rejects_duplicate_edges_even_with_other_queries_between(self):
        with self.assertRaisesRegex(ValueError, "already added"):
            check(b"3 3\n1 1 2\n2 1\n1 1 2\n")

    def test_rejects_incorrect_widths_and_invalid_vertices(self):
        for output in [
            b"3 1\n1 1\n",
            b"3 1\n2 1 2\n",
            b"3 1\n3 1 2\n",
            b"3 1\n1 2 2\n",
            b"3 1\n1 2 1\n",
            b"3 1\n1 1 4\n",
            b"3 1\n2 0\n",
            b"3 1\n3 4\n",
        ]:
            with self.subTest(output=output), self.assertRaises(ValueError):
                check(output)


class ABC420EGeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = build_binary()

    def output(self, seed: int) -> bytes:
        source = CORPUS / "abc420/e/generator.gen"
        output = generate(self.binary, source, seed)
        self.assertEqual(output, generate(self.binary, source, seed))
        check(output)
        return output

    def test_interleaves_edge_queries_with_types_two_and_three(self):
        queries = [list(map(int, row.split())) for row in self.output(0).splitlines()[1:]]
        kinds = [query[0] for query in queries]
        self.assertTrue(
            any(
                kind == 1 and {2, 3} <= set(kinds[:i]) and {2, 3} <= set(kinds[i + 1 :])
                for i, kind in enumerate(kinds)
            )
        )
        for i, query in enumerate(queries, 1):
            if query[0] == 1:
                self.assertEqual(query[2], i + 1)

    def test_can_generate_no_edge_queries_and_no_type_three(self):
        for seed in (19, 100):
            with self.subTest(seed=seed):
                kinds = [row.split()[0] for row in self.output(seed).splitlines()[1:]]
                self.assertNotIn(b"1", kinds)
        self.assertNotIn(b"3", [row.split()[0] for row in self.output(100).splitlines()[1:]])


if __name__ == "__main__":
    unittest.main()
