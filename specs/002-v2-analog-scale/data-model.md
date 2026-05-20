# Phase 1 Data Model — v2.0 Refactor

**Feature**: 002-v2-analog-scale
**Date**: 2026-05-17

Pipeline data flow (v2 is additive on top of v1's Principle II
pipeline):

```text
HDL source
   ↓ rb-parser (extended grammar)
AST   ← new: ClockEdge, AnalogWireDecl, RepeaterInst, ObserverInst,
       ComparatorInst, TargetBlockInst, SignalKind on every WireDecl
   ↓ validate (extended)
Netlist (StableDiGraph)   ← new EndpointRole variants for stateful + observer
   ↓ cycle_detect (extended projection: cut observer.in too)
Netlist (DAG-projected)
   ↓ timing_analysis (NEW STAGE)            ← FR-V12 / data-race detection
TimingMap : NodeIndex → Tick
   ↓ place (SA, with greedy fallback)       ← FR-V11 / SC-V03
Placement
   ↓ build_cost_map (vertical-asymmetric)   ← FR-V18
CostMap + per-net (source, sinks) anchors
   ↓ route (A* + PathFinder + ripup)        ← FR-V07 / FR-V08 / FR-V16
RoutedLayout
   ↓ assemble_grid (extended block catalogue)
BlockGrid
   ↓ write_litematic                         ← v1 carry-over
.litematic
```

Stage-boundary `BudgetGuard::check()` (FR-V10) runs between every
arrow. New `--stats` (FR-V14) reports per-stage wall-clock and peak
RAM.

---

## Shared types — `rb-core` extensions

```rust
// signal.rs — REPLACE v1 SignalKind { Boolean }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signal {
    pub value: u8,           // 0..=15, the redstone signal strength
    pub kind:  SignalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum SignalKind {
    #[default]
    Boolean,         // v1 — value pinned to 0 or 15
    AnalogStrength,  // v2 — value carries the full 0..=15 range
    EdgeTrigger,     // v2 — observer outputs; value is 15 for 1 tick then 0
}

// timing.rs — NEW
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tick(pub u32);   // count of redstone ticks (≈ 0.1s wall time)

impl Tick {
    pub const ZERO: Self = Self(0);
    pub fn saturating_add(self, rhs: Self) -> Self { Self(self.0.saturating_add(rhs.0)) }
}

// block.rs — extend with v2 variants (kept #[non_exhaustive] so this is
// additive, not breaking)
#[non_exhaustive]
pub enum BlockId {
    Air, Stone, OakPlanks, OakStairs,
    RedstoneDust, RedstoneTorch, RedstoneWallTorch,
    Repeater,          // existed in v1 catalogue; now used by user-instantiable HDL
    Comparator,        // existed in v1 catalogue; now used by HDL
    Lever, RedstoneLamp,
    Observer,          // NEW v2
    TargetBlock,       // NEW v2
    Slab,              // NEW v2 (router-only)
    Glass,             // NEW v2 (router-only)
}
```

---

## Stage 1 — AST (`rb-parser`) — extended

### New & changed types

```rust
// Wire declarations now carry a SignalKind hint at parse time.
pub struct WireDecl {
    pub name: Ident,
    pub kind: SignalKind,     // NEW — Boolean by default, AnalogStrength if `analog wire`
    pub span: SourceSpan,
}

// Gate instances now include v2 primitives.
pub enum InstKind {                   // NEW — replaces the bare `kind: GateKind` field in v1
    Combinational(GateKind),          // And, Or, Not, Xor (v1)
    Sequential   { kind: GateKind, clock: ClockEdge }, // DTrigger, MemoryCell (v1+v2)
    Comparator   { mode: CompareMode }, // NEW v2 — compare or subtract
    Observer,                          // NEW v2 — no parameters, just .IN/.OUT
    Repeater     { delay: u8 },        // NEW v2 — 1..=4, LOCK port optional in connections
    TargetBlock,                       // NEW v2 — used in routing context
}

pub enum CompareMode { Compare, Subtract }

pub struct GateInst {
    pub inst_name:  Ident,
    pub kind:       InstKind,            // CHANGED — was GateKind in v1
    pub connections: Vec<Connection>,
    pub span: SourceSpan,
}
```

### Validation rules added (`rb-parser::validate`)

- `analog wire X;` with a boolean-only connection (e.g., `.A(X)` on
  `and`) → `SemanticError::SignalKindMismatch { wire: X, expected:
  Boolean, found: AnalogStrength }`.
- Bit-operations on `analog` wires (`&`, `|`, etc.) — caught at parse
  time by the grammar (no operator on analog wires accepted).
- `comparator c(.A(...), .B(...), .Y(...), .MODE(...));` — `MODE` must
  be a compile-time constant `compare` or `subtract`.
- `repeater r(.IN(...), .OUT(...), .DELAY(N), .LOCK(...));` — `DELAY`
  must be `1..=4`; `LOCK` is optional.
- `observer o(.WATCH(net), .OUT(net));` — exactly one input, one output.
- `target_block t(.IN(...), .OUT(...));` — used to redirect dust.

---

## Stage 2 — Netlist (`rb-synthesis::netlist`) — extended

### Endpoint roles (additive)

```rust
#[non_exhaustive]
pub enum EndpointRole {
    // v1
    ModuleInput, ModuleOutput,
    DataIn(u8), DataOut,
    ClockIn, DataInStateful, WriteEnable, Q,
    // v2
    AnalogIn(u8),       // analog input on comparator
    AnalogOut,          // analog output on comparator
    ObserverWatch,      // input — what the observer watches; cycle-cut
    ObserverPulse,      // output — 1-tick pulse
    RepeaterIn,         // diode input
    RepeaterOut,        // diode output
    RepeaterLock,       // optional lock signal — cycle-cut
}
```

### Cycle-detection projection (v2 update)

The combinational projection rule (v1 FR-005, `rb_synthesis::cycle`) is
extended:

```text
Cut edges whose `to` role is any of:
  DataInStateful  (v1, D-trigger D)
  WriteEnable     (v1, memcell WRITE)
  ObserverWatch   (v2, observer input — async stateful per FR-V12)
  RepeaterLock    (v2, repeater LOCK — stateful per design)
```

A cycle that traverses `ObserverPulse → RepeaterIn → … →
ObserverWatch` is therefore **legal** (sequential), while a pure
combinational cycle remains rejected.

---

## Stage 3 — Static Timing Analysis (NEW)

```rust
pub struct TimingMap {
    pub arrival: BTreeMap<NodeIndex, Tick>,   // earliest stable-output tick
    pub depart:  BTreeMap<NodeIndex, Tick>,   // tick by which inputs must be ready
}

pub enum TimingError {
    DataRace {
        net:   NetId,
        sink:  NodeIndex,
        paths: Vec<(NodeIndex, Tick)>,   // converging paths, with their arrival ticks
    },
}

pub fn analyse_timing(
    netlist: &Netlist,
    cells:   &CellLibrary,   // per-kind tick_delay lookup
) -> Result<TimingMap, TimingError>;
```

**Algorithm**: forward topological pass on the combinational projection.
For each node, `arrival[n] = max over incoming edges (arrival[pred] +
cell.tick_delay[pred.kind])`. A sink node with multiple incoming
paths whose arrival ticks differ by more than a configurable
tolerance (default: 0 ticks for data signals, ∞ for clock signals
which are explicitly allowed to skew) → `DataRace`.

Exit code on `DataRace`: **9**. `--allow-timing-races` downgrades to a
warning (decision-deferred-to-implementation default: hard error,
following FR-V12's "match a documented semantic model").

---

## Stage 4 — Placement (`rb-synthesis::place::sa`) — NEW

```rust
pub struct SaConfig {
    pub seed:              u64,
    pub initial_temp:      f64,
    pub cooling_rate:      f64,    // multiplicative per epoch
    pub epochs:            u32,
    pub swaps_per_epoch:   u32,
}

pub fn place_simulated_annealing(
    netlist:  &Netlist,
    cells:    &CellLibrary,
    cfg:      &SaConfig,
    bounds:   &PlaceConfig,        // v1's max_footprint carries over
) -> Result<Placement, PlaceError>;
```

**Cost function**: sum over nets of half-perimeter-wire-length (HPWL)
plus a penalty for cells whose bounding box exceeds the active
`--max-footprint`. SA accept rule: standard exp(-ΔE / T) with the
project-wide ChaCha8Rng seeded from `cfg.seed`.

**Determinism**: SA proposal order is `(epoch_idx, swap_idx)`-driven,
not wall-clock. The RNG advances deterministically.

`place::greedy::place_row_based()` keeps v1's algorithm exactly,
selectable via `--placer greedy` for backward-compatibility and for
small designs.

---

## Stage 5 — Routing IR (`rb-synthesis::route`) — replaced

### Cost map

```rust
pub struct CostMap {
    pub bounds:   Bbox3,
    pub cells:    Grid3D<CellAttrs>,
    pub present_cost: BTreeMap<Pos3, u16>,    // PathFinder per-iteration penalty
    pub history_cost: BTreeMap<Pos3, u16>,    // PathFinder accumulated penalty
}

pub struct CellAttrs {
    pub base_passable: bool,
    pub vertical:      VerticalCost,
}

pub enum VerticalCost {
    /// Dust here transmits up and down normally.
    Symmetric,
    /// Dust here transmits upward (e.g. slab top), blocks downward.
    UpOnly,
    /// Wire-crossing cell (glass) — orthogonal nets do not couple.
    GlassCross,
    /// Solid block — impassable for redstone dust.
    Blocked,
}
```

### A* + PathFinder

```rust
pub struct AStarConfig {
    pub max_iterations:    u32,   // FR-V16
    pub history_growth:    u16,   // how fast history_cost climbs per overuse
    pub present_growth:    u16,   // how fast present_cost climbs within an iteration
    pub turn_penalty:      u16,
    pub down_penalty:      u16,
    pub up_penalty:        u16,
}

pub fn route_pathfinder(
    cost: &mut CostMap,
    nets: &[(NetTag, Pos3, Vec<Pos3>)],   // multi-sink per net
    cfg:  &AStarConfig,
) -> Result<RoutedLayout, RouteError>;
```

### Rip-up & reroute semantics

```text
Outer loop iteration k:
  1. Drop all `Dust`/`Repeater`/`Obstructed` cells from the cost map
     that belong to a net (keep only `Solid` from placed cells).
  2. For each net in net_id order:
     run A* on `(base_geometry_cost + turn/down/up penalty
                  + present_cost[cell] + history_cost[cell])`
     commit the best path.
  3. Scan the grid for over-used cells (any cell shared by two nets).
     If none → converged, return.
  4. For each over-used cell, bump `history_cost[cell] += history_growth`.
  5. Bump `present_cost[every cell on any committed path] += present_growth`.
  6. k+=1; loop.
On k > cfg.max_iterations: RouteError::ConvergenceExhausted (FR-V16).
```

---

## Stage 6 — Block grid → NBT (extended)

`rb-nbt::palette::block_state_for` extended:

| `BlockId`     | Resolved Minecraft block-state                                                                  |
|---------------|-------------------------------------------------------------------------------------------------|
| `Repeater`    | `minecraft:repeater{ facing, delay=1..4, locked=true|false, powered=true|false }`                |
| `Observer`    | `minecraft:observer{ facing, powered=false }`                                                    |
| `TargetBlock` | `minecraft:target{ power=0 }`                                                                    |
| `Slab`        | `minecraft:stone_slab{ type=bottom, waterlogged=false }`                                         |
| `Glass`       | `minecraft:glass{}` (no properties)                                                              |
| (existing)    | unchanged                                                                                        |

Determinism: property maps stay `BTreeMap<SmolStr, SmolStr>` (v1
convention). Palette indexing rules unchanged — first-encounter in
(y,z,x) iteration order; air at index 0.

---

## Stage 7 — Memory budget (`rb-synthesis::budget`)

```rust
pub struct BudgetGuard {
    cap_bytes:   u64,
    stage_label: &'static str,
}

pub enum MemoryError {
    CapExceeded {
        cap:    u64,
        actual: u64,
        stage:  &'static str,
    },
}

impl BudgetGuard {
    pub fn new(cap_mb: u32, stage: &'static str) -> Self { /* */ }
    pub fn check(&self) -> Result<(), MemoryError> { /* memory-stats query */ }
}
```

Pipeline integration:

```rust
let guard = BudgetGuard::new(cfg.max_ram_mb, "after_parse");
guard.check()?;  // FR-V10 — hard abort if exceeded
```

Called between every stage in `pipeline::run`. On `CapExceeded`,
pipeline returns `PipelineError::MemoryCap(MemoryError)` which maps to
exit code 8.

---

## Determinism guarantees (v2 update)

Same v1 contract (FR-V13 / FR-017), extended:

| Source of non-determinism | Mitigation |
|---|---|
| `HashMap` iteration | use `BTreeMap` everywhere we serialise or compare (already v1 policy) |
| SA proposal RNG | seeded `ChaCha8Rng` (FR-V17) |
| `rayon` parallelism | partition nets into bins; collect_into_vec in net-ID order; cost-table updates buffered to iteration boundary |
| Wall-clock-based timing in NBT metadata | zeroed (v1 carry-over) |
| Memory-stats reading | does not affect output bytes — sampling is read-only |
| Floating-point in SA cost | accumulate in `f64` with stable summation order (sort net IDs) |

The determinism golden-file test from v1 (`determinism.rs`) extends
to all v2 examples and both `--placer` modes.
