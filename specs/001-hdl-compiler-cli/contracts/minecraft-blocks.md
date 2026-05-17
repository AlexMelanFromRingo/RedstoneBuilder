# Contract — Minecraft Block-State Catalogue

**Crate**: `rb-nbt::palette`
**Target game version**: Minecraft Java Edition 26.1 ("Tiny Takeover").

Block-state identifiers and property names follow MC Java 26.1 vanilla
data. Source of truth: <https://minecraft.wiki/w/Java_Edition_data_values>
(filtered to 26.1).

## Block-state representation

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BlockState {
    pub name: BlockName,                       // e.g. "minecraft:repeater"
    pub properties: BTreeMap<SmolStr, SmolStr>, // sorted for determinism
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BlockName(pub SmolStr);             // namespaced ID
```

## Constants needed by the synthesizer

Each macro-cell (data-model.md, Stage 3) draws from this fixed catalogue.
Properties listed are the ones the cell library sets explicitly; any
property not listed takes the vanilla default.

| Const                 | `name`                       | Properties used                                      | Notes |
|-----------------------|------------------------------|------------------------------------------------------|-------|
| `AIR`                 | `minecraft:air`              | —                                                    | empty cell |
| `STONE`               | `minecraft:stone`            | —                                                    | generic solid support block |
| `REDSTONE_DUST`       | `minecraft:redstone_wire`    | `power=0..15`, `north/south/east/west=none/side/up` | router writes `power=0` (game recomputes on load); connection sides come from neighbor analysis |
| `REDSTONE_TORCH`      | `minecraft:redstone_torch`   | `lit=true`                                          | upright torch on top of a block |
| `REDSTONE_WALL_TORCH` | `minecraft:redstone_wall_torch` | `facing=north/south/east/west`, `lit=true`        | side-mounted torch |
| `REPEATER`            | `minecraft:repeater`         | `facing=…`, `delay=1..4`, `locked=false`, `powered=false` | default delay 1 (= 2 game ticks); router may set higher for tuning |
| `COMPARATOR`          | `minecraft:comparator`       | `facing=…`, `mode=compare/subtract`, `powered=false` | reserved for memory cell (`mode=subtract`) |
| `LEVER`               | `minecraft:lever`            | `face=floor/wall/ceiling`, `facing=…`, `powered=false` | one per `input` port |
| `REDSTONE_LAMP`       | `minecraft:redstone_lamp`    | `lit=false`                                          | one per `output` port (visual indicator) |
| `OAK_PLANKS`          | `minecraft:oak_planks`       | —                                                    | non-conductive solid; under dust to prevent power leakage |
| `OAK_STAIRS`          | `minecraft:oak_stairs`       | `facing=…`, `half=bottom`, `shape=straight`         | level-change segments |

## Macro-cell layout rules (cell library)

All cells are placed in a **local coordinate frame** with `+Y` = up,
`+X` = right (cell's output direction), `+Z` = into the build. The
`Macrocell.blocks` vector lists positions in this local frame; the placer
rotates them by `Rotation::R{0,90,180,270}` around `Y` and translates by
the placement origin.

### `Not` (combinational)

```text
y=1: . T .            T = REDSTONE_WALL_TORCH(facing=east)
y=0: I S O            I = input dust anchor   S = STONE    O = output dust anchor
```

- Input anchor at local `(0, 0, 0)`, approach = west.
- Output anchor at local `(2, 0, 0)`, approach = east.
- Tick delay: 1.

### `And` (2-input NAND-of-NOTs)

Two `Not` cells side-by-side feeding a NOR (torch tower). Footprint
3 × 2 × 3 blocks. Tick delay: 3.

### `Or`

Dust merge into a 1-tick repeater. Footprint 3 × 1 × 3 blocks. Tick
delay: 1.

### `Xor`

Standard 4-input redstone XOR (two NOTs + two ANDs + OR). Footprint
5 × 3 × 5 blocks. Tick delay: 3.

### `DTrigger` (edge-triggered D flip-flop)

Clock edge detector (pulse extractor: AND of `clk` with delayed `!clk`)
feeds a gated RS latch holding `D`. Footprint 5 × 2 × 4 blocks.
Tick delay: 4.

### `MemoryCell` (write-gated SR latch)

RS latch where `set = DATA & WRITE`, `reset = !DATA & WRITE`. Footprint
4 × 2 × 4 blocks. Tick delay: 2.

(Exact in-game block-by-block layouts are coded as constants in
`rb-synthesis::cell_library` and verified by golden-file tests that
compare against schematic dumps committed in `tests/fixtures/cells/`.)

## Redstone-dust adjacency model (the routing invariant)

Vanilla MC: a `redstone_wire` block at position `p` is electrically
connected to a `redstone_wire` block at position `q` iff:

- `q ∈ N4(p)` at `q.y = p.y`, OR
- `q ∈ N4(p)` at `q.y = p.y + 1` AND the block at `(p.x, p.y, p.z)`'s
  upward neighbor is air (the dust climbs over a step), OR
- `q ∈ N4(p)` at `q.y = p.y - 1` AND the block above `q` is air (dust
  steps down).

The router's `Obstructed` propagation (see netlist-ir.md) is the
conservative over-approximation of this rule: any 4-lateral neighbor at
`y`, `y+1`, or `y-1` is reserved.

## Litematica palette emission

The Litematica region embeds a `BlockStatePalette` (list compound tag)
whose entries are the distinct `BlockState`s present in the region. The
`BlockStates` long-array packs per-block palette indices, where the bit
width is `max(2, ceil(log2(palette.len())))`.

Palette index 0 is always reserved for `AIR` (see `contracts/litematic-nbt.md`
for the full tag tree). Indices for non-air blocks are assigned in
**first-encounter order** during a preorder walk of placed blocks in
`(y, z, x)` lex order — this is the determinism anchor for the NBT output.
