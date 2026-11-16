use crate::generate;

#[test]
fn generates_a_scalar() {
    let source = "input {\n    N\n}\nconstraints {\n    N in 7..7\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "7\n");
}

#[test]
fn generates_multiple_variables_on_one_line() {
    let source = "input {\n    N M\n}\nconstraints {\n    M in N..N\n    N in 3..3\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "3 3\n");
}

#[test]
fn generates_an_array_with_literal_length() {
    let source = "input {\n    A[1..3]\n}\nconstraints {\n    A[*] in 5..5\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "5 5 5\n");
}

#[test]
fn uses_a_scalar_as_an_array_length() {
    let source = "input {\n    N A[1..N]\n}\nconstraints {\n    N in 3..3\n    A[*] in N..N\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "3 3 3 3\n");
}

#[test]
fn samples_both_ends_of_a_negative_range() {
    let source = "input {\n    N\n}\nconstraints {\n    N in -3..-2\n}\n";
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
fn reproduces_output_for_a_seed() {
    let source = include_str!("../sample.gen");
    let expected = "4 2\n59 65 51 63\n-75 -38\n";
    assert_eq!(generate(source, 42).unwrap(), expected);
    assert_eq!(generate(source, 42).unwrap(), expected);
    assert_ne!(generate(source, 43).unwrap(), expected);
}

#[test]
fn emits_lf_with_no_extra_spaces_or_lines() {
    let source = "input {\r\n  N M\r\n\r\n  A[1..2]\r\n}\r\n\r\nconstraints {\r\n  N in 1..1\r\n  M in 2..2\r\n  A[*] in 3..3\r\n}";
    assert_eq!(generate(source, 42).unwrap().as_bytes(), b"1 2\n3 3\n");
}

#[test]
fn reports_syntax_errors_with_a_line_and_column() {
    let source = "input {\n  A[1..]\n}\nconstraints {\n}\n";
    assert_eq!(
        generate(source, 42).unwrap_err().to_string(),
        "2:8: expected an integer or scalar variable"
    );
}

#[test]
fn generated_values_meet_their_constraints() {
    let source = "input {\n    N L U\n    A[1..N]\n}\nconstraints {\n    N in 1..10\n    L in -100..-1\n    U in L..100\n    A[*] in L..U\n}\n";
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
        ("A[0..3]", "A[*] in 1..1", "array indices must start at 1"),
        ("A[1..2][1..2]", "A[*] in 1..1", "expected whitespace"),
        ("A[1..2]B", "A[*] in 1..1", "expected whitespace"),
        ("A[1..2]", "A[1] in 1..1", "expected `*`"),
        ("N", "N in 1...2", "expected an integer"),
        ("N", "N in 1..2 sorted", "expected end of line"),
        ("N", "N in 0..9223372036854775808", "outside the i64 range"),
    ] {
        assert_dsl_error(row, rule, error);
    }
}

#[test]
fn rejects_invalid_declarations_and_constraints() {
    for (row, rules, error) in [
        ("N N", "N in 1..1", "duplicate declaration"),
        ("N", "N in 1..1\nN in 2..2", "duplicate constraint"),
        ("N", "", "missing constraint"),
        ("N", "M in 1..1", "undefined variable `M`"),
        ("N", "N in M..1", "undefined variable `M`"),
        ("A[1..N]", "A[*] in 1..1", "undefined variable `N`"),
        ("N M", "N in 1..M\nM in 1..1", "`M` is not available"),
        (
            "A[1..N] N",
            "A[*] in 1..1\nN in 1..1",
            "`N` is not available",
        ),
        ("N", "N in N..1", "`N` is not available"),
        ("A[1..1] N", "A[*] in 1..1\nN in 1..A", "expected a scalar"),
        ("A[1..1]", "A in 1..1", "use `A[*]`"),
        ("N", "N[*] in 1..1", "cannot be indexed"),
    ] {
        assert_dsl_error(row, rules, error);
    }
}

#[test]
fn rejects_invalid_bounds_and_lengths_at_generation() {
    assert_dsl_error("N", "N in 2..1", "invalid range");
    assert_dsl_error("N M", "N in 2..2\nM in N..1", "invalid range");
    assert_dsl_error("A[1..0]", "A[*] in 1..1", "invalid array length 0");
    assert_dsl_error(
        "N A[1..N]",
        "N in -1..-1\nA[*] in 0..0",
        "invalid array length -1",
    );
}

#[test]
fn enforces_integer_and_output_limits() {
    assert_dsl_error(
        "A[1..9223372036854775807]",
        "A[*] in 0..0",
        "integer limit exceeded",
    );
    assert_dsl_error(
        "N A[1..1000000]",
        "N in 1..1\nA[*] in 0..0",
        "integer limit exceeded",
    );
    assert_dsl_error(
        "A[1..500000]",
        "A[*] in -9223372036854775808..-9223372036854775808",
        "output size limit exceeded",
    );
}

#[test]
fn samples_the_full_signed_integer_range() {
    let source =
        "input {\n    N\n}\nconstraints {\n    N in -9223372036854775808..9223372036854775807\n}\n";
    assert_eq!(generate(source, 42).unwrap(), "4456085495900499605\n");
}

#[test]
fn reproduces_sampling_after_rejecting_a_random_word() {
    let source = "input {\n    N\n}\nconstraints {\n    N in -9223372036854775808..0\n}\n";
    assert_eq!(generate(source, 3).unwrap(), "-5528608851982440056\n");
}

fn assert_dsl_error(row: &str, constraints: &str, expected: &str) {
    let source = format!("input {{\n{row}\n}}\nconstraints {{\n{constraints}\n}}\n");
    let error = generate(&source, 42).unwrap_err().to_string();
    assert!(
        error.contains(expected),
        "{source}\nexpected {expected:?}, got {error:?}"
    );
}
