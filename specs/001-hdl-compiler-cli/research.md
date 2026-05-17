# Phase 0 Research — HDL → Minecraft Redstone Schematic Compiler

**Feature**: 001-hdl-compiler-cli
**Date**: 2026-05-17

This document records the technology decisions that resolve the open
implementation questions surfaced by the spec and by the user's stack
proposal.

---

## Decision 1 — Parser library

**Decision**: **`pest` v2.x** (PEG-based parser generator).

**Rationale**:

- FR-002 mandates that the HDL grammar be distributed as a written
  specification. A `.pest` grammar file *is* that specification: declarative,
  readable, version-controllable.
- `pest` returns `Pair<Rule>` values with byte-range `Span` info; these
  spans plug directly into `miette::SourceSpan` for the diagnostics required
  by FR-003 / Constitution Principle I.
- Verilog subset is a fixed-shape grammar with no need for backtracking
  heuristics → PEG is a natural fit.
- Parser perf is not the bottleneck — synthesis and P&R dominate. `pest`'s
  modest perf gap vs `nom` is irrelevant at our input sizes (a few thousand
  gate declarations).

**Alternatives considered**:

- `nom` (parser combinators): faster, more flexible, but verbose and
  harder for outside contributors to read. Better for binary formats. Would
  duplicate grammar logic across code instead of one `.pest` file.
- `chumsky`: excellent recovery + diagnostics, but adds API surface we
  don't need and is younger / less stable than `pest`.
- `lalrpop`: LR(1) generator; great for traditional compiler languages but
  the build-time codegen integration is fussy and PEG is enough here.

---

## Decision 2 — NBT serialization library

**Decision**: **`fastnbt` v2.x**.

**Rationale**:

- Most actively maintained NBT crate in the Rust ecosystem (last update ~2
  months ago at time of writing).
- `serde`-based: AST → NBT serialization via `#[derive(Serialize)]` keeps
  the writer code small.
- Supports the modern packed-long block-state encoding (post-1.13 flattening
  + post-1.16 packed-long format) that Litematica regions use.
- Round-trips NBT arrays (`ByteArray`, `IntArray`, `LongArray`) correctly,
  which the older `hematite-nbt` does not handle cleanly.

**Alternatives considered**:

- `hematite-nbt` (PistonDevelopers): last update ~5 years ago. Stagnant.
- `quartz_nbt`: last update ~2 years ago; has built-in gzip/zlib support
  and SNBT, but lower activity than `fastnbt`. Decent fallback if `fastnbt`
  proves limiting.

**Sources**:

- <https://lib.rs/crates/fastnbt>
- <https://crates.io/crates/quartz_nbt>
- <https://github.com/PistonDevelopers/hematite_nbt>

---

## Decision 3 — Gzip wrapper

**Decision**: **`flate2`** with `GzEncoder::new(_, Compression::default())`.

**Rationale**:

- `.litematic` is gzip-compressed NBT (RFC 1952). `flate2` is the de-facto
  Rust gzip library and is what `fastnbt` examples wrap with.
- Use streaming `GzEncoder` writing into the output file — avoids holding
  the entire serialized NBT in memory for large schematics.

**Alternatives considered**:

- `miniz_oxide` directly (no `flate2` wrapper): identical zlib backend,
  more boilerplate, no benefit.

---

## Decision 4 — CLI argument parsing

**Decision**: **`clap` v4 with `derive` feature**.

**Rationale**:

- Standard Rust CLI library; derive macros produce a typed `Args` struct
  that pairs naturally with `miette::Result` returns.
- Supports the full flag surface needed (`--max-footprint`, `--seed`,
  `--dump-ast`, `--dump-netlist`, `--dump-placement`, plus the standard
  `--help`/`--version`).
- Built-in support for value parsers — we can validate `WxHxD` strings at
  parse time.

---

## Decision 5 — Error and diagnostics stack

**Decision**: **`thiserror` for library-level error types + `miette` for the
top-level diagnostic renderer**.

**Rationale**:

- Aligned with user stack proposal and with Constitution Principle I
  ("rich diagnostics with source location").
- `thiserror::Error` lives in the library crates (`parser`, `synthesis`,
  `nbt_export`); each defines a typed `Error` enum.
- `miette::Diagnostic` lives at the CLI binary level: the binary converts
  library errors into diagnostics with labeled spans and prints them via
  `miette::Report`.
- Library code never `panic!`s or `unwrap`s on user input (Principle I).

---

## Decision 6 — Graph library

**Decision**: **`petgraph` v0.6+** (with `Csr` for the dense routing
grid and `StableGraph` for the netlist).

**Rationale**:

- Stable, mature, deterministic iteration order (with `StableGraph`).
- Provides topological sort and SCC (Tarjan) we need for FR-005 cycle
  detection and for the cycle-cutting walk that treats stateful primitive
  inputs as sinks (see data-model.md).

---

## Decision 7 — Place & Route algorithm

**Decision**: For v1, **two-stage P&R**:

1. **Placement**: row-based grid placement.
   - Each macro-cell (synthesized gate implementation) has a fixed bounding
     box from the cell library.
   - Cells are placed in rows along the X axis, grouped by topological
     depth from the inputs (this minimizes long back-flowing wires).
   - Y is depth-into-the-build (height = chunks of stacked logic if needed).
2. **Routing**: **3D Lee's maze router** (BFS) per net, in deterministic
   net order (sorted by net ID = source order in the HDL).
   - Each routed cell claims a 3D "obstruction footprint" representing the
     redstone-adjacency rules (dust connects to dust at 4-neighbors and at
     ±1 Y offset under air); this prevents the FR-008 short-circuit case.
   - Repeater insertion is automatic: any straight dust run of length ≥ 15
     gets a repeater on the 15th block (FR-009).
   - If a net cannot route, the router expands the active bounding box by
     a fixed step (e.g., +16 blocks in the lowest-resistance axis) and
     retries up to `MAX_BBOX_RETRIES = 8` (FR-015). Then hard-fail.

**Rationale**:

- Lee's algorithm is the textbook 3D maze router; guaranteed-optimal for a
  single net under the obstruction model, easy to implement, easy to debug.
- Per-net ordering + deterministic obstruction tie-breaks satisfy FR-017
  (bit-identical output).
- Row-based placement is the simplest deterministic strategy that yields
  short-enough wires for v1.

**Alternatives considered**:

- **Simulated annealing placement**: better quality, needs RNG → seeded
  (still meets FR-017), but more complex. Defer to a future release.
- **Pathfinder negotiated-congestion routing**: standard in modern FPGA
  CAD; needed only when greedy maze routing fails too often. Will measure
  v1's routability against SC-008 and revisit if we miss the target.

---

## Decision 8 — Workspace layout

**Decision**: Cargo workspace with **5 crates**:

- `rb-core` — shared types (positions, source spans, error trait, common
  enums for gate kinds).
- `rb-parser` — `.pest` grammar + tokenizer + AST construction.
- `rb-synthesis` — netlist building, cycle detection, gate-cell library,
  placement, routing.
- `rb-nbt` — `.litematic` writer + Minecraft block-state palette.
- `redstonebuilder` (bin) — CLI front-end (clap + miette) tying it together.

**Rationale**:

- Direct mapping to Constitution Principle III (parsing / graph / file
  formats must be independent modules).
- Cycle-free DAG: `bin → {parser, synthesis, nbt} → core`. No
  back-edges.
- Each library crate is independently testable and benchable
  (Constitution Principle IV).

---

## Decision 9 — Minecraft version target & Litematica compatibility

**Decision**: Target **MC Java Edition 26.1 "Tiny Takeover"** (spec FR-014).
The `.litematic` format itself is stable across recent MC versions — the
version-specific bit is the **block-state palette** (block names + state
properties). Maintain a single hard-coded palette table for 26.1.

**Open risk** (tracked in spec FR-014): Litematica mod build for 26.1 must
exist at release. Mitigation: design `rb-nbt` so swapping the palette table
file is a one-crate, no-API-break change if we need to ship targeting an
older MC version as fallback.

---

## Decision 10 — Determinism strategy (FR-017)

**Decision**:

- Use **`BTreeMap`/`BTreeSet`** wherever ordered iteration matters
  downstream; never iterate `HashMap` directly for output construction.
- Single project-wide RNG: `rand_chacha::ChaCha8Rng` seeded with a
  compile-time default seed (e.g., `0xdeadbeef`), overridable via
  `--seed`. ChaCha is reproducible across platforms; the OS default
  `ThreadRng` is not.
- Block-state palette indices: assigned in first-encountered order during
  a deterministic preorder walk of placed cells.
- File timestamps: do **not** embed wall-clock time anywhere in the NBT
  (Litematica's `Metadata.TimeCreated` field MUST be either zeroed or set
  from a deterministic input — we'll use `0` and document it in
  `contracts/litematic-nbt.md`).

---

## Decision 11 — Benchmarks

**Decision**: One `criterion` benchmark suite in each library crate:

- `rb-parser`: parse a synthetic 10k-line HDL file.
- `rb-synthesis`: synthesize + place + route a 1k-gate reference design
  (SC-006 perf target).
- `rb-nbt`: serialize + gzip a 256³ schematic.

Run on PRs with `cargo bench --no-run` (Constitution Dev Workflow); full
runs on a nightly schedule (out of scope for v1 PR gate).
