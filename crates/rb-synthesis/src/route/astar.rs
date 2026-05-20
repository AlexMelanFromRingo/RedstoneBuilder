//! Per-net A* on the [`CostMap`] (v2).
//!
//! Manhattan-distance heuristic × minimum-edge-cost, so the search is
//! admissible. Deterministic tiebreak: `(g + h, y, z, x)` lexicographic
//! order on the priority queue entry, matching v1's BFS contract.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use rb_core::{Direction, Pos3};

use crate::grid3d::NetTag;
use crate::route::cost::{CostMap, EdgeCostConfig};

/// Run A* from `source` to `sink` on the cost map. Returns the
/// reconstructed path (source-first, sink-last) on success, `None` if
/// no path within `max_explored_cells` cells.
pub fn astar_route(
    cost: &CostMap,
    net: NetTag,
    source: Pos3,
    sink: Pos3,
    cfg: &EdgeCostConfig,
    max_explored_cells: usize,
) -> Option<Vec<Pos3>> {
    astar_route_multisrc(cost, net, &[source], sink, cfg, max_explored_cells)
}

/// Multi-source variant of [`astar_route`]. Every cell in `sources`
/// gets `g_score = 0` and is seeded in the initial frontier, so the
/// algorithm finds the cheapest path from the **nearest** source to
/// `sink`. Used for Steiner-tree fan-out: after routing the first sink
/// of a net, seed the second-sink search with every cell on the first
/// path so the second branch attaches to the existing trunk instead of
/// retracing from the original source.
///
/// The returned path runs from the actual source (the seed cell from
/// which the winning chain originated) to `sink`, source-first.
pub fn astar_route_multisrc(
    cost: &CostMap,
    net: NetTag,
    sources: &[Pos3],
    sink: Pos3,
    cfg: &EdgeCostConfig,
    max_explored_cells: usize,
) -> Option<Vec<Pos3>> {
    if sources.is_empty() {
        return None;
    }
    if sources.contains(&sink) {
        return Some(vec![sink]);
    }

    let mut frontier: BinaryHeap<Entry> = BinaryHeap::new();
    let mut g_score: BTreeMap<Pos3, u32> = BTreeMap::new();
    let mut came_from: BTreeMap<Pos3, (Pos3, Direction)> = BTreeMap::new();
    // Mark seed cells so reconstruct knows where to stop (path may
    // start at any of them; whichever the came_from chain bottoms out at).
    let mut seed_set: BTreeMap<Pos3, ()> = BTreeMap::new();
    for &src in sources {
        seed_set.insert(src, ());
        g_score.insert(src, 0);
        frontier.push(Entry {
            priority: Reverse((heuristic(src, sink), src.y, src.z, src.x)),
            pos: src,
            prev_dir: None,
        });
    }
    let source_for_compat = sources[0];

    let mut explored = 0usize;
    while let Some(Entry { pos, prev_dir, .. }) = frontier.pop() {
        if pos == sink {
            return Some(reconstruct_multi(
                &seed_set,
                source_for_compat,
                sink,
                &came_from,
            ));
        }
        explored += 1;
        if explored > max_explored_cells {
            return None;
        }
        let g_pos = *g_score.get(&pos).unwrap_or(&0);

        for dir in Direction::ALL {
            let next = pos.step(dir);
            let edge = cost.cost_for_edge(pos, prev_dir, dir, next, net, cfg);
            if edge == u32::MAX {
                continue;
            }
            let tentative_g = g_pos.saturating_add(edge);

            let better = g_score
                .get(&next)
                .is_none_or(|&existing| tentative_g < existing);
            if !better {
                continue;
            }
            g_score.insert(next, tentative_g);
            came_from.insert(next, (pos, dir));

            let priority = Reverse((
                tentative_g.saturating_add(heuristic(next, sink)),
                next.y,
                next.z,
                next.x,
            ));
            frontier.push(Entry {
                priority,
                pos: next,
                prev_dir: Some(dir),
            });
        }
    }

    None
}

#[derive(PartialEq, Eq)]
struct Entry {
    priority: Reverse<(u32, i32, i32, i32)>,
    pos: Pos3,
    prev_dir: Option<Direction>,
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

fn heuristic(p: Pos3, sink: Pos3) -> u32 {
    let dx = (p.x - sink.x).unsigned_abs();
    let dy = (p.y - sink.y).unsigned_abs();
    let dz = (p.z - sink.z).unsigned_abs();
    dx.saturating_add(dy).saturating_add(dz)
}

/// Reconstruct a path that ends at `sink` and starts at whichever seed
/// cell the came_from chain bottoms out at. Falls back to
/// `single_source_compat` if no seed cell is reachable from the chain
/// (shouldn't happen for valid A* runs; defensive only).
fn reconstruct_multi(
    seeds: &BTreeMap<Pos3, ()>,
    single_source_compat: Pos3,
    sink: Pos3,
    came_from: &BTreeMap<Pos3, (Pos3, Direction)>,
) -> Vec<Pos3> {
    let mut path = vec![sink];
    let mut cur = sink;
    loop {
        if seeds.contains_key(&cur) {
            break;
        }
        let Some(&(prev, _)) = came_from.get(&cur) else {
            // Defensive fallback: walk all the way to compat source.
            while cur != single_source_compat {
                let Some(&(prev, _)) = came_from.get(&cur) else {
                    break;
                };
                cur = prev;
                path.push(cur);
            }
            break;
        };
        cur = prev;
        path.push(cur);
    }
    path.reverse();
    path
}
