---

description: "Task list for v2.0 epic — analog signals, event-driven primitives, 10 000+ gate scale"
---

# Tasks: v2.0 — Analog Signals, Event-Driven Primitives, and 10 000+ Gate Scale

**Input**: Design documents from `/specs/002-v2-analog-scale/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/*.md, quickstart.md. All v1 tasks (001) are assumed
complete — v2 is **strictly additive**.

**Tests**: Included — constitution Dev Workflow mandates `cargo test
--workspace` on every PR. SC-V05 (v1 regression) is only verifiable
via the existing `tests/end_to_end_us{1,2,3}.rs` continuing to pass.

**Phase organisation**: This task list follows the **user-defined
5-phase refactor structure** (AST → blocks → placement → routing →
repeater-insertion), bookended by a Setup phase (workspace deltas)
and a Polish phase (benches + docs + regression). `[Story]` tags
trace each task to the user story it serves (US1 = analog primitives,
US2 = event primitives, US3 = scale).

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Different files, no in-progress dependencies — safe to run in parallel.
- **[Story]**: `[US1]`/`[US2]`/`[US3]` for tasks that primarily serve one user story; omitted for Setup / Foundational AST / shared infrastructure / Polish.

## Path conventions

Cargo workspace at repo root. v2 is **additive** in v1 crates:
`rb-core`, `rb-parser`, `rb-synthesis`, `rb-nbt`, `redstonebuilder`.
No new crates.

---

## Phase 0: Setup (workspace deltas)

**Purpose**: Add v2 dependencies, restructure v1 `place.rs` / `route.rs` into directory-modules so SA placer and A* router can land alongside the v1 implementations without breaking imports.

- [X] T001 Add `memory-stats = "1"` and `rayon = "1"` to `[workspace.dependencies]` in root `Cargo.toml`.
- [X] T002 [P] In `crates/rb-synthesis/Cargo.toml`, add `memory-stats` and `rayon` to `[dependencies]`.
- [X] T003 Rename `crates/rb-synthesis/src/place.rs` → `crates/rb-synthesis/src/place/greedy.rs`; create `crates/rb-synthesis/src/place/mod.rs` that re-exports the same public items as the old `place.rs` did (so `rb_synthesis::place` keeps working for v1 callers).
- [X] T004 Rename `crates/rb-synthesis/src/route.rs` → `crates/rb-synthesis/src/route/lee.rs`; create `crates/rb-synthesis/src/route/mod.rs` that re-exports the same public items as the old `route.rs` did (so `rb_synthesis::route` keeps working for v1 callers and tests).
- [X] T005 Run `cargo build --workspace && cargo test --workspace` and confirm zero regressions from T003/T004 (all 48 v1 tests still pass on the renamed modules).

**Checkpoint**: Workspace builds and all v1 tests pass. v2 modules can now be added to `place/` and `route/` without import collisions.

---

## Phase 1: Refactor AST and Netlist for Signal Strength + Delays

**Purpose**: Foundational — replace v1's `SignalKind { Boolean }` with the v2 `Signal { value: u8, kind: SignalKind }` type, add the `Tick` newtype, extend the AST with `analog wire` + `InstKind` + v2-primitive fields, extend the netlist with v2 endpoint roles, extend the cycle-detection projection, and add the static-timing-analysis stage.

**⚠️ CRITICAL**: All Phase 2–5 work depends on these types and the timing stage.

### rb-core type changes (data-model.md §"Shared types")

- [X] T006 In `crates/rb-core/src/signal.rs`, replace v1 `SignalKind { Boolean }` with `Signal { value: u8, kind: SignalKind }` plus `SignalKind { #[default] Boolean, AnalogStrength, EdgeTrigger }` (`#[non_exhaustive]`). Keep `Default for SignalKind`.
- [X] T007 [P] In `crates/rb-core/src/timing.rs` (NEW file), introduce `Tick(pub u32)` with `ZERO` const and `saturating_add`. Re-export from `crates/rb-core/src/lib.rs`.
- [X] T008 [P] In `crates/rb-core/src/block.rs`, add `BlockId::Observer`, `BlockId::TargetBlock`, `BlockId::Slab`, `BlockId::Glass`. Enum stays `#[non_exhaustive]`.

### rb-parser AST and grammar (contracts/hdl-grammar.md)

- [X] T009 [P] [US1] In `crates/rb-parser/src/ast.rs`, add `kind: SignalKind` field to `WireDecl` (default `SignalKind::Boolean` for v1-style `wire X;`).
- [X] T010 [P] [US1+US2] In `crates/rb-parser/src/ast.rs`, introduce `InstKind` enum: `Combinational(GateKind)`, `Sequential { kind: GateKind, clock: ClockEdge }`, `Comparator { mode: CompareMode }`, `Observer`, `Repeater { delay: u8 }`, `TargetBlock`. Migrate `GateInst.kind: GateKind` → `kind: InstKind`. (`CompareMode { Compare, Subtract }`.)
- [X] T011 [US1+US2] Extend `crates/rb-parser/src/hdl.pest` per contracts/hdl-grammar.md: `wire_kind = { kw_wire | (kw_analog ~ kw_wire) }`, add `kw_analog`/`kw_comparator`/`kw_subtract`/`kw_compare`/`kw_observer`/`kw_target_block`, `comparator_inst`, `observer_inst`, extend `gate_kind` with `kw_repeater | kw_target_block`. Add all new keywords to the `reserved` rule.
- [X] T012 [US1+US2] In `crates/rb-parser/src/parse.rs`, add `build_comparator_inst`, `build_observer_inst`, extend `build_gate_inst` to recognise `repeater` (parse `.DELAY(N)`/`.LOCK(net)` as part of `conn_list`) and `target_block`, attach `SignalKind` to wires based on the `kw_analog` token presence.
- [X] T013 [P] [US1+US2] In `crates/rb-parser/src/error.rs`, add `SemanticError::SignalKindMismatch { wire, expected, found, at, src }`, `SemanticError::BadDelay { inst, actual, at, src }`, `SemanticError::BadMode { inst, actual, at, src }`. Extend `SemanticError::BadPort` matching for observer's `.WATCH`/`.OUT` and repeater's `.IN`/`.OUT`/`.DELAY`/`.LOCK`.
- [X] T014 [US1+US2] In `crates/rb-parser/src/validate.rs`, enforce all new rules from contracts/hdl-grammar.md §"New rejections": analog wire vs boolean port mismatch (and vice versa), `repeater.DELAY` in `1..=4`, `comparator.MODE` in `{compare, subtract}`, observer port names.
- [X] T015 [P] [US1] Add parser fixture `crates/rb-parser/tests/fixtures/analog_add.hdl` (from quickstart.md). Assert in `crates/rb-parser/tests/golden.rs` that it parses as `Module { instances[0]: GateInst { kind: InstKind::Comparator { mode: Subtract }, … } }` and that `analog_wire_a.kind == AnalogStrength`.
- [X] T016 [P] [US2] Add parser fixture `crates/rb-parser/tests/fixtures/monostable.hdl` (from quickstart.md). Assert in `tests/golden.rs` that it parses as expected `InstKind::Observer` + `InstKind::Repeater { delay: 1 }`.
- [X] T017 [P] [US1] Add negative-fixture `crates/rb-parser/tests/fixtures/signal_kind_mismatch.hdl` (`analog wire X; and g(.A(X), .B(...), .Y(...));`). Test asserts `SemanticError::SignalKindMismatch`.

### rb-synthesis netlist + cycle projection (data-model.md §"Endpoint roles")

- [X] T018 [US1+US2] In `crates/rb-synthesis/src/netlist.rs`, extend `EndpointRole` (`#[non_exhaustive]`) with `AnalogIn(u8)`, `AnalogOut`, `ObserverWatch`, `ObserverPulse`, `RepeaterIn`, `RepeaterOut`, `RepeaterLock`. Update `port_role` to map comparator/observer/repeater connections to these roles.
- [X] T019 [US1+US2] In `crates/rb-synthesis/src/netlist.rs`, ensure `Gate` nodes built from `InstKind::Comparator`/`Observer`/`Repeater`/`TargetBlock` carry their `InstKind` (extend `NetlistNode::Gate` to hold the full `InstKind` instead of just `GateKind`, or add a parallel field — backward-compat for v1 `GateKind` usage stays via accessor).
- [X] T020 [US2] In `crates/rb-synthesis/src/cycle.rs`, extend `is_cycle_cutting` so `ObserverWatch` and `RepeaterLock` are also cycle-cut roles (in addition to v1's `DataInStateful`/`WriteEnable`). Add a unit test in `crates/rb-synthesis/tests/sequential_cycle.rs` (extend existing): observer-feedback loop is legal; pure-combinational loop with observer-elsewhere is still rejected.

### Static timing analysis stage (contracts/timing.md)

- [X] T021 [P] In `crates/rb-synthesis/src/timing.rs` (NEW file), implement `TimingMap`, `TimingError::DataRace`, `TimingConfig { data_race_tolerance, strict }`, and `analyse_timing(&Netlist, &CellLibrary, &TimingConfig) -> Result<TimingMap, TimingError>` per contracts/timing.md §Algorithm.
- [X] T022 [P] Unit test `crates/rb-synthesis/tests/timing.rs`: (a) linear chain → arrival ticks accumulate; (b) two paths converging with equal delay → no error; (c) two paths converging with mismatched delay → `DataRace`; (d) clock-domain net is excluded from race check.

**Checkpoint**: All v1 tests still pass on the refactored AST/netlist. New parser tests for analog/observer/repeater pass. Timing stage compiles and is independently testable.

---

## Phase 2: New blocks (slabs, targets, observers, repeaters) + NBT serialisation

**Purpose**: Extend `rb-synthesis::cell_library` and `rb-nbt::palette` so the new HDL primitives (Phase 1) have concrete redstone implementations + valid NBT output. Slab/glass go in here too even though they're router-only — Phase 4 needs them in the BlockId-to-BlockState mapping.

### Cell library (rb-synthesis)

- [X] T023 [P] [US1] In `crates/rb-synthesis/src/cell_library.rs`, add `comparator_cell(mode: CompareMode) -> Macrocell`. Anchors: `.A` (AnalogIn(0), west), `.B` (AnalogIn(1), south), `.Y` (AnalogOut, east). `tick_delay: 1`. Uses `BlockId::Comparator` (already in v1 catalogue) with `mode` property.
- [X] T024 [P] [US2] In `crates/rb-synthesis/src/cell_library.rs`, add `observer_cell() -> Macrocell`. Anchors: `.WATCH` (ObserverWatch, back face), `.OUT` (ObserverPulse, front face). `tick_delay: 1` (1-tick pulse). Uses `BlockId::Observer`.
- [X] T025 [P] [US2] In `crates/rb-synthesis/src/cell_library.rs`, add `repeater_cell(delay: u8, lock_used: bool) -> Macrocell`. Anchors: `.IN` (RepeaterIn, west), `.OUT` (RepeaterOut, east), conditionally `.LOCK` (RepeaterLock, south or north). `tick_delay: 2 * delay` (each delay step = 2 game ticks). Uses `BlockId::Repeater` with `delay` + optional `locked`.
- [X] T026 [P] [US2] In `crates/rb-synthesis/src/cell_library.rs`, add `target_cell() -> Macrocell`. Anchors: `.IN` (DataIn(0), west), `.OUT` (DataOut, east). `tick_delay: 0`. Uses `BlockId::TargetBlock`.
- [X] T027 [US1+US2] Extend `macrocell_for(kind: GateKind)` (or refactor to `macrocell_for(inst: &InstKind)`) to dispatch to the new cell functions. Backward-compat: a bare `GateKind::And` still returns the v1 `and_cell()`.
- [X] T028 [P] [US1+US2] Extend `crates/rb-synthesis/tests/cell_library.rs` with cases for each new cell: bbox non-empty, anchors carry the expected `EndpointRole`, tick_delay matches contract.

### NBT writer (rb-nbt) — contracts/minecraft-blocks.md

- [X] T029 [P] [US1+US2] In `crates/rb-nbt/src/palette.rs::block_state_for`, add arms for `BlockId::Observer` (`minecraft:observer` with `facing` from arg, `powered: "false"`), `BlockId::TargetBlock` (`minecraft:target`, `power: "0"`), `BlockId::Slab` (`minecraft:stone_slab`, `type: "bottom"`, `waterlogged: "false"`), `BlockId::Glass` (`minecraft:glass`, no props).
- [X] T030 [US1+US2] Extend the existing `block_state_for(BlockId::Repeater, …)` arm with **full** property set: `delay: 1|2|3|4` (REQUIRED — accept a new optional param or thread it via a `BlockState`-builder helper), `locked: "true"|"false"` (REQUIRED — same threading), `powered: "false"`. v1 callers that don't pass delay/locked get defaults `delay=1, locked=false`.
- [X] T031 [P] [US1+US2] Extend `crates/rb-nbt/tests/roundtrip.rs` with a tiny `BlockGrid` containing each new block-state (observer facing east, target, slab, glass, repeater with delay=3 + locked=true). Round-trip through `encode_to_bytes` + `decode_from_bytes`; assert the palette contains each `minecraft:*` name and the correct property values.

**Checkpoint**: Phase 1 + Phase 2 together let the compiler **type-check** v2 HDL files and emit blocks for them, even if placement/routing of the new instances is still primitive. End-to-end: `analog_add.hdl` compiles to a `.litematic` containing a `minecraft:comparator{mode=subtract}` block, even if its wires are short or absent.

---

## Phase 3: New Placement module — Simulated Annealing (research.md §Decision 1)

**Purpose**: Implement the SA placer in `place::sa` that scales to 10 000 gates while preserving determinism (FR-V13). Wire the `--placer {sa,greedy}` flag.

- [X] T032 [US3] In `crates/rb-synthesis/src/place/sa.rs` (NEW file), implement `SaConfig { seed, initial_temp, cooling_rate, epochs, swaps_per_epoch }` plus `place_simulated_annealing(&Netlist, &CellLibrary, &SaConfig, &PlaceConfig) -> Result<Placement, PlaceError>`. RNG: `rand_chacha::ChaCha8Rng::seed_from_u64(cfg.seed)`. Cost function: sum of per-net HPWL + footprint-violation penalty (per data-model.md §Stage 4). Proposal order indexed by `(epoch_idx, swap_idx)` for reproducibility.
- [X] T033 [P] [US3] In `crates/rb-synthesis/src/place/mod.rs`, re-export `place_simulated_annealing` alongside the v1 `place_row_based` (greedy). Add a `pub enum PlacerKind { Sa, Greedy }` and `pub fn place(kind: PlacerKind, ...)` thin dispatcher.
- [X] T034 [P] [US3] In `crates/rb-synthesis/tests/sa_determinism.rs` (NEW): compile the same 50-gate generated netlist 5 times with the same seed; assert the resulting `Placement.cells` is element-identical. Then change the seed; assert the result differs.
- [X] T035 [P] [US3] In `crates/rb-synthesis/tests/sa_quality.rs` (NEW): for a 50-gate netlist, assert SA placer produces a footprint ≤ 1.5× the row-based placer's footprint (sanity gate against catastrophic SA regression).
- [X] T036 [US3] In `crates/redstonebuilder/src/cli.rs`, add `#[arg(long, value_enum, default_value_t = Placer::Sa)] pub placer: Placer` per contracts/cli.md. Add the `Placer { Sa, Greedy }` value enum. Wire `--seed` to `SaConfig.seed`.
- [X] T037 [US3] In `crates/redstonebuilder/src/pipeline.rs`, route the placer choice through to `rb_synthesis::place::place(...)`. Make `--placer greedy` use the v1 row-based path unchanged.

**Checkpoint**: SA placer is deterministic, ships under `--placer sa` (default), v1 greedy stays available under `--placer greedy`. The 50-gate ripple-adder works under both placers.

---

## Phase 4: New Routing module — A* + PathFinder + Rip-up & Reroute (contracts/router-ir.md)

**Purpose**: Replace the v1 BFS hot path with a production-grade router that scales to 10 000-gate designs. The v1 `route::lee::*` stays available for legacy testing and as a fallback under `--placer greedy`.

### Cost map (research.md §Decision 5)

- [X] T038 [US3] In `crates/rb-synthesis/src/route/cost.rs` (NEW file), implement `CostMap { bounds, cells: Grid3D<CellAttrs>, present_cost, history_cost, assigned }` and `CellAttrs { base_passable, vertical: VerticalCost }` per data-model.md §Stage 5.
- [X] T039 [US3] Implement `VerticalCost { Symmetric, UpOnly, GlassCross, Blocked }` and `CostMap::cost_for_edge(from, dir, to, prev_dir, cfg) -> u32` returning `base + turn + vertical + present + history` (per data-model.md §Stage 5). `UpOnly` cells return `u32::MAX` for downward edges (forbidden, captures FR-V18).
- [X] T040 [P] [US3] In `crates/rb-synthesis/tests/cost_map.rs` (NEW): (a) slab cell is `UpOnly`: dust→up returns finite cost, dust→down returns `u32::MAX`; (b) glass cell is `GlassCross`: orthogonal-net cells across the same glass do not couple in `cost_for_edge`; (c) turn cost is added correctly on direction change.

### A* per-net (research.md §Decision 2)

- [X] T041 [US3] In `crates/rb-synthesis/src/route/astar.rs` (NEW file), implement `AStarConfig { turn_penalty, down_penalty, up_penalty, history_growth, present_growth, max_iterations }` and `astar_route(&CostMap, source, sinks, cfg) -> Option<Vec<RouteSegment>>`. Use `BinaryHeap` with tuple priority `(g + h, y, z, x)` for deterministic tiebreak. Multi-sink: route to each sink in `(y, z, x)` lex order, seeding the BFS frontier with all dust already committed for the net.
- [X] T042 [P] [US3] Unit tests in `crates/rb-synthesis/tests/astar.rs` (NEW): (a) trivial 5-block straight route returns a 5-segment path; (b) obstacle forces a detour; (c) two source→sinks of the same net share dust; (d) the same A* run twice with the same inputs returns the identical path (determinism gate).

### PathFinder outer loop + rip-up (research.md §Decision 2)

- [X] T043 [US3] In `crates/rb-synthesis/src/route/pathfinder.rs` (NEW file), implement `PathFinderConfig { astar, deterministic, parallel }` and `route_pathfinder(&mut CostMap, &[(NetTag, Pos3, Vec<Pos3>)], &PathFinderConfig) -> Result<RoutedLayout, RouteError>` per data-model.md §"Rip-up & reroute semantics". The deterministic-rayon partitioning (research.md §Decision 7) goes here.
- [X] T044 [US3] In `crates/rb-synthesis/src/error.rs`, add `RouteError::ConvergenceExhausted { unrouted: Vec<String>, iterations: u32, peak_congestion: u32 }`. Wire into the v1 `RouteError` enum (it stays as the single canonical error type).
- [X] T045 [P] [US3] Tests in `crates/rb-synthesis/tests/pathfinder.rs` (NEW): (a) two parallel non-conflicting nets converge in 1 iteration; (b) two nets that initially conflict converge after R&R within 5 iterations; (c) a contrived dense input fails to converge within `max_iterations=2` and returns `RouteError::ConvergenceExhausted` naming both unrouted nets.

### CLI + pipeline integration

- [X] T046 [US3] In `crates/redstonebuilder/src/cli.rs`, add `#[arg(long, default_value_t = 64)] pub max_routing_iterations: u32` and `#[arg(long)] pub allow_timing_races: bool` per contracts/cli.md.
- [X] T047 [US3] In `crates/redstonebuilder/src/pipeline.rs`, replace the v1 `route_single_bounded` loop with a call to `route_pathfinder(...)` when `--placer sa` is active. `--placer greedy` continues using the v1 `lee::route_single_bounded` path. Wire `cli.max_routing_iterations` into `PathFinderConfig.astar.max_iterations`.
- [X] T048 [US3] In `crates/redstonebuilder/src/pipeline.rs`, extend `PipelineError` with `Route(RouteError)` already exists; ensure the new `ConvergenceExhausted` variant maps to exit code **7** in `ExitCategory::for_error`.

**Checkpoint**: Default `--placer sa` path uses A* + PathFinder; v1 `--placer greedy` path is unchanged. `cargo test --workspace` passes. The pipeline can be exercised on a synthetic 1 000-gate input without hanging.

---

## Phase 5: Auto-repeater insertion on long wires (FR-V08)

**Purpose**: Generalise v1 FR-009 (repeater every 15 dust blocks) to the analog case: along every routed wire, insert repeaters so the destination receives the **same signal strength** the source produced. Apply to both boolean and analog wires.

- [X] T049 [US1] In `crates/rb-synthesis/src/route/astar.rs`, change `astar_route` so it tracks the **distance from the most recent strength source** (gate output anchor or repeater) along each candidate path. When this distance reaches 14, the next cell on the path is annotated as a repeater rather than dust.
- [X] T050 [P] [US1] In `crates/rb-synthesis/src/route/pathfinder.rs`, ensure the per-net "obstruction footprint" propagation (carry over from v1 `route::lee`) is applied to inserted `Repeater` cells the same way it's applied to `Dust` cells.
- [X] T051 [P] [US1] Tests in `crates/rb-synthesis/tests/repeater_insertion.rs` (NEW): (a) a 20-block straight route from source to sink contains exactly 1 inserted repeater placed at block 15 (v1 FR-009 carry-over); (b) a 35-block route contains exactly 2 repeaters at blocks 15 and 30; (c) an analog (AnalogStrength) wire of 30 blocks: signal strength at the sink == strength at the source (FR-V08).
- [X] T052 [US1] In `crates/rb-synthesis/src/cell_library.rs`, ensure the auto-inserted repeater uses `delay=1` (the default 2-game-tick step) so it does not change effective wire delay materially. Confirmed by extending `tests/timing.rs` to assert arrival-tick across a 30-block dust+repeater wire is `2` ticks (one repeater step).

**Checkpoint**: Long routes get correct repeaters automatically. Analog wires no longer degrade between source and sink. Boolean wires behave identically to v1.

---

## Phase 6: Polish — benchmarks, regression, docs, manual integration

**Purpose**: Lock in v2 quality gates (SC-V03, SC-V04, SC-V05, SC-V08, FR-V14), provide the 1 000- and 10 000-gate benchmarks the user requested, and document.

### Stress-design generator (used by benches + e2e tests)

- [X] T053 [P] [US3] In `tools/stress-gen.py` (NEW file at repo root), write a Python 3 script that emits a synthetic HDL netlist of N gates (e.g., 1 000 or 10 000 not-gate chain, or wider parallel structure) to stdout. Document usage in the script header. Used by benches and stress tests below.

### Benchmarks (user requirement)

- [X] T054 [P] [US3] Add `crates/rb-synthesis/benches/route_1k_gates.rs` already exists from v1 — **extend** it to also bench `route_pathfinder` (not just legacy `lee::route_with_retry`) on the same 1 000-gate input. Two `bench_function` calls: `lee_1k_gates` and `pathfinder_1k_gates`. Comparable wall-clock numbers go to stderr.
- [X] T055 [P] [US3] In `crates/rb-synthesis/benches/pathfinder_10k.rs` (NEW file), criterion bench: load a 10 000-gate netlist from `tools/stress-gen.py --gates 10000`, place with SA, run `route_pathfinder`. Wall-clock target SC-V03: **under 5 minutes** on a typical developer laptop. Register via `[[bench]]` block in `crates/rb-synthesis/Cargo.toml`.
- [X] T056 [P] [US3] In `crates/rb-synthesis/benches/sa_place_10k.rs` (NEW file), criterion bench for the SA placer alone (decoupled from routing). Helps profile SA hyperparameter tuning. Register `[[bench]]`.

### `--stats` + `--max-ram` + memory-budget (FR-V09 / FR-V10 / FR-V14)

- [X] T057 [US3] In `crates/rb-synthesis/src/budget.rs` (NEW file), implement `BudgetGuard { cap_bytes, stage_label }`, `MemoryError::CapExceeded { cap, actual, stage }`, and `BudgetGuard::check()` using `memory_stats::memory_stats()` per research.md §Decision 6. Re-export from `crates/rb-synthesis/src/lib.rs`.
- [X] T058 [US3] In `crates/redstonebuilder/src/cli.rs`, add `#[arg(long, default_value_t = 4096)] pub max_ram: u32` (MiB cap) and `#[arg(long)] pub stats: bool` per contracts/cli.md.
- [X] T059 [US3] In `crates/redstonebuilder/src/pipeline.rs`: (a) install a `BudgetGuard::new(cli.max_ram, "<stage>")` and call `.check()?` between every stage (parse/synth/timing/place/route/nbt); (b) extend `PipelineError` with `Memory(MemoryError)` → exit code **8**; (c) track per-stage wall-clock with `Instant::now()`; (d) when `--stats` is set, write the structured stats line to **stderr** per contracts/cli.md §"`--stats` semantics" and quickstart.md.
- [X] T060 [P] [US3] Tests in `crates/redstonebuilder/tests/end_to_end_v2.rs` (NEW): (a) `--max-ram 1` on the half-adder forces an early exit-code-8 abort; (b) `--stats` prints the expected `parse=… synth=…` line to stderr on `half_adder.hdl`.

### Timing-error exit-code wiring (FR-V12, exit code 9)

- [X] T061 [US1] In `crates/redstonebuilder/src/pipeline.rs`, after the timing stage (Phase 1 T021), thread `TimingError::DataRace` into `PipelineError::Timing(TimingError)`; map to exit code **9** in `ExitCategory::for_error`. Honour `--allow-timing-races` by downgrading the error to a stderr warning.
- [X] T062 [P] [US1] Test in `crates/redstonebuilder/tests/end_to_end_v2.rs` (extend T060): a synthesised data-race HDL exits with code 9 by default; same input with `--allow-timing-races` exits 0.

### v1 regression suite (FR-V13 / SC-V05)

- [X] T063 [P] In `crates/redstonebuilder/tests/v1_regression.rs` (NEW): for each of the four v1 examples (`half_adder.hdl`, `dff_demo.hdl`, `full_adder.hdl`, `ripple_adder_8bit.hdl`), compile under both `--placer sa` and `--placer greedy`, assert exit 0 and the `.litematic` file is non-empty. Under `--placer greedy`, **assert byte-identical** output to the v1 baseline (use SHA-256; the baseline hashes live in `tests/golden/v1_baseline.json`).
- [ ] T064 In `crates/redstonebuilder/tests/golden/v1_baseline.json` (NEW), commit SHA-256 hashes of the v1 `.litematic` outputs for the four examples — captured by running the v1 binary once and stashing the hashes. Used by T063.

### v2 examples + e2e

- [X] T065 [P] [US1] Ship `examples/analog_add.hdl` (text from quickstart.md / contracts/hdl-grammar.md). e2e in `crates/redstonebuilder/tests/end_to_end_v2.rs` (extend T060): compile, assert exit 0, decoded palette contains `minecraft:comparator` with `mode=subtract`.
- [X] T066 [P] [US2] Ship `examples/monostable.hdl`. e2e: compile, assert exit 0, palette contains `minecraft:observer` and `minecraft:repeater{delay=1}`.
- [X] T067 [US3] Ship `examples/stress_10k.hdl` (generated via `tools/stress-gen.py --gates 10000 > examples/stress_10k.hdl`). Verify it's checked in. e2e (gated under `#[ignore]` for CI, opt-in via `cargo test --release -- --ignored`): compile, assert exit 0, assert wall time < 300 s, assert peak RAM (from `--stats` line) ≤ 4 GiB.

### Determinism — extend the v1 golden test (SC-V05 + SC-007)

- [X] T068 [P] In `crates/redstonebuilder/tests/determinism.rs`, extend the existing per-example × 10-runs SHA-256 test to additionally cover `examples/analog_add.hdl`, `examples/monostable.hdl`, and `examples/stress_10k.hdl` (the stress one gated by `#[ignore]`). Cover **both** `--placer sa` and `--placer greedy` modes.

### Docs

- [X] T069 [P] Extend top-level `README.md` (v1) with a "v2 highlights" section pointing to `specs/002-v2-analog-scale/quickstart.md`. List the new flags (`--max-ram`, `--max-routing-iterations`, `--stats`, `--placer`, `--allow-timing-races`) and exit codes 7/8/9.
- [X] T070 [P] Update `docs/grammar.md` symlink target if v2 grammar contract has changed file location (it hasn't — `specs/001/contracts/hdl-grammar.md` is the v1 grammar; add a second symlink `docs/grammar-v2.md → ../specs/002-v2-analog-scale/contracts/hdl-grammar.md` for the v2 deltas).
- [X] T071 [P] Update each crate's `Cargo.toml` `description` field if v2 changes its scope (e.g., `rb-synthesis` description should now mention "static timing analysis and simulated-annealing placement"). Keep `keywords`/`categories` from v1.

### CI gates (no source changes, just verification)

- [X] T072 Run `cargo fmt --all -- --check`. Fix any drift introduced by Phase 1–5.
- [X] T073 Run `cargo clippy --workspace --all-targets -- -D warnings`. Fix any drift. New crates allowlisted in test files: `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::result_large_err` for tests as needed (matching v1 convention).
- [X] T074 Run `cargo test --workspace`. All tests (v1 + v2) must pass; assert no test from v1 regressed.
- [X] T075 Run `cargo bench --workspace --no-run`. All benches (v1's `route_1k_gates`, `parse_10k_lines`, `serialize_256_cube`; v2's added `pathfinder_1k_gates`, `pathfinder_10k`, `sa_place_10k`) must compile.

### Manual integration (mirrors v1 T078)

- [ ] T076 Follow `quickstart.md` against a real MC Java 26.1 install with Litematica: load `analog_add.litematic` (assert in-game arithmetic), `monostable.litematic` (assert 1-tick pulse on button press for 5 distinct hold durations per SC-V02), and (optionally) `stress_10k.litematic` (assert Litematica loads it without crashing per SC-V05 acceptance scenario 3). **Status: open — requires a live MC 26.1 + Litematica install, not runnable from CI.** Mirrors v1 T078.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 0 (Setup)**: no dependencies — start immediately.
- **Phase 1 (AST/Netlist refactor)**: depends on Phase 0 (renames). **BLOCKS all of Phase 2–5** because every downstream stage consumes the new `Signal`, `Tick`, `InstKind`, and extended `EndpointRole`.
- **Phase 2 (Blocks + NBT)**: depends on Phase 1. Independent of Phase 3–5 in source-code terms.
- **Phase 3 (SA placement)**: depends on Phase 1 + Phase 2 (needs cells in the library to place).
- **Phase 4 (A* + PathFinder routing)**: depends on Phase 1 + Phase 2 + Phase 3 (routes a placement).
- **Phase 5 (Repeater insertion)**: depends on Phase 4 (modifies the A* path emission).
- **Phase 6 (Polish)**: depends on Phase 1–5 in aggregate; tasks within Phase 6 are mostly [P].

### Within-phase parallelism (highlights)

- Phase 1: T006/T007/T008 [P]; T009/T010 [P]; T013/T015/T016/T017 [P]; T021/T022 [P].
- Phase 2: T023/T024/T025/T026 [P] (each adds a different `_cell` function); T029/T031 [P].
- Phase 3: T033/T034/T035 [P].
- Phase 4: T040/T042/T045 [P] (separate test files); T038 sequential (foundation for T039+).
- Phase 5: T050/T051 [P].
- Phase 6: most of T053–T071 are [P] (different files); T072–T075 are sequential gates.

### Cross-story parallel work after Phase 1 + Phase 2 land

- One contributor on Phase 3 (placement, SA).
- One contributor on Phase 4 (routing, A* + PathFinder), starting once a stub `PlacerKind::Sa` exists.
- One contributor on Phase 5 (repeater insertion) starting once `astar_route` exists.

---

## Parallel example: Phase 1 kickoff

```bash
# Once T001–T005 (Setup) lands, these can fan out:
Task: "T006 [P] Replace SignalKind { Boolean } with Signal { value, kind } in crates/rb-core/src/signal.rs"
Task: "T007 [P] Add Tick newtype in crates/rb-core/src/timing.rs"
Task: "T008 [P] Add BlockId::Observer/TargetBlock/Slab/Glass in crates/rb-core/src/block.rs"
```

Then, once T006–T010 are in:

```bash
Task: "T013 [P] [US1+US2] New SemanticError variants in crates/rb-parser/src/error.rs"
Task: "T015 [P] [US1] Parser fixture analog_add.hdl + golden test"
Task: "T016 [P] [US2] Parser fixture monostable.hdl + golden test"
Task: "T021 [P] Timing-analysis stage in crates/rb-synthesis/src/timing.rs"
```

---

## Implementation strategy

### Vertical-slice first

For internal sanity-checks (and to give early feedback to the design):

1. Land Phase 0 + Phase 1 fully (foundational types). Run `cargo test --workspace` after every major task to keep v1 regression visible.
2. Land Phase 2 minimally — just enough that `analog_add.hdl` compiles to a `.litematic` whose palette contains `minecraft:comparator{mode=subtract}` (Phase 3/4 not yet needed; use `--placer greedy` as v1).
3. Tag `v0.2.0-alpha` — analog primitives work, scale is still v1.
4. Land Phase 3 (SA placer). Tag `v0.2.0-beta1` — analog + SA placement.
5. Land Phase 4 (A* + PathFinder router). Tag `v0.2.0-beta2` — analog + new P&R.
6. Land Phase 5 (auto-repeater on long wires). Tag `v0.2.0-beta3`.
7. Land Phase 6 (benches, regression, docs). Tag `v1.0.0` for the v2 epic.

### Two-contributor split (after Phase 1+2 land)

- **Dev A** (correctness focus): owns Phase 5 + Phase 1 timing analysis + v1 regression suite (T063/T064).
- **Dev B** (scale focus): owns Phase 3 (SA placer) + Phase 4 (A* + PathFinder) + benchmarks (T054/T055/T056) + memory budget (T057–T060).

### Three-contributor split

- **Dev A**: Phase 1 timing + Phase 5 repeater insertion + v1 regression.
- **Dev B**: Phase 3 SA placer.
- **Dev C**: Phase 4 A* + PathFinder router.
- Phase 6 polish split across all three by file.

---

## Notes

- [P] = different files, no in-progress dependencies — safe to parallelise.
- [Story] tags trace each task to its primary user-story owner (US1/US2/US3) so milestone slicing remains possible.
- Run `cargo fmt` + `cargo clippy -D warnings` + `cargo test --workspace` after each major task to keep the v1 regression visible.
- Stop at any Checkpoint to validate the corresponding feature slice independently.
- Do not commit large `.litematic` outputs except those explicitly under `tests/golden/` (golden fixtures for SC-V05 / determinism).
- The MD5-level 10 000-gate target (SC-V03 / SC-V04) is the binding perf gate. Treat T055 + T067 as the canonical "v2 is done" check.
