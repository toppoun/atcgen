use crate::generate;

fn dsl(generation: &str, input: &str) -> String {
    format!("generate {{\n{generation}\n}}\ninput {{\n{input}\n}}\n")
}

fn error(generation: &str, input: &str, expected: &str) {
    let source = dsl(generation, input);
    let actual = generate(&source, 42).unwrap_err().to_string();
    assert!(
        actual.contains(expected),
        "{source}\nexpected {expected:?}, got {actual:?}"
    );
}

#[test]
fn outputs_strings_rows_cells_and_integers_in_normal_and_row_blocks() {
    let generation = "N in 2..=2\nS : string[N]\nG[N] : string[N]\n\
        S[2] = 'b'\nS[1] = 'a'\nG[1][1] = S[2]\nG[1][2] = S[1]\n\
        G[2][1] = G[1][2]\nG[2][2] = G[1][1]";
    let input = "N S S[2] G[1][1] G\nfor i in 1..=N {\n G[i]\n}\n\
        row {\n S\n for i in 1..=N {\n  G[i] G[i][i]\n }\n N\n}";
    let source = dsl(generation, input);
    assert_eq!(
        generate(&source, 42).unwrap(),
        "2 ab b b ba ab\nba\nab\nab ba b ab b 2\n"
    );
    assert_eq!(
        generate(&source.replace('\n', "\r\n"), 42).unwrap(),
        generate(&source, 42).unwrap()
    );
}

#[test]
fn samples_characters_in_decoded_source_order_with_the_integer_sampler() {
    let chars = dsl(
        "S : string[128]\nfor i in 1..=128 {\n S[i] in \"cba\"\n}",
        "S",
    );
    let ints = dsl("A[128]\nfor i in 1..=128 {\n A[i] in 0..=2\n}", "A");
    for seed in [0, 1, 42, u64::MAX] {
        let output = generate(&chars, seed).unwrap();
        let expected: String = generate(&ints, seed)
            .unwrap()
            .split_ascii_whitespace()
            .map(|word| char::from(b"cba"[word.parse::<usize>().unwrap()]))
            .collect();
        assert_eq!(output, format!("{expected}\n"));
        assert_eq!(output, generate(&chars, seed).unwrap());
    }
}

#[test]
fn fixed_initialization_consumes_no_randomness_and_singleton_alphabets_do() {
    let full = "-9223372036854775808..=9223372036854775807";
    let fixed = dsl(
        &format!("S : string[2]\nS[1] = 'x'\nS[2] = S[1]\nN in {full}"),
        "N",
    );
    let random = dsl(
        &format!("S : string[2]\nS[1] in \"x\"\nS[2] = S[1]\nN in {full}"),
        "N",
    );
    let baseline = dsl(&format!("N in {full}"), "N");
    let advanced = dsl(&format!("Unused in 0..=0\nN in {full}"), "N");
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(
            generate(&fixed, seed).unwrap(),
            generate(&baseline, seed).unwrap()
        );
        assert_eq!(
            generate(&random, seed).unwrap(),
            generate(&advanced, seed).unwrap()
        );
    }
    assert_eq!(generate(&fixed, 42).unwrap(), "4456085495900499605\n");
    assert_eq!(generate(&random, 42).unwrap(), "-6273545944727883517\n");
}

#[test]
fn compares_characters_in_generation_input_and_rows() {
    let generation = "S : string[3]\nS[1] = 'a'\nif S[1] == ('a') {\n S[2] = S[1]\n}\n\
        if S[1] != 'b' {\n S[3] = 'c'\n} else {\n S[99] = S[99]\n}";
    let input = "if S[1] == S[2] {\n S\n}\nrow {\n if S[3] != S[2] {\n S[3]\n }\n\
        if 'a' != 'a' {\n S[99]\n } else {\n S[1]\n }\n}";
    assert_eq!(generate(&dsl(generation, input), 42).unwrap(), "aac\nc a\n");
    let generation =
        format!("{generation}\nG[1] : string[1]\nif S[1] == S[2] {{\nG[1][1] = S[3]\n}}");
    assert_eq!(
        generate(&dsl(&generation, "if G[1][1] == S[3] {\nG\n}"), 42).unwrap(),
        "c\n"
    );
    for condition in ["'a' == 'b'", "'a' != 'a'"] {
        assert!(generate(&dsl("", &format!("if {condition} {{\n Missing\n}}")), 42).is_err());
    }
}

#[test]
fn skipped_branches_and_zero_iterations_do_not_initialize_read_or_draw() {
    let generation = "S : string[1]\nif 0 == 1 {\n S[0][1] in \"x\"\n}";
    // Even skipped code is checked for incorrect reference shape.
    error(generation, "", "only a StringArray accepts two subscripts");
    let source = dsl(
        "S : string[1]\nif 'a' != 'a' {\n S[1/0] in \"abc\"\n}\n\
        for i in 1..1 {\n S[1] = S[1]\n}\nS[1] = 'x'\nN in 1..=1000000",
        "if 'a' != 'a' {\n S[1/0]\n}\nfor i in 1..1 {\n S[1/0]\n}\nS N",
    );
    let baseline = dsl("S : string[1]\nS[1] = 'x'\nN in 1..=1000000", "S N");
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(
            generate(&source, seed).unwrap(),
            generate(&baseline, seed).unwrap()
        );
    }
}

#[test]
fn decodes_quotes_backslashes_and_all_printable_ascii() {
    let generation = r#"S : string[6]
S[1] = '\''
S[2] = '\"'
S[3] = '\\'
S[4] = '"'
S[5] in "\'"
S[6] in "\\""#;
    assert_eq!(generate(&dsl(generation, "S"), 42).unwrap(), "'\"\\\"'\\\n");
    let alphabet: String = (0x21u8..=0x7e).map(char::from).collect();
    let escaped = alphabet.replace('\\', "\\\\").replace('"', "\\\"");
    let source = dsl(
        &format!("S : string[4096]\nfor i in 1..=4096 {{\n S[i] in \"{escaped}\"\n}}"),
        "S",
    );
    let output = generate(&source, 42).unwrap();
    let actual: std::collections::BTreeSet<_> = output.trim_end().bytes().collect();
    assert_eq!(actual, (0x21..=0x7e).collect());
}

#[test]
fn rejects_invalid_literals_alphabets_and_escape_sequences() {
    for (rhs, expected) in [
        ("in \"\"", "empty"),
        ("in \"aba\"", "duplicate character"),
        (r#"in "'\'""#, "duplicate character"),
        ("in \"あ\"", "ASCII"),
        ("= 'é'", "ASCII"),
        ("in \"a b\"", "ASCII"),
        ("= ' '", "ASCII"),
        ("in \"a\tb\"", "ASCII"),
        ("= '\u{7f}'", "ASCII"),
        (r#"in "\n""#, "invalid escape"),
        (r#"= '\x'"#, "invalid escape"),
        ("= ''", "empty"),
        ("= 'ab'", "exactly one"),
        ("= 'a", "unterminated"),
        ("in \"abc", "unterminated"),
        ("in \"abc\\", "invalid escape"),
    ] {
        error(&format!("S : string[1]\nS[1] {rhs}"), "", expected);
    }
    error("S : string[1]\n  S[1] in \"é\"", "", "3:12:");
}

#[test]
fn statically_rejects_char_integer_conversions_arithmetic_and_whole_string_operations() {
    let declarations = "N in 1..=1\nA[1]\nS : string[1]\nG[1] : string[1]";
    for statement in [
        "A[1] = 'a'",
        "A[1] = 1",
        "A[1] in \"a\"",
        "S[1] in 1..=2",
        "S[1] = N",
        "S[1] = A[1]",
        "S[1] = S",
        "G[1] = 'a'",
        "S[1] = G[1]",
        "S[1] = S[1]+1",
        "S[1] = +'a'",
        "S[1] = -'a'",
        "S[1] = 'a'-'b'",
        "S[1] = 'a'*2",
        "S[1] = 'a'/2",
        "S[1] = 'a'%2",
        "S['a'] = 'b'",
        "G[1]['a'] = 'b'",
        "A[S[1]] in 1..=1",
        "for i in 'a'..=2 {\n}",
        "if S[1] == 1 {\n}",
        "if N == S[1] {\n}",
        "if S[1] < 'a' {\n}",
        "if 'a' <= S[1] {\n}",
        "if S[1] > 'a' {\n}",
        "if S[1] >= 'a' {\n}",
        "if S == S {\n}",
        "if G == G {\n}",
        "if G[1] != S {\n}",
    ] {
        for wrapper in [
            format!("if 0 == 1 {{\n{statement}\n}}"),
            format!("for i0 in 1..1 {{\n{statement}\n}}"),
        ] {
            let result = generate(&dsl(&format!("{declarations}\n{wrapper}"), ""), 42);
            assert!(result.is_err(), "{statement}");
        }
    }
    for declaration in ["T : string['a']", "T['a'] : string[1]", "K in S[1]..=2"] {
        error(
            &format!("{declarations}\n{declaration}"),
            "",
            "expected Int",
        );
    }
    for input in [
        "if S[1] == N {\n}",
        "if S == S {\n}",
        "G[S[1]]",
        "S[1][1]",
        "G[1][1][1]",
    ] {
        assert!(generate(&dsl(declarations, input), 42).is_err(), "{input}");
    }
}

#[test]
fn tracks_uninitialized_cells_and_rejects_reinitialization_before_reading_rhs() {
    for (generation, input, expected) in [
        ("S : string[2]\nS[1] = 'a'", "S", "S[2]` is uninitialized"),
        (
            "G[2] : string[2]\nG[1][1] = 'a'",
            "G",
            "G[1][2]` is uninitialized",
        ),
        ("G[2] : string[2]", "G[2]", "G[2][1]` is uninitialized"),
        ("S : string[1]\nS[1] = S[1]", "", "S[1]` is uninitialized"),
        (
            "G[1] : string[1]\nS : string[1]\nS[1] = G[1][1]",
            "",
            "G[1][1]` is uninitialized",
        ),
        (
            "S : string[1]",
            "if S[1] == 'a' {\n}",
            "S[1]` is uninitialized",
        ),
        (
            "S : string[2]\nS[1] = 'a'\nS[1] = S[2]",
            "",
            "S[1]` is already initialized",
        ),
        (
            "G[1] : string[1]\nG[1][1] in \"a\"\nG[1][1] in \"b\"",
            "",
            "already initialized",
        ),
        (
            "S : string[1]\nif 0 == 1 {\n S[1] = 'a'\n}",
            "S[1]",
            "uninitialized",
        ),
        (
            "S : string[1]\nfor i in 1..=2 {\n S[i-i+1] = 'a'\n}",
            "",
            "already initialized",
        ),
    ] {
        error(generation, input, expected);
    }
    assert_eq!(
        generate(&dsl("S : string[2]\nS[2] = 'a'", "S[2]"), 42).unwrap(),
        "a\n"
    );
}

#[test]
fn validates_two_subscripts_outer_first_in_targets_reads_and_output() {
    for index in ["0", "-1", "2", "9223372036854775807"] {
        for target in [
            format!("G[{index}][1/0] = 'a'"),
            format!("S[1] = G[{index}][1/0]"),
            format!("if G[{index}][1/0] == 'a' {{\n}}"),
        ] {
            error(
                &format!("G[1] : string[1]\nS : string[1]\n{target}"),
                "",
                &format!("index {index} is out of bounds"),
            );
        }
        for input in [format!("G[{index}][1/0]"), format!("G[{index}]")] {
            error(
                "G[1] : string[1]",
                &input,
                &format!("index {index} is out of bounds"),
            );
        }
        error(
            &format!("G[1] : string[1]\nG[1][{index}] = 'a'"),
            "",
            &format!("index {index} is out of bounds"),
        );
        error(
            "G[1] : string[1]",
            &format!("G[1][{index}]"),
            &format!("index {index} is out of bounds"),
        );
        error("S : string[1]", &format!("S[{index}]"), "out of bounds");
    }
    error("G[1] : string[1]\nG[1][1/0] = 'a'", "", "division by zero");
    error(
        "S : string[2]",
        "if S[1] == S[2] {\n}",
        "S[1]` is uninitialized",
    );
}

#[test]
fn string_declarations_preserve_name_scope_and_top_level_rules() {
    for declaration in ["S : string[1]", "G[1] : string[1]"] {
        error(
            &format!("if 0 == 1 {{\n{declaration}\n}}"),
            "",
            "only array element initialization",
        );
        error(
            &format!("for i in 1..1 {{\n{declaration}\n}}"),
            "",
            "only array element initialization",
        );
    }
    error("S : string[1]\nS in 1..=1", "", "duplicate declaration");
    error("S : string[1]\nfor S in 1..1 {\n}", "", "name collision");
    error(
        "S : string[S[1]]",
        "",
        "not available before its declaration",
    );
    error(
        "G[1] : string[1]\nG[1][1] = T[1]\nT : string[1]",
        "",
        "not available before its declaration",
    );
    let source = dsl(
        "string : string[1]\nif : string[1]\nelse : string[1]\nrow : string[1]\n\
        string[1] = '{'\nif[1] = '}'\nelse[1] = '['\nrow[1] = ']'",
        "string if row else",
    );
    assert_eq!(generate(&source, 42).unwrap(), "{ } ] [\n");
}

#[test]
fn validates_string_capacity_before_allocation_and_separately_from_integer_capacity() {
    for generation in [
        "S : string[1000000]",
        "G[1000] : string[1000]",
        "G[1000000] : string[1]",
        "S : string[999999]\nG[1] : string[1]",
        "A[1000000]\nS : string[1000000]",
    ] {
        assert_eq!(generate(&dsl(generation, ""), 42).unwrap(), "\n");
    }
    for generation in [
        "S : string[0]",
        "S : string[-1]",
        "G[0] : string[1]",
        "G[1] : string[0]",
    ] {
        error(generation, "", "expected a positive integer");
    }
    for generation in [
        "S : string[1000001]",
        "G[1000001] : string[1]",
        "S : string[9223372036854775807]",
        "G[9223372036854775807] : string[9223372036854775807]",
    ] {
        error(generation, "", "dimension limit exceeded");
    }
    error(
        "G[1000000] : string[1000000]",
        "",
        "string capacity limit exceeded",
    );
    error(
        "G[1001] : string[1000]",
        "",
        "string capacity limit exceeded",
    );
    error(
        "G[1000] : string[1000]\nS : string[1]",
        "",
        "total character capacity limit exceeded",
    );
    error("S : string[1/0]", "", "division by zero");
    error("G[0] : string[1/0]", "", "invalid string dimension 0");
    error("G[1/0] : string[0%0]", "", "division by zero");
}

#[test]
fn counts_mixed_output_tokens_and_string_array_expansion_at_the_limit() {
    let generation = "N in 0..=0\nS : string[2]\nS[1] = 'a'\nS[2] = 'b'\nG[2] : string[1]\nG[1][1] = 'x'\nG[2][1] = 'y'";
    let input = "row {\nfor i in 1..=200000 {\nN S S[1] G\n}\n}";
    let output = generate(&dsl(generation, input), 42).unwrap();
    assert_eq!(output.split_ascii_whitespace().count(), 1_000_000);
    error(
        generation,
        &format!("{input}\nS[1]"),
        "output token limit exceeded",
    );
    error(
        generation,
        &format!("{input}\nG"),
        "output token limit exceeded",
    );
    let generation = "G[1000000] : string[1]\nfor i in 1..=1000000 {\nG[i][1] = 'a'\n}";
    assert_eq!(
        generate(&dsl(generation, "G"), 42).unwrap().len(),
        2_000_000
    );
    error(generation, "G G[1]", "output token limit exceeded");
}

#[test]
fn counts_full_string_bytes_spaces_and_lf_at_the_eight_mib_boundary() {
    // Repeated strings count their complete byte length on every appearance.
    let generation = "S : string[1000000]\nfor i in 1..=1000000 {\nS[i] = 'a'\n}";
    let source = dsl(generation, "for i in 1..=8 {\nS\n}");
    assert_eq!(generate(&source, 42).unwrap().len(), 8_000_008);
    error(
        generation,
        "for i in 1..=9 {\nS\n}",
        "output size limit exceeded",
    );
    // Hit both the 1,000,000-token limit and exactly 8 MiB.
    let generation = "S : string[7]\nfor i in 1..=7 {\nS[i] = 'a'\n}\nT : string[388615]\nfor i in 1..=388615 {\nT[i] = 'b'\n}";
    let input = "for i in 1..=999999 {\nS\n}\nT";
    let output = generate(&dsl(generation, input), 42).unwrap();
    assert_eq!(output.len(), 8 * 1024 * 1024);
    error(
        generation,
        &format!("{input}\n"),
        "output size limit exceeded",
    );
    // A single row counts the same spaces as normal lines count LF.
    let input = format!("row {{\n{input}\n}}");
    assert_eq!(
        generate(&dsl(generation, &input), 42).unwrap().len(),
        8 * 1024 * 1024
    );
    error(
        &generation.replace("388615", "388616"),
        &input,
        "output size limit exceeded",
    );
    error(
        &generation.replace("388615", "388617"),
        &input,
        "output size limit exceeded",
    );
}

#[test]
fn counts_second_subscript_expression_depth_and_control_depth() {
    let generation = "G[1] : string[1]\nG[1][1] = 'a'";
    let index = |n| format!("{}1{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(
        generate(
            &dsl(generation, &format!("G[{}][{}]", index(127), index(126))),
            42
        )
        .unwrap(),
        "a\n"
    );
    error(
        generation,
        &format!("G[1][{}]", index(127)),
        "expression nesting limit exceeded",
    );
    error(
        &format!("G[1] : string[1]\nG[1][{}] = 'a'", index(127)),
        "",
        "expression nesting limit exceeded",
    );
    let nested = format!(
        "{}G[1][1]\n{}",
        "if 'a' == 'a' {\n".repeat(32),
        "}\n".repeat(32)
    );
    assert_eq!(
        generate(&dsl(generation, nested.trim_end()), 42).unwrap(),
        "a\n"
    );
    error(
        generation,
        &format!("if 'a' == 'a' {{\n{nested}}}"),
        "control nesting limit exceeded",
    );
}
