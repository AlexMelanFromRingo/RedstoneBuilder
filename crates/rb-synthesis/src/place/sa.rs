//! Simulated-annealing placer (v2).
//!
//! Starts from the v1 row-based greedy placement and iteratively swaps
//! cell positions to lower the per-net half-perimeter wire length
//! (HPWL). The RNG is a seeded `ChaCha8Rng` so the same inputs +
//! `SaConfig.seed` always produce bit-identical output (FR-V13 /
//! v1 FR-017).
//!
//! Proposal order is indexed by `(epoch_idx, swap_idx)`, not by
//! wall-clock — the algorithm is deterministic across machines.

use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

use rb_core::{Bbox3, Pos3};

use crate::error::PlaceError;
use crate::netlist::Netlist;
use crate::place::greedy;
use crate::place::greedy::{PlaceConfig, PlacedCell, Placement};

/// Tunables for the SA placer.
#[derive(Debug, Clone, Copy)]
pub struct SaConfig {
    /// RNG seed (FR-V13).
    pub seed: u64,
    /// Starting temperature. Higher → more exploration.
    pub initial_temp: f64,
    /// Multiplicative cooling per epoch (0 < r < 1).
    pub cooling_rate: f64,
    /// Outer-loop iteration count.
    pub epochs: u32,
    /// Swap proposals per epoch.
    pub swaps_per_epoch: u32,
}

impl SaConfig {
    /// Tuned for designs ≤ a few hundred gates. For 10 000-gate inputs
    /// (SC-V03), bump `epochs` and `swaps_per_epoch` accordingly via
    /// the CLI in Phase 6.
    pub const DEFAULT: Self = Self {
        seed: crate::DEFAULT_SEED,
        initial_temp: 10.0,
        cooling_rate: 0.95,
        epochs: 50,
        swaps_per_epoch: 100,
    };
}

/// Place cells via simulated annealing.
///
/// Determinism contract: for fixed `(netlist, cfg, bounds)` the
/// returned [`Placement`] is element-identical across runs and across
/// machines.
pub fn place_simulated_annealing(
    netlist: &Netlist,
    cfg: &SaConfig,
    bounds: &PlaceConfig,
) -> Result<Placement, PlaceError> {
    let mut placement = greedy::place(netlist, bounds)?;
    if placement.cells.len() < 2 {
        return Ok(placement);
    }

    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut current_cost = hpwl_cost(&placement, netlist);
    let mut temp = cfg.initial_temp;
    let n = placement.cells.len();

    for _epoch in 0..cfg.epochs {
        for _swap_idx in 0..cfg.swaps_per_epoch {
            let i = (rng.next_u32() as usize) % n;
            let j = (rng.next_u32() as usize) % n;
            if i == j {
                continue;
            }

            // Snapshot originals so we can revert if rejected.
            let oi = placement.cells[i].origin;
            let oj = placement.cells[j].origin;

            swap_origins(&mut placement.cells, i, j);
            let new_cost = hpwl_cost(&placement, netlist);
            let delta = new_cost - current_cost;

            let accept = if delta <= 0.0 {
                true
            } else {
                let r = (rng.next_u32() as f64) / (u32::MAX as f64);
                r < (-delta / temp).exp()
            };

            if accept {
                current_cost = new_cost;
            } else {
                restore_origins(&mut placement.cells, i, j, oi, oj);
            }
        }
        temp *= cfg.cooling_rate;
    }

    // Recompute the union bbox from final positions.
    placement.bounds = placement
        .cells
        .iter()
        .map(|c| c.footprint)
        .reduce(|a, b| a.union(&b))
        .unwrap_or(Bbox3::point(Pos3::ORIGIN));

    Ok(placement)
}

fn swap_origins(cells: &mut [PlacedCell], i: usize, j: usize) {
    let oi = cells[i].origin;
    let oj = cells[j].origin;
    set_origin(&mut cells[i], oj);
    set_origin(&mut cells[j], oi);
}

fn restore_origins(cells: &mut [PlacedCell], i: usize, j: usize, oi: Pos3, oj: Pos3) {
    set_origin(&mut cells[i], oi);
    set_origin(&mut cells[j], oj);
}

fn set_origin(cell: &mut PlacedCell, new_origin: Pos3) {
    cell.origin = new_origin;
    cell.footprint = Bbox3 {
        min: new_origin,
        max: Pos3::new(
            new_origin.x + cell.macro_cell.bbox.max.x,
            new_origin.y + cell.macro_cell.bbox.max.y,
            new_origin.z + cell.macro_cell.bbox.max.z,
        ),
    };
}

/// Sum of per-net half-perimeter wire length. Each net's contribution
/// is `(max_x - min_x) + (max_z - min_z)` over the placed cells of all
/// gate endpoints touching the net. Y is ignored — placement is
/// effectively 2D in the SA cost model.
fn hpwl_cost(placement: &Placement, netlist: &Netlist) -> f64 {
    use petgraph::stable_graph::NodeIndex;
    use std::collections::BTreeMap;

    let cell_origin: BTreeMap<NodeIndex, Pos3> =
        placement.cells.iter().map(|c| (c.node, c.origin)).collect();

    let mut total: f64 = 0.0;
    for edge_idx in netlist.graph.edge_indices() {
        let net = netlist.graph[edge_idx].net;
        let Some((src, dst)) = netlist.graph.edge_endpoints(edge_idx) else {
            continue;
        };

        // A single net spans more than one edge in general, but HPWL
        // is summed over all edges as a cheap, monotonic approximation
        // — moving one cell closer to another linked one always
        // reduces the sum. For the SA cost it is dominant by orders
        // of magnitude over the exact bounding-box formulation, but
        // much cheaper to compute (O(E) instead of O(E × V_in_net)).
        let _ = net;
        let p_src = cell_origin.get(&src);
        let p_dst = cell_origin.get(&dst);
        if let (Some(a), Some(b)) = (p_src, p_dst) {
            total += f64::from((a.x - b.x).abs() + (a.z - b.z).abs());
        }
    }
    total
}
