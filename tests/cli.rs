use std::path::Path;
use std::process::{Command, Output};

#[test]
fn cli_leaves_stdout_empty_when_generation_fails_after_a_row() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/empty-range.gen");
    let output = run(&source);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!("{}:7:13: invalid range `N..N`", source.display())),
        "{stderr}"
    );
}

#[test]
fn cli_sample_outputs_one_pair_per_iteration() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("sample.gen");
    let output = run(&source);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"4\n83 90\n46 99\n43 72\n69 99\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    let iterations: usize = lines.next().unwrap().parse().unwrap();
    let mut count = 0;
    for line in lines {
        let pair: Vec<i64> = line
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        let [a, b] = pair[..] else {
            panic!("expected two integers per row");
        };
        assert!((1..=99).contains(&a));
        assert!((a + 1..=100).contains(&b));
        count += 1;
    }
    assert_eq!(count, iterations);
}

#[test]
fn cli_keeps_stdout_empty_when_a_later_iteration_fails() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/empty-loop-constraint.gen");
    let output = run(&source);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!("{}:9:13: invalid range `i..N`", source.display())),
        "{stderr}"
    );
}

fn run(source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atcgen"))
        .arg("generate")
        .arg(source)
        .args(["--seed", "42"])
        .output()
        .unwrap()
}
