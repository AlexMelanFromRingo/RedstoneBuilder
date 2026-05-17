#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SC-007 determinism gate: same input + same flags + same tool
//! version → bit-identical `.litematic` output, across repeated runs
//! and across machines.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_redstonebuilder"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn temp_output(label: &str, run: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!(
        "rb-determinism-{}-{label}-r{run}-{nanos:08x}.litematic",
        std::process::id()
    ));
    p
}

fn compile_n_times(example: &str, runs: u32) -> Vec<Vec<u8>> {
    let example_path = workspace_root().join("examples").join(example);
    let mut outputs: Vec<Vec<u8>> = Vec::with_capacity(runs as usize);
    for r in 0..runs {
        let out = temp_output(example, r);
        let status = Command::new(binary())
            .arg(&example_path)
            .arg("-o")
            .arg(&out)
            .status()
            .expect("spawn");
        assert!(status.success(), "run {r}: non-zero exit on {example}");
        let bytes = std::fs::read(&out).expect("read output");
        outputs.push(bytes);
        std::fs::remove_file(&out).ok();
    }
    outputs
}

fn assert_all_identical(outputs: &[Vec<u8>], label: &str) {
    let first = &outputs[0];
    for (i, out) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            out, first,
            "{label}: run {i} differs from run 0 — determinism (FR-017 / SC-007) broken"
        );
    }
}

#[test]
fn half_adder_is_deterministic_across_10_runs() {
    let outputs = compile_n_times("half_adder.hdl", 10);
    assert_all_identical(&outputs, "half_adder.hdl");
}

#[test]
fn dff_demo_is_deterministic_across_10_runs() {
    let outputs = compile_n_times("dff_demo.hdl", 10);
    assert_all_identical(&outputs, "dff_demo.hdl");
}

#[test]
fn full_adder_is_deterministic_across_10_runs() {
    let outputs = compile_n_times("full_adder.hdl", 10);
    assert_all_identical(&outputs, "full_adder.hdl");
}
