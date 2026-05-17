//! 3D maze router (Lee's algorithm) with adjacency-aware obstruction
//! and automatic repeater insertion.
//!
//! Implements FR-008 (no short-circuit between adjacent nets) and
//! FR-009 (auto-repeater after 15 blocks of dust). FR-015's bbox
//! auto-expansion retry lives in US3 (T061); this module currently
//! does a single attempt.

use std::collections::{BTreeMap, BinaryHeap};

use rb_core::{Direction, Pos3};
use serde::Serialize;

use crate::error::RouteError;
use crate::grid3d::{CellState, Grid3D, NetTag};

/// Maximum signal strength a redstone source produces in vanilla MC.
pub const MAX_SIGNAL: u8 = 15;

/// Tunables for `route_with_retry`.
#[derive(Debug, Clone, Copy)]
pub struct RouteConfig {
    /// Manhattan-distance cap applied to BFS on the first attempt. A
    /// smaller value forces compact routes; the retry loop grows it on
    /// each failure.
    pub initial_max_distance: u32,
    /// How many additional cells of slack each retry adds to the cap.
    pub bbox_grow_step: u32,
    /// Hard cap on the number of retries before [`RouteError::Exhausted`].
    pub max_bbox_retries: u8,
    /// Seed for any randomized step in the router (none in v1; reserved
    /// for forward-compat per FR-017).
    pub seed: u64,
}

impl RouteConfig {
    /// Tuned for v1 reference designs (half-adder, dff, full-adder ≤ ~10
    /// gates). Initial cap 64 keeps per-net BFS bounded (≈ 64³ cells
    /// worst case ≈ 260k visits — fraction of a second in release).
    /// Grow step doubles the explorable sphere per retry; 4 retries
    /// reaches 64+4·64 = 320 Manhattan distance, comfortably covering
    /// the 256³ default `--max-footprint` bound diagonally.
    pub const DEFAULT: Self = Self {
        initial_max_distance: 64,
        bbox_grow_step: 64,
        max_bbox_retries: 4,
        seed: crate::DEFAULT_SEED,
    };
}

/// One segment of a routed wire, in world coords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RouteSegment {
    /// Plain redstone dust at this position.
    Dust {
        /// World position.
        pos: Pos3,
    },
    /// Repeater (with facing) at this position. Resets signal strength
    /// to [`MAX_SIGNAL`] downstream.
    Repeater {
        /// World position.
        pos: Pos3,
        /// Direction the repeater output points.
        facing: Direction,
    },
}

/// A successfully routed wire (one net, one source → one sink).
#[derive(Debug, Clone, Serialize)]
pub struct RoutedWire {
    /// Net the wire belongs to.
    pub net: NetTag,
    /// Wire segments, in source-first order.
    pub segments: Vec<RouteSegment>,
}

/// Outcome of routing a single net.
#[derive(Debug, Clone)]
pub enum SingleRouteOutcome {
    /// BFS found a path and obstructions were committed to the grid.
    Routed(RoutedWire),
    /// No path exists in the current grid.
    Unroutable,
}

/// Route one source→sink pair for the given net on `grid` with no
/// distance cap. On success, the path is committed to `grid` (dust +
/// obstructions) and returned.
pub fn route_single(
    grid: &mut Grid3D,
    net: NetTag,
    source: Pos3,
    sink: Pos3,
) -> SingleRouteOutcome {
    route_single_bounded(grid, net, source, sink, u32::MAX)
}

/// Like [`route_single`], but BFS gives up once any explored path
/// exceeds `max_distance` Manhattan steps from `source`. Used by
/// [`route_with_retry`] to enforce a tightening bbox.
pub fn route_single_bounded(
    grid: &mut Grid3D,
    net: NetTag,
    source: Pos3,
    sink: Pos3,
    max_distance: u32,
) -> SingleRouteOutcome {
    let path = match bfs(grid, net, source, sink, max_distance) {
        Some(p) => p,
        None => return SingleRouteOutcome::Unroutable,
    };

    let segments = annotate_with_repeaters(&path);
    commit_route(grid, net, &segments);
    SingleRouteOutcome::Routed(RoutedWire { net, segments })
}

/// Route a whole list of nets in input order. Each net is one
/// source→sink pair. Multi-sink nets should be expanded to multiple
/// (source, sink) entries with the same `NetTag` — the router will
/// reuse existing dust of that net as additional starting points.
pub fn route_all(
    grid: &mut Grid3D,
    routes: &[(NetTag, Pos3, Pos3)],
) -> Result<Vec<RoutedWire>, RouteError> {
    let mut wires: Vec<RoutedWire> = Vec::with_capacity(routes.len());
    let mut unrouted_names: Vec<String> = Vec::new();

    for (net, source, sink) in routes {
        match route_single(grid, *net, *source, *sink) {
            SingleRouteOutcome::Routed(w) => wires.push(w),
            SingleRouteOutcome::Unroutable => {
                unrouted_names.push(format!("net#{}", net.0));
            }
        }
    }

    if !unrouted_names.is_empty() {
        return Err(RouteError::Exhausted {
            unrouted: unrouted_names,
            retries: 0,
            final_bbox: "n/a".to_string(),
        });
    }
    Ok(wires)
}

/// Route a whole list of nets with bbox-expansion retry (FR-015).
///
/// `template_grid` is the immutable starting state of the obstruction
/// grid — typically pre-populated with the placed cells' `Solid` cells.
/// Each retry clones the template, sets a tighter or looser distance
/// cap on the BFS, and attempts every net afresh.
///
/// Returns the populated grid + the list of routed wires. On
/// exhausting `cfg.max_bbox_retries` without success, returns
/// [`RouteError::Exhausted`] with the names of nets that could not be
/// routed on the final attempt.
pub fn route_with_retry(
    template_grid: &Grid3D,
    routes: &[(NetTag, Pos3, Pos3)],
    cfg: &RouteConfig,
) -> Result<(Grid3D, Vec<RoutedWire>), RouteError> {
    let mut attempt: u8 = 0;

    loop {
        let mut grid = template_grid.clone();
        let mut wires: Vec<RoutedWire> = Vec::with_capacity(routes.len());
        let mut unrouted: Vec<String> = Vec::new();

        let distance_cap = cfg
            .initial_max_distance
            .saturating_add(u32::from(attempt).saturating_mul(cfg.bbox_grow_step));

        for (net, source, sink) in routes {
            match route_single_bounded(&mut grid, *net, *source, *sink, distance_cap) {
                SingleRouteOutcome::Routed(w) => wires.push(w),
                SingleRouteOutcome::Unroutable => unrouted.push(format!("net#{}", net.0)),
            }
        }

        if unrouted.is_empty() {
            return Ok((grid, wires));
        }

        if attempt >= cfg.max_bbox_retries {
            return Err(RouteError::Exhausted {
                unrouted,
                retries: attempt,
                final_bbox: format!("max_distance={distance_cap}"),
            });
        }
        attempt = attempt.saturating_add(1);
    }
}

/// 3D BFS that yields the shortest Manhattan path on the obstruction
/// grid (tie-breaks deterministically by `(y, z, x)` lex order). Gives
/// up once any explored path exceeds `max_distance` steps from `source`.
fn bfs(
    grid: &Grid3D,
    net: NetTag,
    source: Pos3,
    sink: Pos3,
    max_distance: u32,
) -> Option<Vec<Pos3>> {
    if source == sink {
        return Some(vec![source]);
    }

    let mut frontier: BinaryHeap<Entry> = BinaryHeap::new();
    let mut parent: BTreeMap<Pos3, Pos3> = BTreeMap::new();
    let mut visited: BTreeMap<Pos3, ()> = BTreeMap::new();

    frontier.push(Entry {
        priority: std::cmp::Reverse((0u32, source.y, source.z, source.x)),
        pos: source,
    });
    visited.insert(source, ());

    while let Some(Entry {
        priority: std::cmp::Reverse((dist, _, _, _)),
        pos,
    }) = frontier.pop()
    {
        if dist > max_distance {
            return None;
        }
        for dir in Direction::ALL {
            let next = pos.step(dir);
            if visited.contains_key(&next) {
                continue;
            }
            if next != sink && !grid.is_passable_for(next, net) {
                continue;
            }
            visited.insert(next, ());
            parent.insert(next, pos);

            if next == sink {
                return Some(reconstruct(source, sink, &parent));
            }

            frontier.push(Entry {
                priority: std::cmp::Reverse((dist.saturating_add(1), next.y, next.z, next.x)),
                pos: next,
            });
        }
    }

    None
}

#[derive(PartialEq, Eq)]
struct Entry {
    priority: std::cmp::Reverse<(u32, i32, i32, i32)>,
    pos: Pos3,
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

fn reconstruct(source: Pos3, sink: Pos3, parent: &BTreeMap<Pos3, Pos3>) -> Vec<Pos3> {
    let mut path: Vec<Pos3> = vec![sink];
    let mut cur = sink;
    while cur != source {
        match parent.get(&cur) {
            Some(&p) => {
                cur = p;
                path.push(cur);
            }
            None => break,
        }
    }
    path.reverse();
    path
}

fn annotate_with_repeaters(path: &[Pos3]) -> Vec<RouteSegment> {
    // FR-009: insert a repeater on the 15th consecutive dust block so
    // strength never reaches 0 between source and sink. We treat the
    // source cell as block 0 (output anchor of upstream gate) and start
    // counting on the very first path step.
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
    let prev = if i == 0 { path[0] } else { path[i - 1] };
    let cur = path[i];
    let dx = cur.x - prev.x;
    let dz = cur.z - prev.z;
    if dx > 0 {
        Direction::East
    } else if dx < 0 {
        Direction::West
    } else if dz > 0 {
        Direction::South
    } else if dz < 0 {
        Direction::North
    } else {
        Direction::East
    }
}

fn commit_route(grid: &mut Grid3D, net: NetTag, segments: &[RouteSegment]) {
    for seg in segments {
        match *seg {
            RouteSegment::Dust { pos } => {
                grid.set(pos, CellState::Dust(net));
                propagate_obstruction(grid, net, pos);
            }
            RouteSegment::Repeater { pos, .. } => {
                grid.set(pos, CellState::Repeater(net));
                propagate_obstruction(grid, net, pos);
            }
        }
    }
}

fn propagate_obstruction(grid: &mut Grid3D, net: NetTag, p: Pos3) {
    for dy in [-1i32, 0, 1] {
        for n in p.translate(0, dy, 0).neighbors4() {
            if grid.get(n) == CellState::Air {
                grid.set(n, CellState::Obstructed(net));
            }
        }
    }
}
