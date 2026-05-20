# Phase 0 Research — v2.0 Refactor

**Feature**: 002-v2-analog-scale
**Date**: 2026-05-17

This document resolves the open decisions left at spec-level and pins
down the algorithmic choices for v2. The user supplied a strong
direction in the plan-trigger message; this file makes those choices
explicit, justifies them, and lists the alternatives we rejected.

---

## Decision 1 — Placement algorithm

**Decision**: **Simulated Annealing** (SA) with a seeded `ChaCha8Rng`.

**Rationale**:

- Deterministic given a fixed seed (FR-V13 / v1 FR-017): SA is a randomised
  algorithm, but with a fixed seed it produces identical placements
  across runs and machines.
- Standard FPGA-CAD textbook algorithm. Well-understood failure modes,
  hyperparameters (temperature schedule, cooling rate, swap-acceptance
  rule), and quality knobs.
- The cost function — sum of per-net half-perimeter-wire-length (HPWL)
  — is cheap to update incrementally on each swap.
- Easy to constrain peak memory: the SA state is just the
  `cell_idx → (origin, rotation)` map; no per-iteration grids.
- Easy to parallelise within an iteration (multiple swap proposals
  evaluated in independent threads, then committed in deterministic
  order).

**Alternatives considered**:

- **Force-Directed placement**: faster to a good initial layout, but
  the iterative-relaxation step is sensitive to floating-point order
  of summations — making it deterministic across `cargo build --release`
  on different host CPUs is a real pain. Rejected for determinism risk.
- **Analytical placement (quadratic / linear programming)**: highest
  quality at scale, but introduces an LP-solver dependency
  (`good_lp`, etc.) and a non-trivial conversion step from
  continuous → discrete positions. Out of v2 scope.
- **Keep v1 row-based greedy**: trivially deterministic but does not
  scale to 10 000 gates (SC-V03). Rejected — keeping it only as a
  **fallback** for tiny designs where SA overhead isn't worth it
  (`--placer greedy` flag, opt-in).

---

## Decision 2 — Routing algorithm

**Decision**: **A* search per net + PathFinder negotiated-congestion
outer loop + bounded Rip-up & Reroute** (R&R).

Algorithm sketch:

```text
initialize edge-cost map (geometric base cost + vertical asymmetry per FR-V18)
initialize per-cell `present_cost = 1`, `history_cost = 0`
for iter in 0..MAX_ROUTING_ITERATIONS (CLI-capped, FR-V16):
    rip up all nets routed in previous iteration
    for each net in deterministic order (by net_id):
        run A* on the cost map; goal-cost = sum(present_cost) + sum(history_cost)
        commit path; update `present_cost` of touched cells (congestion penalty grows)
    if no over-used cells (every cell ≤ 1 owner): converged, return
    update `history_cost` of over-used cells (persists across iterations)
if loop exhausts: hard-abort with RouteError::ConvergenceExhausted (FR-V16)
```

**Rationale**:

- A* gives **near-optimal single-net paths** in the cost map, much
  faster than BFS once the heuristic (Manhattan distance × min edge
  cost) prunes the explored set.
- PathFinder is the canonical multi-net congestion resolver from VPR
  / FPGA CAD. Iteratively bumps congestion-cost so subsequent
  iterations re-route congested paths around themselves.
- Bounded R&R + hard-cap iteration count (FR-V16) means deterministic
  walltime upper bound — required for SC-V03.
- Vertical-asymmetric cost map naturally absorbs FR-V18 (slab
  semantics: cheap up, blocked down) and FR-V07 (wire-crossings via
  glass: zero-cost crossings only on glass cells).

**Alternatives considered**:

- **Plain A* without negotiated congestion**: greedy on net order;
  fails on dense designs (any net that locks a corridor before its
  neighbours route is unrecoverable without ripup).
- **Boolean satisfiability (SAT) routing**: gives optimal solutions
  but exponential. Out of scope.
- **Keep v1 BFS**: doesn't scale (we proved it on the full-adder in
  v1 implementation logs). Rejected — replaced wholesale.

---

## Decision 3 — Signal type system

**Decision**: Replace v1's `SignalKind { Boolean }` with a richer
`Signal` value type:

```rust
pub struct Signal {
    pub value: u8,         // 0..=15 redstone signal strength
    pub kind:  SignalKind, // Level | Pulse
}

#[non_exhaustive]
pub enum SignalKind {
    Boolean,         // v1 — `value` is 0 or 15 by convention
    AnalogStrength,  // v2 — `value` carries the full 0..=15 range
    EdgeTrigger,     // v2 — `value` is 15 for exactly 1 tick, then 0
}
```

**Rationale**:

- One unified type lets the type-checker reject illegal cross-kind
  operations (e.g. `analog & analog` is a type error per FR-V02).
- `Boolean` remains a subtype of `AnalogStrength` for routing
  purposes — boolean wires use the same router code path with a
  pinned `value = 15 or 0`.
- `EdgeTrigger` captures observer outputs explicitly so timing
  analysis (Decision 4) can model their 1-tick lifetime.

**Alternatives considered**:

- Keep three separate enums (one per kind). Rejected: pushes pattern
  match into every site that consumes a signal value, more boilerplate.

---

## Decision 4 — Static timing analysis

**Decision**: Forward-pass timing analysis on the combinational
projection of the netlist, accumulating per-cell tick delays from the
cell library, producing a `TimingMap: NodeIndex → Tick`. Data-race
detection compares arrival-tick of two converging paths into the same
sink; mismatch beyond a tolerance → diagnostic.

```rust
pub struct Tick(pub u32);          // redstone-tick count, monotonically non-decreasing

pub struct TimingMap {
    pub arrival: BTreeMap<NodeIndex, Tick>,    // earliest stable-output tick per node
    pub depart:  BTreeMap<NodeIndex, Tick>,    // latest stable-input tick per node
}

pub enum TimingDiag {
    DataRace { net: NetId, fast: Tick, slow: Tick, sites: Vec<SourceSpan> },
    DeepCycle { worst_depth: Tick },  // informational, not error
}
```

**Rationale**:

- v1 cell library already encodes per-primitive `tick_delay`; we lift
  that data into a graph algorithm.
- Forward pass (topological order on the combinational projection)
  is O(V + E), fits the perf budget trivially.
- Data races are the silent failure mode that hand-built redstone
  most often suffers from. Catching them at compile time is a
  pure-quality win.
- The arrival-vs-depart split lets the **router** later insert
  compensating delay (extra repeaters on the fast path) automatically
  without breaking determinism.

**Alternatives considered**:

- Full STA with setup/hold separation: overkill for v2, observers/
  D-triggers are the only stateful elements, simple "arrival" model
  is enough.

---

## Decision 5 — Vertical-asymmetric cost map (FR-V18)

**Decision**: Cost-map edges are stored as `(from: Pos3, dir:
Direction, base_cost: u16, kind: EdgeKind)` triples. Each edge can be
unidirectional (e.g., dust → up via slab is cheap; dust ← down via
slab is **absent**), modelling vanilla MC asymmetry directly. Cost
adjustments come from PathFinder's `present_cost` + `history_cost`
penalty tables, keyed on the *target* cell of each edge.

Cost-function components:

```text
edge_cost(from, dir, target_cell) =
      base_geometry_cost                     // 1 for straight, 2 for turn, 3 for level-change
    + turn_penalty(prev_dir, dir)            // 1 if same dir, 4 if 90° turn
    + level_change_penalty(dir)              // 0 lateral, 6 down, 10 up-via-slab
    + present_cost[target_cell]              // congestion penalty (PathFinder)
    + history_cost[target_cell]              // accumulated congestion (PathFinder)
```

**Rationale**:

- Direct expression of "штраф за поворот, штраф за спуск, бонус за
  прямую линию" from the user's plan-trigger.
- Unidirectional edges naturally model slab semantics (FR-V18) without
  bolt-on checks elsewhere in the router.
- PathFinder cost terms cleanly add on top.

**Alternatives considered**:

- Symmetric cost map + per-cell predicates (e.g. "this cell is a
  slab, block downward propagation"). Worked in v1 but tangles
  routing logic with block-semantics. Cleaner to encode in edges.

---

## Decision 6 — Memory budget enforcement (FR-V09 / FR-V10)

**Decision**: Use the **[`memory-stats`](https://crates.io/crates/memory-stats)**
crate to sample resident-set-size at every stage boundary
(parse → synth → place → route → write). Crate is small (~20 lines on
Linux, uses `/proc/self/statm`), no system deps.

Budget checkpoints:

```rust
pub struct BudgetGuard {
    cap_bytes: u64,
    stage: &'static str,
}

impl BudgetGuard {
    pub fn check(&self) -> Result<(), MemoryError> {
        let used = memory_stats::memory_stats()
            .map(|s| s.physical_mem)
            .unwrap_or(0) as u64;
        if used > self.cap_bytes {
            return Err(MemoryError::CapExceeded {
                cap: self.cap_bytes,
                stage: self.stage,
                actual: used,
            });
        }
        Ok(())
    }
}
```

The pipeline calls `BudgetGuard::check()` at every stage boundary
(FR-V10 "stage at which the overrun occurred").

**Alternatives considered**:

- `jemallocator` with stats: gives more precise tracking but adds a
  heavy global allocator dependency. Rejected for v2; reconsider if
  measurements show the resident-set-size sampling is too coarse.
- Custom GlobalAlloc wrapper that counts allocations: most precise,
  most fragile. Rejected.
- `peak_alloc` crate: deprecated, less maintained than
  `memory-stats`.

---

## Decision 7 — Determinism strategy under parallelism

**Decision**: When the router parallelises net BFS via `rayon`,
parallelism is **structured** to preserve determinism:

1. Within one PathFinder iteration, **partition nets into independent
   bins** by chunk of source position. Bins do not share cells in
   their cost-map view (or, if they do, the bin processes them
   sequentially under a deterministic tiebreak).
2. Each bin is routed in parallel; results are committed in
   ascending net-ID order using `rayon::collect_into_vec`.
3. The `present_cost` / `history_cost` updates use atomic-add only
   for accumulating, and the *reading* of those tables happens at
   iteration boundary, not mid-iteration — so order of writes within
   an iteration does not change the next iteration's behaviour.

**Rationale**:

- Achieves rayon-level speedup on per-net A* (typically the hot loop
  for 10 k gates) while maintaining bit-identical output (SC-V05 /
  FR-V13 / v1 FR-017).
- Standard pattern from VPR / FPGA-CAD parallel routers.

**Alternatives considered**:

- Sequential routing always: simpler, but unlikely to hit SC-V03
  (5 min for 10 k gates).
- Lock-free shared mutable cost-map updates: violates determinism;
  rejected outright.

---

## Decision 8 — Workspace layout deltas

**Decision**: No new crates. All v2 work fits inside the existing v1
crates plus a few new modules:

```text
crates/rb-core/
  src/
    signal.rs      # extended Signal { value: u8, kind: SignalKind }
    timing.rs      # NEW — Tick newtype + TimingMap container
crates/rb-parser/
  src/
    hdl.pest       # extended grammar (analog, comparator, observer, repeater, target_block)
    ast.rs         # AnalogWireDecl, RepeaterInst, ObserverInst, ComparatorInst, TargetInst
crates/rb-synthesis/
  src/
    timing.rs      # NEW — static timing analysis
    place/
      mod.rs       # NEW module — re-exports
      greedy.rs    # was place.rs — kept as fallback (`--placer greedy`)
      sa.rs        # NEW — simulated annealing
    route/
      mod.rs       # NEW module — re-exports
      cost.rs      # NEW — vertical-asymmetric cost map
      astar.rs     # NEW — per-net A* on the cost map
      pathfinder.rs # NEW — negotiated-congestion outer loop + ripup
      lee.rs       # was route.rs — kept for the route_exhausted test
    budget.rs      # NEW — BudgetGuard + MemoryError
crates/rb-nbt/
  src/
    palette.rs     # extended block_state_for() — repeater[delay,locked],
                   #   observer, target, stone_slab[type=bottom]
crates/redstonebuilder/
  src/
    cli.rs         # +`--max-ram`, +`--max-routing-iterations`, +`--stats`,
                   #   +`--placer {sa,greedy}`
    pipeline.rs    # stage-boundary BudgetGuard checks; new exit codes;
                   #   --stats reporter
```

**Rationale**:

- Stays within Constitution Principle III (crates form a DAG; no
  cycles introduced).
- Old v1 implementations of placement and routing become **fallbacks**
  rather than dead code — `--placer greedy` keeps the v1 path
  available for tiny inputs and for regression-debugging.
- One new dependency at the workspace level: `memory-stats`. No other
  additions.

---

## Decision 9 — New CLI exit codes

**Decision**: Extending the v1 exit-code table in
`specs/001-hdl-compiler-cli/contracts/cli.md`:

| Code | v1/v2 | Meaning |
|------|-------|---------|
| 0    | v1    | Success. |
| 1    | v1    | Generic CLI / I/O error. |
| 2    | v1    | Parse / semantic error. |
| 3    | v1    | Synthesis / cycle / unsupported. |
| 4    | v1    | Placement: footprint too large. |
| 5    | v1    | Routing: single-attempt exhaustion (legacy Lee's path). |
| 6    | v1    | NBT / output error. |
| **7**  | **v2** | **Routing: PathFinder iteration cap exceeded** (FR-V16). |
| **8**  | **v2** | **Memory cap exceeded** (FR-V10). |
| **9**  | **v2** | **Timing-analysis data race** (newly detected by static timing). |

`9` is informational-strict — the user can opt out via
`--allow-timing-races` (decision-deferred-to-implementation flag) for
exploratory work.

---

## Decision 10 — Backward compatibility plan (FR-V13, SC-V05)

**Decision**: All four v1 example files (`half_adder.hdl`,
`dff_demo.hdl`, `full_adder.hdl`, `ripple_adder_8bit.hdl`) MUST be in
the v2 regression test suite. v2 must compile them with **no source
edits** and the produced `.litematic` MUST be byte-identical to v1
output **or** strictly smaller in block count (SC-V05 wording).

To make this concrete:

- The v2 `--placer greedy` flag → the v1 row-based placement
  algorithm exactly. Using this flag, v2 output for v1-era inputs is
  byte-identical to v1.
- The default `--placer sa` may produce a different (smaller)
  layout. Either output is acceptable per SC-V05.
- The CI golden-file determinism test (`determinism.rs`) extends to
  cover both `--placer sa` and `--placer greedy` paths.

---

## Open items consciously deferred to /speckit-tasks

- Exact SA hyperparameters (initial temperature, cooling schedule):
  empirical, to be tuned against `examples/ripple_adder_8bit.hdl`
  and a synthetic 10 k-gate stress design.
- Default values for `--max-routing-iterations` and `--max-ram`:
  empirical, set after first prototype lands.
- The `--allow-timing-races` flag's exact name and default: implementation-
  level choice.
- Detailed format of the `--stats` output: implementation-level.
