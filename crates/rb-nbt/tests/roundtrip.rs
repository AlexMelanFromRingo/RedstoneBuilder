#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use rb_core::{Bbox3, BlockId, Direction, Pos3};
use rb_nbt::{
    block_state_for, build_root, decode_from_bytes, encode_to_bytes, write_litematic, BlockGrid,
};

fn make_tiny_grid() -> BlockGrid {
    let mut grid = BlockGrid::empty(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 0, 2)));
    grid.insert(
        Pos3::new(0, 0, 0),
        block_state_for(BlockId::RedstoneDust, None),
    );
    grid.insert(
        Pos3::new(2, 0, 0),
        block_state_for(BlockId::RedstoneTorch, None),
    );
    grid.insert(
        Pos3::new(1, 0, 2),
        block_state_for(BlockId::Repeater, Some(Direction::East)),
    );
    grid
}

#[test]
fn root_round_trips_through_encode_decode() {
    let grid = make_tiny_grid();
    let root = build_root(&grid, "tiny", "round-trip test");
    let bytes = encode_to_bytes(&root).expect("encode");
    let decoded = decode_from_bytes(&bytes).expect("decode");

    assert_eq!(decoded.version, root.version);
    assert_eq!(decoded.minecraft_data_version, root.minecraft_data_version);
    assert_eq!(decoded.metadata.name, "tiny");
    assert_eq!(decoded.metadata.total_blocks, 3);
    assert_eq!(decoded.regions.len(), 1);

    let region = decoded.regions.get("main").expect("region 'main'");
    assert_eq!(region.size.x, 3);
    assert_eq!(region.size.y, 1);
    assert_eq!(region.size.z, 3);
    // Palette: AIR + 3 distinct redstone blocks = 4 entries.
    assert_eq!(region.block_state_palette.len(), 4);
    assert_eq!(region.block_state_palette[0].name.as_str(), "minecraft:air");
}

#[test]
fn writes_deterministic_bytes_for_same_input() {
    let grid = make_tiny_grid();
    let root = build_root(&grid, "tiny", "round-trip test");
    let bytes_a = encode_to_bytes(&root).expect("encode A");
    let bytes_b = encode_to_bytes(&root).expect("encode B");
    assert_eq!(bytes_a, bytes_b, "encoding must be bit-identical (FR-017)");
}

#[test]
fn write_litematic_creates_a_gzipped_nbt_file() {
    use std::io::Read;

    let grid = make_tiny_grid();
    let tmp = tempfile_path("write_litematic.litematic");
    write_litematic(&tmp, &grid, "tiny", "round-trip test").expect("write");

    let mut f = std::fs::File::open(&tmp).expect("open");
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).expect("read");

    // gzip magic bytes 0x1f 0x8b.
    assert_eq!(bytes[0], 0x1f);
    assert_eq!(bytes[1], 0x8b);

    let decoded = decode_from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.metadata.name, "tiny");

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn palette_first_encounter_order_is_stable() {
    // Same inputs in same order ⇒ same palette indices (FR-017).
    let mut grid_a = BlockGrid::empty(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 0, 0)));
    grid_a.insert(Pos3::new(0, 0, 0), block_state_for(BlockId::Stone, None));
    grid_a.insert(
        Pos3::new(1, 0, 0),
        block_state_for(BlockId::RedstoneDust, None),
    );
    grid_a.insert(
        Pos3::new(2, 0, 0),
        block_state_for(BlockId::RedstoneTorch, None),
    );

    let mut grid_b = BlockGrid::empty(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 0, 0)));
    // Same content, inserted in different order — palette must still
    // come out identical because palette indexing walks the bbox in
    // (y, z, x) order, not in insertion order.
    grid_b.insert(
        Pos3::new(2, 0, 0),
        block_state_for(BlockId::RedstoneTorch, None),
    );
    grid_b.insert(Pos3::new(0, 0, 0), block_state_for(BlockId::Stone, None));
    grid_b.insert(
        Pos3::new(1, 0, 0),
        block_state_for(BlockId::RedstoneDust, None),
    );

    let root_a = build_root(&grid_a, "x", "");
    let root_b = build_root(&grid_b, "x", "");

    let pa = &root_a.regions["main"].block_state_palette;
    let pb = &root_b.regions["main"].block_state_palette;
    assert_eq!(pa, pb, "palette must be insertion-order-invariant");
}

#[test]
fn v2_block_states_round_trip() {
    let mut grid = BlockGrid::empty(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 0, 0)));
    grid.insert(
        Pos3::new(0, 0, 0),
        block_state_for(BlockId::Observer, Some(Direction::South)),
    );
    grid.insert(
        Pos3::new(1, 0, 0),
        block_state_for(BlockId::TargetBlock, None),
    );
    grid.insert(Pos3::new(2, 0, 0), block_state_for(BlockId::Slab, None));
    grid.insert(Pos3::new(3, 0, 0), block_state_for(BlockId::Glass, None));

    let root = build_root(&grid, "v2_blocks", "round-trip");
    let bytes = encode_to_bytes(&root).expect("encode");
    let decoded = decode_from_bytes(&bytes).expect("decode");

    let region = decoded.regions.get("main").expect("region");
    let names: Vec<&str> = region
        .block_state_palette
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(names.contains(&"minecraft:observer"));
    assert!(names.contains(&"minecraft:target"));
    assert!(names.contains(&"minecraft:stone_slab"));
    assert!(names.contains(&"minecraft:glass"));
}

#[test]
fn override_property_patches_repeater_delay() {
    use rb_nbt::override_property;
    let mut state = block_state_for(BlockId::Repeater, Some(Direction::East));
    assert_eq!(state.properties.get("delay").map(|s| s.as_str()), Some("1"));
    override_property(&mut state, "delay", "3");
    override_property(&mut state, "locked", "true");
    assert_eq!(state.properties.get("delay").map(|s| s.as_str()), Some("3"));
    assert_eq!(
        state.properties.get("locked").map(|s| s.as_str()),
        Some("true")
    );
}

#[test]
fn block_state_for_consistent_property_keys() {
    let r = block_state_for(BlockId::Repeater, Some(Direction::East));
    assert_eq!(r.name.as_str(), "minecraft:repeater");
    let expected_keys: BTreeMap<&str, &str> = [
        ("facing", "east"),
        ("delay", "1"),
        ("locked", "false"),
        ("powered", "false"),
    ]
    .into_iter()
    .collect();
    for (k, v) in expected_keys {
        assert_eq!(r.properties.get(k).map(|s| s.as_str()), Some(v));
    }
}

fn tempfile_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("rb-nbt-test-{}-{}", std::process::id(), name));
    p
}
