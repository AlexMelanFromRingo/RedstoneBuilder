#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verify the Sponge `.schem` reader against the actual redhdl stubs
//! imported into `stub_lib/core/`.

use std::path::PathBuf;

use rb_nbt::decode_sponge_schem;

fn stub_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p.push("stub_lib/core");
    p.push(format!("{name}.schem"));
    p
}

#[test]
fn decodes_adder_dimensions_and_data_version() {
    let bytes = std::fs::read(stub_path("adder")).expect("read adder.schem");
    let schem = decode_sponge_schem(&bytes).expect("decode adder.schem");
    // Known from Python NBT inspection.
    assert_eq!(schem.width, 10);
    assert_eq!(schem.height, 20);
    assert_eq!(schem.length, 10);
    assert_eq!(schem.data_version, 2975); // MC 1.18.2
}

#[test]
fn decodes_adder_palette_contains_expected_blocks() {
    let bytes = std::fs::read(stub_path("adder")).unwrap();
    let schem = decode_sponge_schem(&bytes).unwrap();
    let names: std::collections::BTreeSet<String> = schem
        .blocks
        .values()
        .map(|b| b.name.as_str().to_string())
        .collect();
    // adder.schem from redhdl uses comparators, repeaters, slabs,
    // glass, wool, redstone, signs, furnaces.
    assert!(names.contains("minecraft:comparator"));
    assert!(names.contains("minecraft:redstone_wire"));
    assert!(names.contains("minecraft:repeater"));
    assert!(names.contains("minecraft:glass"));
    // Verify air is NOT in the sparse map (we strip air at decode).
    assert!(!names.contains("minecraft:air"));
}

#[test]
fn decodes_adder_block_entities_include_signs() {
    let bytes = std::fs::read(stub_path("adder")).unwrap();
    let schem = decode_sponge_schem(&bytes).unwrap();
    let sign_count = schem
        .block_entities
        .iter()
        .filter(|be| be.id.as_str() == "minecraft:sign")
        .count();
    // 8 input/output signs + 1 name + 1 credit = 10.
    assert!(sign_count >= 8, "expected ≥ 8 signs, got {sign_count}");
}

#[test]
fn sign_text_extraction_strips_json_wrappers() {
    let bytes = std::fs::read(stub_path("adder")).unwrap();
    let schem = decode_sponge_schem(&bytes).unwrap();
    // Locate the schematic-name sign by content.
    let name_sign = schem
        .block_entities
        .iter()
        .find(|be| {
            be.id.as_str() == "minecraft:sign"
                && be.sign_text().first().is_some_and(|l| l.contains("adder"))
        })
        .expect("no name sign found");
    let lines = name_sign.sign_text();
    assert!(
        lines[0].as_str() == "kan cc adder 8b",
        "extracted name line was {:?}",
        lines[0]
    );
}

#[test]
fn decodes_and_h8b_clean() {
    let bytes = std::fs::read(stub_path("and_h8b")).unwrap();
    let schem = decode_sponge_schem(&bytes).unwrap();
    assert_eq!(schem.width, 17);
    assert_eq!(schem.height, 5);
    assert_eq!(schem.length, 5);
    assert!(!schem.blocks.is_empty());
}

#[test]
fn decodes_not_h8b_clean() {
    let bytes = std::fs::read(stub_path("not_h8b")).unwrap();
    let schem = decode_sponge_schem(&bytes).unwrap();
    assert!(schem.blocks.values().any(|b| b
        .name
        .as_str()
        .starts_with("minecraft:redstone_wall_torch")
        || b.name.as_str().starts_with("minecraft:redstone_torch")));
}
