use std::path::Path;
use std::process::Command;

#[test]
fn cli_leaves_stdout_empty_when_generation_fails_after_a_row() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/empty-range.gen");
    let output = Command::new(env!("CARGO_BIN_EXE_atcgen"))
        .arg("generate")
        .arg(&source)
        .args(["--seed", "42"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!("{}:7:13: invalid range `N..N`", source.display())),
        "{stderr}"
    );
}
