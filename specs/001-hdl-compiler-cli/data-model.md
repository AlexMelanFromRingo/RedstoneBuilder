# Phase 1 Data Model — HDL → Minecraft Redstone Schematic Compiler

**Feature**: 001-hdl-compiler-cli
**Date**: 2026-05-17

Pipeline data flow (per Constitution Principle II):

```text
HDL source ──► AST ──► Netlist (post-synthesis) ──► Placement ──► Routed Layout ──► Litematica NBT
   text         tree        graph + cell library      grid + cells     grid + routes      file bytes
```

Each arrow is a typed function. Stages are invocable independently for
testing (`--dump-ast`, `--dump-netlist`, `--dump-placement`).

---

## Shared types (`rb-core` crate)

```rust
pub struct SourceSpan { pub file: Arc<Path>, pub byte_start: u32, pub byte_end: u32, pub line: u32, pub column: u32 }

pub struct Pos3 { pub x: i32, pub y: i32, pub z: i32 }   // Minecraft coords

pub struct Bbox3 { pub min: Pos3, pub max: Pos3 }        // inclusive

pub enum Direction { North, South, East, West, Up, Down }

pub enum GateKind { And, Or, Not, Xor, DTrigger, MemoryCell }
```

---

## Stage 1 — AST (`rb-parser` output)

| Entity        | Fields                                                                                   | Notes / validation |
|---------------|------------------------------------------------------------------------------------------|--------------------|
| `Module`      | `name: Ident`, `ports: Vec<Port>`, `wires: Vec<WireDecl>`, `instances: Vec<GateInst>`, `span: SourceSpan` | Exactly one top-level module per file in v1. |
| `Port`        | `name: Ident`, `dir: PortDir { Input, Output }`, `span: SourceSpan`                       | Unique name across module's port + wire namespace. |
| `WireDecl`    | `name: Ident`, `span: SourceSpan`                                                         | Unique name. |
| `GateInst`    | `name: Ident`, `kind: GateKind`, `connections: Vec<Connection>`, `span: SourceSpan`        | Instance name unique. Connection arity must match `kind`. |
| `Connection`  | `port_name: Ident`, `net_ref: Ident`, `span: SourceSpan`                                  | `net_ref` must resolve to a declared port or wire. |
| `Ident`       | `text: SmolStr`, `span: SourceSpan`                                                       | UTF-8, matches `[A-Za-z_][A-Za-z0-9_]*`. |

Validation rules (Phase 1.5 — semantic check in `rb-parser`):

- All `net_ref`s resolve to a declared `Port` or `WireDecl`. (FR-004)
- Every `OUTPUT` port appears as the **driven** side of exactly one
  connection. (FR-004)
- No identifier collision between any `Port` and any `WireDecl`. (Edge case.)
- `GateInst.connections` arity is correct for `kind`:
  - `And`/`Or`/`Xor`: ≥ 2 data inputs + 1 output
  - `Not`: 1 input + 1 output
  - `DTrigger`: `D`, `CLK` (inputs), `Q` (output)
  - `MemoryCell`: `DATA`, `WRITE` (inputs), `Q` (output)

---

## Stage 2 — Netlist (`rb-synthesis` input)

The netlist is a `petgraph::StableGraph<NetlistNode, NetlistEdge>`. Nodes
are gate instances and external ports; edges are wires (one per `Net`).

```rust
pub enum NetlistNode {
    Input  { port_name: Ident, span: SourceSpan },
    Output { port_name: Ident, span: SourceSpan },
    Gate   { inst_name: Ident, kind: GateKind, span: SourceSpan },
}

pub struct NetlistEdge { pub net_id: NetId, pub from_port: PortRole, pub to_port: PortRole }

pub enum PortRole { In(u8), Out, ClockIn, DataIn, WriteEnable, Q }  // role on the *gate* endpoint

pub struct Net { pub id: NetId, pub name: Ident, pub driver: NodeIndex, pub loads: Vec<NodeIndex> }
```

**Cycle handling** (FR-005):

- Build a *combinational projection* of the graph: for every `Gate` node
  whose `kind` is stateful (`DTrigger`, `MemoryCell`), **delete** the
  incoming data-input edges (cut the cycle through the latch).
- Run Tarjan SCC on the projection.
- Any SCC with > 1 node, or any self-loop, is an illegal combinational
  cycle → diagnostic listing the wires in that SCC.
- The full (non-projected) netlist is kept for routing.

---

## Stage 3 — Cell library (`rb-synthesis` internal)

Each `GateKind` maps to one **macro-cell**: a fixed 3D block pattern with
named input/output anchor points and a tick-delay annotation.

```rust
pub struct Macrocell {
    pub kind: GateKind,
    pub bbox: Bbox3,                  // local-coord bounding box
    pub blocks: Vec<(Pos3, BlockId)>, // local-coord placements
    pub inputs: Vec<Anchor>,          // local coords + accepted approach direction
    pub outputs: Vec<Anchor>,
    pub tick_delay: u8,               // redstone ticks from input change to output settle
}

pub struct Anchor { pub role: PortRole, pub pos: Pos3, pub approach: Direction }
```

Cell library entries (target: vanilla MC 26.1 redstone):

| Cell             | Pattern (sketch)                                                  | Tick delay |
|------------------|--------------------------------------------------------------------|-----------|
| `Not`            | torch on side of solid block                                      | 1         |
| `And`            | two-input AND: pair of `Not`s feeding a `Nor` (torch-tower)        | 3         |
| `Or`             | dust merge → repeater (signal-strength threshold)                 | 1         |
| `Xor`            | classic redstone XOR (torch + repeater lattice)                   | 3         |
| `DTrigger`       | edge-triggered D flip-flop (latch + clock edge detector)          | 4         |
| `MemoryCell`     | RS latch driven by `WRITE`-gated `DATA`                           | 2         |

Block patterns enumerated in `contracts/minecraft-blocks.md`.

---

## Stage 4 — Placement (`rb-synthesis` output, route input)

```rust
pub struct Placement {
    pub bounds: Bbox3,                  // active footprint (may grow on retry)
    pub cells: Vec<PlacedCell>,
}

pub struct PlacedCell {
    pub instance: NodeIndex,            // back-ref into Netlist
    pub origin: Pos3,                   // world position of cell's local origin
    pub rotation: Rotation { R0, R90, R180, R270 },
    pub footprint: Bbox3,               // world-coord bbox (rotation-applied)
}

pub struct Rotation;  // limited to Y-axis quarter turns for v1
```

**Placement order** (deterministic):

1. Topological sort the netlist's combinational projection
   (`petgraph::algo::toposort`); for stateful nodes, both `D`-input and
   `Q`-output sides go into the order.
2. Group nodes by topological depth.
3. Lay rows along +X, depth groups stack along +Z, with a vertical gap
   sized for the routed wires of each depth's fan-out.
4. If the resulting footprint > `--max-footprint`, fail with the
   "design too large" diagnostic (FR-016).

---

## Stage 5 — Routed layout (`rb-synthesis` final output)

```rust
pub struct RoutedLayout {
    pub placement: Placement,
    pub routes: Vec<RoutedNet>,
}

pub struct RoutedNet {
    pub net_id: NetId,
    pub source_anchor: (NodeIndex, PortRole),
    pub sink_anchors: Vec<(NodeIndex, PortRole)>,
    pub path: Vec<RouteSegment>,        // ordered, source-first
}

pub enum RouteSegment {
    Dust { pos: Pos3 },                 // redstone dust on the solid block at pos.y-1
    Repeater { pos: Pos3, facing: Direction },
    LevelChange { from: Pos3, to: Pos3, // 1-block step up/down via dust-on-stair pattern
                  stair_block: BlockId }, // see Minecraft-blocks contract
}
```

**Routing rules**:

- Each `Dust` cell claims an **obstruction footprint**: its own position
  plus the 4 lateral neighbors at the same Y AND the 4 lateral neighbors
  at Y±1 (the redstone-adjacency graph in vanilla MC).
- A second net cannot place `Dust` in any obstructed cell unless it
  belongs to the same `net_id`.
- Net order = ascending `net_id` (assigned in source order during netlist
  construction).
- Repeater inserted automatically when a straight dust run reaches 15
  consecutive blocks (FR-009).
- If routing fails after `MAX_BBOX_RETRIES` bounding-box expansions →
  hard-fail with unrouted-net list (FR-015).

---

## Stage 6 — Output (`rb-nbt` input)

The `RoutedLayout` is converted to a flat 3D block grid keyed by world
position, then packed into Litematica's NBT structure.

```rust
pub struct BlockGrid {
    pub bounds: Bbox3,
    pub blocks: HashMap<Pos3, BlockState>,   // populated cells only (sparse)
}

pub struct BlockState { pub name: BlockName, pub properties: BTreeMap<String, String> }
// e.g., BlockName("minecraft:repeater"), properties = { "facing": "north", "delay": "1", "locked": "false", "powered": "false" }
```

The `rb-nbt` crate consumes `BlockGrid` and produces a `.litematic` file
via `fastnbt::to_writer` + `flate2::GzEncoder`. See
`contracts/litematic-nbt.md` for the exact tag tree.

---

## Determinism guarantees (per stage)

| Stage | Determinism mechanism |
|-------|------------------------|
| Parse | Pure function of input bytes. |
| Netlist build | Source-order iteration of AST; net IDs assigned in source order; `StableGraph` preserves insertion order. |
| Cycle detection | Tarjan SCC on the projection — deterministic on stable graph. |
| Placement | Toposort with stable tie-break (by net ID); rotation chosen by fixed rule per fan-out direction. |
| Routing | BFS over a totally-ordered priority queue (tie-break by Manhattan distance, then by `(y, z, x)` lex). No RNG in the default path. |
| Palette indexing | First-encounter order during preorder walk of placed cells. |
| NBT writing | Sorted keys in compound tags; `BTreeMap` for properties. |
| Gzip | `flate2` with fixed compression level (`Compression::default()`). |

All RNG, if used (e.g., future SA placement), seeded from
`ChaCha8Rng::seed_from_u64(seed)` where `seed` is the CLI `--seed` value
or the compile-time default constant.
