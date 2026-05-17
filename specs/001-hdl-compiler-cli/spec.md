# Feature Specification: HDL → Minecraft Redstone Schematic Compiler (CLI)

**Feature Branch**: `001-hdl-compiler-cli`

**Created**: 2026-05-17

**Status**: Draft

**Input**: User description: "Разработать CLI-инструмент на Rust для трансляции
текстового описания цифровых схем в играбельные механизмы Minecraft.
Основные требования: вход — текстовый файл (кастомный HDL: INPUT, OUTPUT, WIRE,
AND/OR/NOT/XOR, D-Trigger, Memory Cells); синтез логики из красной пыли,
факелов, повторителей и компараторов; учёт падения сигнала (15 блоков) и
задержек в redstone-тиках; place & route в 3D с маршрутизацией без замыканий
соседних проводов; выход — валидный Sponge `.schem` или Litematica
`.litematic` (NBT)."

## Clarifications

### Session 2026-05-17

- Q: Output format for v1 — Sponge `.schem`, Litematica `.litematic`, or both? → A: **Litematica `.litematic` only** (Sponge `.schem` deferred to a later release).
- Q: Target Minecraft Java Edition version for v1? → A: **Java Edition 26.1 "Tiny Takeover"** (released 2026-03-24; first fully unobfuscated release under the new `year.drop.hotfix` versioning scheme). Chosen for easier study of game source. Requires confirmed Litematica-mod compatibility under 26.1 (see Assumptions).
- Q: HDL surface syntax — Verilog-subset, VHDL-subset, or original? → A: **Verilog subset** (module / input / output / wire, gate instantiations, `always @(posedge clk)` for sequential primitives).
- Q: How to distinguish legal sequential feedback from illegal combinational cycles? → A: A cycle is legal iff it crosses at least one stateful primitive (`D-Trigger` or `Memory Cell`). Purely combinational cycles are rejected with the cycle's wires named.
- Q: What does the router do when it cannot find a valid layout? → A: It auto-expands the bounding box and retries up to a fixed cap, then hard-fails with a diagnostic listing the unrouted nets.
- Q: Is there an upper bound on the schematic footprint? → A: Configurable via `--max-footprint W×H×D` CLI flag; default 16×16 chunks horizontally (256×256 blocks), full Minecraft build height vertically. Exceeding the bound is a compile error.
- Q: Must the output be deterministic for the same input? → A: Yes — same input + same flags + same tool version → bit-identical `.litematic`. Randomized P&R uses a fixed default seed; `--seed N` overrides.
- Q: Should the CLI expose intermediate-stage dumps? → A: Yes — `--dump-ast`, `--dump-netlist`, `--dump-placement` MUST be supported, each emitting a structured (text or JSON) dump of that stage to stdout or to a file path passed to the flag.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Compile a basic combinational circuit (Priority: P1) 🎯 MVP

A Minecraft redstone hobbyist writes a short HDL file describing a few
combinational gates (e.g., a 2-input AND, an XOR, or a half-adder), runs the
CLI tool, drops the produced schematic into their Minecraft world, flips the
input levers, and observes the outputs behaving exactly as the HDL specified.

**Why this priority**: This is the foundational value of the product — proving
that a textual description compiles into a working in-game mechanism. Without
this loop, nothing else matters.

**Independent Test**: Author a half-adder in the HDL. Run the CLI to produce a
schematic. Import it into a vanilla Minecraft world. Toggle each input
combination and verify the sum/carry outputs match the truth table.

**Acceptance Scenarios**:

1. **Given** a valid HDL file describing a 2-input AND gate, **When** the user
   runs the CLI with an output path, **Then** a schematic file is written and
   loading it into Minecraft produces a working AND gate.
2. **Given** an HDL file with a syntax error on line 7, **When** the user runs
   the CLI, **Then** the tool exits with a non-zero status and prints an error
   pointing to file, line 7, column, and the offending token.
3. **Given** an HDL file referencing an undeclared wire, **When** the user runs
   the CLI, **Then** the tool reports the missing declaration with the
   reference site and exits without producing a schematic.

---

### User Story 2 — Compile sequential circuits with timing (Priority: P2)

A user designs a sequential circuit — e.g., a clocked D flip-flop or a small
memory cell — and expects the generated schematic to preserve correct timing
behavior, with repeater delays and tick counts accounted for so the circuit
works at gameplay speeds.

**Why this priority**: Sequential elements (D-trigger, memory cells) are
mandatory product surface. They require timing-aware synthesis and are a
gating capability for any "real" circuit beyond pure combinational logic.

**Independent Test**: Author a single D-trigger with `D`, `CLK`, `Q` ports.
Generate the schematic, import it, drive `D` and pulse `CLK`. Confirm `Q`
latches `D`'s value on the configured clock edge and holds it until the next
clock pulse.

**Acceptance Scenarios**:

1. **Given** an HDL file describing a D-trigger, **When** compiled and loaded
   in Minecraft, **Then** `Q` captures the value of `D` on a `CLK` pulse and
   retains it between pulses.
2. **Given** an HDL file describing a 1-bit memory cell with `WRITE` and
   `DATA` inputs, **When** compiled and loaded, **Then** the cell stores the
   most recent `DATA` value sampled while `WRITE` was high.

---

### User Story 3 — Compile a large multi-gate design (Priority: P3)

A power user submits a netlist on the order of hundreds to thousands of gates
(e.g., a small ALU or a toy CPU). The tool produces a schematic compact
enough to fit within a reasonable in-game footprint, with wires routed
without short-circuits between neighboring lines, and the user can load it
into Minecraft without crashing the import tool.

**Why this priority**: Demonstrates scale and validates the place-and-route
quality. Lower priority than US1/US2 because users can get value from smaller
designs first; this is the "wow" feature.

**Independent Test**: Author an N-bit ripple-carry adder (e.g., 8-bit) in
HDL. Compile. Import into Minecraft. Apply input pairs and verify the sum
matches arithmetic addition for several test vectors.

**Acceptance Scenarios**:

1. **Given** a netlist of at least 500 gates, **When** compiled, **Then** the
   tool produces a schematic in which no two adjacent redstone-dust lines are
   electrically connected unless explicitly wired together in the HDL.
2. **Given** a netlist where some signal paths exceed 15 blocks, **When**
   compiled, **Then** the tool inserts repeaters along those paths so signal
   strength is maintained end-to-end.

---

### Edge Cases

- An HDL file declares an `OUTPUT` that is never driven by any gate or wire →
  the compiler reports the undriven output and aborts.
- The HDL contains a purely combinational cycle (a wire feeds back into its
  own driver without crossing a `D-Trigger` or `Memory Cell`) → the compiler
  reports the cycle, lists the offending nets, and aborts.
- The HDL contains a feedback path that crosses at least one stateful
  primitive (`D-Trigger` or `Memory Cell`) → this is legal sequential
  feedback (e.g., counters, accumulators) and MUST compile successfully.
- The router exhausts its bounding-box auto-expansion retries without
  finding a non-shorting layout → hard-fail with a diagnostic listing the
  unrouted nets and the final attempted bounding box.
- The compiled design's footprint would exceed the active `--max-footprint`
  bound (default 16×16 chunks horizontally, full build height vertically) →
  the tool reports the would-be footprint, the active bound, and exits with
  a "design too large" error.
- A signal path requires more than 15 blocks of straight redstone dust → the
  router inserts repeaters automatically; the schematic remains correct.
- Two parallel wires in the routed layout would short-circuit (adjacent
  redstone dust) → the router separates them (re-route, elevate, or use a
  signal-isolation pattern) so they remain electrically independent.
- The HDL references a gate type the tool does not yet support → the parser
  reports the unsupported construct with the source location and aborts.
- The user passes an output path with an extension other than `.litematic` →
  the tool reports the unsupported format and exits.
- The input file is empty or contains no top-level module → the parser
  reports the missing module and exits.
- Identifier collisions between an `INPUT`, `OUTPUT`, and `WIRE` → the
  parser reports the duplicate declaration with both source locations.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The tool MUST accept an HDL source file path and an output
  schematic file path as command-line arguments.
- **FR-002**: The HDL parser MUST accept a **Verilog-subset** surface syntax
  consisting of: a single top-level `module` with named `input`/`output`/`wire`
  declarations; instantiations of the gate primitives `and`, `or`, `not`, `xor`;
  and `always @(posedge <clk>)` blocks for the sequential primitives
  `D-Trigger` (D flip-flop) and `Memory Cell`. The accepted grammar MUST be
  written down in a grammar document distributed with the tool. Constructs
  outside this subset (e.g., `parameter`, `generate`, multi-bit busses,
  arithmetic operators) are explicitly out of scope for v1 and MUST be
  rejected with a clear "unsupported construct" diagnostic.
- **FR-003**: On any parse, type, or semantic error, the tool MUST report
  the file path, line number, column, the offending source span, and a
  human-readable explanation; the tool MUST exit with a non-zero status and
  MUST NOT produce a partial schematic.
- **FR-004**: The tool MUST validate that every `OUTPUT` is driven by
  exactly one signal source and that every referenced wire/port is declared.
- **FR-005**: The tool MUST detect purely combinational feedback cycles
  (cycles in the signal graph that do NOT cross at least one stateful
  primitive — `D-Trigger` or `Memory Cell`) and reject the design with a
  diagnostic naming the cycle's wires. Feedback paths that cross at least
  one stateful primitive MUST be treated as legal sequential feedback and
  MUST NOT be reported as cycles. Stateful primitives' data inputs act as
  cycle "cuts": the synthesis stage walks the netlist by treating each
  stateful primitive's data inputs as sinks and its outputs as sources,
  reducing the active graph to a DAG.
- **FR-006**: The logic synthesizer MUST translate each supported HDL
  primitive into a concrete redstone implementation built from the in-game
  primitives — redstone dust, redstone torches, repeaters, and comparators —
  such that the in-game behavior matches the HDL's logical semantics.
- **FR-007**: The synthesizer MUST account for redstone-tick delays so that
  sequential elements (D-trigger, memory cell) latch correctly under the
  expected clock conditions.
- **FR-008**: The router MUST place gates in a 3D layout and connect them
  with redstone-dust paths such that no two adjacent dust lines short
  together unless they belong to the same net.
- **FR-009**: The router MUST insert a repeater on any dust path before it
  exceeds the 15-block signal-strength limit, so end-to-end signal strength
  is preserved across the entire schematic.
- **FR-010**: The router MUST emit a layout that is compatible with
  Minecraft chunk boundaries (the schematic loads and operates correctly
  even when its footprint spans multiple chunks).
- **FR-011**: The tool MUST write the output as a Litematica `.litematic`
  binary file. Sponge `.schem` output is explicitly out of scope for v1 (it
  may be added in a future release).
- **FR-012**: The generated `.litematic` file MUST be byte-valid for the
  Litematica format and MUST be loadable without errors by the Litematica
  Minecraft mod.
- **FR-013**: The CLI MUST report success on stdout with the output path and
  a short summary (e.g., gate count, footprint, build time); errors MUST go
  to stderr.
- **FR-014**: The tool MUST target **Minecraft Java Edition 26.1
  ("Tiny Takeover", released 2026-03-24)** as the single supported game
  version for v1, using its block-state identifiers and vanilla redstone
  behavior. Multi-version support is out of scope for v1. Choice rationale:
  26.1 is the first fully unobfuscated Java Edition release, easing source
  inspection and cross-referencing during development. The tool's promise
  to produce a `.litematic` loadable by the Litematica mod is conditional
  on a Litematica release supporting MC 26.1 being available; if no such
  Litematica release exists at integration time, the project MUST either
  (a) pin to a stable Litematica-compatible version and re-open this
  clarification, or (b) document the gap and ship targeting 26.1 anyway
  pending Litematica compatibility.
- **FR-015**: The router MUST attempt to lay out the design within an
  initial bounding box; if it cannot route all nets without short-circuits,
  it MUST auto-expand the bounding box and retry up to a fixed cap
  (default: capped expansion budget configured per tool release). If all
  retries fail, the tool MUST hard-fail with a diagnostic that names the
  unrouted nets and reports the final attempted bounding-box dimensions.
- **FR-016**: The CLI MUST accept a `--max-footprint W×H×D` option that
  bounds the schematic's footprint in blocks (width × height × depth). The
  default value is `256×<MC build-height>×256` (i.e., 16×16 chunks
  horizontally, full vertical build limit). If the synthesizer or router
  determines the design would exceed the active bound, the tool MUST report
  the would-be footprint, the active bound, and exit with a non-zero status.
- **FR-017**: For a given input file, set of CLI flags, and tool version,
  the produced `.litematic` MUST be **bit-identical** across runs and across
  machines. Any randomized algorithm used by synthesis or P&R MUST be seeded
  with a fixed default seed; the CLI MUST expose `--seed N` to override the
  seed for experimentation.
- **FR-018**: The CLI MUST support stage-dump flags for debugging and
  test-fixture generation:
  `--dump-ast[=PATH]`, `--dump-netlist[=PATH]`, `--dump-placement[=PATH]`.
  Each MUST emit a structured (text or JSON) dump of the named pipeline
  stage to the given file path, or to stdout if no path is given. These
  flags do not suppress normal schematic generation unless the user passes
  `--dump-only` (advisory; final flag naming is plan-level).

### Key Entities

- **HDL Module**: A user-authored top-level design. Contains lists of inputs,
  outputs, internal wires, and gate instances.
- **Port (Input / Output)**: A named signal on the boundary of the module.
  Inputs map to in-game levers/buttons; outputs map to observable signals
  (lamps, signal-strength readouts).
- **Wire**: A named internal signal connecting one driver to one or more
  loads.
- **Gate Instance**: An instance of a primitive (`AND`, `OR`, `NOT`, `XOR`,
  `D-Trigger`, `Memory Cell`) with named port connections.
- **Net**: The logical equivalence class of one driver and all loads
  connected to it via wires; one net = one electrical signal in the routed
  layout.
- **Schematic**: The output artifact — a 3D arrangement of Minecraft blocks
  packaged as a Litematica `.litematic` file.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can take a 5-gate combinational HDL design, run the
  CLI, import the result into Minecraft, and verify correct truth-table
  behavior — end to end in under 5 minutes — without manually editing the
  schematic.
- **SC-002**: For all designs up to 100 gates that compile successfully,
  100% of generated `.litematic` files import into the Litematica mod
  (Minecraft Java Edition 26.1) without errors and behave according to
  their HDL semantics on a curated test suite of at least 20 reference
  designs.
- **SC-003**: For at least 95% of designs in the test suite, no manual
  in-game fix-up is needed after import (no broken wires, no unintended
  short-circuits, no missing repeaters).
- **SC-004**: Every diagnostic the compiler emits for invalid input includes
  the file path, line, and column of the problem; at least 90% of test
  users can locate the offending line on the first try from the message
  alone.
- **SC-005**: A reference 8-bit ripple-carry adder design (≈ 50 gates)
  compiles end-to-end in under 10 seconds on a typical developer laptop.
- **SC-006**: A reference design of 1,000 gates compiles in under 2 minutes
  on a typical developer laptop and produces a schematic that loads in
  Minecraft without crashing the import tool.
- **SC-007**: For any input file in the reference test suite, compiling it
  ten times in a row produces ten byte-identical `.litematic` files
  (verified by SHA-256 over the output bytes).
- **SC-008**: For at least 95% of the reference test-suite designs that fit
  inside the default footprint bound, the router succeeds on the first
  attempt (i.e., without invoking bounding-box auto-expansion).

## Post-MVP Epic — Analog Signals & Block-Update Events (out of scope for v1)

This epic is **deferred to a future release** (provisionally Phase 4 of
project execution). Captured here so v1 architecture stays compatible.

**Motivation**: vanilla redstone is not a pure 2-valued logic system. Two
features that v1 does not expose limit the kind of circuits a user can
build:

1. **Comparators** treat redstone signals as analog values 0–15. They
   support `compare` and `subtract` modes and read directly from
   inventories. Without them we cannot model signal-strength arithmetic
   used by many real redstone contraptions (memory cells with strength
   coding, analog adders, item-frame counters).
2. **Observers** are block-update detectors that emit a **1-tick pulse**
   when an adjacent block changes state. They model edge-triggered events
   rather than levels.

**Post-MVP user stories** (priorities indicative):

- **EPIC-US1**: Compile a netlist that uses `comparator` primitives in
  both `compare` and `subtract` modes; the synthesizer emits valid
  redstone comparator placements; the analog semantics in-game match
  what the HDL specified.
- **EPIC-US2**: Compile a netlist that uses an `observer` primitive
  triggered by a block-update event; the synthesizer emits a valid
  observer block in the correct facing.
- **EPIC-US3** (stretch): support multi-bit analog signals (`wire [3:0] s;`
  or similar) so that comparator-based arithmetic can be expressed
  natively rather than as 4 separate 1-bit nets.

**Scope of v1 that this epic must NOT break**:

- The v1 boolean-only HDL grammar MUST remain valid input under the
  Post-MVP grammar (the analog/observer additions are pure extensions).
- The boolean-only synthesis path MUST keep its v1 behavior and
  performance (the analog path is additive, not a replacement).
- A v1 user reading an EPIC-era schematic does not need to relearn the
  CLI: the `--max-footprint`, `--seed`, `--dump-*` flags remain stable.

**Forward-compatibility requirements on the v1 codebase** (binding on
the v1 implementation team — see plan.md §Forward-Compatibility for
the analog/event epic):

- The `GateKind` enum MUST be declared `#[non_exhaustive]` so that the
  future variants `Comparator { mode: CompareMode }` and `Observer`
  can be added without a SemVer-major bump of `rb-core`.
- The signal-type concept MUST exist as a `SignalKind` enum in
  `rb-core`, even if v1 only populates the `Boolean` variant. The
  netlist's net data type uses this enum so the path from `Boolean` to
  `AnalogStrength(0..=15)` and `EdgeTrigger` is a variant addition, not
  a refactor.
- The router's signal-strength model in v1 is already 0–15 internally
  (FR-009 repeater insertion uses signal-strength counting); the analog
  epic MUST be able to expose this internal value to user-level wires
  rather than collapsing to boolean.

**This epic is explicitly out of scope** for the v1 functional
requirements (FR-001 through FR-018), success criteria (SC-001 through
SC-008), and user stories (US1, US2, US3). It does not affect the v1
clarifications (Litematica-only, MC 26.1, Verilog subset).

## Assumptions

- Target audience is Minecraft redstone enthusiasts comfortable with text
  files and command-line tools.
- Target game is Minecraft Java Edition 26.1 "Tiny Takeover" (Bedrock has
  different redstone physics and a different schematic ecosystem; out of
  scope for v1).
- Redstone behavior follows vanilla Java Edition 26.1 rules (15-block
  signal decay, standard 1-redstone-tick repeater base delay = 2 game ticks).
  If 26.1 introduces redstone-behavior changes relative to earlier 1.x
  baselines, the synthesizer's primitive layouts MUST follow 26.1 semantics.
- Schematics are loaded with the Litematica mod for MC 26.1; generating a
  valid `.litematic` is the contract — actual in-world placement (printer
  mode or manual build) is the user's responsibility. Litematica is a
  third-party mod with its own release cadence; if no Litematica build
  exists for 26.1 at integration time, see FR-014 for fallback policy.
- HDL surface syntax is a Verilog subset (see FR-002); users familiar with
  Verilog can read and write designs with no domain-specific re-learning.
- The user provides input as UTF-8 plain text.
- Performance reference hardware: a modern multi-core laptop (≥ 4 cores,
  ≥ 16 GB RAM); single-threaded compilation is acceptable for v1 but the
  design must allow parallelization later (see project constitution).
- "No short-circuits between neighboring wires" means: in the routed layout,
  two distinct nets are never placed on adjacent redstone-dust positions
  that the game would electrically merge.
