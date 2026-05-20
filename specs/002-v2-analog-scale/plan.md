# Implementation Plan: v2.0 — Analog Signals, Event-Driven Primitives, and 10 000+ Gate Scale

**Branch**: `002-v2-analog-scale` | **Date**: 2026-05-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-v2-analog-scale/spec.md`

## Summary

Refactor the v1 compiler to scale to **10 000+ gates** and to compile
**analog (signal-strength 0–15) + event-driven (observer / pulse)**
designs alongside v1's boolean primitives. The change set is large
but **strictly additive at the user-facing level**: every v1 HDL file
continues to compile (FR-V13, SC-V05).

Major refactors:

1. **Signal type system** (`rb-core`): replace `SignalKind { Boolean }`
   with `Signal { value: u8, kind: SignalKind }` where
   `SignalKind ∈ {Boolean, AnalogStrength, EdgeTrigger}`. Add a
   `Tick(u32)` newtype for timing.
2. **Parser + AST** (`rb-parser`): grammar adds `analog wire`,
   `comparator`, `observer`, `repeater` (delay 1–4 + optional lock),
   `target_block`. New `InstKind` enum replaces `kind: GateKind` on
   `GateInst`. New `SignalKind` field on `WireDecl`.
3. **Static timing analysis** (`rb-synthesis::timing` — NEW stage):
   forward-pass `arrival[node]` on the combinational projection;
   data-race detection on converging paths.
4. **Placement** (`rb-synthesis::place`): default switches to
   **simulated annealing** with seeded `ChaCha8Rng`. v1 row-based
   placer becomes `place::greedy`, opt-in via `--placer greedy`.
5. **Routing** (`rb-synthesis::route`): v1 Lee's BFS is preserved as
   `route::lee::*` (legacy / `--placer greedy` path); v2 default is
   **A\* search per net + PathFinder negotiated congestion + bounded
   Rip-up & Reroute** on a **vertically-asymmetric cost map** that
   models slab/glass physics (FR-V18). Parallelised via `rayon` with
   determinism-preserving net partitioning.
6. **Memory budget** (`rb-synthesis::budget` — NEW): `BudgetGuard`
   samples resident-set-size at every stage boundary via the
   `memory-stats` crate; hard-abort on cap exceed (FR-V10).
7. **NBT writer** (`rb-nbt::palette`): catalogue extended with
   `minecraft:observer`, `minecraft:target`,
   `minecraft:stone_slab`, `minecraft:glass`, and the full
   `minecraft:repeater[delay,locked,facing]` property set.
8. **CLI** (`redstonebuilder`): new flags `--max-ram`,
   `--max-routing-iterations`, `--stats`, `--placer {sa,greedy}`,
   `--allow-timing-races`. Exit codes 7/8/9 added.

## Technical Context

**Language/Version**: Rust stable, edition 2021. MSRV pinned at 1.86
(unchanged from v1).

**Primary Dependencies** (v2 additions over v1):

| Crate              | Version | Purpose                                                      |
|--------------------|---------|--------------------------------------------------------------|
| `memory-stats`     | 1.x     | Resident-set-size sampling for `BudgetGuard` (FR-V09 / FR-V10) |
| `rayon`            | 1.x     | Parallel net routing within a PathFinder iteration (SC-V03) |

All other v1 dependencies (`clap`, `thiserror`, `miette`, `pest`,
`smol_str`, `petgraph`, `rand_chacha`, `fastnbt`, `flate2`,
`serde`, `serde_json`, `criterion`) carry over unchanged.

**Storage**: file-based only — input HDL on disk, `.litematic`
written to disk (unchanged from v1).

**Testing**: `cargo test --workspace` continues to be the gate.
Adds: `tests/timing.rs` (timing-analysis), `tests/sa_determinism.rs`
(SA placer is deterministic with fixed seed), `tests/pathfinder.rs`
(A* + R&R correctness on small + medium inputs),
`tests/v1_regression.rs` (all v1 example files compile under v2),
`tests/end_to_end_v2.rs` (analog adder, monostable, 10 k stress).

**Target Platform**: any platform Rust supports. The
`memory-stats` crate works on Linux/macOS/Windows (per-OS
implementations).

**Project Type**: compiler / CLI tool (unchanged).

**Performance Goals** (v2):

- **SC-V03**: 10 000-gate design compiles in **under 5 min** on a
  typical developer laptop.
- **SC-V04**: peak resident memory **≤ 4 GB** on the same compile.
- All v1 performance gates (SC-005, SC-006) continue to hold for v1
  reference inputs.

**Constraints**:

- All v1 FRs / constitutional principles carry over unchanged.
- Determinism (v1 FR-017 / SC-007 / v2 FR-V13) is preserved
  unconditionally — every parallel algorithm introduced for SC-V03
  MUST be determinism-preserving (research.md §Decision 7).
- The new memory-stats dependency is the only new top-level dep; no
  global allocator switch.

**Scale/Scope**: v2 targets HDL designs up to **10 000 gates** with
the new analog/observer primitives. Larger designs are not
guaranteed but should remain bounded by `--max-ram`/`--max-routing-iterations`
rather than crash.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1
design.*

| Principle | v2 compliance | Evidence |
|---|---|---|
| **I. Rust-First, No Panics** | ✅ | New algorithms (SA placer, A* router, PathFinder, timing analysis) all return `Result`; new `MemoryError` / `RouteError::ConvergenceExhausted` / `TimingError::DataRace` map to documented exit codes (7/8/9). Library code keeps the `unwrap_used = warn` lint. |
| **II. Strict Pipeline Architecture** | ✅ | New stages slot into the v1 pipeline cleanly: `parse → validate → netlist → cycle → **timing** → place → route → nbt`. Each new stage has a typed input + typed output; `--dump-*` flags continue to bracket every stage. |
| **III. Module Isolation via Cargo Workspace** | ✅ | No new crates. New modules (`timing`, `place::sa`, `route::cost/astar/pathfinder`, `budget`) all live in `rb-synthesis`. DAG `bin → {parser, synthesis, nbt} → core` is preserved. |
| **IV. Built for High-Throughput Compilation** | ✅ | v2 *exercises* this principle. `rayon`-parallel routing inside PathFinder iterations targets SC-V03 (10 k gates / 5 min). `BudgetGuard` makes memory pressure observable (FR-V10). New `criterion` benches for SA placer and PathFinder router. |

**No violations to justify.** Complexity Tracking table below is empty.

**Post-design re-check** (after Phase 1 contracts authored): still ✅.
The contracts in `contracts/` keep every new IR typed (`Signal`,
`Tick`, `CostMap`, `TimingMap`), preserve determinism explicitly
(see `data-model.md` §"Determinism guarantees"), and document the
new exit codes (7/8/9) before any implementation lands.

## Project Structure

### Documentation (this feature)

```text
specs/002-v2-analog-scale/
├── plan.md                       # THIS file
├── spec.md                       # /speckit-specify + /speckit-clarify output
├── research.md                   # Phase 0 — 10 algorithmic / dependency decisions
├── data-model.md                 # Phase 1 — Signal, Tick, TimingMap, CostMap, …
├── quickstart.md                 # Phase 1 — v2 user onboarding
├── contracts/
│   ├── cli.md                    # New flags + exit codes 7/8/9
│   ├── hdl-grammar.md            # Grammar deltas: analog wire, comparator, observer, repeater, target_block
│   ├── router-ir.md              # CostMap, A*, PathFinder, RouteError additions
│   ├── timing.md                 # NEW timing-analysis stage contract
│   └── minecraft-blocks.md       # NEW block-state entries: observer, target, slab, glass, full repeater
└── checklists/
    └── requirements.md           # Spec quality checklist (all green)
```

### Source Code (additive deltas in existing v1 crates)

```text
RedstoneBuilder/
├── Cargo.toml                    # +memory-stats, +rayon at [workspace.dependencies]
├── crates/
│   ├── rb-core/                  # extended types
│   │   ├── src/
│   │   │   ├── signal.rs         # CHANGED — Signal { value, kind } + extended SignalKind
│   │   │   ├── timing.rs         # NEW — Tick newtype
│   │   │   ├── block.rs          # +Observer, +TargetBlock, +Slab, +Glass BlockId variants
│   │   │   └── …
│   ├── rb-parser/                # extended grammar
│   │   ├── src/
│   │   │   ├── hdl.pest          # +analog, +comparator, +observer, +repeater (logic prim),
│   │   │   │                     #  +target_block (logic prim), +CompareMode keywords
│   │   │   ├── ast.rs            # InstKind replaces bare GateKind on GateInst;
│   │   │   │                     #  WireDecl.kind: SignalKind; new RepeaterInst params
│   │   │   ├── validate.rs       # +SignalKindMismatch, +BadDelay, +BadMode, +BadPort for observer
│   │   │   └── error.rs          # +new SemanticError variants
│   ├── rb-synthesis/             # the heaviest crate in v2
│   │   ├── Cargo.toml            # +rayon dev-dep for benches
│   │   ├── src/
│   │   │   ├── netlist.rs        # +AnalogIn, +AnalogOut, +ObserverWatch, +ObserverPulse,
│   │   │   │                     #  +RepeaterIn, +RepeaterOut, +RepeaterLock EndpointRole variants
│   │   │   ├── cycle.rs          # projection extended: cut ObserverWatch + RepeaterLock too
│   │   │   ├── cell_library.rs   # +observer_cell, +target_cell, +variable-delay repeater_cell,
│   │   │   │                     #  +comparator_cell (already had Comparator BlockId in v1)
│   │   │   ├── timing.rs         # NEW — analyse_timing + TimingMap + TimingError
│   │   │   ├── budget.rs         # NEW — BudgetGuard + MemoryError
│   │   │   ├── place/
│   │   │   │   ├── mod.rs        # NEW module — re-exports
│   │   │   │   ├── greedy.rs     # was place.rs — kept; selectable via --placer greedy
│   │   │   │   └── sa.rs         # NEW — simulated annealing placer + SaConfig
│   │   │   ├── route/
│   │   │   │   ├── mod.rs        # NEW module — re-exports
│   │   │   │   ├── lee.rs        # was route.rs — kept verbatim; powers --placer greedy path
│   │   │   │   ├── cost.rs       # NEW — CostMap with vertical-asymmetric edges (FR-V18)
│   │   │   │   ├── astar.rs      # NEW — per-net A* on CostMap
│   │   │   │   └── pathfinder.rs # NEW — negotiated-congestion outer loop + ripup
│   │   │   ├── error.rs          # +PathFinderConvergence variant on RouteError
│   │   │   └── lib.rs            # re-exports
│   │   ├── benches/
│   │   │   ├── route_1k_gates.rs # v1 carry-over
│   │   │   ├── sa_place_10k.rs   # NEW — SA placer on synthetic 10k-gate input
│   │   │   └── pathfinder_10k.rs # NEW — PathFinder + A* on synthetic 10k-gate input
│   │   └── tests/
│   │       ├── (v1 carry-overs)
│   │       ├── timing.rs         # NEW — data-race detection
│   │       ├── sa_determinism.rs # NEW — SA placer is deterministic with fixed seed
│   │       ├── pathfinder.rs     # NEW — A* + R&R correctness on small + medium inputs
│   │       └── cost_map.rs       # NEW — vertical-asymmetry edge tests (slab up-only)
│   ├── rb-nbt/                   # extended block catalogue
│   │   ├── src/
│   │   │   └── palette.rs        # +block_state_for(Observer), +(TargetBlock), +(Slab), +(Glass),
│   │   │                         #  full repeater property set (delay 1–4, locked)
│   │   └── tests/
│   │       └── roundtrip.rs      # v1 + new v2 block-state round-trip cases
│   └── redstonebuilder/          # binary
│       ├── src/
│       │   ├── cli.rs            # +max_ram, +max_routing_iterations, +stats, +placer, +allow_timing_races
│       │   ├── pipeline.rs       # +timing stage; +BudgetGuard at each stage boundary;
│       │   │                     #  +new PipelineError variants → exit 7/8/9; +stats reporter
│       │   ├── main.rs           # +exit-code mapping for 7/8/9
│       │   └── report.rs         # v1 carry-over
│       └── tests/
│           ├── (v1 carry-overs — all four must still pass)
│           ├── v1_regression.rs  # NEW — every v1 example compiles under v2 (FR-V13, SC-V05)
│           ├── end_to_end_v2.rs  # NEW — analog adder, monostable, stress
│           └── determinism.rs    # extended — now covers v2 examples + both --placer modes
├── examples/
│   ├── (v1 carry-overs)
│   ├── analog_add.hdl            # NEW — US1 reference design
│   ├── monostable.hdl            # NEW — US2 reference design
│   └── stress_10k.hdl            # NEW — US3 reference design (generated, committed)
└── tools/
    └── stress-gen.py             # NEW — generates synthetic N-gate HDL stress designs
```

**Structure Decision**: no new crates; v2 is module-additions inside
the v1 workspace. Direct mapping to Constitution Principle III (the
DAG is preserved). v1 algorithms (`place::greedy`, `route::lee`) are
**not deleted** — they become opt-in backward-compat paths
selectable via `--placer greedy`.

## Phase 2 (next) — Tasks

Implementation tasks will be generated by `/speckit-tasks` from this
plan + spec + contracts. Anticipated task grouping (by user story
from spec):

- **Setup (extends v1 workspace)** — new deps `memory-stats` +
  `rayon`; new module directories `place/`, `route/`; rename v1
  `place.rs` → `place/greedy.rs`, `route.rs` → `route/lee.rs`.
- **Foundational** — `rb-core` `Signal` + `SignalKind` + `Tick`
  refactor; `Pos3`/`Bbox3` carry over; `BlockId` extensions.
- **US1 (P1) — Analog primitives** — parser/AST/validate extensions;
  comparator + analog wire support; first analog example end-to-end.
- **US2 (P2) — Event primitives** — observer / repeater (delay+lock) /
  target_block in parser, cell library, NBT writer; monostable
  example.
- **US3 (P3) — Scale** — `BudgetGuard`; SA placer; A* + PathFinder
  router + cost map with FR-V18 asymmetry; rayon parallelism with
  determinism-preserving partition; stress test up to 10 k gates;
  perf gates SC-V03 / SC-V04.
- **Timing** (cross-cutting) — `rb_synthesis::timing` stage,
  `TimingError::DataRace`, exit code 9, `--allow-timing-races`.
- **Polish** — `--stats` reporter, v1 regression (FR-V13 / SC-V05),
  v2 determinism golden test, criterion benches for SA + PathFinder,
  README + quickstart updates, manual MC integration on the new
  examples (T-manual, mirrors v1's T078).

## Complexity Tracking

> Empty — Constitution Check passes without violations.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| —         | —          | —                                   |
