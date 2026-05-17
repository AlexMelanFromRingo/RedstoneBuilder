# Implementation Plan: HDL → Minecraft Redstone Schematic Compiler (CLI)

**Branch**: `001-hdl-compiler-cli` | **Date**: 2026-05-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-hdl-compiler-cli/spec.md`

## Summary

Build a Rust CLI tool that compiles a Verilog-subset HDL into a
Minecraft Litematica `.litematic` schematic targeting MC Java Edition
26.1. The pipeline is a classic 5-stage compiler — Parser → AST →
Logic Synthesis → 3D Place & Route → NBT Generation — implemented as a
Cargo workspace of 4 library crates + 1 binary. Place & Route is row-
based deterministic placement plus per-net 3D Lee's maze routing with
an adjacency-aware obstruction model to prevent redstone-dust shorts.
Output is fully deterministic (bit-identical for a given input + flags +
tool version).

## Technical Context

**Language/Version**: Rust stable, edition 2021. MSRV pinned at 1.86.

**Primary Dependencies**:

| Crate          | Version | Purpose                                              |
|----------------|---------|------------------------------------------------------|
| `clap`         | 4.x (derive feat) | CLI front-end (FR-001, FR-013, FR-016, FR-017, FR-018) |
| `thiserror`    | 1.x     | Library-level error enums                            |
| `miette`       | 7.x     | User-facing diagnostics with spans (FR-003)          |
| `pest`         | 2.x     | PEG parser for HDL surface syntax (FR-002)           |
| `pest_derive`  | 2.x     | Compile `.pest` grammar at build time                |
| `smol_str`     | 0.2.x   | Cheap interned identifiers in AST                    |
| `petgraph`     | 0.6.x   | Netlist graph, toposort, SCC for cycle detection     |
| `rand_chacha`  | 0.3.x   | Reproducible RNG seeding (FR-017)                    |
| `fastnbt`      | 2.x     | NBT serialization (FR-011, FR-012)                   |
| `flate2`       | 1.x     | Gzip wrapper for `.litematic` (FR-011)               |
| `serde`        | 1.x     | Derive `Serialize` on AST / netlist / placement IR   |
| `serde_json`   | 1.x     | `--dump-*` JSON output                               |
| `criterion`    | 0.5.x   | Benchmarks (constitution Principle IV)               |

**Storage**: file-based only — input HDL on disk, `.litematic` written
to disk. No databases.

**Testing**: `cargo test` per crate; `cargo test --workspace` in CI;
golden-file fixtures under `tests/fixtures/` for parser, netlist, and
NBT output (SC-007). `criterion` benches per crate.

**Target Platform**: any platform Rust supports (Linux, macOS, Windows).
CLI tool, no platform-specific code.

**Project Type**: compiler / CLI tool.

**Performance Goals**:

- 50-gate design: < 10 s wall (SC-005).
- 1,000-gate design: < 120 s wall (SC-006).
- Single-threaded acceptable for v1 (parallelization deferred — see
  Constitution Principle IV "design must allow parallelization later").

**Constraints**:

- No `unwrap()` / `panic!()` on input-reachable paths (Principle I).
- Bit-identical output across runs (FR-017).
- Default footprint cap 256×<MC build height>×256 (FR-016).
- All inter-stage data flows through typed IRs documented in
  `contracts/` (Principle II).

**Scale/Scope**: v1 targets HDL designs up to ~1,000 gates fitting
inside the default footprint. Larger designs are technically allowed
(via `--max-footprint`) but unverified at this milestone.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Plan compliance | Evidence |
|-----------|------------------|----------|
| **I. Rust-First, No Panics** | ✅ | All errors flow through `thiserror`/`miette` types defined per crate (see contracts/ast.md, contracts/netlist-ir.md, contracts/litematic-nbt.md). No `unwrap`/`panic`/`expect`/`unreachable` in input-reachable code. Tests may use `unwrap`. |
| **II. Strict Pipeline Architecture** | ✅ | Pipeline stages 1–6 in data-model.md (Parser → AST → Netlist → Placement → Routed Layout → NBT). Each is a typed function. `--dump-ast`, `--dump-netlist`, `--dump-placement` flags expose every stage independently. No back-edges. |
| **III. Module Isolation via Cargo Workspace** | ✅ | 5 crates (`rb-core`, `rb-parser`, `rb-synthesis`, `rb-nbt`, `redstonebuilder` bin) with strict DAG: `bin → {parser, synthesis, nbt} → core`. Each library crate independently compiles, tests, benches. |
| **IV. Built for High-Throughput Compilation** | ✅ (with v1 caveat) | Single-threaded v1 with `BTreeMap`-based determinism. Hot paths (router BFS, palette emission) designed with `rayon` parallelization in mind (per-net routing is embarrassingly parallel once the obstruction grid is partitioned). `criterion` benches in each crate. SC-005/SC-006 are the perf gates. |

**No violations to justify.** Complexity Tracking table below is empty.

Post-design re-check (after Phase 1 contracts authored): still ✅. The
contracts in `contracts/` explicitly enumerate inter-stage IRs as
typed Rust structs, the cycle-detection algorithm (FR-005) is a pure
function over a `petgraph::StableDiGraph` projection, and the NBT writer
takes a sparse `BlockGrid` so it has no knowledge of upstream stages.

## Project Structure

### Documentation (this feature)

```text
specs/001-hdl-compiler-cli/
├── plan.md                       # THIS file
├── spec.md                       # /speckit-specify + /speckit-clarify output
├── research.md                   # Phase 0 — tech decisions
├── data-model.md                 # Phase 1 — entities & pipeline stages
├── quickstart.md                 # Phase 1 — onboarding doc
├── contracts/
│   ├── cli.md                    # CLI command schema (clap)
│   ├── hdl-grammar.md            # Verilog-subset PEG grammar
│   ├── ast.md                    # AST types (rb-parser public surface)
│   ├── netlist-ir.md             # Netlist + synth + place + route IR
│   ├── minecraft-blocks.md       # MC 26.1 block-state catalogue + cell library
│   └── litematic-nbt.md          # Output NBT tag tree
└── checklists/
    └── requirements.md           # Spec quality checklist (all green)
```

### Source Code (repository root)

```text
RedstoneBuilder/                  # Cargo workspace root
├── Cargo.toml                    # [workspace] declaration
├── rust-toolchain.toml           # pinned stable
├── crates/
│   ├── rb-core/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── pos.rs            # Pos3, Bbox3, Direction
│   │   │   ├── gate.rs           # GateKind enum
│   │   │   └── span.rs           # SourceSpan
│   │   └── tests/
│   ├── rb-parser/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── hdl.pest          # PEG grammar (the FR-002 contract)
│   │   │   ├── ast.rs            # AST types per contracts/ast.md
│   │   │   ├── parse.rs          # parse() entry point
│   │   │   ├── validate.rs       # semantic validation
│   │   │   └── error.rs          # ParseError, SemanticError (thiserror/miette)
│   │   ├── benches/
│   │   │   └── parse_10k_lines.rs
│   │   └── tests/
│   │       ├── golden/           # golden-file AST dumps
│   │       └── fixtures/         # *.hdl examples
│   ├── rb-synthesis/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── netlist.rs        # NetlistGraph builder
│   │   │   ├── cycle.rs          # FR-005 cycle detection
│   │   │   ├── cell_library.rs   # Macrocell constants for AND/OR/NOT/XOR/D/Mem
│   │   │   ├── place.rs          # row-based placement
│   │   │   ├── route.rs          # 3D Lee's maze router + obstruction grid
│   │   │   ├── grid3d.rs         # internal Grid3D<CellState>
│   │   │   ├── error.rs          # CycleError, PlaceError, RouteError
│   │   │   └── dump.rs           # --dump-netlist / --dump-placement
│   │   ├── benches/
│   │   │   └── route_1k_gates.rs # SC-006 perf gate
│   │   └── tests/
│   │       ├── golden/           # netlist/placement golden JSON
│   │       └── fixtures/         # cycle / large / corner cases
│   ├── rb-nbt/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── palette.rs        # Block-state catalogue + index assignment
│   │   │   ├── pack.rs           # packed-long BlockStates encoder
│   │   │   ├── litematic.rs      # root tag construction
│   │   │   ├── writer.rs         # gzip+fastnbt write_litematic()
│   │   │   └── error.rs          # NbtError
│   │   ├── benches/
│   │   │   └── serialize_256_cube.rs
│   │   └── tests/
│   │       ├── golden/           # SHA-256 of reference outputs (SC-007)
│   │       └── roundtrip.rs      # parse-our-own-output
│   └── redstonebuilder/          # binary crate
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs           # entry; calls cli::run()
│       │   ├── cli.rs            # clap Args struct
│       │   ├── pipeline.rs       # wires parse → synth → nbt
│       │   └── report.rs         # miette renderer setup
│       └── tests/
│           └── end_to_end.rs     # examples/half_adder.hdl → .litematic
├── examples/                     # shipped HDL examples
│   ├── half_adder.hdl
│   ├── dff_demo.hdl
│   └── ripple_adder_8bit.hdl
└── docs/
    └── grammar.md                # rendered hdl-grammar.md (user-facing)
```

**Structure Decision**: Cargo workspace with the 5 crates listed above.
Direct mapping to Constitution Principle III. The folder layout matches
the contracts in `specs/001-hdl-compiler-cli/contracts/` one-to-one
(each contract describes the public surface of one crate or one inter-
stage IR).

## Phase 2 (next) — Tasks

Implementation tasks will be generated by `/speckit-tasks` from this
plan + spec + contracts. Tasks will be grouped by:

- **Setup** (workspace skeleton, CI, lint/format config).
- **Foundational** (`rb-core` types, error scaffolding, miette wiring).
- **User Story 1 (P1, MVP)** — combinational compilation:
  parser for combinational subset → netlist → placement → router →
  NBT for AND/OR/NOT/XOR cells.
- **User Story 2 (P2)** — sequential support:
  D-Trigger / Memory Cell cells, edge-trigger detection, stateful-cycle
  legalization (FR-005 projection).
- **User Story 3 (P3)** — scale:
  bbox-expansion retry (FR-015), `--max-footprint`, performance work to
  hit SC-006, 8-bit ripple-adder reference design.
- **Polish** — docs, golden fixtures, criterion benches, release CI.

## Forward-Compatibility for the Analog/Event Epic (Post-MVP)

The v1 codebase MUST stay compatible with the Post-MVP epic described
in `spec.md` §"Post-MVP Epic — Analog Signals & Block-Update Events".
Concrete v1 obligations:

1. **`rb_core::GateKind`** is `#[non_exhaustive]`. Future variants
   `Comparator { mode: CompareMode }` and `Observer` can be added in a
   minor release. v1 match-arms on `GateKind` must therefore include a
   catch-all (`_ => ...`) wherever they could otherwise become exhaustive.
2. **`rb_core::SignalKind`** exists with a single `Boolean` variant,
   also `#[non_exhaustive]`. Future variants planned:
   `AnalogStrength` (carrying an in-game 0–15 signal strength) and
   `EdgeTrigger` (carrying a 1-tick pulse semantics). v1 netlist's
   per-net data MUST carry a `SignalKind` field so widening is additive,
   not a refactor.
3. **Router signal-strength model**: the v1 router already counts
   signal strength 0–15 internally for repeater insertion (FR-009). The
   epic will surface this count at the user-net level rather than
   collapsing it to boolean. The router's data model MUST keep the
   strength as a first-class quantity (not a derived `if strength==0`
   flag) so analog routing reuses the same maze-router core.
4. **Grammar** (`hdl.pest`): no v1 reservation of `comparator` /
   `observer` keywords is required (they will be added by the epic).
   However, the `reserved` list MUST stay easy to extend — keep it as
   a single PEG rule rather than scattering keyword exclusion
   throughout the grammar.
5. **NBT writer**: comparators and observers introduce new block-state
   palette entries (`minecraft:comparator` already in the v1 palette
   catalogue per `contracts/minecraft-blocks.md`; `minecraft:observer`
   to be added). No structural change to the `.litematic` writer is
   anticipated.

These obligations are **v1 design constraints** — they do not turn the
epic into v1 work. They cost nothing if respected from the start; they
become expensive refactors if ignored.

## Complexity Tracking

> Empty — Constitution Check passes without violations.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| —         | —          | —                                   |
