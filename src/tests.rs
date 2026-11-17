use std::collections::BTreeSet;

use crate::generate;

const SAMPLE: &str = include_str!("../sample.gen");

#[test]
fn generates_and_outputs_scalars() {
    let source = dsl("N in 5..=5\nM in (N*2)..=(N*2)", "M N M");
    assert_eq!(generate(&source, 42).unwrap(), "10 5 10\n");
}

#[test]
fn generates_scalars_and_arrays_from_dynamic_ranges() {
    let source = dsl(
        "N in 2..=10\nM in 1..N\nA[(N+M)*2]\n\
         for i in 1..=(N+M)*2 {\n A[i] in -(N*M)..=(N*M)\n}",
        "N M\nA",
    );
    for seed in 0..64 {
        let output = generate(&source, seed).unwrap();
        let mut lines = output.lines();
        let header = parse_integers(lines.next().unwrap());
        let [n, m] = header[..] else {
            panic!("expected two scalars");
        };
        assert!((2..=10).contains(&n));
        assert!((1..n).contains(&m));
        let elements = parse_integers(lines.next().unwrap());
        assert_eq!(elements.len(), usize::try_from((n + m) * 2).unwrap());
        assert!(
            elements
                .iter()
                .all(|value| (-(n * m)..=n * m).contains(value))
        );
        assert!(lines.next().is_none());
    }
}

#[test]
fn initializes_elements_using_the_current_and_previous_iterations() {
    let source = dsl(
        "N in 3..=3\nA[N]\nB[N-1]\nA[1] in 1..=1\n\
         for i in 2..=N {\n A[i] in (A[i-1]+3)..=(A[i-1]+3)\n B[i-1] in (A[i]+1)..=(A[i]+1)\n}\n\
         M in (A[N]+B[N-1])..=(A[N]+B[N-1])",
        "for i in 2..=N {\n A[i] B[i-1]\n}\nA M",
    );
    assert_eq!(generate(&source, 42).unwrap(), "4 5\n7 8\n1 4 7 15\n");
}

#[test]
fn uses_nested_element_references_in_lengths_indices_and_bounds() {
    let source = dsl(
        "A[2]\nA[1] in 1..=1\nA[2] in 5..=5\nB[A[1]]\n\
         for i in A[1]..=A[1] {\n B[A[i]] in A[A[i]+1]..=A[A[i]+1]\n}",
        "for i in A[1]..=A[1] {\n B[A[i]] A[A[i]+1]\n}",
    );
    assert_eq!(generate(&source, 42).unwrap(), "5 5\n");
}

#[test]
fn expands_whole_arrays_in_index_order_and_reuses_values() {
    let source = dsl(
        "N in 2..=2\nA[N]\nA[N] in -6..=-6\nA[1] in 5..=5\nB[1]\nB[1] in 9..=9",
        "N\n\nA B N A[N] A",
    );
    assert_eq!(generate(&source, 42).unwrap(), "2\n\n5 -6 9 2 -6 5 -6\n");
}

#[test]
fn output_layout_does_not_affect_the_generated_values() {
    let (generation, _) = SAMPLE.split_once("input {").unwrap();
    let expected = "3\n83 90\n46 99\n43 72\n";
    assert_eq!(generate(SAMPLE, 42).unwrap(), expected);
    assert_eq!(generate(SAMPLE, 42).unwrap(), expected);
    assert_ne!(generate(SAMPLE, 43).unwrap(), expected);
    let arrays = format!("{generation}input {{\nB\nN\nA A B[1]\n}}\n");
    assert_eq!(
        generate(&arrays, 42).unwrap(),
        "90 99 72\n3\n83 46 43 83 46 43 90\n"
    );
    let unused = format!("{generation}input {{\n}}\n");
    assert_eq!(generate(&unused, 42).unwrap(), "");
}

#[test]
fn preserves_explicit_blank_lines_and_normalizes_spaces_and_newlines() {
    let source = "\r\n generate {\r\n N \t in 0..=0 \r\n\t\r\n A[1]\r\n A[1] in 7..=7\r\n}\r\n\r\n\
                  input {\r\n\t\r\n N \t A [ 1 ] \t A \t\r\n for i in 1..=2 {\r\n \r\n A[1]\r\n\r\n }\r\n}";
    assert_eq!(
        generate(source, 42).unwrap().as_bytes(),
        b"\n0 7 7\n\n7\n\n\n7\n\n"
    );
}

#[test]
fn accepts_contextual_keywords_and_lexical_boundaries() {
    let source = dsl(
        "generate in(-2)..=-2\ninput in generate..=generate\nin in 0003..=3\n\
         for for_ in 1..1 {\n}",
        "generate input in",
    );
    assert_eq!(generate(&source, 42).unwrap(), "-2 -2 3\n");
}

#[test]
fn empty_blocks_loops_and_loop_bodies_are_valid() {
    assert_eq!(generate("generate {\n}\ninput {\n}", 42).unwrap(), "");
    for range in ["N..N", "N..=(N-1)", "(N+1)..N"] {
        let generation = format!(
            "N in 3..=3\nA[1]\nfor i in {range} {{\n A[1/0] in 1..1\n}}\n\
             for i in 1..=2 {{\n}}\nZ in 1..=99"
        );
        let input = format!("Z\nfor i in {range} {{\n A[1/0]\n \n}}\nfor i in 1..=2 {{\n}}");
        let skipped = generate(&dsl(&generation, &input), 42).unwrap();
        let baseline = generate(&dsl("N in 3..=3\nZ in 1..=99", "Z"), 42).unwrap();
        assert_eq!(skipped, baseline);
    }
    assert_error("for i in 1..=1/0 {\n}", "", "2:15: division by zero");
    assert_error("", "for i in 1..=1/0 {\n}", "4:15: division by zero");
}

#[test]
fn loops_have_independent_scopes_in_both_blocks() {
    let source = dsl(
        "A[2]\nfor i in 1..2 {\n A[i] in 2..=2\n}\nfor i in 2..=2 {\n A[i] in 3..=3\n}",
        "for i in A[1]..=A[1] {\n A[i]\n}\nfor i in 1..A[1] {\n A[i]\n}",
    );
    assert_eq!(generate(&source, 42).unwrap(), "3\n2\n");
}

#[test]
fn nested_loops_preserve_generation_order_and_outer_bindings() {
    let generation = "A[6]\nfor i in 1..=3 {\n  for j in 1..=i {\n    \
        A[i*(i-1)/2+j] in (i*10+j)..=(i*10+j+8)\n  }\n}\nZ in A[6]..=A[6]";
    let flat = "A[6]\nA[1] in 11..=19\nA[2] in 21..=29\nA[3] in 22..=30\n\
        A[4] in 31..=39\nA[5] in 32..=40\nA[6] in 33..=41\nZ in A[6]..=A[6]";
    let input = "for i in 1..=3 {\n  for j in 1..=i {\n    A[i*(i-1)/2+j]\n  }\n}\nZ";
    let flat_input = "A[1]\nA[2]\nA[3]\nA[4]\nA[5]\nA[6]\nZ";
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(
            generate(&dsl(generation, input), seed).unwrap(),
            generate(&dsl(flat, flat_input), seed).unwrap(),
        );
    }
}

#[test]
fn nested_loops_can_reuse_sibling_names() {
    let generation = "A[2]\nfor i in 1..=1 {\n  for j in i..=i {\n    A[j] in 7..=7\n  }\n\
        for j in (A[i]-5)..=2 {\n    A[j] in A[i]..=A[i]\n  }\n}";
    let input = "for i in 1..=1 {\n  for j in i..=i {\n    A[j]\n  }\n\
        for j in 2..=(i+1) {\n    A[j] Z\n  }\n}";
    let source = dsl(&format!("{generation}\nZ in A[2]..=A[2]"), input);
    assert_eq!(generate(&source, 42).unwrap(), "7\n7 7\n");
}

#[test]
fn nested_loops_check_scopes_and_declarations_in_empty_paths() {
    for (generation, input, expected) in [
        (
            "for i in 1..1 {\n for i in 1..1 {\n }\n}",
            "",
            "3:6: name collision for `i`",
        ),
        (
            "",
            "for i in 1..1 {\n row {\n  for i in 1..1 {\n  }\n }\n}",
            "6:7: name collision for `i`",
        ),
        (
            "for i in 1..1 {\n for j in j..=i {\n }\n}",
            "",
            "3:11: loop variable `j` is outside",
        ),
        (
            "",
            "for i in 1..1 {\n for j in 1..1 {\n }\n j\n}",
            "7:2: loop variable `j` is outside",
        ),
        (
            "for i in 1..1 {\n for j in 1..1 {\n  A[1]\n }\n}",
            "",
            "4:3: only array element initialization",
        ),
        (
            "",
            "row {\n for i in 1..1 {\n  M\n }\n}",
            "6:3: undefined variable `M`",
        ),
    ] {
        assert_error(generation, input, expected);
    }
}

#[test]
fn row_appends_references_and_whole_arrays_across_nested_loops() {
    let generation = "row[2]\nrow[1] in -1..=-1\nrow[2] in 2..=2\nN in 7..=7";
    let input = "row[2]\nrow row[2]\nrow{\n row[1] N\n \t\n\
        for i in 1..=2 {\n  for j in i..=i {\n   row[j] N\n  }\n }\n row\n}";
    assert_eq!(
        generate(&dsl(generation, input), 42).unwrap(),
        "2\n-1 2 2\n-1 7 -1 7 2 7 -1 2\n"
    );
}

#[test]
fn empty_rows_and_empty_loops_keep_distinct_line_boundaries() {
    let input = "row {\n}\nrow {\n \t\n for i in 1..1 {\n  A[1/0]\n \n }\n}\n\n\
        for i in 1..1 {\n row {\n }\n}\nfor i in 1..=1 {\n\n  for j in i..=i {\n   A[j]\n  }\n}";
    assert_eq!(
        generate(&dsl("A[1]\nA[1] in 4..=4", input), 42).unwrap(),
        "\n\n\n\n4\n"
    );
}

#[test]
fn enforces_control_nesting_depth_in_both_blocks() {
    let nested = |depth: usize, body: &str| {
        let headers: String = (0..depth)
            .map(|i| format!("for i{i} in 1..=1 {{\n"))
            .collect();
        format!("{headers}{body}{}", "}\n".repeat(depth))
            .trim_end_matches('\n')
            .to_owned()
    };
    let generation = format!("A[1]\n{}", nested(32, "A[1] in (i0+i31)..=(i0+i31)\n"));
    let input = nested(31, "row {\n A[i0*i30]\n}\n");
    assert_eq!(generate(&dsl(&generation, &input), 42).unwrap(), "2\n");
    assert_error(
        &nested(33, ""),
        "",
        "34:1: control nesting limit exceeded (32)",
    );
    assert_error(
        "",
        &nested(32, "row {\n}\n"),
        "36:1: control nesting limit exceeded (32)",
    );
}

#[test]
fn counts_execution_work_per_extended_block() {
    let near_limit = "for i in 1..=10 {\n for j in 1..=999997 {\n }\n}";
    let exact = format!("{near_limit}\nfor k in 1..=8 {{\n}}");
    let exact_row = format!("row {{\n{near_limit}\n\nfor k in 1..=7 {{\n}}\n}}");
    assert_eq!(generate(&dsl(&exact, &exact_row), 42).unwrap(), "\n");

    let over = format!("{near_limit}\nfor k in 1..=9 {{\n}}");
    assert_error(&over, "", "6:14: execution work limit exceeded (10000000)");
    assert_error("", &over, "8:14: execution work limit exceeded (10000000)");
    assert_error(
        "",
        &format!("{exact_row}\nrow {{\n}}"),
        "13:1: execution work limit exceeded (10000000)",
    );

    let legacy = "for i in 1..=1000000 {\n}\n".repeat(11);
    assert_eq!(generate(&dsl(&legacy, &exact_row), 42).unwrap(), "\n");
    assert_eq!(
        generate(&dsl(&exact, legacy.trim_end_matches('\n')), 42).unwrap(),
        ""
    );
    assert_error(
        &format!("{legacy}\nfor i in 1..1 {{\n for j in 1..1 {{\n }}\n}}"),
        "",
        "execution work limit exceeded (10000000)",
    );
}

#[test]
fn checks_element_state_and_bounds_in_nested_execution() {
    assert_error(
        "A[2]\nfor i in 1..=2 {\n for j in 1..=i {\n  A[j] in 1..=1\n }\n}",
        "",
        "5:3: array element `A[1]` is already initialized",
    );
    assert_error(
        "A[1]\nfor i in 1..=2 {\n for j in i..=i {\n  A[j] in 1..=1\n }\n}",
        "",
        "5:5: index 2 is out of bounds",
    );
    assert_error(
        "A[2]\nA[1] in 1..=1",
        "row {\n for i in 1..=2 {\n  A[i]\n }\n}",
        "8:3: array element `A[2]` is uninitialized",
    );
}

#[test]
fn checks_static_errors_before_generation_even_in_empty_loops() {
    for (generation, input, expected) in [
        ("N in N..=1", "", "2:6: `N` is not available"),
        ("A[N]\nN in 1..=1", "", "2:3: `N` is not available"),
        ("A[A[1]]", "", "2:3: `A` is not available"),
        ("A[1] in 1..=1\nA[1]", "", "2:1: `A` is not available"),
        (
            "N in 1..1\nA[N] in 1..=1",
            "",
            "3:1: undefined variable `A`",
        ),
        ("N in 1..1", "X", "5:1: undefined variable `X`"),
        ("A[1]\nN in (2+A)..=3", "", "3:9: `A` is an array"),
        ("N in 1..=1\nN[1] in 1..=1", "", "3:1: `N` is a scalar"),
        ("N in 1..=1\nM in N[1]..=1", "", "3:6: `N` is a scalar"),
        ("N in 1..=1", "N[1]", "5:1: `N` is a scalar"),
        ("A[1]\nA in 1..=1", "", "3:1: duplicate declaration of `A`"),
        (
            "A[1]\nfor A in 1..1 {\n}",
            "",
            "3:5: name collision for `A`",
        ),
        (
            "for i in 1..1 {\n}\ni[1]",
            "",
            "4:1: name collision for `i`",
        ),
        (
            "i in 1..=1",
            "for i in 1..1 {\n}",
            "5:5: name collision for `i`",
        ),
        (
            "for i in i..=2 {\n}",
            "",
            "2:10: loop variable `i` is outside",
        ),
        (
            "for i in 1..1 {\n}\nN in i..=1",
            "",
            "4:6: loop variable `i` is outside",
        ),
        (
            "for i in 1..1 {\n i[1] in 1..=1\n}",
            "",
            "3:2: loop variable `i` cannot be indexed",
        ),
        (
            "for i in 1..1 {\n A[1] in 1..=1\n}",
            "",
            "3:2: undefined variable `A`",
        ),
        (
            "A[1]\nfor i in 1..1 {\n A[i] in (1+X)..=1\n}",
            "",
            "4:13: undefined variable `X`",
        ),
        (
            "A[1]",
            "for i in 1..1 {\n i\n}",
            "6:2: loop variable `i` cannot be output directly",
        ),
        (
            "A[1]",
            "for i in 1..1 {\n A[X]\n}",
            "6:4: undefined variable `X`",
        ),
        (
            "A[1]",
            "for i in 1..1 {\n A[i[1]]\n}",
            "6:4: loop variable `i` cannot be indexed",
        ),
        (
            "A[1]",
            "for i in 1..1 {\n}\nfor j in 1..1 {\n A[i]\n}",
            "8:4: loop variable `i` is outside",
        ),
    ] {
        assert_error(generation, input, expected);
    }
}

#[test]
fn rejects_old_syntax_and_unsupported_operations() {
    for (source, expected) in [
        (
            "input {\nN\n}\nconstraints {\nN in 1..=1\n}",
            "1:1: expected `generate`",
        ),
        ("input {\n}\ngenerate {\n}", "1:1: expected `generate`"),
        ("generate {\n}\n", "3:1: expected `input {`"),
        (
            "generate {\n}\ngenerate {\n}\ninput {\n}",
            "3:1: expected `input`",
        ),
        (
            "generate {\n}\ninput {\n}\ninput {\n}",
            "5:1: unexpected text",
        ),
        (
            "generate {\n}\ninput {\n}\nconstraints {\n}",
            "5:1: unexpected text",
        ),
        ("generate {\n}\ninput {", "3:8: expected an output line"),
    ] {
        assert_source_error(source, expected);
    }
    for (generation, input, expected) in [
        ("N in 1..=1", "N in 1..=1", "expected an identifier"),
        ("A[1]", "A[1] in 1..=1", "expected an identifier"),
        ("A[1]", "A[*]", "expected an integer"),
        ("A[1..2]", "", "2:4: expected `]`"),
        ("A[1]", "A[1]A[1]", "5:5: expected whitespace"),
        ("", "row {\n row {\n }\n}", "5:2: nested `row`"),
        (
            "",
            "row {\n for i in 1..1 {\n  row {\n  }\n }\n}",
            "6:3: nested `row`",
        ),
        ("row {\n}", "", "2:5: expected an identifier"),
        (
            "for i in 1..1 {\n N in 1..=1\n}",
            "",
            "3:2: only array element initialization",
        ),
        (
            "for i in 1..1 {\n A[1]\n}",
            "",
            "3:2: only array element initialization",
        ),
        ("N in 1..=1 sorted", "", "expected end of line"),
    ] {
        assert_error(generation, input, expected);
    }
}

#[test]
fn rejects_invalid_lexemes_and_reports_source_byte_columns() {
    for (rule, expected) in [
        ("N in 1...2", "2:7: invalid range operator"),
        ("N in 1..==2", "2:7: invalid range operator"),
        ("N in 1.. =2", "2:10: expected an integer"),
        ("N in 1 2", "2:8: expected range operator"),
        ("N in1..=2", "2:3: expected `in`"),
        ("N in 1..(2+3", "expected `)`"),
        ("A[(2+3]", "expected `)`"),
        ("N in 1..(2+3))", "expected end of line"),
        ("N in 1..2^3", "2:10: expected end of line"),
        ("N in 1..2**3", "2:11: expected an integer"),
        ("N in 1..\n2", "2:9: expected an integer"),
        (
            "N in 0..9223372036854775808",
            "2:9: integer literal is outside",
        ),
        (
            "N in -9223372036854775809..0",
            "2:6: integer literal is outside",
        ),
        ("\tN in 1..=1　", "2:12: expected end of line"),
        (" # comment", "2:2: expected an identifier"),
    ] {
        assert_error(rule, "", expected);
    }
    assert_source_error(
        "\u{feff}generate {\n}\ninput {\n}",
        "1:1: expected an identifier",
    );
    assert_source_error("generate {\n}\ninput {\n}\r", "4:2: expected end of line");
    assert_source_error("generate {\r}\ninput {\n}", "1:11: expected end of line");
}

#[test]
fn diagnoses_empty_generation_ranges_and_nonpositive_array_lengths() {
    for (range, column) in [("1..1", 9), ("2..1", 9), ("6..=5", 10)] {
        assert_error(
            &format!("N in {range}"),
            "",
            &format!("2:{column}: invalid range"),
        );
    }
    assert_error(
        "N in 2..=2\n    M in (1+N)..(N-1)",
        "",
        "3:17: invalid range `(1+N)..(N-1)`: empty with lower bound 3 and upper bound 1",
    );
    assert_error("A[0]", "", "2:3: invalid array length 0");
    assert_error("N in -1..=-1\nA[N]", "", "3:3: invalid array length -1");
    assert_error(
        "A[3]\nfor i in 1..=3 {\n A[i] in i..3\n}",
        "A",
        "4:13: invalid range `i..3`: empty with lower bound 3 and upper bound 3",
    );
}

#[test]
fn permits_partial_initialization_when_unread_elements_are_unused() {
    let generation = "K in 1..=2\nA[2]\nA[K] in 7..=7";
    for seed in 0..16 {
        let output = generate(&dsl(generation, "K A[K] A[K]"), seed).unwrap();
        assert!(output == "1 7 7\n" || output == "2 7 7\n");
        let other = generate(&dsl(generation, "A[1]"), seed);
        if output.starts_with('1') {
            assert_eq!(other.unwrap(), "7\n");
        } else {
            assert!(
                other
                    .unwrap_err()
                    .to_string()
                    .contains("A[1]` is uninitialized")
            );
        }
        let whole = generate(&dsl(generation, "A"), seed)
            .unwrap_err()
            .to_string();
        let missing = if output.starts_with('1') { 2 } else { 1 };
        assert_eq!(
            whole,
            format!("7:1: array element `A[{missing}]` is uninitialized")
        );
    }
}

#[test]
fn reports_uninitialized_elements_at_the_array_name() {
    assert_error(
        "A[1]\nN in A[1]..=1",
        "",
        "3:6: array element `A[1]` is uninitialized",
    );
    assert_error(
        "A[1]\nB[A[1]]",
        "",
        "3:3: array element `A[1]` is uninitialized",
    );
    assert_error(
        "A[3]\nA[1] in 5..=5",
        " \tA[3]",
        "6:3: array element `A[3]` is uninitialized",
    );
    assert_error(
        "A[3]\nA[1] in 5..=5",
        " A",
        "6:2: array element `A[2]` is uninitialized",
    );
    assert_error(
        "A[1]",
        "for i in A[1]..=0 {\n}",
        "5:10: array element `A[1]` is uninitialized",
    );
}

#[test]
fn checks_initialization_targets_before_evaluating_the_range() {
    assert_error(
        "A[2]\nA[1] in 5..=5\n A[2-1] in 1/0..=9",
        "",
        "4:2: array element `A[1]` is already initialized",
    );
    assert_error(
        "A[1]\nfor i in 1..=1 {\n A[i] in 1..=1\n}\nfor i in 1..=1 {\n A[i] in 2..=2\n}",
        "",
        "7:2: array element `A[1]` is already initialized",
    );
    assert_error(
        "A[1]\nA[ (2) ] in 1/0..=9",
        "",
        "3:4: index 2 is out of bounds",
    );
    assert_error(
        "A[1]\nA[1] in A[1]..=9",
        "",
        "3:9: array element `A[1]` is uninitialized",
    );
}

#[test]
fn checks_indices_in_all_contexts_without_overflow() {
    assert_error("A[1]\nN in A[0]..=1", "", "3:8: index 0 is out of bounds");
    assert_error(
        "A[1]",
        "A[-9223372036854775808]",
        "5:3: index -9223372036854775808 is out of bounds",
    );
    assert_error(
        "A[1]",
        "A[9223372036854775807]",
        "5:3: index 9223372036854775807 is out of bounds",
    );
    assert_error(
        "A[1]",
        "A[1+(6/0)]",
        "5:7: division by zero in expression `1+(6/0)`",
    );
}

#[test]
fn evaluates_checked_arithmetic_with_precedence_and_left_associativity() {
    for (expr, value) in [
        ("5+8", 13),
        ("5-8", -3),
        ("3*4", 12),
        ("7/3", 2),
        ("-7/3", -2),
        ("7/-3", -2),
        ("-7%3", -1),
        ("7%-3", 1),
        ("2 + 3*4 - 8/2%3", 13),
        ("20-6-3", 11),
        ("24/3*2%5", 1),
        ("(2 + 3) * 4", 20),
        ("7 / (3 + 1)", 1),
        ("20-(6-3)", 17),
        ("-(2 + 3)", -5),
        ("+-(-3)", 3),
        ("-+(2)*3", -6),
        ("7+-3", 4),
        ("- 9223372036854775808", i64::MIN),
    ] {
        assert_expression(expr, value);
    }
    for expr in [
        "9223372036854775807+1-1",
        "-9223372036854775808-1",
        "9223372036854775807*2",
        "-9223372036854775808/-1",
        "-9223372036854775808%-1",
        "--9223372036854775808",
    ] {
        assert_error(&format!("N in {expr}..0"), "", "integer overflow");
    }
    assert_error(
        "N in 2..=2\n A[(12/(N-2))]",
        "",
        "3:7: division by zero in expression `(12/(N-2))`",
    );
    assert_error(
        "N in 0..10%0",
        "",
        "2:11: remainder by zero in expression `10%0`",
    );
    assert_error(
        "N in 0..=(9223372036854775807+1)",
        "",
        "integer overflow in expression `(9223372036854775807+1)`",
    );
}

#[test]
fn evaluates_bounds_and_binary_operands_left_to_right() {
    assert_error("N in 1/0..=0%0", "", "2:7: division by zero");
    assert_error(
        "A[1]\nN in A[1]+1/0..=0",
        "",
        "3:6: array element `A[1]` is uninitialized",
    );
    assert_error("", "for i in 1/0..=0%0 {\n}", "4:11: division by zero");
}

#[test]
fn counts_mixed_expression_depth_across_subscripts() {
    fn nested(parentheses: usize) -> String {
        format!(
            "{}{}A[1]{}",
            "(".repeat(parentheses),
            "+".repeat(63),
            ")".repeat(parentheses)
        )
    }

    let generation = "A[1]\nA[1] in 1..=1\nB[1]";
    let safe = nested(63);
    let deep = nested(64);
    let accepted_lines =
        format!("{generation}\nB[{safe}] in 7..=7\nC[{deep}]\nN in B[{safe}]..=B[{safe}]");
    assert_eq!(
        generate(&dsl(&accepted_lines, &format!("B[{safe}] N")), 42).unwrap(),
        "7 7\n"
    );
    let error_column = deep.find("A[1]").unwrap() + 5;
    assert_error(
        generation,
        &format!("B[{deep}]"),
        &format!("7:{error_column}: expression nesting limit exceeded"),
    );
    assert_error(
        &format!("{generation}\nB[{deep}] in 7..=7"),
        "",
        &format!("5:{error_column}: expression nesting limit exceeded"),
    );
    assert_error(
        &format!("{generation}\nN in B[{deep}]..=1"),
        "",
        &format!("5:{}: expression nesting limit exceeded", error_column + 5),
    );
    assert_error(
        &format!("{generation}\nC[{}]", nested(65)),
        "",
        &format!("5:{}: expression nesting limit exceeded", error_column + 1),
    );
}

#[test]
fn limits_nesting_but_handles_long_operator_chains() {
    let nested = format!("{}1{}", "(".repeat(128), ")".repeat(128));
    assert_expression(&nested, 1);
    assert_error(
        &format!("N in 1..({nested})"),
        "",
        "expression nesting limit exceeded",
    );
    assert_error(
        &format!("N in 1..{}1", "+".repeat(129)),
        "",
        "expression nesting limit exceeded",
    );
    assert_expression(&format!("1{}", "+1".repeat(10_000)), 10_001);
}

#[test]
fn samples_inclusive_and_exclusive_endpoints() {
    for (range, first, last) in [("1..10", 1, 9), ("1..=10", 1, 10), ("-3..=-2", -3, -2)] {
        let source = dsl(
            &format!("A[256]\nfor i in 1..=256 {{\n A[i] in {range}\n}}"),
            "A",
        );
        let sampled: BTreeSet<_> = parse_integers(&generate(&source, 42).unwrap())
            .into_iter()
            .collect();
        assert_eq!(sampled, (first..=last).collect(), "{range}");
    }
}

#[test]
fn preserves_splitmix64_sampling_including_rejection_and_width_one() {
    let min = i64::MIN;
    let max = i64::MAX;
    let full = dsl(&format!("N in {min}..={max}"), "N");
    assert_eq!(generate(&full, 42).unwrap(), "4456085495900499605\n");
    let rejected = dsl(&format!("N in {min}..=0"), "N");
    assert_eq!(generate(&rejected, 3).unwrap(), "-5528608851982440056\n");
    let after_one = dsl(
        &format!("A[2]\nA[2] in -1..=-1\nB[1]\nA[1] in {min}..={max}"),
        "A",
    );
    assert_eq!(
        generate(&after_one, 42).unwrap(),
        "-6273545944727883517 -1\n"
    );
    for (range, value) in [
        (format!("{min}..={min}"), min),
        (format!("{min}..{}", min + 1), min),
        (format!("{max}..={max}"), max),
        (format!("{}..{max}", max - 1), max - 1),
    ] {
        assert_eq!(
            generate(&dsl(&format!("N in {range}"), "N"), 42).unwrap(),
            format!("{value}\n")
        );
    }
}

#[test]
fn iterates_safely_at_the_signed_boundaries() {
    let min = i64::MIN;
    let max = i64::MAX;
    for (start, range, count, expected) in [
        (
            min,
            format!("{min}..{}", min + 2),
            2,
            format!("{min}\n{}\n", min + 1),
        ),
        (
            max - 1,
            format!("{}..={max}", max - 1),
            2,
            format!("{}\n{max}\n", max - 1),
        ),
        (max, format!("{max}..={max}"), 1, format!("{max}\n")),
        (max, format!("{max}..={min}"), 1, String::new()),
    ] {
        let index = format!("i-({start})+1");
        let generation = format!("A[{count}]\nfor i in {range} {{\n A[{index}] in i..=i\n}}");
        let input = format!("for i in {range} {{\n A[{index}]\n}}");
        assert_eq!(generate(&dsl(&generation, &input), 42).unwrap(), expected);
    }
    let range = format!("for i in {min}..={max} {{\n}}");
    assert_error(&range, "", "has 18446744073709551616 iterations");
    assert_error("", &range, "has 18446744073709551616 iterations");
}

#[test]
fn enforces_per_array_and_total_capacity_at_declaration() {
    for generation in ["A[1000000]", "A[999999]\nB[1]"] {
        assert_eq!(generate(&dsl(generation, ""), 42).unwrap(), "");
    }
    for length in ["1000001", "9223372036854775807"] {
        assert_error(
            &format!("A[{length}]"),
            "",
            "2:3: array length limit exceeded (1000000)",
        );
    }
    assert_error(
        "A[1000000]\nB[1]",
        "",
        "3:3: total array capacity limit exceeded (1000000)",
    );
}

#[test]
fn counts_generated_integers_separately_from_capacity_and_output() {
    let source =
        |count| format!("N in 0..=0\nA[1000000]\nfor i in 1..={count} {{\n  A[i] in 2..=2\n}}");
    assert_eq!(
        generate(&dsl(&source(999_999), "A[999999]"), 42).unwrap(),
        "2\n"
    );
    assert_error(
        &source(1_000_000),
        "",
        "5:3: generated integer limit exceeded (1000000)",
    );
}

#[test]
fn counts_output_integers_per_reference_and_on_every_reuse() {
    let generation = "A[2]\nA[1] in 0..=0\nA[2] in 1..=1";
    let input = "for i in 1..=500000 {\n A\n}";
    let output = generate(&dsl(generation, input), 42).unwrap();
    assert_eq!(output.len(), 2_000_000);
    assert!(output.as_bytes().chunks_exact(4).all(|row| row == b"0 1\n"));
    assert_error(
        generation,
        &format!("{input}\n  A[1]"),
        "10:3: output integer limit exceeded (1000000)",
    );
    let input = "for i in 1..=499999 {\n A\n}\n  A[2] A";
    assert_error(
        generation,
        input,
        "10:8: output integer limit exceeded (1000000)",
    );
    assert_error(
        generation,
        "row {\n for i in 1..=500000 {\n  A\n }\n A[1]\n}",
        "11:2: output integer limit exceeded (1000000)",
    );
}

#[test]
fn enforces_loop_iteration_limits_even_for_empty_bodies() {
    let exact = "for i in 1..=1000000 {\n}";
    assert_eq!(generate(&dsl(exact, exact), 42).unwrap(), "");
    let too_many = "for i in 1..=1000001 {\n}";
    assert_error(
        too_many,
        "",
        "2:14: loop iteration limit exceeded (1000000)",
    );
    assert_error(
        "",
        too_many,
        "4:14: loop iteration limit exceeded (1000000)",
    );
}

#[test]
fn enforces_output_byte_limit_at_references_and_line_endings() {
    let generation = "N in -9223372036854775808..=-9223372036854775808\n\
                      A in -12..=-12\nB in 123456..=123456\n\
                      C in 12345678901..=12345678901\nD in 123456789012..=123456789012";
    let input = "for i in 1..=399457 {\n N\n}";
    let exact = generate(&dsl(generation, &format!("{input}\n  A B")), 42).unwrap();
    assert_eq!(exact.len(), 8 * 1024 * 1024);
    assert!(exact.ends_with("-12 123456\n"));
    assert_error(
        generation,
        &format!("{input}\n  C"),
        "12:1: output size limit exceeded (8388608 bytes)",
    );
    assert_error(
        generation,
        &format!("{input}\n  D"),
        "12:3: output size limit exceeded (8388608 bytes)",
    );
    assert_error(
        generation,
        &format!("  row {{\n{input}\n C\n}}"),
        "9:3: output size limit exceeded (8388608 bytes)",
    );

    let blank_rows = format!("for i in 1..=1000000 {{\n{}}}", "\n".repeat(9));
    assert_error(
        "",
        &blank_rows,
        "10:1: output size limit exceeded (8388608 bytes)",
    );
}

#[test]
fn compares_integers_with_all_six_operators_in_both_blocks() {
    for (operator, compare) in [
        ("==", (|a, b| a == b) as fn(i64, i64) -> bool),
        ("!=", |a, b| a != b),
        ("<", |a, b| a < b),
        ("<=", |a, b| a <= b),
        (">", |a, b| a > b),
        (">=", |a, b| a >= b),
    ] {
        for (left, right) in [(1, 2), (2, 2), (3, 2), (i64::MIN, i64::MAX)] {
            let generation = format!(
                "L in {left}..={left}\nR[1]\nR[1] in {right}..={right}\nA[1]\n\
                 if L{operator}R[1] {{\n A[1] in 7..=7\n}} else {{\n A[1] in 9..=9\n}}"
            );
            let input = format!("if L {operator} R[1] {{\n A\n}} else {{\n A[1] A[1]\n}}");
            let expected = if compare(left, right) { "7\n" } else { "9 9\n" };
            assert_eq!(generate(&dsl(&generation, &input), 42).unwrap(), expected);
        }
    }
}

#[test]
fn permits_optional_else_empty_branches_and_contextual_names() {
    let generation = "if in 1..=1\nrow in 2..=2\nelse[1]\nelse[1] in 3..=3\n\
        if_ in 4..=4\nA[1]\nif if == 1 {\n A[1] in 8..=8\n}\n\
        if if != 1 {\n A[1] in 1..=1\n}\nif 0 == 1 {\n} else {\n}\n\
        for if_loop in 1..1 {\n}\nfor else in 1..1 {\n}";
    // `else` above collides with an array even in an empty loop.
    assert_error(generation, "", "name collision for `else`");
    let generation = generation.replace("for else in 1..1 {\n}", "");
    let input = "if row else\nif_\nif if == 1{\n A[1]\n}\n\
        if if != 1 {\n A[1/0]\n\n}\nif 0 == 1 {\n} else {\n}\n\
        row {\n if if == 0 {\n  A[1/0]\n }\n}\n\
        if 1 == 1 {\n\n} else {\n\n}\nelse[1]";
    assert_eq!(
        generate(&dsl(&generation, input).replace('\n', "\r\n"), 42).unwrap(),
        "1 2 3\n4\n8\n\n\n3\n"
    );
    let source = dsl(
        "if[1]\nif[1] in 2..=2\nelse in 5..=5",
        "if[1] else\nif if[1] == 2 {\n else\n}",
    );
    assert_eq!(generate(&source, 42).unwrap(), "2 5\n5\n");
    assert_eq!(
        generate(&dsl("", "for if in 1..=1 {\n if if == 1 {\n }\n}"), 42).unwrap(),
        ""
    );
}

#[test]
fn nests_if_and_for_and_preserves_the_current_row() {
    let generation = "A[4]\nfor i in 1..=2 {\n if i == 1 {\n\
        for j in 1..=2 {\n A[j] in (i+j)..=(i+j)\n }\n} else {\n\
        if i*2 == 4 {\n for j in 3..=4 {\n A[j] in j..=j\n }\n }\n}\n}";
    let input = "if A[1] == 2 {\n for i in 1..=2 {\n row {\n A[i]\n\
        if i == 1 {\n\n for j in 3..=4 {\n if A[j] >= 3 {\n A[j]\n }\n }\n\
        } else {\n if i == A[1] {\n A\n }\n }\n A[4]\n }\n }\n}";
    assert_eq!(
        generate(&dsl(generation, input), 42).unwrap(),
        "2 3 4 4\n3 2 3 3 4 4\n"
    );
}

#[test]
fn checks_static_names_types_and_scopes_in_unselected_branches() {
    for (generation, input, expected) in [
        (
            "A[1]\nif 0 == 1 {\n Missing[1] in 1..=1\n}",
            "",
            "undefined variable `Missing`",
        ),
        (
            "N in 1..=1\nif 1 == 1 {\n} else {\n N[1] in 1..=1\n}",
            "",
            "`N` is a scalar; expected an array",
        ),
        (
            "A[1]\nif 0 == 1 {\n A[1] in Later..=Later\n}\nLater in 1..=1",
            "",
            "`Later` is not available",
        ),
        (
            "A[1]\nif 0 == 1 {\n if A == 1 {\n }\n}",
            "",
            "`A` is an array; expected a scalar",
        ),
        (
            "",
            "if 0 == 1 {\n Missing\n}",
            "undefined variable `Missing`",
        ),
        (
            "N in 1..=1",
            "if 1 == 1 {\n} else {\n N[1]\n}",
            "`N` is a scalar; expected an array",
        ),
        (
            "A[1]",
            "row {\n if 0 == 1 {\n  if A == 0 {\n  }\n }\n}",
            "`A` is an array; expected a scalar",
        ),
        (
            "",
            "if 0 == 1 {\n for i in 1..1 {\n  i\n }\n}",
            "loop variable `i` cannot be output directly",
        ),
        (
            "",
            "for i in 1..=1 {\n if 0 == 1 {\n  for i in 1..1 {\n  }\n }\n}",
            "name collision for `i`",
        ),
        (
            "",
            "if 0 == 1 {\n for i in 1..1 {\n }\n} else {\n if i == 1 {\n }\n}",
            "loop variable `i` is outside this scope",
        ),
        (
            "",
            "if 0 == 1 {\n for i in 1..1 {\n }\n}\nif i == 1 {\n}",
            "loop variable `i` is outside this scope",
        ),
        (
            "N in 1..=1",
            "if Missing == N {\n}",
            "undefined variable `Missing`",
        ),
    ] {
        assert_error(generation, input, expected);
    }
    // Loops in separate branches have independent scopes.
    let source = dsl(
        "if 1 == 1 {\n for i in 1..1 {\n }\n} else {\n for i in 1..1 {\n }\n}",
        "if 0 == 1 {\n for i in 1..1 {\n }\n} else {\n for i in 1..1 {\n }\n}",
    );
    assert_eq!(generate(&source, 42).unwrap(), "");
}

#[test]
fn rejects_declarations_and_unsupported_conditional_syntax() {
    for declaration in ["N in 1..=1", "A[1]"] {
        for (open, close) in [("if 0 == 1 {", "}"), ("if 1 == 1 {\n} else {", "}")] {
            assert_error(
                &format!("{open}\n {declaration}\n{close}"),
                "",
                "only array element initialization",
            );
        }
    }
    for condition in [
        "1",
        "1 = 1",
        "1 === 1",
        "1 < = 2",
        "1 < 2 < 3",
        "1 == 1 == 1",
        "1 == 1 && 2 == 2",
        "1 == 1 || 2 == 2",
        "!1 == 1",
        "(1 == 1)",
        "'a' == 'a'",
    ] {
        let block = format!("if {condition} {{\n}}");
        assert!(generate(&dsl(&block, ""), 42).is_err(), "{condition}");
        assert!(generate(&dsl("", &block), 42).is_err(), "{condition}");
    }
    for block in [
        "else {\n}",
        "if 1 == 1 {\n}\nelse {\n}",
        "if 1 == 1 {\n} else if 1 == 1 {\n}",
        "if 1 == 1 {\n} else {\n} else {\n}",
        "for i in 1..1 {\n} else {\n}",
        "if 0 == 1 {\n for i in 1..1 {\n } else {\n }\n}",
    ] {
        assert!(generate(&dsl(block, ""), 42).is_err(), "{block}");
        assert!(generate(&dsl("", block), 42).is_err(), "{block}");
    }
    assert_error(
        "",
        "row {\n if 0 == 1 {\n  row {\n  }\n }\n}",
        "nested `row` is not allowed",
    );
}

#[test]
fn skips_runtime_evaluation_and_random_draws_in_unselected_branches() {
    let bad_generation = "A[2] in 1..=1\nA[1] in 2..=2\nA[1/0] in 1..1\n\
        A[1] in A[2]..=A[2]\nfor i in 1..=1000001 {\n}\n\
        if 9223372036854775807+1 == 0 {\n}";
    let bad_output = "A[2]\nA[1/0]\nfor i in A[2]..=1000001 {\n A\n}\n\
        if 1%0 == 1 {\n}\nrow {\n A\n}";
    let straight = dsl("A[1]\nA[1] in 1..=99\nZ in 1..=9999", "A Z");
    for (open, middle) in [
        ("if 0 == 1 {", "} else {"),
        ("if 1 == 1 {\n} else {", "}\nif 1 == 1 {"),
    ] {
        let generation =
            format!("A[1]\n{open}\n{bad_generation}\n{middle}\n A[1] in 1..=99\n}}\nZ in 1..=9999");
        let input = format!("{open}\n{bad_output}\n{middle}\n A Z\n}}");
        let branched = dsl(&generation, &input);
        for seed in [0, 1, 42, 100, u64::MAX] {
            let expected = generate(&straight, seed).unwrap();
            assert_eq!(generate(&branched, seed).unwrap(), expected);
            assert_eq!(generate(&branched, seed).unwrap(), expected);
        }
    }
    // The other branch may contain arbitrarily many draws, without consuming any.
    let source = dsl(
        "A[1]\nif 1 == 1 {\n A[1] in 1..=99\n} else {\n\
         for i in 1..=1000000 {\n A[1] in 0..=0\n }\n}\nZ in 1..=9999",
        "A Z",
    );
    assert_eq!(
        generate(&source, 42).unwrap(),
        generate(&straight, 42).unwrap()
    );
}

#[test]
fn evaluates_condition_operands_left_to_right_in_both_blocks() {
    for (condition, expected) in [
        ("1/0 == A[1]", "division by zero"),
        ("A[1] == 1/0", "array element `A[1]` is uninitialized"),
        ("1 == 1/0", "division by zero"),
        ("A[2] < 1%0", "index 2 is out of bounds"),
        ("9223372036854775807+1 > 0", "integer overflow"),
    ] {
        let block = format!("if {condition} {{\n}}");
        assert_error(&format!("A[1]\n{block}"), "", expected);
        assert_error("A[1]", &block, expected);
        assert_error("A[1]", &format!("row {{\n{block}\n}}"), expected);
    }
    assert_error("A[1]\nif 1/0 == 0 {\n}", "", "3:5: division by zero");
    assert_error("A[1]", "if 1 == 1/0 {\n}", "5:10: division by zero");
}

#[test]
fn tracks_actual_partial_initialization_without_matching_branch_inference() {
    let generation = "K in 1..=2\nA[2]\nif K == 1 {\n A[1] in 7..=7\n} else {\n A[2] in 9..=9\n}";
    let source = dsl(generation, "if K == 1 {\n A[1]\n} else {\n A[2]\n}");
    let mut values = BTreeSet::new();
    for seed in 0..32 {
        values.insert(generate(&source, seed).unwrap());
        let unmatched = dsl(generation, "if K != 1 {\n A[1]\n} else {\n A[2]\n}");
        assert!(
            generate(&unmatched, seed)
                .unwrap_err()
                .to_string()
                .contains("uninitialized")
        );
        let whole = dsl(generation, "A");
        assert!(
            generate(&whole, seed)
                .unwrap_err()
                .to_string()
                .contains("uninitialized")
        );
    }
    assert_eq!(values, BTreeSet::from(["7\n".to_owned(), "9\n".to_owned()]));
    assert_error(
        "A[1]\nif 0 == 1 {\n A[1] in 1..=1\n}",
        "A[1]",
        "array element `A[1]` is uninitialized",
    );
    for selected in ["1 == 1", "0 == 1"] {
        assert_error(
            &format!(
                "A[1]\nA[1] in 1..=1\nif {selected} {{\n A[1] in 2..=2\n}} else {{\n A[1] in 3..=3\n}}"
            ),
            "",
            "array element `A[1]` is already initialized",
        );
    }
    let source = dsl(
        "A[1]\nif 1 == 1 {\n A[1] in 5..=5\n}\nN in A[1]..=A[1]",
        "if N == A[1] {\n A N\n}",
    );
    assert_eq!(generate(&source, 42).unwrap(), "5 5\n");
}

#[test]
fn counts_if_for_and_row_together_at_the_control_depth_limit() {
    let nested = |depth: usize, leaf: &str| {
        let headers: String = (0..depth)
            .map(|i| {
                if i % 2 == 0 {
                    "if 1 == 1 {\n".to_owned()
                } else {
                    format!("for i{i} in 1..=1 {{\n")
                }
            })
            .collect();
        format!("{headers}{leaf}{}", "}\n".repeat(depth))
            .trim_end()
            .to_owned()
    };
    let generation = format!("A[1]\n{}", nested(32, "A[1] in 6..=6\n"));
    let input = nested(31, "row {\n A\n}\n");
    assert_eq!(generate(&dsl(&generation, &input), 42).unwrap(), "6\n");
    assert_error(&nested(33, ""), "", "control nesting limit exceeded (32)");
    assert_error("", &nested(33, ""), "control nesting limit exceeded (32)");
    assert_error(
        "",
        &nested(32, "row {\n}\n"),
        "control nesting limit exceeded (32)",
    );
    // Else occupies the same depth as its corresponding if, including all-false paths.
    let elses = |depth| {
        format!(
            "{}{}",
            "if 0 == 1 {\n} else {\n".repeat(depth),
            "}\n".repeat(depth)
        )
        .trim_end()
        .to_owned()
    };
    assert_eq!(generate(&dsl(&elses(32), &elses(32)), 42).unwrap(), "");
    assert_error(&elses(33), "", "control nesting limit exceeded (32)");
    // The parser checks depth even when the path is never executed.
    assert_error(
        "",
        &format!("if 0 == 1 {{\n{}\n}}", nested(32, "")),
        "control nesting limit exceeded (32)",
    );
}

#[test]
fn charges_only_executed_branch_work_and_enforces_the_exact_budget() {
    let near = "for i in 1..=10 {\n for j in 1..=999997 {\n }\n}";
    let branch = |extra| {
        format!(
            "if 1 == 1 {{\n{near}\nfor k in 1..={extra} {{\n}}\n}} else {{\n{}\n}}",
            "for unused in 1..=1000000 {\n}\n".repeat(11)
        )
    };
    // 1 if + 9,999,991 nested-loop work + 1 for + 7 iterations.
    assert_eq!(generate(&dsl(&branch(7), &branch(7)), 42).unwrap(), "");
    assert_error(&branch(8), "", "execution work limit exceeded (10000000)");
    assert_error("", &branch(8), "execution work limit exceeded (10000000)");
    let legacy = "for i in 1..=1000000 {\n}\n".repeat(11);
    let extended = format!("{legacy}if 0 == 1 {{\n}}");
    assert_error(&extended, "", "execution work limit exceeded (10000000)");
    assert_error("", &extended, "execution work limit exceeded (10000000)");
    for input in [false, true] {
        let block = "if 1 == 1 {\n for i in 1..=1000001 {\n }\n}";
        if input {
            assert_error("", block, "loop iteration limit exceeded (1000000)");
        } else {
            assert_error(block, "", "loop iteration limit exceeded (1000000)");
        }
    }
}

fn parse_integers(line: &str) -> Vec<i64> {
    line.split_ascii_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

fn dsl(generation: &str, input: &str) -> String {
    let newline = |block: &str| if block.is_empty() { "" } else { "\n" };
    format!(
        "generate {{\n{generation}{}}}\ninput {{\n{input}{}}}\n",
        newline(generation),
        newline(input),
    )
}

fn assert_expression(expr: &str, expected: i64) {
    let source = dsl(&format!("N in {expr}..={expr}"), "N");
    assert_eq!(
        generate(&source, 42).unwrap(),
        format!("{expected}\n"),
        "{expr}"
    );
}

fn assert_error(generation: &str, input: &str, expected: &str) {
    assert_source_error(&dsl(generation, input), expected);
}

fn assert_source_error(source: &str, expected: &str) {
    let error = generate(source, 42).unwrap_err().to_string();
    assert!(
        error.contains(expected),
        "{source}\nexpected {expected:?}, got {error:?}"
    );
}
