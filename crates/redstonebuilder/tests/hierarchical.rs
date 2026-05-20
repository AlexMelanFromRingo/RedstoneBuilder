#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! End-to-end coverage for hierarchical HDL (V3.P5): a file with a
//! sub-module instantiated several times must elaborate, synthesise
//! and compile to a `.litematic` like any flat design.

use std::io::Read;
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

fn temp_output(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!(
        "rb-hier-{}-{}-{nanos:08x}.litematic",
        std::process::id(),
        name
    ));
    p
}

#[test]
fn hierarchical_adder4_compiles_end_to_end() {
    let example = workspace_root().join("examples/adder4_hier.hdl");
    let out = temp_output("adder4");

    // Lee router: fast under the debug-built test binary. This test
    // exercises elaboration → synthesis → pipeline → NBT, not routing
    // quality, so a few unrouted nets (which Lee warns about and
    // emits anyway) do not matter here.
    let output = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--router")
        .arg("lee")
        .output()
        .expect("spawn");

    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    // Schematic name comes from the input file stem.
    assert_eq!(root.metadata.name, "adder4_hier");
    // 4 full adders worth of blocks — comfortably over 200.
    assert!(
        root.metadata.total_blocks > 200,
        "expected a real schematic, got {} blocks",
        root.metadata.total_blocks
    );
    std::fs::remove_file(&out).ok();
}
