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
