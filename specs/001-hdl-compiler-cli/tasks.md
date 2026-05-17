---

description: "Task list for feature 001-hdl-compiler-cli"
---

# Tasks: HDL → Minecraft Redstone Schematic Compiler (CLI)

**Input**: Design documents from `/specs/001-hdl-compiler-cli/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/*.md, quickstart.md

**Tests**: Included — the constitution requires `cargo test --workspace` to
pass on every PR (`.specify/memory/constitution.md`, Dev Workflow), and
SC-007 (deterministic output) is only verifiable via golden-file tests.
Test tasks are not strictly TDD-ordered; they live next to the
implementation they exercise.

**Organization**: Tasks are grouped by user story to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks).
- **[Story]**: Which user story this task belongs to (US1, US2, US3).
- Setup and Foundational phases have NO story label.
- Polish phase has NO story label.

## Path Conventions

Cargo workspace at repo root. Crates under `crates/`. Examples under
`examples/`. See plan.md §"Project Structure" for the full tree.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace scaffolding, toolchain pinning, lint/format/CI config.

- [X] T001 Create Cargo workspace root `Cargo.toml` with `[workspace]` members list referencing `crates/rb-core`, `crates/rb-parser`, `crates/rb-synthesis`, `crates/rb-nbt`, `crates/redstonebuilder`.
- [X] T002 [P] Add `rust-toolchain.toml` at repo root pinning `channel = "stable"`, components `rustfmt, clippy`.
- [X] T003 [P] Add workspace-wide `[lints]` table to root `Cargo.toml` enabling clippy `-D warnings` (constitution Dev Workflow).
- [X] T004 [P] Add CI workflow `.github/workflows/ci.yml` running `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo bench --workspace --no-run`.
- [X] T005 [P] Add `.gitignore` entries: `target/`, `*.litematic`, `*.litematic.gz`, `.DS_Store`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types in `rb-core` + skeletons for all dependent crates. NO user-story work can begin until this phase completes.

**⚠️ CRITICAL**: All user stories depend on `rb-core` types and on each library crate being a buildable shell.

- [X] T006 Create `crates/rb-core/Cargo.toml` (name `rb-core`, edition 2021, deps: `serde` with `derive`) and `crates/rb-core/src/lib.rs` re-exporting submodules.
- [X] T007 [P] Implement `Pos3`, `Bbox3`, `Direction` in `crates/rb-core/src/pos.rs` per data-model.md §Shared types.
- [X] T008 [P] Implement `GateKind` enum (`And`, `Or`, `Not`, `Xor`, `DTrigger`, `MemoryCell`) in `crates/rb-core/src/gate.rs`.
- [X] T009 [P] Implement `SourceSpan` (file, byte_start, byte_end, line, column) in `crates/rb-core/src/span.rs`.
- [X] T010 [P] Unit tests for `Pos3` / `Bbox3` arithmetic and containment in `crates/rb-core/tests/pos.rs`.
- [X] T011 Create `crates/rb-parser/Cargo.toml` (deps: `rb-core`, `pest`, `pest_derive`, `thiserror`, `miette`, `serde`, `smol_str`) and stub `crates/rb-parser/src/lib.rs`.
- [X] T012 Create `crates/rb-synthesis/Cargo.toml` (deps: `rb-core`, `rb-parser`, `petgraph`, `rand_chacha`, `thiserror`, `miette`, `serde`) and stub `crates/rb-synthesis/src/lib.rs`.
- [X] T013 Create `crates/rb-nbt/Cargo.toml` (deps: `rb-core`, `fastnbt`, `flate2`, `thiserror`, `miette`, `serde`, `smol_str`) and stub `crates/rb-nbt/src/lib.rs`.
- [X] T014 Create `crates/redstonebuilder/Cargo.toml` (binary; deps: all four lib crates + `clap` with `derive` + `miette` with `fancy` + `serde_json`) and stub `crates/redstonebuilder/src/main.rs` printing "redstonebuilder vX".
- [X] T015 Implement miette renderer setup (fancy theme + `--no-color` fallback) in `crates/redstonebuilder/src/report.rs`.
- [X] T016 Verify `cargo build --workspace` and `cargo test --workspace` pass on the empty shells before any story work begins.

**Checkpoint**: Foundation ready — all five crates compile, `rb-core` types are testable.

---

## Phase 3: User Story 1 — Compile a basic combinational circuit (Priority: P1) 🎯 MVP

**Goal**: A user can compile a combinational HDL (e.g., half-adder) to a `.litematic` that loads in Minecraft and behaves per the truth table.

**Independent Test**: Run `target/release/redstonebuilder examples/half_adder.hdl -o ha.litematic`; assert exit 0 and file exists; load `ha.litematic` in Litematica/MC 26.1 and verify the truth table holds. See `quickstart.md`.

### Parser (combinational subset)

- [X] T017 [P] [US1] Write the combinational subset of `hdl.pest` (file/module/port_list/port_decl/wire_decl/gate_kind/gate_inst/conn_list/conn + lexical rules) in `crates/rb-parser/src/hdl.pest` per `contracts/hdl-grammar.md`.
- [X] T018 [P] [US1] Implement AST types (`Module`, `Port`, `PortDir`, `WireDecl`, `GateInst`, `Connection`, `Ident`) with `serde::Serialize` derives in `crates/rb-parser/src/ast.rs` per `contracts/ast.md` (omit `ClockEdge`/`Edge` for now — added in US2).
- [X] T019 [P] [US1] Implement `ParseError` and `SemanticError` enums with `thiserror::Error` + `miette::Diagnostic` derives in `crates/rb-parser/src/error.rs`.
- [X] T020 [US1] Implement `parse(source, file)` in `crates/rb-parser/src/parse.rs` using `pest::Parser` derive + AST builder; map `pest::Error` spans to `miette::SourceSpan`.
- [X] T021 [US1] Implement `validate(module)` in `crates/rb-parser/src/validate.rs`: undeclared net, undriven output, multiply-driven output, duplicate identifier, gate-arity / bad-port checks (FR-004).
- [X] T022 [P] [US1] Golden-file parser tests in `crates/rb-parser/tests/golden.rs` covering: valid half-adder (assert AST), syntax-error fixture (assert `ParseError::Syntax` with line/column), undeclared-wire fixture (assert `SemanticError::UndeclaredNet`).
- [X] T023 [P] [US1] Fixture files `crates/rb-parser/tests/fixtures/half_adder.hdl`, `syntax_error.hdl` (missing `;`), `undeclared.hdl` (references wire `c` never declared).

### Synthesis — netlist + cycle + cell library (combinational only)

- [X] T024 [P] [US1] Implement `NetlistNode`, `NetlistEdge`, `NetId`, `EndpointRole` types and `NetlistGraph` alias in `crates/rb-synthesis/src/netlist.rs` per `contracts/netlist-ir.md`; `NetId`s assigned in source order.
- [X] T025 [US1] Implement `build_netlist(&Module) -> Result<Netlist, SynthError>` in `crates/rb-synthesis/src/netlist.rs`.
- [X] T026 [P] [US1] Implement `detect_cycles(&Netlist)` in `crates/rb-synthesis/src/cycle.rs` using `petgraph::algo::tarjan_scc` on the full graph (combinational-only — stateful projection added in US2).
- [X] T027 [P] [US1] Implement `CycleError`, `PlaceError`, `RouteError` (incl. `Exhausted` and `TooLarge`) types in `crates/rb-synthesis/src/error.rs` with `thiserror` + `miette::Diagnostic`.
- [X] T028 [P] [US1] Implement `Macrocell` struct + cell-library constants for `And`, `Or`, `Not`, `Xor` (block patterns from `contracts/minecraft-blocks.md` §Macro-cell layout rules) in `crates/rb-synthesis/src/cell_library.rs`.
- [X] T029 [US1] Implement `Grid3D<CellState>` (sparse-or-dense 3D grid) in `crates/rb-synthesis/src/grid3d.rs`.
- [X] T030 [US1] Implement row-based `place(&Netlist, &PlaceConfig)` in `crates/rb-synthesis/src/place.rs`: toposort → group by depth → rows along +X → return `Placement`.
- [X] T031 [US1] Implement 3D Lee's maze router `route(&Placement, &Netlist, &RouteConfig)` in `crates/rb-synthesis/src/route.rs`: per-net BFS in deterministic net-ID order, obstruction propagation (4-laterals at y, y±1), repeater insertion at run length 15 (FR-008, FR-009). Bbox retry stub returns first-attempt result for now (full retry in US3).
- [X] T032 [P] [US1] Unit test in `crates/rb-synthesis/tests/cycle.rs`: combinational 3-gate cycle → `CycleError::Combinational` listing wire names.
- [X] T033 [P] [US1] Unit test in `crates/rb-synthesis/tests/route_adjacency.rs`: two independent nets routed in parallel through a 3×3 corridor → assert no `Dust` of net A is adjacent (4-lateral, same y) to `Dust` of net B.
- [X] T034 [P] [US1] Unit test in `crates/rb-synthesis/tests/repeater.rs`: a single net forced to span 20 blocks → assert exactly one repeater inserted at block 15.

### NBT writer

- [X] T035 [P] [US1] Define `BlockState`, `BlockName`, `BlockGrid` in `crates/rb-nbt/src/lib.rs`.
- [X] T036 [P] [US1] Implement block-state catalogue constants (`AIR`, `STONE`, `REDSTONE_DUST`, `REDSTONE_TORCH`, `REDSTONE_WALL_TORCH`, `REPEATER`, `COMPARATOR`, `LEVER`, `REDSTONE_LAMP`, `OAK_PLANKS`, `OAK_STAIRS`) in `crates/rb-nbt/src/palette.rs` per `contracts/minecraft-blocks.md`.
- [X] T037 [US1] Implement palette index assignment (index 0 = AIR, then first-encounter order during `(y,z,x)` preorder walk) in `crates/rb-nbt/src/palette.rs`.
- [X] T038 [US1] Implement packed-long `BlockStates` encoder in `crates/rb-nbt/src/pack.rs`: `bits = max(2, ceil(log2(palette.len())))`, little-endian within each `i64`, iteration order `(y, z, x)`.
- [X] T039 [US1] Implement Litematica root tag construction in `crates/rb-nbt/src/litematic.rs`: `Metadata` (with zeroed `TimeCreated`/`TimeModified`) + single `Regions["main"]` compound per `contracts/litematic-nbt.md`.
- [X] T040 [US1] Implement `write_litematic(path, &BlockGrid, &Metadata)` using `flate2::GzEncoder::new(BufWriter::new(file), Compression::default())` + `fastnbt::to_writer` in `crates/rb-nbt/src/writer.rs`.
- [X] T041 [P] [US1] Implement `NbtError` enum (`Io`, `Nbt`, `PaletteOverflow`) in `crates/rb-nbt/src/error.rs`.
- [X] T042 [P] [US1] Round-trip test in `crates/rb-nbt/tests/roundtrip.rs`: build a tiny `BlockGrid`, write to a temp `.litematic`, parse back with `fastnbt`, assert `BlockGrid` reconstructs identically.

### CLI + end-to-end wiring

- [X] T043 [P] [US1] Implement `Cli` struct (clap derive) with `input`, `--output`, `--dump-ast/--dump-netlist/--dump-placement` (each `Option<PathBuf>` with `default_missing_value = "-"`), `--dump-only`, `--verbose` in `crates/redstonebuilder/src/cli.rs`. `--max-footprint` and `--seed` accepted but unused in US1 (defaults applied).
- [X] T044 [US1] Implement `pipeline::run(&Cli) -> Result<RunSummary, Report>` wiring parse → validate → build_netlist → detect_cycles → place → route → BlockGrid build → write_litematic in `crates/redstonebuilder/src/pipeline.rs`.
- [X] T045 [US1] Implement `--dump-ast`/`--dump-netlist`/`--dump-placement` JSON output via `serde_json::to_writer_pretty` (stdout if value is `-`, file otherwise) inside `crates/redstonebuilder/src/pipeline.rs`.
- [X] T046 [US1] Implement `main()` in `crates/redstonebuilder/src/main.rs`: parse `Cli`, install miette handler from `report.rs`, dispatch to `pipeline::run`, map errors to exit codes per `contracts/cli.md`.
- [X] T047 [P] [US1] Ship `examples/half_adder.hdl` (text from `contracts/hdl-grammar.md` §Example — half adder).
- [X] T048 [US1] End-to-end test `crates/redstonebuilder/tests/end_to_end_us1.rs`: compile `examples/half_adder.hdl` to a temp dir, assert exit 0, assert file exists and is non-empty, parse the output back via `fastnbt` and assert region size is reasonable (> 0 blocks of `minecraft:redstone_wire` present).

**Checkpoint**: US1 MVP complete. Combinational HDL → `.litematic` end-to-end is testable; `cargo run --release -- examples/half_adder.hdl -o ha.litematic` succeeds.

---

## Phase 4: User Story 2 — Compile sequential circuits with timing (Priority: P2)

**Goal**: D-Trigger and Memory Cell instances compile and behave correctly in-game; legal sequential feedback (cycle through a stateful primitive) is accepted while pure combinational cycles are still rejected.

**Independent Test**: Compile `examples/dff_demo.hdl`; assert exit 0; load in Litematica/MC 26.1 and verify `Q` latches `D` on clock posedge.

### Parser — sequential extensions

- [X] T049 [US2] Extend `crates/rb-parser/src/hdl.pest` with `always_block`, `edge_spec` (`posedge`/`negedge`), `stateful_inst` (`dtrigger`/`memcell`), `nonblock_assign`.
- [X] T050 [US2] Extend AST in `crates/rb-parser/src/ast.rs` with `ClockEdge { edge, clock_net, span }` and `Edge { Posedge, Negedge }`; add `clock: Option<ClockEdge>` to `GateInst`.
- [X] T051 [US2] Update `parse()` in `crates/rb-parser/src/parse.rs` to attach the surrounding `always @(edge clk)` to each contained `stateful_inst`'s `clock` field.
- [X] T052 [P] [US2] Parser fixtures `crates/rb-parser/tests/fixtures/dff.hdl`, `memcell.hdl`; assertion tests in `crates/rb-parser/tests/golden.rs` verifying `GateKind::DTrigger` / `MemoryCell` plus `clock = Some(Posedge, "clk")`.

### Synthesis — stateful nodes + cycle projection

- [X] T053 [US2] Extend `crates/rb-synthesis/src/netlist.rs` to construct `NetlistNode::Gate { kind: DTrigger | MemoryCell, clock: Some(_) }` with correct `EndpointRole`s (`ClockIn`, `DataInStateful`, `WriteEnable`, `Q`).
- [X] T054 [US2] Rewrite `detect_cycles` in `crates/rb-synthesis/src/cycle.rs` to first build the combinational projection (remove incoming edges of role `DataInStateful` / `WriteEnable` from stateful gates), then run Tarjan SCC (FR-005).
- [X] T055 [P] [US2] Add `DTrigger` and `MemoryCell` macro-cells with correct anchors and tick-delays to `crates/rb-synthesis/src/cell_library.rs` per `contracts/minecraft-blocks.md`.
- [X] T056 [US2] Update placement in `crates/rb-synthesis/src/place.rs` to treat each stateful node as both a topological sink (for its data inputs) and a source (for its Q output) — assign placement depth based on the source side.
- [X] T057 [P] [US2] Test in `crates/rb-synthesis/tests/sequential_cycle.rs`: `dff(D, CLK, Q)` with `Q → D` feedback compiles (no `CycleError`); a 3-gate XOR feedback loop fails with `CycleError::Combinational` listing the three wires.
- [X] T058 [P] [US2] Test in `crates/rb-synthesis/tests/cell_library.rs`: instantiating each macro-cell yields the expected bbox and anchor count.

### CLI + e2e

- [X] T059 [P] [US2] Ship `examples/dff_demo.hdl` (text from `contracts/hdl-grammar.md` §Example — clocked D flip-flop).
- [X] T060 [US2] Extend `crates/redstonebuilder/tests/end_to_end_us1.rs` (or new `end_to_end_us2.rs`) to compile `examples/dff_demo.hdl`; assert exit 0 and file exists.

**Checkpoint**: US2 complete. Sequential circuits compile end-to-end.

---

## Phase 5: User Story 3 — Compile a large multi-gate design (Priority: P3)

**Goal**: 500+ gate designs compile within performance budgets; `--max-footprint` is honored; router auto-expands bbox on failure and reports unrouted nets cleanly.

**Independent Test**: Compile `examples/ripple_adder_8bit.hdl` (≈ 50 gates) in < 10 s; compile a stress-test 1k-gate design in < 120 s; design that intentionally exceeds default footprint reports `PlaceError::TooLarge`; design that cannot route (pathological dense input) reports `RouteError::Exhausted` with the unrouted net list.

### Place & route — scale features

- [X] T061 [US3] Implement bbox auto-expansion loop in `crates/rb-synthesis/src/route.rs`: on per-net BFS failure, grow active bbox by `BBOX_GROW_STEP` (e.g., 16 blocks) in the lowest-resistance axis and retry, up to `MAX_BBOX_RETRIES = 8`; on final failure return `RouteError::Exhausted { unrouted, retries, final_bbox }` (FR-015).
- [X] T062 [US3] Implement `--max-footprint W×H×D` enforcement in `crates/rb-synthesis/src/place.rs`: after row layout, compare actual bounds to `PlaceConfig.max_footprint`; on overflow return `PlaceError::TooLarge { actual, bound }` (FR-016).
- [X] T063 [P] [US3] Implement `parse_footprint("256x320x256")` value parser (reject 0 or > 4096 per dimension) in `crates/redstonebuilder/src/cli.rs`; wire `--max-footprint` into `PlaceConfig`.
- [X] T064 [P] [US3] Wire `--seed` flag (default = `rb_synthesis::DEFAULT_SEED` const) into `RouteConfig.seed`; if any randomized step is added later, it must consume only this seed (FR-017).
- [X] T065 [P] [US3] Implement `--dump-only` flag behavior in `crates/redstonebuilder/src/pipeline.rs`: when set with any `--dump-*`, skip the write_litematic step.

### Examples + tests

- [X] T066 [P] [US3] Ship `examples/ripple_adder_8bit.hdl` — an 8-bit ripple-carry adder built from `xor`/`and`/`or` instances.
- [X] T067 [P] [US3] Test in `crates/rb-synthesis/tests/footprint.rs`: a synthesized design exceeding 256×320×256 → `PlaceError::TooLarge` with the actual and bound dimensions in the diagnostic.
- [X] T068 [P] [US3] Test in `crates/rb-synthesis/tests/route_exhausted.rs`: a pathological dense netlist where routing cannot succeed within `MAX_BBOX_RETRIES` → `RouteError::Exhausted` listing the unrouted net names.
- [X] T069 [US3] End-to-end test `crates/redstonebuilder/tests/end_to_end_us3.rs`: compile `examples/ripple_adder_8bit.hdl`; assert exit 0; assert wall time < 10 s (SC-005); assert output bounds within default footprint.
- [X] T070 [P] [US3] `criterion` benchmark `crates/rb-synthesis/benches/route_1k_gates.rs` synthesizing a 1k-gate generator output; perf gate is SC-006 (< 120 s) but bench runs in `--no-run` on PRs (full runs nightly).

**Checkpoint**: US3 complete. All three user stories shipped.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Determinism verification, benchmarks for all crates, docs, lint gates.

- [X] T071 [P] Determinism golden-file test `crates/redstonebuilder/tests/determinism.rs` (SC-007): for each shipped example, compile 10 times in a row, assert all 10 outputs SHA-256-match each other and match a committed reference hash under `tests/golden/`.
- [X] T072 [P] `criterion` benchmark `crates/rb-parser/benches/parse_10k_lines.rs` parsing a generator-produced 10 000-line HDL file.
- [X] T073 [P] `criterion` benchmark `crates/rb-nbt/benches/serialize_256_cube.rs` serializing a fully populated 256³ `BlockGrid`.
- [X] T074 [P] Generate `docs/grammar.md` from `contracts/hdl-grammar.md` (either symlink or render via a small build script) so the grammar lives at a stable user-facing path.
- [X] T075 [P] Add top-level `README.md` pointing to `specs/001-hdl-compiler-cli/quickstart.md` and listing the five workspace crates with a one-line description of each.
- [X] T076 [P] Fill `Cargo.toml` package metadata (`description`, `license`, `repository`, `keywords`, `categories`) for each crate so they are publish-ready.
- [X] T077 Run `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` against the final tree; fix any drift.
- [ ] T078 Manual integration: follow `quickstart.md` against a real MC Java 26.1 install with Litematica; verify the half-adder schematic loads and the truth table holds. If no Litematica build exists for 26.1 yet, document the gap in the spec per FR-014 fallback policy. **Status: open — requires a live MC 26.1 + Litematica install, not runnable from CI. All in-tree gates (compile / test / fmt / clippy / bench --no-run / determinism golden / round-trip NBT) pass.**

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS all user stories.
- **User Story 1 (Phase 3 — P1, MVP)**: Depends on Foundational. Independent of US2/US3.
- **User Story 2 (Phase 4 — P2)**: Depends on Foundational + US1 (extends parser/netlist/cell-library/CLI built in US1).
- **User Story 3 (Phase 5 — P3)**: Depends on Foundational + US1 (extends placement/router/CLI built in US1). Independent of US2.
- **Polish (Phase 6)**: Depends on US1 minimum (early polish tasks); full polish after US3.

### Within User Story 1 (key chains)

- T017 (grammar) → T020 (parse) → T044 (pipeline) → T048 (e2e).
- T018 (AST) → T020 (parse) / T024 (netlist).
- T024 (netlist) → T025 (build), T026 (cycle), T030 (place).
- T028 (cell library) → T030 (place) → T031 (route).
- T035, T036, T037, T038, T039 → T040 (writer) → T042 (round-trip), T044 (pipeline).

### Within User Story 2

- T049 (grammar) → T051 (parse) → T053 (netlist) → T054 (cycle projection).
- T055 (stateful cells) → T056 (placement) → T060 (e2e).

### Within User Story 3

- T061 (bbox retry) and T062 (footprint enforcement) are independent and can run in parallel.
- T063, T064, T065 (CLI flag wiring) can run in parallel once T061/T062 land.

### Parallel Opportunities

- All Phase 1 [P] setup tasks can run in parallel.
- Within Foundational: T007/T008/T009 + T010 are [P]; T011–T014 are independent crate skeletons and can run in parallel; T015 is independent.
- Within US1: T017/T018/T019 [P]; T024/T026/T027/T028 [P] (different files); T035/T036/T041 [P]; T043/T047 [P]; tests T022/T032/T033/T034/T042 all [P].
- Across user stories with sufficient capacity: US2 and US3 can be developed in parallel by different contributors after US1 lands (they touch mostly different code regions — US2 touches parser/cycle/cell-library, US3 touches route/place/CLI).

---

## Parallel Example: User Story 1

```bash
# After Foundational checkpoint, kick off these in parallel:
Task: "T017 [P] [US1] Write hdl.pest combinational grammar in crates/rb-parser/src/hdl.pest"
Task: "T018 [P] [US1] Implement AST types in crates/rb-parser/src/ast.rs"
Task: "T019 [P] [US1] Implement ParseError/SemanticError in crates/rb-parser/src/error.rs"
Task: "T024 [P] [US1] Implement NetlistNode/NetlistEdge/NetId types in crates/rb-synthesis/src/netlist.rs"
Task: "T027 [P] [US1] Implement CycleError/PlaceError/RouteError in crates/rb-synthesis/src/error.rs"
Task: "T028 [P] [US1] Implement And/Or/Not/Xor macro-cells in crates/rb-synthesis/src/cell_library.rs"
Task: "T035 [P] [US1] Define BlockState/BlockName/BlockGrid in crates/rb-nbt/src/lib.rs"
Task: "T036 [P] [US1] Block-state catalogue in crates/rb-nbt/src/palette.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (BLOCKS US1).
3. Complete Phase 3: User Story 1 (combinational compilation, half-adder demo).
4. **STOP and VALIDATE**: Run `quickstart.md` end-to-end with the half-adder; manually verify in Litematica/MC 26.1.
5. Tag `v0.1.0-mvp`. Ship if criteria SC-001, SC-002 (combinational subset), SC-004, SC-005 are met.

### Incremental Delivery

- **Milestone 1 (US1)** — combinational compilation works. Tag `v0.1.0`.
- **Milestone 2 (US2)** — D-Trigger + Memory Cell + legal sequential feedback. Tag `v0.2.0`.
- **Milestone 3 (US3)** — bbox-retry, `--max-footprint`, 1k-gate perf. Tag `v0.3.0`.
- **Milestone 4 (Polish)** — determinism golden tests, all benches, README, manual 26.1 validation. Tag `v1.0.0`.

### Parallel Team Strategy

Once Foundational is in:

- **Dev A**: Owns US1 (parser → netlist → router → NBT — the critical path through every crate).
- **Dev B**: Can start US2 (parser + cycle extensions + stateful cells) as soon as US1's parser and netlist skeletons exist (~T020, T024).
- **Dev C**: Can start US3 (route/place scale features) as soon as US1's router skeleton exists (~T031).

---

## Notes

- [P] = different files, no in-progress dependencies — safe to run in parallel.
- [Story] maps each task to a user story for traceability and milestone-cut.
- Verify the CI gate (`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo bench --no-run`) passes after each task or logical group.
- Stop at any Checkpoint to validate the corresponding user story independently — the spec's per-story Acceptance Scenarios are the bar.
- Do not commit `.litematic` files into git except those explicitly under `tests/golden/` (golden fixtures for SC-007 / SHA-256 reference hashes).
