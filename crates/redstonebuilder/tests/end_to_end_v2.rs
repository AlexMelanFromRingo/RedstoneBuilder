#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

fn temp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!(
        "rb-e2e-v2-{}-{name}-{nanos:08x}.litematic",
        std::process::id()
    ));
    p
}

#[test]
fn analog_add_compiles_with_comparator_in_palette() {
    let example = workspace_root().join("examples/analog_add.hdl");
    let out = temp("analog_add");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn");
    assert!(status.success(), "compile failed: {status:?}");

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    let region = root.regions.get("main").expect("region");
    let names: Vec<&str> = region
        .block_state_palette
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        names.contains(&"minecraft:comparator"),
        "expected minecraft:comparator in palette, got {names:?}"
    );
    let comparator = region
        .block_state_palette
        .iter()
        .find(|s| s.name.as_str() == "minecraft:comparator")
        .expect("comparator block-state");
    assert_eq!(
        comparator.properties.get("mode").map(|s| s.as_str()),
        Some("subtract"),
        "comparator mode must be subtract per .MODE(subtract)"
    );

    std::fs::remove_file(&out).ok();
}

#[test]
fn monostable_compiles_with_observer_and_repeater() {
    let example = workspace_root().join("examples/monostable.hdl");
    let out = temp("monostable");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn");
    assert!(status.success(), "compile failed: {status:?}");

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    let region = root.regions.get("main").expect("region");
    let names: Vec<&str> = region
        .block_state_palette
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        names.contains(&"minecraft:observer"),
        "expected observer; got {names:?}"
    );
    assert!(
        names.contains(&"minecraft:repeater"),
        "expected repeater; got {names:?}"
    );

    std::fs::remove_file(&out).ok();
}

#[test]
fn stats_flag_prints_summary_to_stderr() {
    let example = workspace_root().join("examples/half_adder.hdl");
    let out = temp("ha_stats");

    let output = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--stats")
        .output()
        .expect("spawn");
    assert!(output.status.success(), "compile failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stats:") && stderr.contains("peak_ram=") && stderr.contains("MiB"),
        "expected stats line on stderr, got:\n{stderr}"
    );

    std::fs::remove_file(&out).ok();
}

#[test]
fn strict_timing_on_full_adder_exits_with_code_9() {
    let example = workspace_root().join("examples/full_adder.hdl");
    let out = temp("fa_strict");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--strict-timing")
        .status()
        .expect("spawn");

    assert_eq!(
        status.code(),
        Some(9),
        "full_adder under --strict-timing must exit with code 9 (TimingRace), got {status:?}"
    );
}

#[test]
fn max_ram_at_zero_aborts_with_exit_code_8() {
    let example = workspace_root().join("examples/half_adder.hdl");
    let out = temp("ha_oom");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .arg("--max-ram")
        .arg("1") // 1 MiB — any non-trivial compile blows this
        .status()
        .expect("spawn");

    assert_eq!(
        status.code(),
        Some(8),
        "--max-ram=1 must abort with exit code 8 (MemoryCap), got {status:?}"
    );
}
