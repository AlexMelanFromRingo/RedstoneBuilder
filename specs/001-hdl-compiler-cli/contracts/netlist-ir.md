# Contract — Netlist IR & Synthesis

**Crate**: `rb-synthesis`

Stage owner: takes `Module` from `rb-parser`, produces a typed
`Netlist` (graph) + a `Placement` + a `RoutedLayout` (see data-model.md).

## Netlist graph

```rust
use petgraph::stable_graph::{StableDiGraph, NodeIndex, EdgeIndex};
use rb_core::{GateKind, SourceSpan};
use rb_parser::ast::Ident;

pub type NetlistGraph = StableDiGraph<NetlistNode, NetlistEdge>;

#[derive(Debug, Clone, serde::Serialize)]
pub enum NetlistNode {
    Input  { port: Ident, span: SourceSpan },
    Output { port: Ident, span: SourceSpan },
    Gate   { inst: Ident, kind: GateKind, clock: Option<Ident>, span: SourceSpan },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub struct NetId(pub u32);

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetlistEdge {
    pub net: NetId,
    pub from: EndpointRole,   // role on the *source* node
    pub to:   EndpointRole,   // role on the *sink* node
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum EndpointRole {
    ModuleInput,     // external input port → fans out
    ModuleOutput,    // external output port → driven by exactly one source
    DataIn(u8),      // numbered data input on a gate
    DataOut,         // unique output of combinational gate
    ClockIn,         // CLK input of stateful gate
    DataInStateful,  // D / DATA input of stateful gate
    WriteEnable,     // WRITE input of memory cell
    Q,               // Q output of stateful gate
}
```

Net IDs are assigned **in source order**: every wire declaration and
every module port becomes a `NetId` in declaration order. This is the
core determinism anchor (FR-017).

## Synthesis API

```rust
pub fn build_netlist(module: &rb_parser::ast::Module)
    -> Result<Netlist, SynthError>;

pub fn detect_cycles(netlist: &Netlist)
    -> Result<(), CycleError>;     // FR-005

pub fn place(netlist: &Netlist, cfg: &PlaceConfig)
    -> Result<Placement, PlaceError>;       // FR-016

pub fn route(placement: &Placement, netlist: &Netlist, cfg: &RouteConfig)
    -> Result<RoutedLayout, RouteError>;    // FR-008, FR-009, FR-015

pub struct PlaceConfig {
    pub max_footprint: Footprint,    // from CLI --max-footprint
}

pub struct RouteConfig {
    pub max_bbox_retries: u8,        // default 8
    pub bbox_grow_step:   u32,       // blocks per retry
    pub seed:             u64,       // FR-017
}
```

## Cycle detection algorithm (FR-005)

```text
Input:  NetlistGraph G
Output: Ok(()) | CycleError listing wires in offending SCC

1. Build combinational projection G':
     for each Gate node n with n.kind ∈ {DTrigger, MemoryCell}:
         remove all incoming edges to n whose `to` role is one of
         {DataInStateful, WriteEnable}.
         (Q output remains; gate becomes a "source" of state in the DAG.)
2. Run tarjan_scc on G'.
3. For each SCC s where |s| > 1 OR s has a self-loop:
     collect the net IDs on edges within s
     emit CycleError { wires: [Ident, ...] }
4. If no SCCs found, return Ok(()).
```

Note: this projection is also what placement uses for topological-depth
grouping (data-model.md Stage 4).

## Routing algorithm (FR-008, FR-009, FR-015)

3D Lee's maze router. Per-net BFS over a `Grid3D<CellState>`:

```rust
pub enum CellState {
    Air,
    Solid,                       // cell hosts a non-redstone block (placed cell, dust support)
    Dust { owner: NetId },
    Repeater { owner: NetId, facing: Direction },
    Obstructed { by: NetId },    // adjacency-rule shadow of an owning Dust cell
}
```

**Obstruction propagation** (the key invariant for FR-008):

For every `Dust { owner: n }` at position `p`, mark `Obstructed { by: n }`
at the following positions if currently `Air`:

```text
N4(p) at y     = p.y          // 4 same-level lateral neighbors
N4(p) at y     = p.y + 1      // 4 above (dust climbs slabs/stairs)
N4(p) at y     = p.y - 1      // 4 below (dust climbs down)
```

A second net's BFS may not step into a cell whose state is
`Obstructed { by: m }` when `m != n`. It MAY step into its own
`Obstructed { by: n }` (no self-short).

**Repeater insertion** (FR-009):

The BFS tracks running signal strength along the current path; when it
would reach 0 (i.e., the 15th block of straight dust), the router places
a `Repeater` at that cell, resets strength to 15, and continues.

**Bbox-expansion retry** (FR-015):

```text
budget = MAX_BBOX_RETRIES
loop:
    layout = try_route_all(placement, netlist, current_bbox, seed)
    match layout:
        Ok(l) -> return l
        Err(unrouted) ->
            if budget == 0: return Err(RouteError::Exhausted { unrouted, final_bbox })
            current_bbox.grow_by(BBOX_GROW_STEP)
            budget -= 1
```

## Dump formats

- `--dump-netlist`: JSON of `NetlistGraph` (nodes + edges + net table),
  using `serde_json::to_writer_pretty`.
- `--dump-placement`: JSON of `Placement` (each cell's origin, rotation,
  footprint) + an embedded ASCII XY-layer map for quick visual scan.

## Error types

```rust
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum CycleError {
    #[error("combinational feedback cycle through wires: {wires:?}")]
    #[diagnostic(code(rb_synthesis::cycle),
                 help("insert a D-Trigger or Memory Cell on the feedback path"))]
    Combinational { wires: Vec<String>, sites: Vec<miette::SourceSpan> /* ... */ },
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum PlaceError {
    #[error("design too large: {actual} would exceed --max-footprint {bound}")]
    #[diagnostic(code(rb_synthesis::footprint))]
    TooLarge { actual: String, bound: String },
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum RouteError {
    #[error("routing failed: {} unrouted net(s) after {retries} bbox expansions", unrouted.len())]
    #[diagnostic(code(rb_synthesis::unroutable))]
    Exhausted { unrouted: Vec<String>, retries: u8, final_bbox: String },
}
```
