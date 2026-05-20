//! Smoke test: load 2× `not_h8b` stubs, place them side-by-side, and
//! route 8 wires between stub-A's `out[0..8]` and stub-B's `in[0..8]`
//! through the v2 PathFinder router. Write the combined result as a
//! single `.litematic`.
//!
//! Logically this is a 16-bit NOT chain (two cascaded inverters), which
//! is the identity function — so when both inputs are set, all outputs
//! should glow. Useful as a "the wires connect at all" sanity check.
//!
//! Pipeline:
//!
//! ```text
//!   2× not_h8b stubs  ──┐
//!                       ├── place at world offsets ──┐
//!   Stub library  ──────┘                            │
//!                                                    ▼
//!                                           CostMap (block stubs)
//!                                                    │
//!                                                    ▼
//!                                       route_pathfinder(8 nets)
//!                                                    │
//!                                                    ▼
//!                                     materialise into BlockGrid
//!                                                    │
//!                                                    ▼
//!                                     write_litematic → .litematic
//! ```
//!
//! Usage:
//!     cargo run -p rb-stubs --example compose_two_stubs -- \
//!         out/v3_two_not_chain.litematic [num_bits]
//!
//! `num_bits` defaults to 4. Routing 8 parallel adjacent buses through
//! the current PathFinder is unreliable without adjacency-isolation in
//! the cost model — that's tracked separately as a v2 router gap.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;

use rb_core::{Bbox3, BlockId, Direction, Pos3};
use rb_nbt::{block_state_for, write_litematic, BlockGrid, BlockState};
use rb_stubs::{load_stub_library, PortDir, Stub, StubLibrary};
use rb_synthesis::{
    grid3d::NetTag,
    route::{
        cost::CostMap,
        lee::{RouteSegment, RoutedWire},
        pathfinder::{route_pathfinder, PathFinderConfig},
    },
};

/// Gap (in cells) between the two stubs along the X axis. Big enough
/// to give the router room to lay 8 parallel dust runs side-by-side
/// without obvious congestion.
const STUB_GAP: i32 = 18;

/// Padding around the world bbox so the cost map covers the routing
/// channel plus a vertical strip for repeater jogs.
const BBOX_PAD: i32 = 12;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse: <out> [num_bits] [--cosmetic]
    let mut positional: Vec<String> = Vec::new();
    let mut cosmetic = false;
    for a in env::args().skip(1) {
        match a.as_str() {
            "--cosmetic" => cosmetic = true,
            _ => positional.push(a),
        }
    }
    let out_path = positional
        .first()
        .cloned()
        .unwrap_or_else(|| "out/v3_two_not_chain.litematic".to_string());
    let num_bits: u32 = positional.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);

    let stub_lib_path = workspace_root().join("stub_lib").join("core");
    eprintln!("Loading stubs from {}...", stub_lib_path.display());
    let lib: StubLibrary = load_stub_library(&stub_lib_path)?;
    eprintln!("  loaded {} stubs", lib.len());

    let stub = lib
        .get("not_h8b")
        .ok_or("stub 'not_h8b' not in library — check stub_lib/core/")?;
    eprintln!(
        "Using stub {:?}: bbox {:?}, {} blocks, {} ports",
        stub.name,
        stub.bbox,
        stub.blocks.len(),
        stub.ports.len()
    );

    let port_in = stub.port("in").ok_or("not_h8b missing port 'in'")?;
    let port_out = stub.port("out").ok_or("not_h8b missing port 'out'")?;
    if port_in.dir != PortDir::In || port_out.dir != PortDir::Out {
        return Err(format!(
            "unexpected port dirs: in={:?} out={:?}",
            port_in.dir, port_out.dir
        )
        .into());
    }
    if port_in.width != port_out.width {
        return Err(format!(
            "width mismatch: in={} out={}",
            port_in.width, port_out.width
        )
        .into());
    }
    let stub_width = port_in.width;
    let width = num_bits.min(stub_width);
    eprintln!("  stub width = {stub_width} bits, routing {width} bit(s)");

    // ── 1. Place the two stub instances at distinct world origins ──
    let (sw, _sh, _sd) = stub.size();
    let stub_a_world = Pos3::new(0, 0, 0);
    let stub_b_world = Pos3::new(sw + STUB_GAP, 0, 0);
    eprintln!(
        "Placement: A @ {:?}, B @ {:?} (stub size = {}×{}×{}, gap = {})",
        stub_a_world, stub_b_world, sw, _sh, _sd, STUB_GAP
    );

    // ── 2. Materialise each stub into a (world_pos → BlockState) map ──
    let stub_a_blocks = stub_world_blocks(stub, stub_a_world);
    let stub_b_blocks = stub_world_blocks(stub, stub_b_world);

    // ── 3. Compute the union bbox + pin world positions ──
    let world_bbox = union_bbox(&stub_a_blocks, &stub_b_blocks).expand(BBOX_PAD);
    eprintln!("World bbox (padded by {}): {:?}", BBOX_PAD, world_bbox);

    let pin_pairs: Vec<(NetTag, Pos3, Pos3)> = (0..width)
        .map(|i| {
            let src_local = port_out.pin_pos(i);
            let dst_local = port_in.pin_pos(i);
            let src_world = to_world(src_local, stub.bbox.min, stub_a_world);
            let dst_world = to_world(dst_local, stub.bbox.min, stub_b_world);
            (NetTag(i), src_world, dst_world)
        })
        .collect();
    for (tag, s, d) in &pin_pairs {
        eprintln!("  net#{}: {:?} → {:?}", tag.0, s, d);
    }

    // ── 4. Build the cost map: every stub interior cell is Blocked
    //       (except the pin cells themselves, which the router needs
    //       to enter/exit). ──
    let mut cost = CostMap::new(world_bbox);
    let pin_cells: BTreeSet<Pos3> = pin_pairs.iter().flat_map(|(_, s, d)| [*s, *d]).collect();
    for &p in stub_a_blocks.keys().chain(stub_b_blocks.keys()) {
        if !pin_cells.contains(&p) {
            cost.mark_blocked(p);
        }
    }

    // ── 5. Route via PathFinder. ──
    let pf_cfg = PathFinderConfig {
        max_iterations: 64,
        max_explored_cells: 512_000,
        history_growth: 16,
        present_growth: 4,
        adjacency_isolation: !cosmetic,
        ..PathFinderConfig::DEFAULT
    };
    eprintln!(
        "Routing {} nets with PathFinder (max_iter={}, max_explore={}, adj_iso={})…",
        pin_pairs.len(),
        pf_cfg.max_iterations,
        pf_cfg.max_explored_cells,
        pf_cfg.adjacency_isolation,
    );
    let wires = match route_pathfinder(&mut cost, &pin_pairs, &pf_cfg) {
        Ok(w) => {
            eprintln!("  routed {} nets", w.len());
            w
        }
        Err(e) => {
            eprintln!("Routing failed: {e}");
            return Err(Box::new(e));
        }
    };

    // ── 6. Materialise everything into a BlockGrid. ──
    let mut grid = BlockGrid::empty(world_bbox);
    let mut owned: BTreeMap<Pos3, ()> = BTreeMap::new();

    // Stub blocks first (they are the gate primitives).
    for (pos, state) in stub_a_blocks.iter().chain(stub_b_blocks.iter()) {
        if world_bbox.contains(*pos) {
            grid.insert(*pos, state.clone());
            owned.insert(*pos, ());
        }
    }

    // Then routed wires with support stone underneath.
    for wire in &wires {
        for seg in &wire.segments {
            let (pos, block, facing) = match *seg {
                RouteSegment::Dust { pos } => (pos, BlockId::RedstoneDust, None),
                RouteSegment::Repeater { pos, facing } => (pos, BlockId::Repeater, Some(facing)),
            };
            if !world_bbox.contains(pos) || owned.contains_key(&pos) {
                continue;
            }
            grid.insert(pos, block_state_for(block, facing));
            owned.insert(pos, ());

            let support = Pos3::new(pos.x, pos.y - 1, pos.z);
            if world_bbox.contains(support) {
                if let std::collections::btree_map::Entry::Vacant(e) = owned.entry(support) {
                    grid.insert(support, block_state_for(BlockId::Stone, None));
                    e.insert(());
                }
            }
        }
    }
    let _ = wire_count(&wires);

    // ── 7. Write the .litematic. ──
    let out = PathBuf::from(out_path);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    write_litematic(
        &out,
        &grid,
        "v3_two_not_chain",
        "RedstoneBuilder v3: two not_h8b stubs composed via PathFinder router",
    )?;
    eprintln!("Wrote {}", out.display());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Translate a stub-local position into world coords for the instance
/// placed at `world_origin`. Stub coords may not start at 0 — we offset
/// by `bbox_min` so the lowest-corner cell lands on `world_origin`.
fn to_world(local: Pos3, bbox_min: Pos3, world_origin: Pos3) -> Pos3 {
    Pos3::new(
        local.x - bbox_min.x + world_origin.x,
        local.y - bbox_min.y + world_origin.y,
        local.z - bbox_min.z + world_origin.z,
    )
}

/// Build a `(world_pos → BlockState)` map for one stub instance.
fn stub_world_blocks(stub: &Stub, world_origin: Pos3) -> BTreeMap<Pos3, BlockState> {
    stub.blocks
        .iter()
        .map(|(p, st)| (to_world(*p, stub.bbox.min, world_origin), st.clone()))
        .collect()
}

/// Smallest bbox covering both block maps. Returns a unit bbox at the
/// origin when both maps are empty (shouldn't happen for real stubs but
/// keeps the function total).
fn union_bbox(a: &BTreeMap<Pos3, BlockState>, b: &BTreeMap<Pos3, BlockState>) -> Bbox3 {
    let mut iter = a.keys().chain(b.keys()).copied();
    let Some(first) = iter.next() else {
        return Bbox3::point(Pos3::ORIGIN);
    };
    let mut bb = Bbox3::point(first);
    for p in iter {
        bb = bb.union(&Bbox3::point(p));
    }
    bb
}

/// Pad the bbox by `n` cells on every face (clamped to non-negative Y).
trait Bbox3Ext {
    fn expand(self, n: i32) -> Bbox3;
}
impl Bbox3Ext for Bbox3 {
    fn expand(self, n: i32) -> Bbox3 {
        Bbox3 {
            min: Pos3::new(self.min.x - n, (self.min.y - n).max(-64), self.min.z - n),
            max: Pos3::new(self.max.x + n, self.max.y + n, self.max.z + n),
        }
    }
}

/// Sum of segment counts across all routed wires (for logging).
fn wire_count(wires: &[RoutedWire]) -> usize {
    let n = wires.iter().map(|w| w.segments.len()).sum::<usize>();
    eprintln!("  total routed cells: {n} across {} wires", wires.len());
    let _ = Direction::ALL;
    n
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p
}
