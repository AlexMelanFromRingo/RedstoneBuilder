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

fn temp_output(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!(
        "rb-e2e-us2-{}-{}-{nanos:08x}.litematic",
        std::process::id(),
        name
    ));
    p
}

#[test]
fn dff_demo_compiles_end_to_end() {
    let example = workspace_root().join("examples/dff_demo.hdl");
    let out = temp_output("dff_demo");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn redstonebuilder");

    assert!(status.success(), "non-zero exit: {status:?}");

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open")
        .read_to_end(&mut bytes)
        .expect("read");
    assert_eq!(bytes[0], 0x1f);
    assert_eq!(bytes[1], 0x8b);

    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode");
    assert_eq!(root.metadata.name, "dff_demo");
    assert!(root.metadata.total_blocks > 0);
    let region = root.regions.get("main").expect("region 'main'");
    assert!(
        region
            .block_state_palette
            .iter()
            .any(|s| s.name.as_str() == "minecraft:redstone_torch"
                || s.name.as_str() == "minecraft:repeater"),
        "expected at least one stateful-cell block (torch or repeater) in palette"
    );

    std::fs::remove_file(&out).ok();
}
