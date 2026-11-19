use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn fault_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let directory = root.join("target/phase8b-test-faults");
        std::fs::create_dir_all(&directory).unwrap();
        let binary = directory.join(format!(
            "fault-cli-{}{}",
            std::process::id(),
            std::env::consts::EXE_SUFFIX
        ));
        let result = Command::new("rustc")
            .args(["--edition=2024", "--cfg", "test"])
            .arg(root.join("tests/fixtures/fault-cli.rs"))
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        binary
    })
}

#[test]
fn injected_reservation_and_second_pass_failures_leave_actual_cli_stdout_empty() {
    let directory =
        std::env::temp_dir().join(format!("atcgen-output-faults-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("case.gen");
    for (fault, body, expected) in [
        ("output", "N in 7..=7", "unable to allocate output buffer"),
        (
            "values",
            "N in 7..=7",
            "unable to allocate declaration values",
        ),
        (
            "array",
            "N in 7..=7\nA[1000000]",
            "unable to allocate integer array",
        ),
        (
            "characters",
            "N in 7..=7\nS : string[1000000]",
            "unable to allocate character cells",
        ),
        ("bytes", "N in 7..=7", "internal output inconsistency"),
        ("tokens", "N in 7..=7", "internal output inconsistency"),
        ("final", "N in 7..=7", "internal output inconsistency"),
    ] {
        std::fs::write(
            &path,
            format!("generate {{\n{body}\n}}\ninput {{\nN N\n}}\n"),
        )
        .unwrap();
        let result = Command::new(fault_binary())
            .env("ATCGEN_TEST_FAULT", fault)
            .arg("generate")
            .arg(&path)
            .args(["--seed", "42"])
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(1),
            "{fault}: {:?}",
            result.stderr
        );
        assert!(result.stdout.is_empty(), "{fault}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{fault}: {:?}",
            result.stderr
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_first_pass_reads_arithmetic_and_lf_overflow_leave_stdout_empty() {
    let path = std::env::temp_dir().join(format!("atcgen-output-late-{}.gen", std::process::id()));
    for (body, output, expected) in [
        ("A[2]\nA[1] in 1..=1", "A[1]\nA[2]", "uninitialized"),
        ("A[1]\nA[1] in 1..=1", "A[1]\nA[2]", "out of bounds"),
        ("A[1]\nA[1] in 1..=1", "A[1]\nA[1/0]", "division by zero"),
        (
            "A[1]\nA[1] in 1..=1",
            "A[1]\nA[9223372036854775807+1]",
            "integer overflow",
        ),
        // 8*1,000,000 blank lines plus 388,608 more exactly fills 8 MiB.
        // The final empty row's LF is the first byte beyond the limit (T=0).
        (
            "",
            "for i in 1..=1000000 {\n\n\n\n\n\n\n\n\n}\nfor j in 1..=388608 {\n\n}\n\n",
            "output size limit exceeded",
        ),
    ] {
        std::fs::write(
            &path,
            format!("generate {{\n{body}\n}}\ninput {{\n{output}\n}}\n"),
        )
        .unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_atcgen"))
            .arg("generate")
            .arg(&path)
            .args(["--seed", "42"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{:?}",
            result.stderr
        );
    }
    std::fs::remove_file(path).unwrap();
}
