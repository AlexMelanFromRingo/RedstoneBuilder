//! PathFinder negotiated-congestion routing + bounded Rip-up & Reroute
//! on top of [`astar_route`] (FR-V16).
//!
//! Each outer iteration: rip up all previously-committed dust, route
//! every net via A* on the current cost map, then bump the
//! `present_cost` of every committed cell. If any cell ends up
//! claimed by two or more distinct nets ("over-used"), bump its
//! `history_cost` and retry. After `cfg.max_iterations` retries
//! without convergence, hard-abort with [`RouteError::ConvergenceExhausted`].

use std::collections::BTreeMap;

use rayon::prelude::*;
use rb_core::{Direction, Pos3};

use crate::error::RouteError;
use crate::grid3d::NetTag;
use crate::route::astar::astar_route;
use crate::route::cost::{CostMap, EdgeCostConfig};
use crate::route::lee::{RouteSegment, RoutedWire, MAX_SIGNAL};

/// Tunables for [`route_pathfinder`].
#[derive(Debug, Clone, Copy)]
pub struct PathFinderConfig {
    /// Edge-cost tuning.
    pub edge: EdgeCostConfig,
    /// Cap on PathFinder iterations (FR-V16).
    pub max_iterations: u32,
    /// `history_cost` bump per over-used cell per iteration.
    pub history_growth: u16,
    /// `present_cost` bump per committed cell per iteration.
    pub present_growth: u16,
    /// Cap on cells A* may explore per net before giving up.
    pub max_explored_cells: usize,
    /// If true, every committed dust cell shadows its 6 orthogonal
    /// neighbours so foreign nets can't sit one cell away and short
    /// in real MC (matches v1 `Grid3D::Obstructed` semantics). Pin
    /// terminals (`CostMap::pins`) are exempt. Default: true — turn
    /// off only for cosmetic/visualisation routing where MC
    /// correctness is not required.
    pub adjacency_isolation: bool,
    /// Early-stop guard: if the congestion metric (over-used +
    /// unrouted cells) fails to improve for this many consecutive
    /// iterations, abort instead of grinding out every
    /// `max_iterations`. A non-converging large design otherwise
    /// burns the full iteration budget for nothing. `0` disables it.
    pub stall_limit: u32,
}

impl PathFinderConfig {
    /// Tuned defaults for the v2 reference designs and up to ~1 000
    /// gates. For 10 000-gate inputs bump `max_iterations` and
    /// `max_explored_cells` accordingly.
    pub const DEFAULT: Self = Self {
        edge: EdgeCostConfig::DEFAULT,
        max_iterations: 8,
        history_growth: 8,
        present_growth: 2,
        max_explored_cells: 64_000,
        adjacency_isolation: true,
        stall_limit: 6,
    };
}

/// Run PathFinder on the given source→sink routes. Returns the routed
/// wires on success. The returned [`RoutedWire`] type is the same the
/// v1 router uses, so downstream pipeline code is uniform.
pub fn route_pathfinder(
    cost: &mut CostMap,
    routes: &[(NetTag, Pos3, Pos3)],
    cfg: &PathFinderConfig,
) -> Result<Vec<RoutedWire>, RouteError> {
    // Register every source and sink as a pin terminal so it stays
    // reachable through adjacency shadows that earlier nets may cast.
    for (_, source, sink) in routes {
        cost.mark_pin(*source);
        cost.mark_pin(*sink);
    }

    let mut attempt: u32 = 0;
    // Early-stop bookkeeping: best congestion seen, and how many
    // consecutive iterations have failed to beat it.
    let mut best_congestion: usize = usize::MAX;
    let mut stale: u32 = 0;

    loop {
        cost.clear_iteration_state();
        let mut committed: Vec<RoutedWire> = Vec::with_capacity(routes.len());
        let mut unrouted: Vec<String> = Vec::new();
        let mut owners: BTreeMap<Pos3, Vec<NetTag>> = BTreeMap::new();

        // Plan paths. Three strategies:
        //   adj_iso ON  → sequential, interleaved with commit so
        //                  later nets see shadow halo of earlier ones.
        //   adj_iso OFF, ≥32 routes → parallel A* on shared snapshot,
        //                              commit in deterministic order
        //                              afterwards.
        //   adj_iso OFF, <32 routes → sequential, interleaved with
        //                              commit (rayon overhead dominates).
        let paths: Vec<Option<Vec<Pos3>>> = if !cfg.adjacency_isolation && routes.len() >= 32 {
            routes
                .par_iter()
                .map(|(net, source, sink)| {
                    astar_route(
                        cost,
                        *net,
                        *source,
                        *sink,
                        &cfg.edge,
                        cfg.max_explored_cells,
                    )
                })
                .collect()
        } else {
            // Sequential interleaved A* + provisional commit so the
            // next A* in this iteration sees the obstruction. The
            // canonical commit phase below re-applies it with full
            // bookkeeping, so we wipe what we pre-stamped at the end.
            let mut out: Vec<Option<Vec<Pos3>>> = Vec::with_capacity(routes.len());
            for (net, source, sink) in routes {
                let p = astar_route(
                    cost,
                    *net,
                    *source,
                    *sink,
                    &cfg.edge,
                    cfg.max_explored_cells,
                );
                if let Some(ref path) = p {
                    if !path.is_empty() {
                        for &cell in path {
                            cost.assigned.insert(cell, *net);
                        }
                        if cfg.adjacency_isolation {
                            for &cell in path {
                                for d in Direction::ALL {
                                    let (dx, dy, dz) = d.offset();
                                    let n = Pos3::new(cell.x + dx, cell.y + dy, cell.z + dz);
                                    if cost.bounds.contains(n) && !cost.pins.contains(&n) {
                                        cost.mark_shadow(n, *net);
                                    }
                                }
                            }
                        }
                    }
                }
                out.push(p);
            }
            cost.assigned.clear();
            cost.shadows.clear();
            out
        };

        // Canonical commit phase: mutates `cost.assigned`,
        // `cost.shadows`, `cost.present_cost`. Order matches `routes`
        // for determinism.
        for ((net, _source, _sink), path) in routes.iter().zip(paths) {
            match path {
                Some(p) if !p.is_empty() => {
                    for &cell in &p {
                        cost.assigned.insert(cell, *net);
                        owners.entry(cell).or_default().push(*net);
                    }
                    // FR-V?? (parity with v1 FR-008): mark adjacency
                    // shadow so foreign nets can't sit one cell away
                    // and short to this dust in real MC. Skip when
                    // `cfg.adjacency_isolation` is off (cosmetic mode).
                    // Pin terminals are never shadowed — that would
                    // strand other nets at their own sources/sinks.
                    if cfg.adjacency_isolation {
                        for &cell in &p {
                            for d in Direction::ALL {
                                let (dx, dy, dz) = d.offset();
                                let n = Pos3::new(cell.x + dx, cell.y + dy, cell.z + dz);
                                if cost.bounds.contains(n) && !cost.pins.contains(&n) {
                                    cost.mark_shadow(n, *net);
                                }
                            }
                        }
                    }
                    cost.bump_present(p.iter().copied(), cfg.present_growth);
                    committed.push(RoutedWire {
                        net: *net,
                        segments: path_to_segments(&p),
                    });
                }
                _ => {
                    unrouted.push(format!("net#{}", net.0));
                }
            }
        }

        // Find over-used cells.
        let overused: Vec<Pos3> = owners
            .iter()
            .filter_map(|(p, ns)| {
                let mut deduped = ns.clone();
                deduped.sort_by_key(|n| n.0);
                deduped.dedup();
                if deduped.len() > 1 {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect();

        if overused.is_empty() && unrouted.is_empty() {
            return Ok(committed);
        }

        let _ = committed; // committed of failed iteration is discarded; future enrichment slot

        // Track convergence progress for the early-stop guard.
        let congestion = overused.len() + unrouted.len();
        if congestion < best_congestion {
            best_congestion = congestion;
            stale = 0;
        } else {
            stale = stale.saturating_add(1);
        }
        let stalled = cfg.stall_limit != 0 && stale >= cfg.stall_limit;

        if attempt >= cfg.max_iterations || stalled {
            return Err(RouteError::ConvergenceExhausted {
                unrouted: if unrouted.is_empty() {
                    overused.iter().map(|p| format!("overuse@{p:?}")).collect()
                } else {
                    unrouted
                },
                iterations: attempt,
                peak_congestion: overused.len() as u32,
            });
        }

        cost.bump_history(overused, cfg.history_growth);
        attempt = attempt.saturating_add(1);
    }
}

fn path_to_segments(path: &[Pos3]) -> Vec<RouteSegment> {
    // FR-V08 (generalises v1 FR-009): insert a Repeater on the 15th
    // consecutive dust cell so signal strength is refreshed to
    // MAX_SIGNAL and the destination always reads the same strength
    // the source produced. Mirrors v1 `lee::annotate_with_repeaters`.
    let mut segs: Vec<RouteSegment> = Vec::with_capacity(path.len());
    let mut run: u8 = 0;
    for i in 0..path.len() {
        let p = path[i];
        if run >= MAX_SIGNAL.saturating_sub(1) {
            let facing = facing_for(path, i);
            segs.push(RouteSegment::Repeater { pos: p, facing });
            run = 0;
        } else {
            segs.push(RouteSegment::Dust { pos: p });
            run = run.saturating_add(1);
        }
    }
    segs
}

fn facing_for(path: &[Pos3], i: usize) -> Direction {
    // MC convention: `facing` = direction of repeater BACK (input side) =
    // opposite of signal-flow direction. See `lee.rs::facing_for` doc.
    let prev = if i == 0 { path[0] } else { path[i - 1] };
    let cur = path[i];
    let dx = cur.x - prev.x;
    let dz = cur.z - prev.z;
    if dx > 0 {
        Direction::West
    } else if dx < 0 {
        Direction::East
    } else if dz > 0 {
        Direction::North
    } else if dz < 0 {
        Direction::South
    } else {
        Direction::West
    }
}
