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
    p.push(format!(
        "rb-e2e-{}-{}-{}.litematic",
        std::process::id(),
        name,
        rand_suffix()
    ));
    p
}

fn rand_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{nanos:08x}")
}

#[test]
fn half_adder_compiles_end_to_end() {
    let example = workspace_root().join("examples/half_adder.hdl");
    let out = temp_output("half_adder");

    let status = Command::new(binary())
        .arg(&example)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn redstonebuilder");

    assert!(
        status.success(),
        "redstonebuilder exited non-zero: {status:?}"
    );

    let mut bytes = Vec::new();
    std::fs::File::open(&out)
        .expect("open output")
        .read_to_end(&mut bytes)
        .expect("read output");

    assert!(!bytes.is_empty(), "output file is empty");
    assert_eq!(bytes[0], 0x1f, "expected gzip magic");
    assert_eq!(bytes[1], 0x8b, "expected gzip magic");

    let root = rb_nbt::decode_from_bytes(&bytes).expect("decode .litematic");
    assert_eq!(root.metadata.name, "half_adder");
    assert!(root.metadata.total_blocks > 0, "schematic has no blocks");

    let region = root.regions.get("main").expect("region 'main'");
    let palette = &region.block_state_palette;
    assert!(
        palette
            .iter()
            .any(|s| s.name.as_str() == "minecraft:redstone_wire"
                || s.name.as_str() == "minecraft:redstone_torch"
                || s.name.as_str() == "minecraft:redstone_wall_torch"),
        "expected at least one redstone block in palette; got {palette:?}"
    );

    std::fs::remove_file(&out).ok();
}

#[test]
fn syntax_error_exits_with_parse_code() {
    let workspace = workspace_root();
    let bad = workspace.join("crates/rb-parser/tests/fixtures/syntax_error.hdl");
    let out = temp_output("syntax_error");

    let status = Command::new(binary())
        .arg(&bad)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("spawn");

    assert_eq!(
        status.code(),
        Some(2),
        "syntax error must exit with code 2 (got {:?})",
        status.code()
    );
}

#[test]
fn dump_only_writes_ast_and_skips_schematic() {
    let example = workspace_root().join("examples/half_adder.hdl");
    let out = temp_output("dump_only");

    let status = Command::new(binary())
        .arg(&example)
        .arg("--dump-ast")
        .arg(&out)
        .arg("--dump-only")
        .status()
        .expect("spawn");

    assert!(status.success());
    let ast = std::fs::read_to_string(&out).expect("read AST dump");
    assert!(
        ast.contains("half_adder"),
        "expected module name in AST dump"
    );
    assert!(
        ast.contains("xor") || ast.contains("Xor"),
        "expected XOR gate"
    );

    std::fs::remove_file(&out).ok();
}
