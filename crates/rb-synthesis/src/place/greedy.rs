//! Row-based deterministic placement (FR-016).
//!
//! Strategy: topologically sort the netlist, group nodes by depth, lay
//! each depth as a row along +X. Rows stack along +Z with a fixed
//! per-row gap for routed wires. The world-Y of each cell is its
//! macro-cell's local-Y origin (i.e., cells sit on the build floor;
//! cells with positive local-Y extend upward).
//!
//! Bbox-enforcement against `--max-footprint` is FR-016 territory and
//! lives in US3 (T062); v1 always passes `PlaceConfig::MAX_PERMISSIVE`.

use std::collections::BTreeMap;

use petgraph::algo::toposort;
use petgraph::graph::NodeIndex as ProjNodeIndex;
use petgraph::stable_graph::NodeIndex;

use crate::cycle::build_projection;
use rb_core::{Bbox3, Pos3};
use serde::Serialize;

use crate::cell_library::{macrocell_for, Macrocell};
use crate::error::{PlaceError, SynthError};
use crate::netlist::{Netlist, NetlistNode};

/// Configuration knobs for placement.
#[derive(Debug, Clone, Copy)]
pub struct PlaceConfig {
    /// Maximum permitted footprint (W × H × D) in blocks.
    pub max_footprint: (u32, u32, u32),
    /// Gap, in blocks, inserted between two rows of cells along +Z.
    /// The router uses this gap for inter-row wires.
    pub row_gap_z: i32,
}

impl PlaceConfig {
    /// Generous default: 256×<MC build height>×256 per FR-016, and a
    /// 4-block row gap (room for a few wires per row).
    pub const DEFAULT: Self = Self {
        max_footprint: (256, 384, 256),
        row_gap_z: 4,
    };

    /// Effectively-unbounded footprint, for tests.
    pub const MAX_PERMISSIVE: Self = Self {
        max_footprint: (i32::MAX as u32, i32::MAX as u32, i32::MAX as u32),
        row_gap_z: 4,
    };
}

/// One placed gate instance, in world coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct PlacedCell {
    /// Back-reference to the source node in [`Netlist::graph`].
    #[serde(skip)]
    pub node: NodeIndex,
    /// World-coordinate origin of the cell's local frame.
    pub origin: Pos3,
    /// World-coordinate bounding box.
    pub footprint: Bbox3,
    /// Tick-delay through the cell (FR-007 / cell library).
    pub tick_delay: u8,
    /// The macro-cell definition (cloned for ease of use downstream).
    pub macro_cell: Macrocell,
}

/// Output of placement.
#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    /// Overall world-coordinate bounding box (union of all cells).
    pub bounds: Bbox3,
    /// Cells in placement order (= topological order).
    pub cells: Vec<PlacedCell>,
}

/// Place every gate in the netlist into world coordinates.
pub fn place(netlist: &Netlist, cfg: &PlaceConfig) -> Result<Placement, PlaceError> {
    let (proj, back) = build_projection(netlist);
    let proj_order = toposort(&proj, None).unwrap_or_else(|_| proj.node_indices().collect());
    let order: Vec<NodeIndex> = proj_order
        .into_iter()
        .filter_map(|p: ProjNodeIndex| back.get(&p).copied())
        .collect();

    let depths = compute_depths(netlist);

    let mut by_depth: BTreeMap<u32, Vec<NodeIndex>> = BTreeMap::new();
    for n in order {
        if !is_gate(&netlist.graph[n]) {
            continue;
        }
        let d = depths.get(&n).copied().unwrap_or(0);
        by_depth.entry(d).or_default().push(n);
    }

    let mut cells: Vec<PlacedCell> = Vec::new();
    let mut cursor_z: i32 = 0;
    let mut overall: Option<Bbox3> = None;

    for (_d, group) in by_depth {
        let mut cursor_x: i32 = 0;
        let mut row_max_z_extent: i32 = 0;

        for node in group {
            let (kind, repeater_delay, compare_mode) = match &netlist.graph[node] {
                NetlistNode::Gate {
                    kind,
                    repeater_delay,
                    compare_mode,
                    ..
                } => (*kind, *repeater_delay, *compare_mode),
                _ => continue,
            };
            let mut macro_cell =
                macrocell_for(kind).map_err(|e: SynthError| PlaceError::TooLarge {
                    actual: format!("(internal synth error: {e})"),
                    bound: "n/a".to_string(),
                })?;
            // v2: thread per-instance attributes from the netlist node
            // onto the macro-cell so the NBT writer can patch the
            // resulting block-state.
            if repeater_delay.is_some() {
                macro_cell.repeater_delay = repeater_delay;
            }
            if compare_mode.is_some() {
                macro_cell.comparator_mode = compare_mode;
            }

            let origin = Pos3::new(cursor_x, 0, cursor_z);
            let footprint = Bbox3 {
                min: origin,
                max: Pos3::new(
                    origin.x + macro_cell.bbox.max.x,
                    origin.y + macro_cell.bbox.max.y,
                    origin.z + macro_cell.bbox.max.z,
                ),
            };

            cursor_x = footprint.max.x + 2;
            row_max_z_extent = row_max_z_extent.max(macro_cell.bbox.max.z);

            overall = Some(match overall {
                Some(b) => b.union(&footprint),
                None => footprint,
            });
            cells.push(PlacedCell {
                node,
                origin,
                footprint,
                tick_delay: macro_cell.tick_delay,
                macro_cell,
            });
        }

        cursor_z += row_max_z_extent + 1 + cfg.row_gap_z;
    }

    let bounds = overall.unwrap_or(Bbox3 {
        min: Pos3::ORIGIN,
        max: Pos3::ORIGIN,
    });

    let (w, h, d) = (bounds.width(), bounds.height(), bounds.depth());
    let (bw, bh, bd) = cfg.max_footprint;
    if w > bw || h > bh || d > bd {
        return Err(PlaceError::TooLarge {
            actual: format!("{w}×{h}×{d}"),
            bound: format!("{bw}×{bh}×{bd}"),
        });
    }

    Ok(Placement { bounds, cells })
}

fn compute_depths(netlist: &Netlist) -> BTreeMap<NodeIndex, u32> {
    let mut depths: BTreeMap<NodeIndex, u32> = BTreeMap::new();
    let (proj, back) = build_projection(netlist);
    let proj_order = toposort(&proj, None).unwrap_or_else(|_| Vec::new());

    for p in proj_order {
        let Some(&n) = back.get(&p) else { continue };
        let mut d: u32 = 0;
        for pred_proj in proj.neighbors_directed(p, petgraph::Direction::Incoming) {
            if let Some(&pred) = back.get(&pred_proj) {
                d = d.max(depths.get(&pred).copied().unwrap_or(0) + 1);
            }
        }
        depths.insert(n, d);
    }
    depths
}

fn is_gate(n: &NetlistNode) -> bool {
    matches!(n, NetlistNode::Gate { .. })
}
