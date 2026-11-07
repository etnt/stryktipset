//! Golden CLI tests: verify the binary's stdout, stderr, exit codes and
//! output files stay byte-identical to the captured goldens.
//!
//! `tests/golden.sh capture` regenerates them (only do this deliberately,
//! from the pre-refactor HEAD binary or after an approved change).
//! `tests/golden.sh check` compares against them.
//!
//! The test needs the binary, which only exists with the `cli` feature, so
//! it is skipped for builds like `cargo test --no-default-features`.
#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn cli_matches_goldens() {
    let bin = env!("CARGO_BIN_EXE_reduce-rs");
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.sh");
    let status = Command::new(script)
        .arg("check")
        .arg(bin)
        .status()
        .expect("failed to run tests/golden.sh");
    assert!(
        status.success(),
        "CLI output drifted from the goldens; run tests/golden.sh to inspect the diff"
    );
}
