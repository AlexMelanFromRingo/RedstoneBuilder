# Contract — Router IR (v2)

**Crate**: `rb-synthesis::route`
**Replaces**: v1 `rb-synthesis::route` (BFS).
**Keeps**: v1's `route_single` / `route_with_retry` as `route::lee::*`
for the legacy `route_exhausted.rs` test and for the
`--placer greedy` fallback path.

## Public surface

```rust
pub mod cost;
pub mod astar;
pub mod pathfinder;
pub mod lee;        // legacy v1 BFS, unchanged

pub use cost::{CostMap, CellAttrs, VerticalCost};
pub use astar::{astar_route, AStarConfig};
pub use pathfinder::{route_pathfinder, PathFinderConfig, RouteError};
pub use lee::{route_single, route_with_retry, RouteConfig};  // v1 alias
```

## CostMap

```rust
pub struct CostMap {
    pub bounds: Bbox3,
    pub cells:  Grid3D<CellAttrs>,                  // per-cell physical attributes
    pub present_cost: BTreeMap<Pos3, u16>,          // PathFinder per-iteration penalty
    pub history_cost: BTreeMap<Pos3, u16>,          // PathFinder accumulated penalty
    pub assigned: BTreeMap<Pos3, NetTag>,           // who claims this cell (current iter)
}

pub struct CellAttrs {
    pub base_passable: bool,
    pub vertical:      VerticalCost,
}

pub enum VerticalCost {
    Symmetric,    // ordinary dust-on-block: bidirectional propagation
    UpOnly,       // slab top: dust transmits up, blocks down (FR-V18)
    GlassCross,   // glass: enables an orthogonal-net crossing
    Blocked,      // solid block cell — no dust here
}
```

`CostMap::cost_for_edge(from, dir, to)` returns the A* edge weight:

```text
base_geometry_cost                  // 1 lateral straight, 2 turn, 3 level-change
+ turn_penalty(prev_dir, dir)       // 0 if same, +cfg.turn_penalty if 90°
+ vertical_penalty(dir, to)         // 0 lateral, +cfg.down_penalty / +cfg.up_penalty
+ present_cost[to]                  // PathFinder current-iter congestion
+ history_cost[to]                  // PathFinder accumulated congestion
+ ∞ if edge is forbidden by VerticalCost::UpOnly going down, or Blocked
```

## A* per-net

```rust
pub struct AStarConfig {
    pub turn_penalty:  u16,         // default 4
    pub down_penalty:  u16,         // default 6 (cheap-ish; vanilla allows it)
    pub up_penalty:    u16,         // default 10 (requires slab; more constraints)
    pub history_growth: u16,        // PathFinder: +N per overuse per iter
    pub present_growth: u16,        // PathFinder: +N per touch in current iter
    pub max_iterations: u32,        // FR-V16 cap
}

pub fn astar_route(
    cost:   &CostMap,
    source: Pos3,
    sinks:  &[Pos3],          // multi-sink: each yields a sub-path; reuses prior dust
    cfg:    &AStarConfig,
) -> Option<Vec<RouteSegment>>;
```

Heuristic: Manhattan distance × minimum-edge-cost. Admissible, so A*
remains optimal per net.

For multi-sink nets, sinks are routed in ascending `(y, z, x)` order;
each subsequent sink's BFS frontier is seeded with all cells of the
net already committed.

## PathFinder outer loop

```rust
pub struct PathFinderConfig {
    pub astar:           AStarConfig,
    pub deterministic:   bool,             // always true in v2 (FR-V13)
    pub parallel:        bool,             // rayon-parallel net routing within an iteration
}

pub fn route_pathfinder(
    cost:  &mut CostMap,
    nets:  &[(NetTag, Pos3, Vec<Pos3>)],
    cfg:   &PathFinderConfig,
) -> Result<RoutedLayout, RouteError>;
```

Algorithm: see `data-model.md` §"Rip-up & reroute semantics".

## RouteError (v2 deltas)

```rust
pub enum RouteError {
    // v1 carry-overs (used by the legacy lee.rs path)
    Exhausted { unrouted: Vec<String>, retries: u8, final_bbox: String },

    // NEW v2
    ConvergenceExhausted {
        unrouted:        Vec<String>,
        iterations:      u32,
        peak_congestion: u32,
    },
}
```

`ConvergenceExhausted` maps to exit code **7** (FR-V16).

## Determinism contract

- Net order = `net_id` ascending.
- Sink order within a net = `(y, z, x)` lex of sink position.
- A* tiebreak when two cells have equal `g + h` = `(y, z, x)` lex of
  cell position.
- `present_cost` / `history_cost` updates are commutative (additive),
  applied at iteration boundary.
- `rayon` parallelism partitions nets into row-major chunks (8 nets
  per chunk by default); within a chunk, A* is sequential; results
  are `collect_into_vec` in net_id order before commit. This pins the
  commit order regardless of rayon's worker-thread schedule.

## Backward compatibility with v1 router

- `route::lee::*` re-exports v1's exact module unchanged.
- `--placer greedy` pipeline path uses `lee::route_single_bounded`,
  matching v1's exit-code-5 semantics for `RouteError::Exhausted`.
- The `route_exhausted.rs` and `route_adjacency.rs` v1 tests continue
  to test `lee::*` and pass under v2.
