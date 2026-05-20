#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Regression coverage for `--router pathfinder` (V3.P4.1).
//!
//! Locks in the gain over the v1 Lee router on `full_adder`: Lee
//! leaves 2 nets unrouted, PathFinder leaves 0. Also exercises the
//! smaller `half_adder` so we don't accidentally make PF crash on a
//! design Lee handles trivially.

use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

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
        "rb-router-pf-{}-{}-{nanos:08x}.litematic",
        std::process::id(),
        name
    ));
    p
}

fn compile(example_rel: &str, out: &PathBuf) -> (std::process::Output, Duration) {
    let example = workspace_root().join(example_rel);
    let started = Instant::now();
    let output = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(out)
        .arg("--router")
        .arg("pathfinder")
        .output()
        .expect("spawn");
    (output, started.elapsed())
}

fn assert_no_unrouted(stderr: &str) {
    let bad = stderr
        .lines()
        .filter(|l| {
            l.contains("[warn]")
                && (l.contains("net(s) left unrouted")
                    || l.contains("sink(s) left unrouted")
                    || l.contains("did not converge"))
        })
        .collect::<Vec<_>>();
    assert!(
        bad.is_empty(),
        "pathfinder regressed: {} unrouted/converge warnings:\n{}",
        bad.len(),
        stderr,
    );
}

#[test]
fn half_adder_pathfinder_routes_cleanly() {
    let out = temp_output("half_adder");
    let (output, elapsed) = compile("examples/half_adder.hdl", &out);
    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "half_adder PF compile {elapsed:?} too slow"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_no_unrouted(&stderr);

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    assert_eq!(root.metadata.name, "half_adder");
    std::fs::remove_file(&out).ok();
}

#[test]
fn full_adder_pathfinder_beats_lee_on_unrouted_nets() {
    let out = temp_output("full_adder");
    let (output, elapsed) = compile("examples/full_adder.hdl", &out);
    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "full_adder PF compile {elapsed:?} too slow"
    );
    // PathFinder must route every net of full_adder; the v1 Lee router
    // leaves 2 unrouted. If this assertion ever fires we regressed
    // either fan-out (multi-source A*) or pin exemption.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_no_unrouted(&stderr);

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    assert_eq!(root.metadata.name, "full_adder");
    // Sanity: PF version should ship MORE blocks than Lee's 573, since
    // the 2 unrouted nets it now routes add dust + support stones.
    assert!(
        root.metadata.total_blocks > 573,
        "PF should emit > 573 blocks (Lee baseline); got {}",
        root.metadata.total_blocks
    );
    std::fs::remove_file(&out).ok();
}

/// The full 8-bit ripple-carry adder (~104 gates after XOR lowering)
/// is the scale gate. The v1 Lee router hangs on it for tens of
/// minutes; the parallel PathFinder compiles it in ~70 s release.
/// `#[ignore]` because that wall-time exceeds a normal unit-test
/// budget — run manually with:
///   `cargo test --release -p redstonebuilder --test router_pathfinder -- --ignored`
#[ignore]
#[test]
fn ripple_adder_8bit_pathfinder_compiles_at_scale() {
    let example = workspace_root().join("examples/ripple_adder_8bit.hdl");
    let out = temp_output("ripple_adder_8bit");

    let started = Instant::now();
    let output = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--router")
        .arg("pathfinder")
        .arg("--max-footprint")
        .arg("512x48x512")
        .output()
        .expect("spawn");
    let elapsed = started.elapsed();

    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // Generous ceiling: ~70 s release, but debug builds are much
    // slower — this test is meant to be run with `--release`.
    assert!(
        elapsed < Duration::from_secs(600),
        "ripple_adder_8bit PF compile {elapsed:?} exceeded 10 min ceiling"
    );

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    assert_eq!(root.metadata.name, "ripple_adder_8bit");
    assert!(
        root.metadata.total_blocks > 3000,
        "expected a large schematic for the 8-bit adder, got {} blocks",
        root.metadata.total_blocks
    );
    std::fs::remove_file(&out).ok();
}

/// V3 Milestone 3 — the 4-bit ALU (~106 gates after XOR lowering).
/// SUB and the operation MUX are pure-gate constructions, no extra
/// stubs. `#[ignore]` for the same wall-time reason as the 8-bit
/// adder; run with `cargo test --release ... -- --ignored`.
#[ignore]
#[test]
fn alu_4bit_pathfinder_compiles_at_scale() {
    let example = workspace_root().join("examples/alu_4bit.hdl");
    let out = temp_output("alu_4bit");

    let started = Instant::now();
    let output = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--router")
        .arg("pathfinder")
        .arg("--max-footprint")
        .arg("512x48x512")
        .output()
        .expect("spawn");
    let elapsed = started.elapsed();

    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(600),
        "alu_4bit PF compile {elapsed:?} exceeded 10 min ceiling"
    );

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    assert_eq!(root.metadata.name, "alu_4bit");
    assert!(
        root.metadata.total_blocks > 3000,
        "expected a large schematic for the 4-bit ALU, got {} blocks",
        root.metadata.total_blocks
    );
    std::fs::remove_file(&out).ok();
}
