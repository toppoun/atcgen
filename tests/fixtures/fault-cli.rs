// Compiled explicitly with --cfg test by tests/output_buffer.rs. This is the
// actual library plus actual CLI, with test hooks; normal builds have no hooks.
#![allow(dead_code, unused_imports)]
include!("../../src/lib.rs");
extern crate self as atcgen;

mod cli {
    include!("../../src/main.rs");
    pub fn entry() -> std::process::ExitCode {
        main()
    }
}

fn main() -> std::process::ExitCode {
    let fault = match std::env::var("ATCGEN_TEST_FAULT").unwrap().as_str() {
        "output" => faults::Fault::Allocation("output buffer"),
        "array" => faults::Fault::Allocation("integer array"),
        "characters" => faults::Fault::Allocation("character cells"),
        "values" => faults::Fault::Allocation("declaration values"),
        "bytes" => faults::Fault::OutputBytes,
        "tokens" => faults::Fault::OutputTokens,
        "final" => faults::Fault::FinalCounts,
        _ => panic!("unknown test fault"),
    };
    faults::with(fault, cli::entry)
}
