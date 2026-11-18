use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn cli_sample_outputs_one_pair_per_iteration() {
    let output = run(&source("sample.gen"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"3\n83 90\n46 99\n43 72\n");
}

#[test]
fn cli_expands_whole_arrays() {
    let output = run(&source("tests/fixtures/whole-arrays.gen"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"2\n\n5 -6 9 2 -6\n");
}

#[test]
fn cli_leaves_stdout_empty_when_generation_fails_after_a_value() {
    assert_error("empty-range.gen", "3:13: invalid range `N..N`");
}

#[test]
fn cli_leaves_stdout_empty_when_a_later_generation_iteration_fails() {
    assert_error("empty-loop-range.gen", "5:20: invalid range `i..N`");
}

#[test]
fn cli_leaves_stdout_empty_when_a_later_output_iteration_fails() {
    assert_error(
        "uninitialized-output.gen",
        "11:9: array element `A[3]` is uninitialized",
    );
}

#[test]
fn cli_leaves_stdout_empty_when_a_nested_generation_iteration_fails() {
    assert_error("nested-generation-error.gen", "5:25: invalid range `i..=1`");
}

#[test]
fn cli_leaves_stdout_empty_when_a_later_row_fails() {
    assert_error(
        "nested-row-error.gen",
        "11:22: array element `A[2]` is uninitialized",
    );
}

#[test]
fn cli_leaves_stdout_empty_when_a_later_conditional_row_fails() {
    assert_error(
        "conditional-output-error.gen",
        "14:21: array element `A[2]` is uninitialized",
    );
}

#[test]
fn cli_outputs_strings_grids_and_mixed_rows() {
    let output = run(&source("tests/fixtures/strings.gen"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"2 ab\nba\nab\nab b ba ab 2\n");
}

#[test]
fn cli_string_errors_at_every_validation_stage_leave_stdout_empty() {
    // Parse, static type checking, generation, capacity, output reads, token
    // count, and output byte count must all fail before the first write.
    let cases = [
        ("S : string[1]\nS[1] in \"aa\"", "", "duplicate character"),
        ("S : string[1]\nS[1] in \"é\"", "", "ASCII"),
        (
            "S : string[1]\nif 0 == 1 {\nS[1] = 1\n}",
            "",
            "expected Char",
        ),
        (
            "S : string[1]\nS[1] = 'a'\nS[1] in \"b\"",
            "",
            "already initialized",
        ),
        (
            "G[1000] : string[1000]\nS : string[1]",
            "",
            "capacity limit exceeded",
        ),
        (
            "S : string[2]\nS[1] = 'a'",
            "S[1]\nrow {\nS[1]\nS\n}",
            "S[2]` is uninitialized",
        ),
        (
            "G[1] : string[1]\nG[1][1] = 'a'",
            "G[1]\nG[0][1/0]",
            "index 0 is out of bounds",
        ),
        (
            "S : string[1]\nS[1] = 'a'",
            "for i in 1..=1000000 {\nS\n}\nS[1]",
            "output token limit exceeded",
        ),
        (
            "S : string[1000000]\nfor i in 1..=1000000 {\nS[i] = 'a'\n}",
            "for i in 1..=9 {\nS\n}",
            "output size limit exceeded",
        ),
    ];
    for (index, (generation, input, expected)) in cases.iter().enumerate() {
        let path = std::env::temp_dir().join(format!(
            "atcgen-string-cli-{}-{index}.gen",
            std::process::id()
        ));
        std::fs::write(
            &path,
            format!("generate {{\n{generation}\n}}\ninput {{\n{input}\n}}\n"),
        )
        .unwrap();
        let output = run(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(output.status.code(), Some(1), "{expected}");
        assert!(output.stdout.is_empty(), "{expected}");
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.starts_with(&format!("{}:", path.display())),
            "{diagnostic}"
        );
        assert!(diagnostic.contains(expected), "{diagnostic}");
    }
}

fn assert_error(file: &str, expected: &str) {
    let source = source(&format!("tests/fixtures/{file}"));
    let output = run(&source);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!("{}:{expected}", source.display())),
        "{stderr}"
    );
}

fn source(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(file)
}

fn run(source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atcgen"))
        .arg("generate")
        .arg(source)
        .args(["--seed", "42"])
        .output()
        .unwrap()
}
