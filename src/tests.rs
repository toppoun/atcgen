use std::collections::BTreeSet;

use crate::generate;

const EXPRESSION_EXAMPLE: &str = "input {
    N M
    A[N*M]
    B[M]
}

constraints {
    N in 2..=10
    M in 1..N
    A[*] in -(N*M)..=(N*M)
    B[*] in -100..=100
}
";

#[test]
fn generates_a_scalar() {
    let source = dsl("N", "N in 5..=5");
    assert_eq!(generate(&source, 42).unwrap(), "5\n");
}

#[test]
fn generates_multiple_variables_on_one_line() {
    let source = "input {\n    N M\n}\nconstraints {\n    M in N..=N\n    N in 3..=3\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "3 3\n");
}

#[test]
fn generates_an_array_with_literal_length() {
    let source = "input {\n    A[3]\n}\nconstraints {\n    A[*] in 5..=5\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "5 5 5\n");
}

#[test]
fn uses_a_scalar_as_an_array_length() {
    let source = "input {\n    N A[N]\n}\nconstraints {\n    N in 3..=3\n    A[*] in N..=N\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "3 3 3 3\n");
}

#[test]
fn samples_both_ends_of_a_negative_range() {
    let source = "input {\n    N\n}\nconstraints {\n    N in -3..=-2\n}\n";
    let values: Vec<_> = (0..16)
        .map(|seed| generate(source, seed).unwrap())
        .collect();
    assert!(
        values
            .iter()
            .all(|value| value == "-3\n" || value == "-2\n")
    );
    assert!(values.iter().any(|value| value == "-3\n"));
    assert!(values.iter().any(|value| value == "-2\n"));
}

#[test]
fn preserves_mvp_output_for_the_same_seed() {
    let source = "input {\n    N M\n    A[N]\n    B[M]\n}\n\nconstraints {\n    N in 1..=10\n    M in 1..=10\n    A[*] in 1..=100\n    B[*] in -100..=100\n}\n";
    let expected = "4 2\n59 65 51 63\n-75 -38\n";
    assert_eq!(generate(source, 42).unwrap(), expected);
    assert_ne!(generate(source, 43).unwrap(), expected);
}

#[test]
fn preserves_phase_two_output_for_equivalent_ranges() {
    let source = dsl(
        "N M\nA[N*M]\nB[M]",
        "N in 1..=10\nM in 1..=N\nA[*] in -(N*M)..=(N*M)\nB[*] in -100..=100",
    );
    let expected = "4 4\n8 -4 -3 -7 -9 -14 -6 16 -14 -12 -8 -12 16 -11 7 -7\n2 71 92 9\n";
    assert_eq!(generate(&source, 42).unwrap(), expected);
}

#[test]
fn distinguishes_exclusive_and_inclusive_endpoints() {
    for (range, last) in [("1..10", 9), ("1..=10", 10)] {
        let source = dsl("A[256]", &format!("A[*] in {range}"));
        let output = generate(&source, 42).unwrap();
        let sampled: BTreeSet<_> = parse_integers(&output).into_iter().collect();
        assert_eq!(sampled, (1..=last).collect(), "{range}");
    }
}

#[test]
fn emits_lf_with_no_extra_spaces_or_lines() {
    let source = "input {\r\n  N M\r\n\r\n  A[2]\r\n}\r\n\r\nconstraints {\r\n  N in 1..=1\r\n  M in 2..=2\r\n  A[*] in 3..=3\r\n}";
    assert_eq!(generate(source, 42).unwrap().as_bytes(), b"1 2\n3 3\n");
}

#[test]
fn reports_syntax_errors_with_a_line_and_column() {
    let source = "input {\n  A[]\n}\nconstraints {\n}\n";
    assert_eq!(
        generate(source, 42).unwrap_err().to_string(),
        "2:5: expected an integer, scalar variable or `(`"
    );
}

#[test]
fn generated_values_meet_their_constraints() {
    let source = "input {\n    N L U\n    A[N]\n}\nconstraints {\n    N in 1..=10\n    L in -100..=-1\n    U in L..=100\n    A[*] in L..=U\n}\n";
    for seed in 0..64 {
        let output = generate(source, seed).unwrap();
        let mut lines = output.lines();
        let scalars: Vec<i64> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        let [n, lower, upper] = scalars[..] else {
            panic!("expected three scalars");
        };
        assert!((1..=10).contains(&n));
        assert!((-100..=-1).contains(&lower));
        assert!((lower..=100).contains(&upper));
        let elements = lines.next().unwrap().split_whitespace();
        assert_eq!(elements.clone().count(), usize::try_from(n).unwrap());
        for value in elements {
            assert!((lower..=upper).contains(&value.parse().unwrap()));
        }
        assert!(lines.next().is_none());
    }
}

#[test]
fn rejects_unsupported_and_malformed_syntax() {
    for (row, rule, error) in [
        ("A[2][2]", "A[*] in 1..=1", "expected whitespace"),
        ("A[2]B", "A[*] in 1..=1", "expected whitespace"),
        ("A[2]", "A[1] in 1..=1", "expected `*`"),
        ("N", "N in 1..2 sorted", "expected end of line"),
        ("N", "N in 0..9223372036854775808", "outside the i64 range"),
    ] {
        assert_dsl_error(row, rule, error);
    }
}

#[test]
fn diagnoses_malformed_range_operators() {
    for (rule, expected) in [
        ("N in 1...2", "5:7: invalid range operator `...`"),
        ("N in 1..==2", "5:7: invalid range operator `..==`"),
        ("N in 1 2", "5:8: expected range operator `..` or `..=`"),
    ] {
        assert_dsl_error("N", rule, expected);
    }
}

#[test]
fn rejects_legacy_array_range_notation() {
    for row in ["A[1..N]", "A[1..=N]"] {
        assert_dsl_error(
            row,
            "A[*] in 1..=1",
            "2:4: expected an array length expression",
        );
    }
}

#[test]
fn rejects_invalid_declarations_and_constraints() {
    for (row, rules, error) in [
        ("N N", "N in 1..=1", "duplicate declaration"),
        ("N", "N in 1..=1\nN in 2..=2", "duplicate constraint"),
        ("N", "", "missing constraint"),
        ("N", "M in 1..=1", "undefined variable `M`"),
        ("N", "N in M..=1", "undefined variable `M`"),
        ("A[N]", "A[*] in 1..=1", "undefined variable `N`"),
        ("N M", "N in 1..M\nM in 1..=1", "`M` is not available"),
        (
            "A[N] N",
            "A[*] in 1..=1\nN in 1..=1",
            "`N` is not available",
        ),
        ("N", "N in N..=1", "`N` is not available"),
        ("A[1] N", "A[*] in 1..=1\nN in 1..A", "expected a scalar"),
        ("A[1]", "A in 1..=1", "use `A[*]`"),
        ("N", "N[*] in 1..=1", "cannot be indexed"),
    ] {
        assert_dsl_error(row, rules, error);
    }
}

#[test]
fn rejects_invalid_bounds_and_lengths_at_generation() {
    for range in ["1..1", "2..1", "6..=5"] {
        assert_dsl_error("N", &format!("N in {range}"), "invalid range");
    }
    assert_dsl_error("N M", "N in 2..=2\nM in N..1", "invalid range");
    assert_dsl_error("A[0]", "A[*] in 1..=1", "invalid array length 0");
    assert_dsl_error(
        "N A[N]",
        "N in -1..=-1\nA[*] in 0..=0",
        "invalid array length -1",
    );
}

#[test]
fn enforces_integer_and_output_limits() {
    assert_dsl_error(
        "A[9223372036854775807]",
        "A[*] in 0..=0",
        "integer limit exceeded",
    );
    assert_dsl_error(
        "N A[1000000]",
        "N in 1..=1\nA[*] in 0..=0",
        "integer limit exceeded",
    );
    assert_dsl_error(
        "A[500000]",
        "A[*] in -9223372036854775808..=-9223372036854775808",
        "output size limit exceeded",
    );
}

#[test]
fn samples_the_full_signed_integer_range() {
    let source = "input {\n    N\n}\nconstraints {\n    N in -9223372036854775808..=9223372036854775807\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "4456085495900499605\n");
}

#[test]
fn safely_samples_ranges_at_signed_boundaries() {
    let min = i64::MIN;
    let max = i64::MAX;
    for (range, value) in [
        (format!("{min}..={min}"), min),
        (format!("{min}..{}", min + 1), min),
        (format!("{max}..={max}"), max),
        (format!("{}..{max}", max - 1), max - 1),
    ] {
        let source = dsl("N", &format!("N in {range}"));
        assert_eq!(
            generate(&source, 42).unwrap(),
            format!("{value}\n"),
            "{range}"
        );
    }
    assert_dsl_error("N", &format!("N in {min}..{min}"), "invalid range");

    let half_open = dsl("A[32]", &format!("A[*] in {min}..{max}"));
    let closed = dsl("A[32]", &format!("A[*] in {min}..={}", max - 1));
    assert_eq!(
        generate(&half_open, 42).unwrap(),
        generate(&closed, 42).unwrap()
    );
}

#[test]
fn reproduces_sampling_after_rejecting_a_random_word() {
    let source = "input {\n    N\n}\nconstraints {\n    N in -9223372036854775808..=0\n}\n";
    assert_eq!(generate(source, 3).unwrap(), "-5528608851982440056\n");
}

#[test]
fn evaluates_binary_arithmetic() {
    for (expr, value) in [
        ("5+8", 13),
        ("5-8", -3),
        ("3*4", 12),
        ("7/3", 2),
        ("-7/3", -2),
        ("7/-3", -2),
        ("-7%3", -1),
        ("7%-3", 1),
    ] {
        assert_expression(expr, value);
    }
}

#[test]
fn respects_precedence_and_left_associativity() {
    assert_expression("2 + 3*4 - 8/2%3", 13);
    assert_expression("20-6-3", 11);
    assert_expression("24/3*2%5", 1);
}

#[test]
fn parentheses_override_precedence() {
    assert_expression("(2 + 3) * 4", 20);
    assert_expression("7 / (3 + 1)", 1);
    assert_expression("20-(6-3)", 17);
}

#[test]
fn evaluates_unary_operators() {
    assert_expression("-(2 + 3)", -5);
    assert_expression("+-(-3)", 3);
    assert_expression("-+(2)*3", -6);
    assert_expression("7+-3", 4);
    assert_expression("- 9223372036854775808", i64::MIN);
}

#[test]
fn evaluates_multiple_scalar_references_in_generation_order() {
    let source = dsl("N M K", "K in +(N+M)..=(M*2-N)\nM in 6..=6\nN in 3..=3");
    assert_eq!(generate(&source, 42).unwrap(), "3 6 9\n");
}

#[test]
fn generates_an_array_of_length_n_minus_one() {
    let source = dsl("N A[N-1]", "A[*] in 9..=9\nN in 4..=4");
    assert_eq!(generate(&source, 42).unwrap(), "4 9 9 9\n");
}

#[test]
fn generates_an_array_using_a_parenthesized_length_expression() {
    let source = dsl("N M\nA[(N+M)*2]", "N in 1..=1\nM in 2..=2\nA[*] in 7..=7");
    assert_eq!(generate(&source, 42).unwrap(), "1 2\n7 7 7 7 7 7\n");
}

#[test]
fn generates_product_length_and_dynamic_bounds() {
    for seed in 0..64 {
        let output = generate(EXPRESSION_EXAMPLE, seed).unwrap();
        let mut lines = output.lines();
        let header = parse_integers(lines.next().unwrap());
        let [n, m] = header[..] else {
            panic!("expected two scalars");
        };
        assert!((2..=10).contains(&n));
        assert!((1..n).contains(&m));

        let product = n * m;
        let a = parse_integers(lines.next().unwrap());
        assert_eq!(a.len(), usize::try_from(product).unwrap());
        assert!(a.iter().all(|value| (-product..=product).contains(value)));
        let b = parse_integers(lines.next().unwrap());
        assert_eq!(b.len(), usize::try_from(m).unwrap());
        assert!(b.iter().all(|value| (-100..=100).contains(value)));
        assert!(lines.next().is_none());
    }
}

#[test]
fn expressions_preserve_reproducibility() {
    let expected = "3 2\n4 -3 1 1 4 6\n-45 25\n";
    assert_eq!(generate(EXPRESSION_EXAMPLE, 42).unwrap(), expected);
    assert_eq!(generate(EXPRESSION_EXAMPLE, 42).unwrap(), expected);
    assert_ne!(generate(EXPRESSION_EXAMPLE, 43).unwrap(), expected);
}

#[test]
fn diagnoses_arithmetic_errors_at_the_operator() {
    let source = dsl("    N\n    A[(12/(N-2))]", "N in 2..=2\nA[*] in 1..=1");
    assert_eq!(
        generate(&source, 42).unwrap_err().to_string(),
        "3:10: division by zero in expression `(12/(N-2))`"
    );
    assert_dsl_error("N M", "N in 1..=1\nM in 0..10%(N-1)", "remainder by zero");
}

#[test]
fn detects_overflow_in_each_arithmetic_operation() {
    for expr in [
        "9223372036854775807+1-1",
        "-9223372036854775808-1",
        "9223372036854775807*2",
        "-9223372036854775808/-1",
        "-9223372036854775808%-1",
        "--9223372036854775808",
    ] {
        assert_dsl_error("N", &format!("N in {expr}..0"), "integer overflow");
    }
    assert_dsl_error(
        "N M",
        "N in -9223372036854775808..=-9223372036854775808\nM in -N..1",
        "integer overflow in expression `-N`",
    );
}

#[test]
fn diagnoses_overflow_in_an_upper_bound_and_array_length() {
    assert_dsl_error(
        "N M",
        "N in 9223372036854775807..=9223372036854775807\nM in 0..=(N+1)",
        "6:12: integer overflow in expression `(N+1)`",
    );
    assert_dsl_error(
        "N A[N*2]",
        "N in 9223372036854775807..=9223372036854775807\nA[*] in 0..=0",
        "2:6: integer overflow in expression `N*2`",
    );
}

#[test]
fn validates_references_nested_inside_expressions() {
    let source = dsl("N", "    N in 1..(2 + X*3)");
    assert_eq!(
        generate(&source, 42).unwrap_err().to_string(),
        "5:18: undefined variable `X`"
    );
    for (row, rules, error) in [
        (
            "N M",
            "N in -(-M)..2\nM in (N+1)..3",
            "`M` is not available",
        ),
        ("N", "N in (2+N*3)..2", "`N` is not available"),
        (
            "A[2] N",
            "A[*] in 1..=1\nN in 1..(2+A)",
            "expected a scalar",
        ),
        (
            "A[N*2] N",
            "A[*] in 1..=1\nN in 1..2",
            "`N` is not available",
        ),
    ] {
        assert_dsl_error(row, rules, error);
    }
}

#[test]
fn diagnoses_invalid_evaluated_ranges_and_lengths() {
    let source = dsl("N M", "    N in 2..=2\n    M in (1+N)..(N-1)");
    assert_eq!(
        generate(&source, 42).unwrap_err().to_string(),
        "6:17: invalid range `(1+N)..(N-1)`: empty with lower bound 3 and upper bound 1"
    );
    assert_dsl_error(
        "N A[N-1]",
        "N in 1..=1\nA[*] in 1..=1",
        "invalid array length 0",
    );
    assert_dsl_error(
        "N A[N*2]",
        "N in 500000..=500000\nA[*] in 1..=1",
        "integer limit exceeded (1000000) in expression `N*2`",
    );
}

#[test]
fn rejects_unbalanced_parentheses_and_unsupported_operators() {
    assert_dsl_error("N", "N in 1..(2+3", "expected `)`");
    assert_dsl_error("A[(2+3]", "A[*] in 1..=1", "expected `)`");
    assert_dsl_error("N", "N in 1..(2+3))", "expected end of line");
    assert_dsl_error("N", "N in 1..2^3", "5:10: expected end of line");
    assert_dsl_error("N", "N in 1..2**3", "5:11: expected an integer");
}

#[test]
fn limits_nesting_but_handles_long_operator_chains() {
    let nested = format!("{}1{}", "(".repeat(128), ")".repeat(128));
    assert_expression(&nested, 1);
    assert_dsl_error(
        "N",
        &format!("N in 1..({nested})"),
        "expression nesting limit exceeded",
    );
    assert_dsl_error(
        "N",
        &format!("N in 1..{}1", "+".repeat(129)),
        "expression nesting limit exceeded",
    );
    assert_expression(&format!("1{}", "+1".repeat(10_000)), 10_001);
}

fn parse_integers(line: &str) -> Vec<i64> {
    line.split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

fn dsl(row: &str, constraints: &str) -> String {
    format!("input {{\n{row}\n}}\nconstraints {{\n{constraints}\n}}\n")
}

fn assert_expression(expr: &str, expected: i64) {
    let source = dsl("N", &format!("N in {expr}..={expr}"));
    assert_eq!(
        generate(&source, 42).unwrap(),
        format!("{expected}\n"),
        "{expr}"
    );
}

fn assert_dsl_error(row: &str, constraints: &str, expected: &str) {
    let source = dsl(row, constraints);
    let error = generate(&source, 42).unwrap_err().to_string();
    assert!(
        error.contains(expected),
        "{source}\nexpected {expected:?}, got {error:?}"
    );
}
