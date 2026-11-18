"""Reproduce the explicit ASCII/string declarations required by the current DSL.

Run `python scripts/build_abc246_f.py --check` to compare without writing.
No generated inputs or language implementation are involved.
"""

import argparse
from pathlib import Path
import string


TARGET = Path(__file__).resolve().parents[1] / "corpus/atcoder/abc246/f/generator.gen"


def render() -> str:
    lines = ["generate {", "    N in 1..=18", "    L in 1..=1000000000",
             "    Length[18]", "    Index[18*27]", "    Alphabet : string[26]"]
    lines += [f"    Alphabet[{i}] = '{char}'" for i, char in enumerate(string.ascii_lowercase, 1)]
    lines += ["    for t in 1..=18 {", "        Length[t] in 1..=26",
              "        Index[(t-1)*27+1] in 0..=0",
              "        for j in 1..=Length[t] {",
              "            Index[(t-1)*27+j+1] in (Index[(t-1)*27+j]+1)..=(26-Length[t]+j)",
              "        }", "    }"]
    lines += [f"    S{i} : string[Length[{i}]]" for i in range(1, 19)]
    for i in range(1, 19):
        lines += [f"    for j in 1..=Length[{i}] {{",
                  f"        S{i}[j] = Alphabet[Index[{(i-1)*27}+j+1]]", "    }"]
    lines += ["}", "", "input {", "    N L", "    for t in 1..=N {"]
    for i in range(1, 19):
        lines += [f"        if t == {i} {{", f"            S{i}", "        }"]
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = render()
    if args.check:
        return int(not TARGET.is_file() or TARGET.read_text(encoding="utf-8") != source)
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text(source, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
