# RedstoneBuilder v3 — Compose-from-stubs Architecture

**Status**: in design (2026-05-17)
**Supersedes**: v1 (procedural P&R) and v2 (analog + scale add-ons)

## Why a new architecture

v1/v2 fought MC redstone physics procedurally: invent every macrocell,
prove its block-update validity, route wires with abstract A* on a
3D grid. After ~6 iterations on physical correctness we still produced
visually broken schematics for anything past 5 gates.

The proven approach (redhdl, MinecraftHDL, FabricHDL) is
**compose-from-stubs**: hand-built `.litematic` mini-schematics for
each primitive, a synthesizer that assembles user designs from those
stubs, a router that bus-pads between stub I/O pins. Physics is
guaranteed by the stubs; the tool only does the *composition*.

v3 ports that architecture into our Rust stack so we get:
- Type-safety and ~10× speedups over the alpha Python redhdl
- Correct MC 26.1 NBT (`Version=7`, `SubVersion=1`, `DataVersion=4786`)
- Our existing `place_simulated_annealing` and `route_pathfinder`
- A real path to SHA-256-scale designs

## High-level pipeline

```
.hdl / .sv source
        │
        ▼
┌─────────────────┐       (rb-parser, extended)
│  HDL frontend   │ ──── + iverilog -tvhdl for .sv path
└─────────────────┘
        │  AST (variadic gates, FFs, parametric widths)
        ▼
┌─────────────────┐       (new: rb-synth/tech_map.rs)
│  Tech mapper    │
│  - N-input OR → dust junction
│  - N-input AND → deMorgan via NOR
│  - XOR → comparator pair
│  - DFF, SR latch → stub instance
└─────────────────┘
        │  Lowered netlist of stub instances
        ▼
┌─────────────────┐       (new: rb-stubs crate)
│  Stub loader    │ ──── reads .litematic/.schem from stub_lib/
└─────────────────┘
        │  Each instance = SchematicInstance { bbox, ports[], blocks }
        ▼
┌─────────────────┐       (existing: place_simulated_annealing)
│  SA placement   │
└─────────────────┘
        │  InstancePlacement (world pos + rotation per instance)
        ▼
┌─────────────────┐       (existing: route_pathfinder, extended)
│  Bussing router │ ──── PathFinder with negotiated congestion
│                 │       + stub-aware padding
│                 │       + repeater insertion every 15 cells
└─────────────────┘
        │  Routed bus paths
        ▼
┌─────────────────┐       (existing: rb-nbt::write_litematic)
│  NBT writer     │ ──── .litematic w/ MC 26.1 schema v7
└─────────────────┘
        │
        ▼
   output.litematic  (load in Litematica 0.27.x)
```

## HDL surface (extended `.hdl`)

Backward-compatible with v1/v2 grammar. New features:

```verilog
// Variadic gates: explicit named ports OR positional inputs.
and  g_and8(.A(a0), .B(a1), .C(a2), .D(a3),
            .E(a4), .F(a5), .G(a6), .H(a7), .Y(q));
or   g_or16(.IN({i15,i14,...,i0}), .Y(q));   // braced concat

// Parametric widths.
wire [7:0] data, addr;
wire [15:0] wide;

// Flip-flops.
dff #(.WIDTH(8)) reg8 (.D(d), .CLK(clk), .Q(q));
sr_latch ff (.S(s), .R(r), .Q(q));

// Module instantiation (composable).
module shift_reg_8 #(parameter int WIDTH = 8) (
    input  [WIDTH-1:0] din,
    input              clk,
    output [WIDTH-1:0] dout
);
   wire [WIDTH-1:0] stages [0:WIDTH];
   assign stages[0] = din;
   genvar i;
   generate for (i = 0; i < WIDTH; i = i + 1) begin
       dff cell (.D(stages[i]), .CLK(clk), .Q(stages[i+1]));
   end endgenerate
   assign dout = stages[WIDTH];
endmodule
```

For SV input, run iverilog and consume its JSON/VHDL netlist
(reuse redhdl's `vhdl_netlist.py` logic ported to Rust).

## Tech mapper — the key optimization

For each gate kind in the netlist, pick the BEST physical realization
from the stub library:

| Logical gate | Naive (n-input cascade) | Optimized | Rule |
|---|---|---|---|
| `or(a,b)` | OR2 stub | OR2 stub | (trivial) |
| `or(a₀..aₙ₋₁)` | n−1 × OR2 (tree) | **OR_n** stub (single dust junction) | OR associativity → dust merge |
| `and(a,b)` | AND2 stub | AND2 stub | (trivial) |
| `and(a₀..aₙ₋₁)` | n−1 × AND2 | NOT_n+1 + OR_n (deMorgan via NOR) | `AND = NOT(OR(NOT…))` |
| `nand` | AND2 + NOT | NAND2 stub if available | direct stub |
| `nor` | OR_n + NOT | NOR_n stub if available | direct stub |
| `xor(a,b)` | composite | XOR_comparator stub | redhdl's `xor.schem` works |
| `dff` | composite from NANDs | DFF stub | hand-built master-slave |

The tech-mapper is a function `Module → Module` that runs BEFORE
`build_netlist`. Mirrors what we already do for v2's `lower_xor_gates`.

## Stub library format

A "stub" is a `.litematic` (or `.schem` for redhdl interop) whose:

1. **Bounding box** is delimited by 2 glass blocks at opposing corners
   (any of the 4 main diagonals, any glass variant).
2. **Name sign** sits adjacent to one of the glass corners; first
   line = module name (`and_h8b`, `dff`, `or_8to1`).
3. **Port-pin signs** elsewhere have first line
   `input <name>[<index>]` or `output <name>[<index>]`. Index optional
   for 1-bit ports.
4. **Core circuit** lives between the glass corners; the loader strips
   the glass corners and signs at write time.

Already-proven by redhdl, we just port the loader. Layout:

```
project/
├── stub_lib/
│   ├── core/          (universal primitives — required for any compile)
│   │   ├── not_h8b.litematic
│   │   ├── and2.litematic
│   │   ├── or2.litematic
│   │   ├── xor.litematic
│   │   ├── dff.litematic
│   │   └── target_bridge.litematic   (wire crossing)
│   ├── opt/           (optimised variants — used by tech-mapper)
│   │   ├── or_4to1.litematic, or_8to1.litematic, ...
│   │   ├── and_8to1.litematic
│   │   ├── adder_8b.litematic        (Kan-CC, redhdl's adder.schem)
│   │   └── ...
│   └── community/     (drop user / Abfielder schematics here)
└── stubs.toml         (metadata: kind, pin name aliases, tick delay)
```

## Implementation plan (phased)

### Phase 1 — Foundation (this session start)

- Create `crates/rb-stubs` crate.
- Port `redhdl/netlist/schematic_instance.py::glass_corner_positions`
  and `schematic_instance_from_schem` to Rust.
- Import the 7 working redhdl stubs into `stub_lib/core/`.
- Define `stubs.toml` schema.

### Phase 2 — HDL extensions

- Extend `rb-parser/src/hdl.pest`:
  - Allow `>2` ports per connection list on `and`/`or`/`nor`/`nand`/`xor`/`xnor`.
  - Parametric widths: `[N-1:0]` ranges.
  - `genvar` / `generate for` loops (basic form).
  - `dff`, `sr_latch`, `latch` as primitive kinds.
- New tests covering each.

### Phase 3 — Tech mapper

- New module `crates/rb-synthesis/src/tech_map.rs`.
- Functions: `lower_or_n`, `lower_and_n_demorgan`, `lower_xor_chain`,
  `lower_ff_to_stub`.
- Run BEFORE `build_netlist` in the pipeline.

### Phase 4 — Bussing integration

- Wrap `route_pathfinder` with stub-aware padding (`xz_padded(1)`
  like redhdl).
- Use `CostMap::new(bounds_from_placement)`.
- Repeater insertion every 15 cells (already implemented).

### Phase 5 — End-to-end tests

Each milestone produces a verified `.litematic`:
1. `examples/v3_adder_8b.hdl` → 8-bit binary adder
2. `examples/v3_shift_reg_8.hdl` → 8-bit shift register
3. `examples/v3_alu_4b.hdl` → 4-bit ALU (add/sub/and/or/not)
4. `examples/v3_cpu_8b.hdl` → minimal RISC CPU

### Phase 6 — Polish

- SV ingress via iverilog (`crates/rb-sv-frontend`).
- CLI flag `--stub-dir`, `--router {lee,pathfinder,redhdl}`.
- Docs + tutorial.

## Out of scope (v3)

- Auto-generation of new stubs from rules (we tried in v2, didn't
  converge). User-built or community-sourced stubs only.
- ASIC-style optimization (Yosys integration, K-map minimization).
  Phase ≥6 if ever.
- Mojang DataFixer reverse-engineering for older `.schem`s. Use
  Litematica's import-and-resave for now.

## Risks

- **Stub quality**: still our biggest physical risk. Mitigated by
  starting with redhdl's proven stubs.
- **Router scaling**: PathFinder hasn't been stress-tested past
  1000 gates. SHA-256 may need parallel router or per-region search.
  Phase ≥5 work.
- **HDL ambiguity**: variadic gate syntax not standard Verilog —
  documenting it carefully matters.
