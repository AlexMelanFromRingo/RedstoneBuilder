# redhdl bug-fix patches

These patches make [andrewsmike/redhdl](https://github.com/andrewsmike/redhdl)
actually compile new SystemVerilog → `.schem` for non-trivial designs.
Without them, every fresh user input crashes somewhere in the pipeline.

## How to apply

```sh
git clone https://github.com/andrewsmike/redhdl.git
cd redhdl
git apply /path/to/RedstoneBuilder/third_party/redhdl_patches/000*.patch
pip install -e .
sudo apt install -y iverilog       # SystemVerilog → VHDL front-end
```

## What each patch does

### 0001 — schematic loader robustness
`redhdl/netlist/schematic_instance.py`, `redhdl/voxel/region.py`

- `glass_corner_positions` now accepts any glass variant
  (`stained_glass`, `tinted_glass`, …), not only `minecraft:glass`.
- Drops the strict "glass MUST sit at AABB extrema" assertion.
  The 2 corner markers may lie on any of the 4 main diagonals of the
  AABB; the loader picks them by proximity to the schematic-name sign.
- Probes all 6 faces of `bottom_right_pos` for the schematic-name
  sign instead of hard-coding `BR - (0,0,1)` (different stubs orient
  it differently — `and_h8b` uses -Z, `adder` uses +X).
- Port-pin sign collection now scans the full schematic (not just
  the region outside the inferred bbox), filtering by the
  `"input "/"output "` prefix.
- `CompositeRegion.min_pos/max_pos` return a non-empty sentinel when
  `subregions` is empty so `intersects()` short-circuits cleanly.

**Unlocks**: `adder.schem`, `bitwise_and_h8b.schem`, `xor.schem`,
`not_h8b.schem` all load as stubs. `or.schem` still skipped (has zero
signs, fundamentally not a valid stub).

### 0002 — bussing empty-list guards
`redhdl/bussing/naive_bussing.py`

Adds explicit "if empty → 0" returns to every metric that divides by
the count of source→dest pin pairs / bus regions:
`bussing_avg_length`, `bussing_max_length`, `bussing_avg_min_length`,
`bussing_max_min_length`, `pin_pair_interrupted_line_of_sight_pct`,
`pin_pair_excessive_downwards_pct`, `pin_pair_straight_up_pct`,
`misaligned_bus_pct`, `stride_aligned_bus_pct`, `crossed_bus_pct`.

**Unlocks**: any design with a single instance or a single output port
(every one of these metrics crashed with `ZeroDivisionError` before).

### 0003 — placement + assembly polish
`redhdl/assembly/placement.py`, `redhdl/assembly/assembly.py`

- `mutated_placement` clamps `instances_to_tweak_count` to the actual
  placement size so tiny designs (1 instance) don't hit
  `Sample larger than population` in `random.sample`.
- `schematic_placement_from_netlist`'s fallback no longer drops into
  `pdb.set_trace()` (which breaks any non-interactive use). Surfaces
  the underlying exception via `RuntimeError` instead.

## End-to-end smoke test that worked after the patches

```systemverilog
// modules/Inv_Demo.sv
module Inv_Demo (input [7:0] x, output [7:0] y);
    Not_H8b inv (.In(x), .Out(y));
endmodule
```

```sh
mkdir -p build/checkpoints
python scripts/sv_to_schem.py inv_demo
# → produces inv_demo.schem (4×2×15, loadable in Litematica 0.27.x).
```

### 0004 — multi-bit-width + VHDL reserved words
`redhdl/netlist/vhdl_netlist.py`, `redhdl/netlist/netlist_template.py`

- Drop `assert bitrange == (0, 7)` in wire-alias resolution. Was a
  hard-block on any signal that wasn't exactly 8-bit (so 1-bit
  primitives like `Xor`, `Diagonal_Not`, and any wider-than-8-bit bus
  crashed). The alias-grouping logic itself doesn't care about width.

- `_resolve_schem` fallback: iverilog's `-tvhdl` rewrites Verilog
  module names that collide with VHDL reserved words by appending
  `_module` (e.g. `xor` → `xor_module`). Stub schematics keep the
  original name (`xor.schem`). The resolver now tries both.

**Unlocks**: 1-bit demos compile (`xor_demo.schem`, `diagonal_not_demo.schem`).
Any non-8-bit data path is also now allowed.

## End-to-end smoke tests that worked

Sized from trivial → moderate:

| SV demo                | Stubs used               | Output size  |
|------------------------|--------------------------|--------------|
| Diagonal_Not_Demo      | Diagonal_Not (1-bit NOT) | 332 B        |
| Xor_Demo               | Xor (1-bit XOR)          | 394 B        |
| Inv_Demo               | Not_H8b                  | 4×2×15       |
| Bitwise_Not_Demo       | Bitwise_Not_H8b          | 4×2×15       |
| And_Demo / Bitwise_And | And_H8b / Bitwise_And_H8b | 3×4×15      |
| Two_Not_Chain          | 2× Not_H8b cascaded      | ~445 B       |
| Nand_H8b               | Bitwise_And→Bitwise_Not  | ~463 B       |
| Single_Adder           | Adder (Kan-CC 8-bit)     | ~883 B       |

Use `python scripts/sv_to_schem.py <module_name>` (lowercase). Output
file is `<module_name>.schem` in the cwd.

## Still broken

- **`Adder_Chain` (2× Adder cascade)** raises `BussingImpossibleError`
  even after raising `max_bussing_steps` 50 → 10_000. Specific to
  the Adder stub's port geometry (the 8-bit Kan-CC has its A, B
  inputs and C output on 3 different faces, and the SA-placed pair
  apparently has unroutable pin pairs). Smaller cascades work
  (`Two_Not_Chain`, `Nand_H8b`), so this is a stub-specific routing
  limit, not a generic bussing crash. Needs deeper work on
  `bussing/redstone_bussing.py` or stub-aware placement padding.

- **`or.schem`** stub has no signs at all and can't be loaded. Treat
  as missing stub.
