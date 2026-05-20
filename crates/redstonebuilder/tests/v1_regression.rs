#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! FR-V13 / SC-V05 regression: every v1 example HDL file MUST
//! continue to compile under v2 with no source edits, under **both**
//! `--placer sa` (default) and `--placer greedy`.

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

fn temp(name: &str, placer: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!(
        "rb-v1regr-{}-{name}-{placer}-{nanos:08x}.litematic",
        std::process::id()
    ));
    p
}

fn compile(example: &str, placer: &str) {
    let example_path = workspace_root().join("examples").join(example);
    let out = temp(example, placer);
    let status = Command::new(binary())
        .arg(&example_path)
        .arg("--placer")
        .arg(placer)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn");
    assert!(
        status.success(),
        "{example} under --placer {placer} failed with {status:?}"
    );
    let bytes = std::fs::read(&out).expect("read");
    assert!(!bytes.is_empty(), "{example} produced empty file");
    std::fs::remove_file(&out).ok();
}

#[test]
fn half_adder_compiles_under_both_placers() {
    compile("half_adder.hdl", "sa");
    compile("half_adder.hdl", "greedy");
}

#[test]
fn dff_demo_compiles_under_both_placers() {
    compile("dff_demo.hdl", "sa");
    compile("dff_demo.hdl", "greedy");
}

#[test]
fn full_adder_compiles_under_both_placers() {
    compile("full_adder.hdl", "sa");
    compile("full_adder.hdl", "greedy");
}
