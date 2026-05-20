//! V3 — interactive wrapper for any single stub in `stub_lib/core/`.
//!
//! Loads the named stub, places it at the origin, then attaches a wall
//! LEVER to every input pin's outward face and a REDSTONE_LAMP next
//! to every output pin's outward face. Writes a `.litematic`.
//!
//! Drop the result into a Litematica install, paste it in a creative
//! world, flip levers, watch lamps. No PathFinder routing; every
//! external block sits directly adjacent to its pin block.
//!
//! Caveat: the redhdl stub convention uses `red_wool` as the
//! cosmetic pin marker, with the actual signal emitter typically a
//! REPEATER one cell ABOVE the wool. If lamps don't light, try
//! `--lamp-up` to lift the output lamps to `pin + facing + up`, which
//! aligns with the redhdl output-repeater convention.
//!
//! Usage:
//!     cargo run -p rb-stubs --example stub_interactive -- \
//!         <stub_name> <out.litematic> [--lamp-up]
//!
//! Example:
//!     cargo run -p rb-stubs --example stub_interactive -- \
//!         not_h8b out/v3_not_h8b_interactive.litematic

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use rb_core::{Bbox3, BlockId, Direction, Pos3};
use rb_nbt::{block_state_for, override_property, write_litematic, BlockGrid};
use rb_stubs::{load_stub_library, PortDir, Stub, StubPort};

/// Padding around the world bbox so external levers/lamps fit cleanly.
const BBOX_PAD: i32 = 2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut positional: Vec<String> = Vec::new();
    let mut lamp_up = false;
    for a in env::args().skip(1) {
        match a.as_str() {
            "--lamp-up" => lamp_up = true,
            _ => positional.push(a),
        }
    }
    let stub_name = positional
        .first()
        .cloned()
        .ok_or("usage: stub_interactive <stub_name> <out.litematic> [--lamp-up]")?;
    let out_path = positional
        .get(1)
        .cloned()
        .ok_or("usage: stub_interactive <stub_name> <out.litematic> [--lamp-up]")?;

    let stub_lib_path = workspace_root().join("stub_lib").join("core");
    eprintln!("Loading stubs from {}...", stub_lib_path.display());
    let lib = load_stub_library(&stub_lib_path)?;
    let stub = lib
        .get(&stub_name)
        .ok_or_else(|| format!("stub {stub_name:?} not in library; have {:?}", lib.names()))?;
    eprintln!(
        "Stub {:?}: bbox {:?}, {} blocks, {} ports (lamp_up={})",
        stub.name,
        stub.bbox,
        stub.blocks.len(),
        stub.ports.len(),
        lamp_up
    );
    for p in &stub.ports {
        eprintln!(
            "  port {:?} {:?} width={} facing={:?}",
            p.name, p.dir, p.width, p.facing
        );
    }

    let world_origin = Pos3::ORIGIN;
    let stub_blocks: BTreeMap<Pos3, _> = stub
        .blocks
        .iter()
        .map(|(p, st)| (to_world(*p, stub.bbox.min, world_origin), st.clone()))
        .collect();

    let mut bbox = world_bbox_of(&stub_blocks);
    for port in &stub.ports {
        for i in 0..port.width {
            let pin_world = to_world(port.pin_pos(i), stub.bbox.min, world_origin);
            let attach = step(pin_world, port.facing);
            bbox = bbox.union(&Bbox3::point(attach));
            if lamp_up && port.dir == PortDir::Out {
                bbox = bbox.union(&Bbox3::point(step(attach, Direction::Up)));
            }
        }
    }
    let bounds = expand_bbox(bbox, BBOX_PAD);
    eprintln!("World bounds (padded {}): {:?}", BBOX_PAD, bounds);

    let mut grid = BlockGrid::empty(bounds);
    for (pos, state) in &stub_blocks {
        if bounds.contains(*pos) {
            grid.insert(*pos, state.clone());
        }
    }

    let mut levers = 0usize;
    for port in inputs(stub) {
        for i in 0..port.width {
            let pin_world = to_world(port.pin_pos(i), stub.bbox.min, world_origin);
            let attach = step(pin_world, port.facing);
            if !bounds.contains(attach) {
                continue;
            }
            grid.insert(attach, make_wall_lever(port.facing));
            levers += 1;
        }
        eprintln!(
            "  port {:?} (in): attached {} levers on {:?} face",
            port.name, port.width, port.facing
        );
    }

    let mut lamps = 0usize;
    for port in outputs(stub) {
        for i in 0..port.width {
            let pin_world = to_world(port.pin_pos(i), stub.bbox.min, world_origin);
            let mut attach = step(pin_world, port.facing);
            if lamp_up {
                attach = step(attach, Direction::Up);
            }
            if !bounds.contains(attach) {
                continue;
            }
            grid.insert(attach, block_state_for(BlockId::RedstoneLamp, None));
            lamps += 1;
        }
        eprintln!(
            "  port {:?} (out): placed {} lamps on {:?} face{}",
            port.name,
            port.width,
            port.facing,
            if lamp_up { " (+up)" } else { "" }
        );
    }
    eprintln!("Total: {levers} levers, {lamps} lamps");

    let out = PathBuf::from(out_path);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let title = format!("v3_{}_interactive", stub.name);
    write_litematic(
        &out,
        &grid,
        &title,
        &format!(
            "RedstoneBuilder v3 interactive harness for stub {:?}{}",
            stub.name,
            if lamp_up { " (lamp_up)" } else { "" }
        ),
    )?;
    eprintln!("Wrote {}", out.display());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

fn inputs(stub: &Stub) -> impl Iterator<Item = &StubPort> {
    stub.ports.iter().filter(|p| p.dir == PortDir::In)
}
fn outputs(stub: &Stub) -> impl Iterator<Item = &StubPort> {
    stub.ports.iter().filter(|p| p.dir == PortDir::Out)
}

fn to_world(local: Pos3, bbox_min: Pos3, origin: Pos3) -> Pos3 {
    Pos3::new(
        local.x - bbox_min.x + origin.x,
        local.y - bbox_min.y + origin.y,
        local.z - bbox_min.z + origin.z,
    )
}

fn step(p: Pos3, dir: Direction) -> Pos3 {
    let (dx, dy, dz) = dir.offset();
    Pos3::new(p.x + dx, p.y + dy, p.z + dz)
}

fn make_wall_lever(port_facing: Direction) -> rb_nbt::BlockState {
    let opp = opposite(port_facing);
    let mut state = block_state_for(BlockId::Lever, Some(opp));
    override_property(&mut state, "face", "wall");
    state
}

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East => Direction::West,
        Direction::West => Direction::East,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

fn world_bbox_of(blocks: &BTreeMap<Pos3, rb_nbt::BlockState>) -> Bbox3 {
    let mut iter = blocks.keys().copied();
    let Some(first) = iter.next() else {
        return Bbox3::point(Pos3::ORIGIN);
    };
    let mut bb = Bbox3::point(first);
    for p in iter {
        bb = bb.union(&Bbox3::point(p));
    }
    bb
}

fn expand_bbox(b: Bbox3, n: i32) -> Bbox3 {
    Bbox3 {
        min: Pos3::new(b.min.x - n, b.min.y - n, b.min.z - n),
        max: Pos3::new(b.max.x + n, b.max.y + n, b.max.z + n),
    }
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}
