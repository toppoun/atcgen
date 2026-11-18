"""Independent valid/invalid inputs, unsupported Unicode, and limit isolation."""

from pathlib import Path
import tempfile
import unittest

import corpus_batch3 as b
import verify_corpus as v


def check(problem, output):
    case = v.Input(output)
    b.CHECKERS[problem](case)
    case.finish()


VALID = {
    ("abc301", "d"): [b"0\n1\n", b"111\n1\n", b"?01?\n1000000000000000000\n"],
    ("abc205", "d"): [b"1 1\n1000000000000000000\n1\n", b"4 2\n1 2 7 999999999999999999\n1\n1000000000000000000\n"],
    ("abc425", "b"): [b"1\n-1\n", b"4\n2 2 -1 4\n", b"3\n3 1 2\n"],
    ("abc326", "d"): [b"3\nAAA\nBBB\n", b"5\nABCBA\nCBCAC\n"],
    # Interleaved supports are valid, as are repetitions within A or B.
    ("abc383", "e"): [b"4 4 4\n1 3 1\n2 4 1000000000\n2 3 3\n1 4 7\n1 3 1 3\n4 2 4 2\n",
                        b"2 1 1\n1 2 1\n2\n1\n"],
    ("abc401", "f"): [b"1\n1\n", b"4\n4 2\n1 3\n3 2\n3\n3 1\n2 1\n"],
    ("abc253", "f"): [b"1 1 1\n3 1 1\n", b"2 3 4\n1 1 3 1000000000\n3 2 3\n2 2 1\n1 2 2 1\n"],
    ("abc420", "c"): [b"1 2\n1\n1000000000\nB 1 1\nA 1 1000000000\n"],
    # Unequal widths and true prefix relationships must pass the checker.
    ("abc353", "e"): [b"4\na ab abc a\n", b"3\na z aa\n"],
    ("xmascon18", "j"): [(s + "\n").encode() for s in
                           ("〇", "六", "〇の〇乗", "四の三の二乗乗", "四の三乗の二乗",
                            "千億千万千", "一億一万一", "九千九百九十九億九千九百九十九万九千九百九十九")],
}

INVALID = {
    ("abc301", "d"): [b"\n1\n", b"2\n1\n", b"0\n0\n", b"?\n1000000000000000001\n", b"?" * 61 + b"\n1\n"],
    ("abc205", "d"): [b"0 1\n\n1\n", b"1 0\n1\n", b"2 1\n1 1\n1\n", b"2 1\n2 1\n1\n",
                        b"1 1\n0\n1\n", b"1 1\n1000000000000000001\n1\n", b"1 1\n1\n0\n",
                        b"1 1\n1\n1000000000000000001\n", b"2 1\n1\n1\n"],
    ("abc425", "b"): [b"0\n", b"11\n", b"1\n0\n", b"1\n-2\n", b"1\n2\n", b"2\n1\n"],
    ("abc326", "d"): [b"2\nAA\nBB\n", b"6\nABCABC\nABCABC\n", b"3\nAB\nABC\n", b"3\nABD\nABC\n"],
    ("abc383", "e"): [b"1 0 1\n1\n1\n", b"3 1 1\n1 2 1\n1\n2\n",
                        b"2 1 0\n1 2 1\n\n\n", b"2 1 3\n1 2 1\n1 1 1\n2 2 2\n",
                        b"2 1 1\n1 1 1\n1\n2\n", b"2 1 1\n2 1 1\n1\n2\n",
                        b"2 1 1\n1 3 1\n1\n2\n", b"2 1 1\n1 2 0\n1\n2\n",
                        b"2 1 1\n1 2 1000000001\n1\n2\n", b"2 1 1\n1 2 1\n1\n1\n",
                        b"2 1 1\n1 2 1\n0\n2\n", b"2 1 1\n1 2 1\n1\n3\n",
                        b"3 3 1\n1 2 1\n1 2 2\n2 3 1\n1\n2\n",
                        b"4 3 1\n1 2 1\n2 3 1\n1 3 1\n1\n4\n"],
    ("abc401", "f"): [b"0\n1\n", b"200001\n", b"2\n1 1\n1\n", b"2\n1 3\n1\n",
                        b"3\n1 2\n2 1\n1\n", b"4\n1 2\n2 3\n3 1\n1\n", b"2\n0 1\n1\n",
                        b"1\n2\n1 1\n"],
    ("abc253", "f"): [b"0 1 1\n3 1 1\n", b"1 0 1\n3 1 1\n", b"1 1 0\n",
                        b"1 1 1\n1 1 1 1\n", b"1 2 2\n1 2 1 1\n3 1 1\n",
                        b"1 1 2\n1 0 1 1\n3 1 1\n", b"1 1 2\n1 1 2 1\n3 1 1\n",
                        b"1 1 2\n1 1 1 0\n3 1 1\n", b"1 1 2\n2 2 1\n3 1 1\n",
                        b"1 1 2\n2 1 1000000001\n3 1 1\n", b"1 1 1\n3 0 1\n",
                        b"1 1 1\n3 1 2\n", b"1 1 1\n3 1\n", b"1 1 1\n3 1 1 1\n",
                        b"1 1 1\n4 1 1\n"],
    ("abc420", "c"): [b"0 1\n", b"1 0\n1\n1\n", b"1 1\n0\n1\nA 1 1\n",
                        b"1 1\n1\n1000000001\nA 1 1\n", b"1 1\n1\n1\nC 1 1\n",
                        b"1 1\n1\n1\nA 0 1\n", b"1 1\n1\n1\nB 2 1\n",
                        b"1 1\n1\n1\nA 1 0\n", b"1 1\n1\n1\nA 1 1000000001\n",
                        b"1 1\n1\n1\nA 1\n", b"1 1\n1\n1\nA x 1\n"],
    ("abc353", "e"): [b"1\na\n", b"300001\n", b"2\na\n", b"2\na b c\n", b"2\na\nb\n",
                        b"2\na A\n", b"2\na 1\n", "2\na ａ\n".encode(),
                        b"2\n" + b"a" * 150001 + b" " + b"b" * 150000 + b"\n"],
    ("xmascon18", "j"): [b"\xff\n", *[(s + "\n").encode() for s in
                           ("", "1", "六", "\ufeff六", "一千", "万", "億", "〇一", "一〇", "一十",
                            "百千", "一万一億", "一億一億", "の一乗", "一の乗", "一乗", "一の一", "一の一乗二")]],
}


def maximum_graph_input():
    n = 200000
    return (f"{n} {n} {n}\n" + "".join(f"{i} {i+1} 1\n" for i in range(1, n))
            + f"1 {n} 1\n" + "1 " * (n - 1) + "1\n" + "2 " * (n - 1) + "2\n").encode()


def update_input(n, q, value, index):
    row = (f"{value} " * (n - 1) + f"{value}\n")
    return (f"{n} {q}\n" + row * 2 + f"A {index} {value}\n" * q).encode()


class Batch3CheckerTests(unittest.TestCase):
    def test_accepts_independent_valid_inputs_for_all_ten(self):
        self.assertEqual(set(VALID), set(b.BATCH3))
        for problem, cases in VALID.items():
            for output in cases:
                with self.subTest(problem=problem, output=output[:100]):
                    check(problem, output)

    def test_rejects_all_independent_constraint_violations(self):
        self.assertEqual(set(INVALID), set(b.BATCH3))
        for problem, cases in INVALID.items():
            for output in cases:
                with self.subTest(problem=problem, output=output[:100]), self.assertRaises(ValueError):
                    check(problem, output)

    def test_rejects_missing_and_extra_rows(self):
        for problem, cases in VALID.items():
            output = cases[0]
            for bad in (b"".join(output.splitlines(keepends=True)[:-1]), output + b"0\n"):
                with self.subTest(problem=problem), self.assertRaises(ValueError):
                    check(problem, bad)

    def test_integer_tokens_are_strict(self):
        for problem in b.BATCH3:
            if problem[0] in ("abc301", "xmascon18"):
                continue
            output = VALID[problem][0]
            with self.subTest(problem=problem), self.assertRaises(ValueError):
                check(problem, b"x" + output[1:])

    def test_increasing_extremes_and_huge_gaps(self):
        n = 100000
        for values in (range(1, n + 1), range(10**18 - n + 1, 10**18 + 1),
                       (1 + i * ((10**18 - 1) // (n - 1)) for i in range(n))):
            check(("abc205", "d"), (f"{n} 1\n" + " ".join(map(str, values)) + "\n1\n").encode())

    def test_complete_canonical_numerals_below_10000(self):
        digits = "〇一二三四五六七八九"
        for value in range(1, 10000):
            rest = value
            text = ""
            for base, unit in ((1000, "千"), (100, "百"), (10, "十")):
                quotient, rest = divmod(rest, base)
                if quotient:
                    text += (digits[quotient] if quotient > 1 else "") + unit
            if rest:
                text += digits[rest]
            self.assertTrue(b.canonical_numeral(text), text)
            check(("xmascon18", "j"), (text + "\n").encode())

    def test_unicode_maximum_length_and_deep_nesting(self):
        expression = "一の" * 33333 + "一" + "乗" * 33333
        self.assertEqual(len(expression), 100000)
        check(("xmascon18", "j"), (expression + "\n").encode())
        with self.assertRaisesRegex(ValueError, "codepoints"):
            check(("xmascon18", "j"), ("一の" + expression + "乗\n").encode())

    def test_official_graph_maximum_is_valid_but_exceeds_token_spec(self):
        output = maximum_graph_input()
        self.assertEqual(len(output.split()), 1000003)
        self.assertLess(len(output), 8 * 1024 * 1024)
        check(("abc383", "e"), output)

    def test_token_and_byte_limit_fixtures_isolate_different_limits(self):
        token = update_input(200000, 200000, 1, 1)
        byte = update_input(200000, 199999, 10**9, 200000)
        self.assertEqual((len(token.split()), len(token)), (1000002, 2000014))
        self.assertEqual((len(byte.split()), len(byte)), (999999, 8399994))
        for output in (token, byte):
            check(("abc420", "c"), output)

    def test_two_maximum_trees_are_valid_independently_of_storage(self):
        n = 200000
        tree = (f"{n}\n" + "".join(f"{i} {i+1}\n" for i in range(1, n))).encode()
        check(("abc401", "f"), tree * 2)
        self.assertEqual(2 * (3 * n - 2), 1199996)

    def test_strings_maximum_total_and_true_prefix(self):
        check(("abc353", "e"), b"2\na " + b"a" * 299999 + b"\n")
        check(("abc353", "e"), b"300000\n" + b"a " * 299999 + b"a\n")

    def test_unsupported_metadata_has_no_generator(self):
        path = v.CORPUS / "xmascon18/j/problem.toml"
        self.assertEqual(v.read_metadata(path), ("unsupported", "none"))
        self.assertFalse(path.with_name("generator.gen").exists())
        self.assertEqual(len(list(v.CORPUS.rglob("problem.toml"))), 40)

    def test_preserved_report_paths_are_rejected_before_any_work(self):
        for name in ("phase7b.json", "phase7c.json"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "preserved historical report"):
                v.main(["--report", str(v.ROOT / "corpus/reports" / name)])

    def test_new_generators_reproduce_and_pass_independent_checkers(self):
        binary = v.build_binary()
        for problem in b.BATCH3:
            if problem == ("xmascon18", "j"):
                continue
            path = v.CORPUS / problem[0] / problem[1] / "generator.gen"
            with self.subTest(problem=problem):
                a = v.generate(binary, path, 42)
                self.assertEqual(a, v.generate(binary, path, 42))
                check(problem, a)


if __name__ == "__main__":
    unittest.main()
