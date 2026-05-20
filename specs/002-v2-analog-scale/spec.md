# Feature Specification: v2.0 — Analog Signals, Event-Driven Primitives, and 10 000+ Gate Scale

**Feature Branch**: `002-v2-analog-scale`

**Created**: 2026-05-17

**Status**: Draft

**Input**: User description: "Эпик v2.0: масштабирование до 10 000+ гейтов
(уровень алгоритма MD5) и аналоговая/событийная логика. Аналоговый сигнал
с автоматическими повторителями. Новые компоненты: Repeaters (delay 1–4,
диод, locking), Observers (edge detection, 1-tick импульс), Target Blocks
(компактный redirect), полублоки/стекло (vertical-up передача и
wire-crossing). Жёсткий лимит RAM при компиляции."

This spec realises the **Post-MVP Epic** previously scoped in
`specs/001-hdl-compiler-cli/spec.md` (§"Post-MVP Epic — Analog Signals
& Block-Update Events"). It is **additive on top of v1**: every valid
v1 HDL file MUST still compile under v2 with identical (or improved)
output.

## Clarifications

### Session 2026-05-17

- Q: How does the user declare an analog (signal-strength 0–15) wire in the HDL? → A: **New `analog wire <name>;` type keyword.** Signal strength is its own scalar type, not a bit-vector — no `&`/`|` over analog wires; comparator is the only way to combine them. The parser adds one optional `analog` modifier to the existing wire declaration rule. Verilog-style multi-bit busses remain out of scope.
- Q: What happens when the compiler's peak memory would exceed `--max-ram`? → A: **Hard abort** with a structured diagnostic naming the cap, the stage, and the actual memory at the moment of abort, plus a distinct documented exit code. No best-effort degrade, no warning-and-continue — predictable UX, user re-runs with a higher cap if they want.
- Q: How is the `observer` primitive's edge-trigger semantics modelled across the pipeline? → A: **Asynchronous, MC-native.** The observer's behaviour is exactly what vanilla MC does — fire a 1-tick pulse whenever the watched signal changes, no compiler-generated common clock. The cycle detector treats the observer's output as a state element (its data input is a cycle-cut, same projection as D-trigger / Memory Cell).
- Q: What happens when the iterative router (Negotiated Congestion + Rip-up & Reroute) cannot converge within its iteration budget? → A: **Hard abort, symmetric to FR-V10 RAM-cap behaviour.** A new CLI flag `--max-routing-iterations N` caps the iteration count (default chosen to fit the 10 000-gate workload). On exceeding the cap, the compiler exits with a distinct documented exit code and a structured diagnostic naming the unrouted nets, the iteration count reached, and the peak congestion seen. No best-effort partial output, no auto-fallback to a slower router.
- Q: Does the v2 pipeline support iterative placement↔routing feedback (placer re-spreads dense regions when routing fails), or is it strict single-pass placement → routing? → A: **Strict single-pass.** Placement is computed once, then frozen for routing. If routing exhausts its iteration budget (FR-V16), the compiler hard-aborts — the user re-runs with a larger `--max-footprint`, a higher `--max-routing-iterations`, or a simpler input. The v1 `--dump-placement` contract (single deterministic snapshot) carries over unchanged. Iterative P&R feedback is explicitly **deferred to v3** as a future quality-improvement epic.
- Q: Are slab and glass primitives user-instantiable in HDL, or router-only internal tools? → A: **Router-only.** The router uses slab and glass as routing-tool blocks (asymmetric-cost edges in its cost map: dust-on-slab transmits upward at low cost but blocks downward propagation; glass enables wire-crossings without electrical coupling). Users do **not** declare `slab` or `glass` in HDL — they appear only in the output `.litematic`, never on the netlist's signal graph. This keeps the HDL surface focused on logic and centralises routing decisions in one optimiser. Manual override (when needed) goes through editing the `--dump-placement` JSON, not through new HDL keywords.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Build an analog-arithmetic circuit (Priority: P1) 🎯 v2 MVP

A power user writes an HDL design that needs to **add two redstone
signal-strength values** (e.g., two 4-bit analog counters merging into
a sum) using a comparator in subtract mode. The compiler accepts the
new analog primitives, routes them with automatic signal-strength
buffering, and produces a `.litematic` whose in-game behaviour matches
the analog arithmetic described in the source.

**Why this priority**: The single biggest user-facing v1 limitation is
that wires are boolean — every comparator-based design has to be hand-
built in Minecraft. Closing this gap unlocks an entire class of
redstone circuits (analog adders, signal-strength memory, item-frame
counters, music-block sequencers).

**Independent Test**: Author a 4-bit analog adder (two comparator
cascades feeding a comparator in subtract mode). Compile. Load in
Minecraft. Drive both inputs with signal-strength sources of varying
levels. Verify the output reads the arithmetic sum (clamped to 15).

**Acceptance Scenarios**:

1. **Given** an HDL file using the `analog` signal type and `comparator`
   primitive, **When** the user runs the compiler, **Then** the produced
   schematic loads in Minecraft and the in-game signal-strength output
   matches the arithmetic the HDL declared.
2. **Given** an analog wire that would carry a signal across more than
   15 blocks of dust, **When** compiled, **Then** the router places
   automatic signal-strength buffers (repeaters) so the destination
   receives the **same** strength the source produced.
3. **Given** an HDL file that mixes legacy boolean primitives with new
   analog primitives, **When** compiled, **Then** both parts coexist
   correctly (boolean nets remain boolean; analog nets carry strength).

---

### User Story 2 — Use event-driven primitives (Priority: P2)

A user builds an edge-triggered mechanism — e.g., a 1-tick pulse
extractor driven by a button press, or a pulse-counter using a
`comparator`-in-subtract feedback. The HDL exposes `observer`,
`repeater` (with delay/lock), and `target_block` as first-class
primitives. The compiler emits the corresponding redstone blocks in
the correct orientation and lock state.

**Why this priority**: Edge-triggered designs are the backbone of
clocks, debouncers, and pulse-shaping circuits. Without them, v1's
sequential logic is limited to level-triggered latches.

**Independent Test**: Author a monostable circuit (one-shot pulse on
button press) using an `observer` watching a `target_block`. Compile.
Load in Minecraft. Press the button: verify the output drives high
for exactly 1 redstone tick, then returns low — regardless of how long
the button is held.

**Acceptance Scenarios**:

1. **Given** an HDL file with an `observer` primitive watching a
   user-named signal, **When** compiled and loaded, **Then** the
   observer emits a 1-tick pulse when its watched signal changes.
2. **Given** an HDL `repeater r(.IN(a), .OUT(b), .DELAY(3));`,
   **When** compiled and loaded, **Then** the in-game repeater has its
   delay slot set to 3 ticks and propagates `a` to `b` with that delay.
3. **Given** an HDL `repeater` instance with a `LOCK` port,
   **When** compiled and loaded, **Then** the in-game repeater enters
   locked mode when `LOCK` is high (holding its last output) and resumes
   normal operation when `LOCK` falls.
4. **Given** an HDL design that uses `target_block` to redirect a dust
   line, **When** compiled, **Then** the resulting footprint is
   smaller than the same redirect built with a solid block + torch.

---

### User Story 3 — Compile a 10 000-gate design at production scale (Priority: P3)

A user submits a netlist on the order of **10 000+ gates** —
representative of compiling a single round of MD5 (≈ 12 000 gates) or
a small RISC pipeline stage. The compiler completes the build in a
predictable wall-clock budget, never exceeds a configurable RAM cap,
and produces a `.litematic` that imports without crashing.

**Why this priority**: v1's compile-time grew super-linearly with gate
count; designs > 1 k gates were unverified. This story is the
"compiler is real software" gate. Lower priority than US1/US2 because
users can already get value from analog and event primitives at
smaller scale; this story is about reaching a level where the tool
can replace hand-built mega-circuits.

**Independent Test**: Author a 10 000-gate stress-design (e.g., a
ripple-carry adder chained 100× wide, or a generator that emits
arbitrary primitive counts). Compile with the default RAM cap.
Verify: (a) the compile completes within the v2 wall-clock budget;
(b) peak resident memory stays under the configured cap; (c) the
output `.litematic` imports into the Litematica mod without crashing.

**Acceptance Scenarios**:

1. **Given** a synthesised 10 000-gate HDL input, **When** the user
   runs the compiler with default flags, **Then** the compile finishes
   within the v2 wall-clock budget and the output file exists.
2. **Given** a design whose intermediate state would exceed the active
   RAM cap, **When** compiled, **Then** the compiler exits cleanly
   with a "memory cap exceeded" diagnostic naming the cap and the
   stage at which the overrun occurred — it does **not** OOM-kill the
   user's machine.
3. **Given** a 10 000-gate design that fits within the RAM cap,
   **When** compiled and loaded into the Litematica mod, **Then** the
   mod parses and previews the schematic without crashing.

---

### Edge Cases

- An analog wire is driven by **two sources** at different signal
  strengths → the compiler reports the multi-driver error (analog wires
  remain single-driver, same as boolean v1).
- An `observer` watches a signal that changes faster than the observer
  can emit a pulse (e.g., a 1-tick clock) → the observer emits one
  pulse per stable edge; rapid transitions may merge (documented
  vanilla MC behaviour).
- A `repeater` is `locked` by a source that is itself driven by the
  repeater's own output (combinational lock self-loop) → the compiler
  detects this as a stateful-feedback cycle that the cycle projection
  must legalise (it's a real MC pattern) **only if** the lock path
  crosses a state element; otherwise it's a combinational cycle and is
  rejected.
- A signal path requires more than 15 blocks of dust → the router
  inserts an analog-strength-preserving buffer (repeater on default
  delay) so the destination receives full strength. (Generalisation of
  v1 FR-009.)
- A `slab` is placed under dust where the dust would otherwise jump
  upward and short into another net → the slab's transparency lets the
  dust pass through without coupling.
- Two analog wires must cross at the same Y level → the router uses a
  slab/glass-based wire-crossing pattern so the two strengths do not
  mix.
- A design exceeds the RAM cap mid-route → the compiler aborts with a
  diagnostic naming the stage and the actual RAM consumed at the
  moment of abort.
- A v1-era HDL file (boolean-only, no v2 primitives) is compiled under
  v2 → produces a `.litematic` byte-identical to (or strictly better
  than) the v1 output for the same source.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-V01**: The HDL grammar MUST be extended (backward-compatibly)
  with new **logic-primitive** instantiation forms for `comparator`,
  `observer`, `repeater`, and `target_block`. Existing v1 combinational
  and sequential constructs MUST keep their v1 semantics. `slab` and
  `glass` are **not** added to the HDL surface — they are router-only
  routing-tool blocks per FR-V07, never user-declared.
- **FR-V02**: The HDL signal-type system MUST support analog wires
  carrying integer signal strength in the range 0..=15, in addition to
  the boolean wires inherited from v1. The user declares an analog wire
  with the **`analog wire <name>;`** form — `analog` is an optional
  type modifier on the v1 `wire` declaration rule, fully
  backward-compatible with bare `wire <name>;` (which remains boolean).
  Bit-vector / multi-bit-bus syntax (`wire [3:0] s;`) is **not**
  supported in v2; analog strength is a scalar type, not a bit field.
  Bit-operations (`&`, `|`, etc.) over analog wires are a type error
  and MUST be rejected with a clear diagnostic.
- **FR-V03**: The synthesizer MUST translate `comparator` instances
  into in-game `minecraft:comparator` blocks in either `compare` or
  `subtract` mode as specified by the HDL, and reflect the
  analog-arithmetic semantics those blocks carry in vanilla MC.
- **FR-V04**: The synthesizer MUST translate `repeater` instances into
  in-game `minecraft:repeater` blocks with the user-specified delay
  (1–4 ticks). When a `LOCK` port is connected, the synthesizer MUST
  position an adjacent locking source (perpendicular repeater /
  comparator output) so the in-game `locked` property is driven by the
  HDL's lock signal.
- **FR-V05**: The synthesizer MUST translate `observer` instances into
  in-game `minecraft:observer` blocks oriented to watch the connected
  source signal, such that a state change on the watched signal
  produces a single-tick pulse on the observer's output face.
- **FR-V06**: The synthesizer MUST translate `target_block` instances
  into in-game `minecraft:target` blocks and use them in routes where
  they replace bulkier solid-block-plus-torch patterns for dust
  redirection.
- **FR-V07**: The router MUST be able to use slab and glass primitives
  to implement (a) **upward-only signal transmission** when a route
  must rise without enabling downward leakage, and (b) **wire
  crossings** that allow two distinct nets to cross at the same Y
  without electrical coupling. Patterns produced MUST behave correctly
  per vanilla MC Java Edition 26.1 redstone rules. Slab and glass are
  **router-internal** routing-tool blocks: the router decides where
  to place them based on its cost model; users do not instantiate
  them in HDL. They appear in the output `.litematic` but never on
  the netlist's signal graph or in any user-visible IR before
  routing.
- **FR-V08**: For every analog wire, the router MUST insert
  signal-strength buffers (repeaters on default delay) at intervals
  such that the destination receives the **same** signal strength the
  source produced — i.e., no analog signal degrades from source to sink.
  This generalises v1 FR-009 to the analog domain.
- **FR-V09**: The compiler MUST accept a CLI flag `--max-ram MB` (or
  equivalent) that caps the compiler's peak resident memory. The
  default cap MUST be conservative enough to compile a 10 000-gate
  design on a 16 GB-RAM developer laptop without exhausting the
  machine.
- **FR-V10**: When the compiler's actual peak memory would exceed the
  active `--max-ram` cap, the compiler MUST **hard-abort** with a
  structured diagnostic naming (a) the cap, (b) the stage at which the
  overrun occurred (parser / synthesis / placement / routing / NBT),
  and (c) the actual memory used at the moment of abort. The
  diagnostic MUST exit with a distinct, documented exit code (separate
  from the v1 exit categories in `specs/001-hdl-compiler-cli/contracts/cli.md`).
  The compiler MUST NOT crash the user's machine or be killed by the
  OOM killer. There is **no** best-effort degrade or warning-and-continue
  mode in v2 — the cap is a hard contract, and the user re-runs with a
  higher `--max-ram` if they need more headroom.
- **FR-V11**: For HDL inputs of up to 10 000 gates, the compiler MUST
  complete the full pipeline (parse → synth → place → route → write)
  within the v2 wall-clock budget (see Success Criteria) on a typical
  developer laptop.
- **FR-V12**: Edge-triggered behaviour of `observer` primitives MUST
  follow **asynchronous, vanilla MC-native semantics**: the observer
  fires a 1-tick pulse on its output face whenever its watched signal
  changes, regardless of any global clock. There is no
  compiler-generated clock domain for observers. The cycle detector
  MUST treat the observer's data input as a stateful cycle-cut
  (analogous to D-trigger / Memory Cell `D` / `DATA` inputs in v1
  FR-005), so feedback loops that traverse an observer are legal
  sequential designs.
- **FR-V13**: All existing v1 functional requirements (FR-001 through
  FR-018) MUST continue to hold. Specifically: deterministic output
  (FR-017) MUST extend to all v2 primitives; `--max-footprint` and
  `--seed` MUST continue to work; all v1 example HDL files MUST
  continue to compile successfully under v2 with no source edits.
- **FR-V14**: The CLI MUST expose `--stats` (or equivalent) that, on
  every compile, prints peak RAM usage, per-stage wall-clock breakdown,
  and gate / net / block counts. This makes the user-facing RAM cap
  observable.
- **FR-V15**: The compiler MUST detect and reject analog-wire designs
  with multiple drivers on the same net (same single-driver rule as
  v1 boolean wires).
- **FR-V16**: The compiler MUST accept a CLI flag
  `--max-routing-iterations N` capping the number of iterations the
  iterative router (rip-up & reroute / negotiated congestion / similar
  multi-pass scheme) may run. The default cap MUST be tuned so a
  representative 10 000-gate reference design converges well within
  it. On exceeding the cap, the compiler MUST **hard-abort** with a
  structured diagnostic naming (a) the cap, (b) the unrouted nets
  remaining at the moment of abort, and (c) a per-stage routing-effort
  summary (iteration count reached, peak congestion seen). The exit
  code MUST be distinct and documented (separate from the RAM-overflow
  exit code from FR-V10 and from the v1 routing exhaustion exit code).
  No best-effort partial schematic is written, and no auto-fallback to
  a slower router is performed — the user re-runs with a higher cap
  (or a simpler input) if they need more iterations.
- **FR-V17**: The compile pipeline MUST be **strict single-pass**
  through placement and routing: placement is computed once, frozen,
  then handed to routing. There is no placement↔routing feedback loop
  in v2 — if routing exhausts its iteration budget (FR-V16) or the
  placement footprint exceeds the cap (v1 FR-016 carries forward), the
  compiler hard-aborts and the user re-runs with adjusted flags. The
  v1 `--dump-placement` contract (a single deterministic snapshot per
  compile) carries over unchanged. Iterative placement re-spreading
  driven by routing feedback is explicitly **out of scope for v2** and
  is recorded as a v3 epic.
- **FR-V18**: The router's cost model MUST capture the **vertical
  asymmetry** of vanilla MC redstone propagation through transparent
  blocks: dust laid on top of a slab transmits a signal **upward** to
  dust above with low cost, but **does not** transmit the same signal
  downward to dust below. The cost-map edge weights and passability
  rules MUST reflect this asymmetry so the router never produces a
  layout whose in-game behaviour silently differs from the netlist's
  intent. (User-facing impact: certain routing patterns the v1 router
  could never produce — vertical signal towers, layered wire
  crossings — become available without leaking unintended downward
  signals into adjacent nets.)

### Key Entities

The v1 entities (HDL Module, Port, Wire, Gate Instance, Net,
Schematic) all carry over. v2 adds:

- **Signal Type**: The kind of value a wire carries. `Boolean` (v1) or
  `Analog0_15` (v2). User declares the type at wire-declaration time.
- **Analog Primitive**: A gate-like instance whose inputs and/or output
  carry analog signal strength rather than boolean — `comparator` is
  the canonical example.
- **Event Primitive**: A gate-like instance whose output is a
  1-tick pulse — `observer` is the canonical example.
- **Routing Primitive**: A non-logic block that the router uses to
  shape paths — `target_block`, `slab`, `glass`. These do not appear
  on the netlist's signal graph; they are placement / routing tools.
- **Repeater Instance**: A first-class HDL primitive (no longer just an
  internal router artefact). Carries `IN`, `OUT`, optional `LOCK`, and
  a `DELAY` parameter (1–4).
- **Resource Budget**: A run-time accounting of peak memory consumed
  by the compiler, checked against the active `--max-ram` cap at every
  stage boundary.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-V01**: A 4-bit analog adder (two comparator cascades + final
  subtract-mode comparator) compiles end-to-end; the resulting
  `.litematic`, when loaded in Minecraft 26.1, produces an in-game
  output signal whose strength equals the arithmetic sum of the two
  inputs (clamped to 15) for every input combination in a test of
  ≥ 16 combinations.
- **SC-V02**: A monostable circuit using `observer` + `target_block`
  emits an output pulse of measured duration 1 redstone tick (± 0
  ticks) regardless of how long its trigger signal is held high, for
  ≥ 5 distinct hold durations between 1 tick and 30 seconds.
- **SC-V03**: A reference 10 000-gate HDL design compiles end-to-end
  in **under 5 minutes** on a typical developer laptop (≥ 4 cores,
  ≥ 16 GB RAM, NVMe storage).
- **SC-V04**: Peak resident memory during the 10 000-gate compile from
  SC-V03 stays under the default `--max-ram` cap (which itself MUST be
  ≤ 4 GB to leave headroom on a 16 GB laptop).
- **SC-V05**: For ≥ 95% of v1 reference HDL files re-compiled under
  v2, the output `.litematic` is **either** byte-identical to the v1
  output **or** strictly smaller in footprint while preserving
  in-game behaviour (no regressions for v1 users).
- **SC-V06**: For routes that previously needed a solid-block-plus-
  torch redirect pattern, the v2 router using `target_block` reduces
  the route's block count by at least 25 % compared with the v1 router
  on the same netlist.
- **SC-V07**: Two analog wires that must cross at the same Y level
  using the slab/glass wire-crossing pattern carry their independent
  signal strengths to their respective destinations with no
  measurable cross-coupling (output strength on each net within ± 0 of
  the source on a curated test of ≥ 8 crossing configurations).
- **SC-V08**: When a design would exceed the active `--max-ram` cap,
  the compiler exits cleanly within 5 seconds of detecting the
  overrun, with a non-zero exit code and a diagnostic naming the cap,
  the stage, and the actual memory used — **never** killed by the OS
  OOM killer in the v2 test suite.

## Assumptions

- Target game version remains Minecraft Java Edition 26.1 (inherited
  from v1 FR-014).
- Output format remains Litematica `.litematic` only (Sponge `.schem`
  remains out of scope; v1 FR-011 carries forward).
- "Typical developer laptop" for performance targets: ≥ 4 cores,
  ≥ 16 GB RAM, NVMe storage. Same baseline as v1.
- Multi-threading is permitted (and likely required to hit the
  10 000-gate budget) — the v1 design constraint that the architecture
  "MUST allow parallelisation later" (constitution Principle IV) is
  now exercised.
- All v1 examples (`half_adder.hdl`, `dff_demo.hdl`, `full_adder.hdl`,
  `ripple_adder_8bit.hdl`) MUST continue to be in the test suite and
  continue to pass under v2 (regression coverage).
- Determinism (v1 FR-017) is preserved unconditionally: same input +
  same flags + same tool version → byte-identical output. Any
  parallel algorithm introduced for SC-V03 / SC-V04 MUST be
  determinism-preserving (e.g., stable partitioning, ordered reduce).
- The `Comparator` and `Observer` `BlockId` variants are already
  forward-compat-marked in the v1 block catalogue
  (`rb_core::block::BlockId` is `#[non_exhaustive]`); v2 adds the
  synthesiser cells and HDL surface syntax that exercise them.
- "Hard RAM limit" is a user-observable contract enforced at stage
  boundaries (not after every allocation). The cap is approximate —
  the v2 test suite asserts the cap was not violated by more than a
  small fixed slack at each checkpoint.
