use std::collections::BTreeSet;

use crate::generate;

fn dsl(generation: &str, input: &str) -> String {
    format!("generate {{\n{generation}\n}}\ninput {{\n{input}\n}}\n")
}

fn integers(output: &str) -> Vec<i64> {
    output
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect()
}

fn error(generation: &str, expected: &str) {
    let source = dsl(generation, "");
    let actual = generate(&source, 42).unwrap_err().to_string();
    assert!(
        actual.contains(expected),
        "{source}\n{actual:?} != {expected:?}"
    );
}

#[test]
fn unique_generates_distinct_ordered_samples_and_complete_permutations() {
    for count in [1, 5, 11] {
        let source = dsl(&format!("A[{count}]\nA in unique(-5..=5)"), "A");
        let mut outputs = BTreeSet::new();
        for seed in [0, 1, 3, 42, 255, u64::MAX] {
            let output = generate(&source, seed).unwrap();
            assert_eq!(output, generate(&source, seed).unwrap());
            let values = integers(&output);
            assert_eq!(values.len(), count);
            let set: BTreeSet<_> = values.iter().copied().collect();
            assert_eq!(set.len(), count);
            assert!(values.iter().all(|v| (-5..=5).contains(v)));
            if count == 11 {
                assert_eq!(set, (-5..=5).collect());
            }
            outputs.insert(output);
        }
        assert!(outputs.len() > 1);
    }
}

#[test]
fn unique_mixed_exclusions_preserve_the_source_arrays() {
    let source = dsl(
        "X in 0..=0\nE[3]\nE[1] in -2..=-2\nE[2] in 3..=3\nE[3] in 99..=99\n\
         A[4]\nA in unique(-3..=3) except(E, X+1, E[2], -2, -2, 999, -999)",
        "E\nA",
    );
    for seed in [0, 42, u64::MAX] {
        let output = generate(&source, seed).unwrap();
        let mut lines = output.lines();
        assert_eq!(lines.next(), Some("-2 3 99"));
        let values: BTreeSet<_> = integers(lines.next().unwrap()).into_iter().collect();
        assert_eq!(values, [-3, -1, 0, 2].into_iter().collect());
    }
}

#[test]
fn unique_enforces_uniqueness_across_multiple_arrays() {
    let source = dsl(
        "A[3]\nB[4]\nC[3]\nA in unique(1..=10)\n\
         B in unique(1..=10) except(A)\nC in unique(1..=10) except(A, B)",
        "A B C",
    );
    for seed in [0, 1, 42, u64::MAX] {
        let values = integers(&generate(&source, seed).unwrap());
        assert_eq!(values.len(), 10);
        assert_eq!(
            values.into_iter().collect::<BTreeSet<_>>(),
            (1..=10).collect()
        );
    }
}

#[test]
fn unique_exclusion_order_duplicates_and_out_of_range_values_do_not_change_draws() {
    let first = dsl("A[4]\nA in unique(1..=9) except(2, 4, 6)", "A");
    let second = dsl(
        "A[4]\nA in unique(1..=9) except(1000, 6, 4, 2, 2, -1000, 6)",
        "A",
    );
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(
            generate(&first, seed).unwrap(),
            generate(&second, seed).unwrap()
        );
    }
}

#[test]
fn unique_matches_forward_fisher_yates_using_legacy_sampler_draws() {
    // The oracle obtains random positions exclusively through the old scalar /
    // element syntax. It checks the new operation's swap order and the exact
    // position of subsequent integer and character draws, without sharing its
    // implementation or relying on a frequency test.
    for (lower, upper, count, excluded) in [
        (-3, 7, 5, vec![-1, 0, 7]),
        (1, 4, 4, vec![]),
        (1, 3, 2, vec![]),
        (8, 8, 1, vec![]),
    ] {
        let domain: Vec<_> = (lower..=upper).filter(|v| !excluded.contains(v)).collect();
        let suffix = "Z in -9223372036854775808..=9223372036854775807\n\
                      S : string[3]\nfor i in 1..=3 {\nS[i] in \"cba\"\n}";
        let reference = dsl(
            &format!(
                "Draw[{count}]\nfor i in 1..={count} {{\nDraw[i] in i..={}\n}}\n{suffix}",
                domain.len()
            ),
            "Draw\nZ S",
        );
        let except = if excluded.is_empty() {
            String::new()
        } else {
            format!(
                " except({})",
                excluded
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let source = dsl(
            &format!("A[{count}]\nA in unique({lower}..={upper}){except}\n{suffix}"),
            "A\nZ S",
        );
        // This last seed produces a zero first word, forcing rejection for a
        // width of three (2^64 mod 3 = 1).
        let rejecting_seed = 0u64.wrapping_sub(0x9e37_79b9_7f4a_7c15);
        for seed in [0, 1, 3, 42, 255, u64::MAX, rejecting_seed] {
            let oracle = generate(&reference, seed).unwrap();
            let (draws, following) = oracle.split_once('\n').unwrap();
            let mut expected = domain.clone();
            for (index, selected) in integers(draws).into_iter().enumerate() {
                expected.swap(index, selected as usize - 1);
            }
            let output = generate(&source, seed).unwrap();
            let (actual, rest) = output.split_once('\n').unwrap();
            assert_eq!(integers(actual), expected[..count]);
            assert_eq!(rest, following);
        }
    }
}

#[test]
fn unique_singleton_and_last_permutation_position_consume_a_draw() {
    for (count, range) in [(1, "9..=9"), (4, "1..=4")] {
        let suffix = "Z in -9223372036854775808..=9223372036854775807";
        let source = dsl(&format!("A[{count}]\nA in unique({range})\n{suffix}"), "Z");
        let legacy = dsl(
            &format!("Unused[{count}]\nfor i in 1..={count} {{\nUnused[i] in 0..=0\n}}\n{suffix}"),
            "Z",
        );
        for seed in [0, 42, u64::MAX] {
            assert_eq!(
                generate(&source, seed).unwrap(),
                generate(&legacy, seed).unwrap()
            );
        }
    }
}

#[test]
fn unique_handles_signed_endpoints_exclusive_bounds_and_exclusions() {
    for (range, expected) in [
        (
            "-9223372036854775808..=-9223372036854775806",
            vec![i64::MIN, i64::MIN + 1, i64::MIN + 2],
        ),
        (
            "9223372036854775805..=9223372036854775807",
            vec![i64::MAX - 2, i64::MAX - 1, i64::MAX],
        ),
        (
            "9223372036854775804..9223372036854775807",
            vec![i64::MAX - 3, i64::MAX - 2, i64::MAX - 1],
        ),
        ("-4..-1", vec![-4, -3, -2]),
    ] {
        let source = dsl(&format!("A[3]\nA in unique({range})"), "A");
        for seed in [0, 42, u64::MAX] {
            let mut actual = integers(&generate(&source, seed).unwrap());
            actual.sort_unstable();
            assert_eq!(actual, expected);
        }
    }
    let source = dsl(
        "A[1]\nA in unique(9223372036854775806..=9223372036854775807) except(9223372036854775807)",
        "A",
    );
    assert_eq!(integers(&generate(&source, 42).unwrap()), [i64::MAX - 1]);
    let source = dsl(
        "A[1]\nA in unique(-9223372036854775808..=-9223372036854775807) except(-9223372036854775808)",
        "A",
    );
    assert_eq!(integers(&generate(&source, 42).unwrap()), [i64::MIN + 1]);
}

#[test]
fn unique_rejects_shortage_and_empty_ranges_with_useful_diagnostics() {
    error(
        "A[3]\nA in unique(1..=4) except(1, 1, 2, 99)",
        "insufficient unique candidates: need 3, range width 4, distinct in-range exclusions 2, available 2",
    );
    error("A[1]\nA in unique(1..=1) except(1)", "available 0");
    for range in ["1..1", "2..=1", "-1..-2"] {
        error(&format!("A[1]\nA in unique({range})"), "invalid range");
    }
}

#[test]
fn unique_checks_target_before_rhs_and_exclusions_left_to_right() {
    error(
        "A[2]\nE[1]\nA[2] in 8..=8\nA in unique(1/0..=9) except(E)",
        "array element `A[2]` is already initialized",
    );
    error(
        "A[1]\nE[1]\nA in unique(1/0..=9) except(E)",
        "division by zero",
    );
    error(
        "A[1]\nE[1]\nA in unique(1..=1/0) except(E)",
        "division by zero",
    );
    error(
        "A[1]\nE[2]\nE[1] in 4..=4\nA in unique(1..=9) except(E, 1/0)",
        "array element `E[2]` is uninitialized",
    );
    error(
        "A[1]\nE[1]\nA in unique(1..=9) except(1/0, E)",
        "division by zero",
    );
    error("A[1]\nA in unique(1..1) except(1/0)", "invalid range");
    error(
        "A[1]\nA in unique(1..=9) except(1000, 1/0)",
        "division by zero",
    );
    error(
        "A[1]\nA in unique(1..=9) except(9223372036854775807+1)",
        "integer overflow",
    );
}

#[test]
fn unique_cannot_reinitialize_a_cell_or_array() {
    error(
        "A[2]\nA in unique(1..=2)\nA[1] in 9..=9",
        "already initialized",
    );
    error(
        "A[2]\nA in unique(1..=2)\nA in unique(1..=2)",
        "A[1]` is already initialized",
    );
}

#[test]
fn unique_is_top_level_only_and_does_not_add_element_exclusions_or_declarations() {
    for block in ["for i in 1..=1", "for i in 1..1", "if 0 == 1"] {
        error(
            &format!("A[1]\n{block} {{\nA in unique(1..=2)\n}}"),
            "only array element initialization",
        );
    }
    assert!(generate(&dsl("A[1]", "A in unique(1..=9)"), 42).is_err());
    error("A[1] in 1..=9", "undefined variable `A`");
    error("A[1] in unique(1..=9)", "expected range operator");
    error("A[1]\nA[1] in 1..=9 except(2)", "expected end of line");
    error("A[1]\nA in unique(1..=9) except()", "expected an integer");
    error("A[1]\nA in unique(1..=9) except(1,)", "expected an integer");
    error(
        "A[1]\nA in unique(1..=9) except(1) except(2)",
        "expected end of line",
    );
}

#[test]
fn unique_checks_names_types_and_all_nested_self_exclusions() {
    for source in [
        "A in 1..=1\nA in unique(1..=9)",
        "A : string[1]\nA in unique(1..=9)",
        "A[1] : string[1]\nA in unique(1..=9)",
    ] {
        error(source, "requires a declared integer array");
    }
    error("A in unique(1..=9)", "undefined variable `A`");
    error("A in unique(1..=9)\nA[1]", "before its declaration");
    error(
        "A[1]\nA in unique(1..=9) except(E)\nE[1]",
        "before its declaration",
    );
    error(
        "A[1]\nA in unique(1..=9) except(Missing)",
        "undefined variable `Missing`",
    );
    for excluded in ["'a'", "S", "S[1]", "G", "G[1]", "G[1][1]"] {
        error(
            &format!(
                "A[1]\nS : string[1]\nG[1] : string[1]\nA in unique(1..=9) except({excluded})"
            ),
            "type mismatch",
        );
    }
    for excluded in ["A", "(A)", "A[1]", "A[1]+1", "B[A[1]]", "1+B[1+A[1]]"] {
        error(
            &format!("A[1]\nB[1]\nA in unique(1..=9) except({excluded})"),
            "cannot exclude its own target",
        );
    }
    error(
        "A[1]\nE[1]\nA in unique(1..=9) except(E+1)",
        "is an array; expected a scalar",
    );
    error("A[1]\nX in A..=9", "is an array; expected a scalar");
    error("A[1]\nA in unique('a'..=9)", "expected Int, got Char");
}

#[test]
fn unique_and_except_remain_contextual_keywords() {
    let source = dsl(
        "unique in 3..=3\nexcept in unique..=unique\n\
         A[unique]\nA in unique(1..=9) except(except)",
        "unique except A",
    );
    let values = integers(&generate(&source, 42).unwrap());
    assert_eq!(&values[..2], [3, 3]);
    assert!(values[2..].iter().all(|v| *v != 3));
    let source = dsl(
        "unique[2]\nunique in unique(1..=3)\nexcept[1]\nexcept in unique(1..=3) except(unique)",
        "unique except",
    );
    assert_eq!(
        integers(&generate(&source, 42).unwrap())
            .into_iter()
            .collect::<BTreeSet<_>>(),
        (1..=3).collect()
    );
    let spaced = source
        .replace("unique(", "unique \t (")
        .replace("except(", "except \t (");
    assert_eq!(
        generate(&source, 42).unwrap(),
        generate(&spaced.replace('\n', "\r\n"), 42).unwrap()
    );
    error("A[1]\nA in uniqueX(1..=9)", "expected range operator");
    error(
        "A[1]\nA in unique(1..=9) exceptX(1)",
        "expected end of line",
    );
}

#[test]
fn unique_candidate_width_limit_is_separate_from_ordinary_integer_ranges() {
    let source = dsl("A[1]\nA in unique(1..=1000000)", "A");
    let value = integers(&generate(&source, 42).unwrap())[0];
    assert!((1..=1_000_000).contains(&value));
    for range in ["1..=1000001", "-9223372036854775808..=9223372036854775807"] {
        error(
            &format!("A[1]\nA in unique({range})"),
            "unique candidate width limit exceeded (1000000)",
        );
    }
    assert_eq!(
        generate(
            &dsl("X in -9223372036854775808..=9223372036854775807", "X"),
            42
        )
        .unwrap(),
        "4456085495900499605\n"
    );
}

#[test]
fn unique_read_limit_counts_duplicate_arrays_and_out_of_range_values_before_deduplication() {
    let prefix = "E[500000]\nA[1]\nfor i in 1..=500000 {\nE[i] in -1..=-1\n}";
    // One million reads fit the read limit, but sorting them exceeds the
    // independent work limit. The next read must fail at the read limit first.
    error(
        &format!("{prefix}\nA in unique(1..=1) except(E,E)"),
        "execution work limit exceeded",
    );
    error(
        &format!("{prefix}\nA in unique(1..=1) except(E,E,0)"),
        "unique exclusion read limit exceeded (1000000)",
    );
}

#[test]
fn unique_generated_integer_limit_counts_results_not_internal_position_draws() {
    assert_eq!(
        generate(
            &dsl("Pad in 0..=0\nA[999999]\nA in unique(1..=1000000)", "A[1]"),
            42
        )
        .unwrap()
        .lines()
        .count(),
        1
    );
    error(
        "Pad in 0..=0\nOther in 0..=0\nA[999999]\nA in unique(1..=1000000)",
        "generated integer limit exceeded (1000000)",
    );
}

#[test]
fn unique_activates_the_block_work_limit_and_charges_the_documented_formula() {
    // Nine empty million-iteration loops cost 9,000,009, and the array costs 1.
    // The final partial loop costs iterations+1. Unique costs its header plus
    // k + W + e*ceil(log2(e+1)): 3 without exclusions, 9 with three reads.
    let loops = "for i in 1..=1000000 {\n}\n".repeat(9);
    for (iterations, except) in [(999986, ""), (999980, " except(0,0,0)")] {
        let source =
            format!("A[1]\n{loops}for i in 1..={iterations} {{\n}}\nA in unique(1..=1){except}");
        assert_eq!(generate(&dsl(&source, "A"), 42).unwrap(), "1\n");
        error(
            &source.replace(
                &format!("1..={iterations}"),
                &format!("1..={}", iterations + 1),
            ),
            "execution work limit exceeded (10000000)",
        );
    }
}

#[test]
fn unique_output_can_be_referenced_from_later_generation_nested_loops_and_rows() {
    let source = dsl(
        "A[3]\nA in unique(1..=3)\nB[3]\nfor i in 1..=3 {\nB[i] in A[i]..=A[i]\n}",
        "A\nrow {\nfor i in 1..=3 {\nif A[i] == B[i] {\nB[i]\n}\n}\n}\nA",
    );
    let output = generate(&source, 42).unwrap();
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], lines[1]);
    assert_eq!(lines[1], lines[2]);
}
