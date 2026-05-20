# Contract — Litematica `.litematic` NBT Structure

**Crate**: `rb-nbt`
**Format**: gzip-compressed NBT (RFC 1952 outside; NBT inside).
**Target Litematica schema**: v7 + `SubVersion = 1` (current schema written
by Litematica 0.27.x on MC 26.1.x — see upstream
`LitematicaSchematic.java` constants `SCHEMATIC_VERSION = 7`,
`SCHEMATIC_VERSION_SUB = 1` on branch
`pre-rewrite/fabric/1.21.1-masa`).

## Top-level structure

The root tag is an unnamed `TAG_Compound` with fields:

```text
TAG_Compound (root)
├── "MinecraftDataVersion": TAG_Int = 4786 // Java Edition data version (MC 26.1 = 4786)
├── "Version":              TAG_Int = 7    // Litematica schema version
├── "SubVersion":           TAG_Int = 1    // schema data-fix counter
├── "Metadata":             TAG_Compound   // see below
└── "Regions":              TAG_Compound   // map: region name → region compound
```

### `Metadata`

```text
TAG_Compound
├── "Name":            TAG_String     = <module name from HDL>
├── "Author":          TAG_String     = "redstonebuilder"
├── "Description":     TAG_String     = "compiled from <input file>"
├── "TimeCreated":     TAG_Long       = 0                // FR-017: zero, never wall-clock
├── "TimeModified":    TAG_Long       = 0
├── "TotalBlocks":     TAG_Int        = <non-air block count>
├── "TotalVolume":     TAG_Int        = W * H * D
├── "RegionCount":     TAG_Int        = 1                // v1 emits a single region
├── "EnclosingSize":   TAG_Compound { x, y, z: TAG_Int }
└── "PreviewImageData": TAG_IntArray  = [] (empty, no preview)
```

### `Regions` → `<region name>` (single region in v1)

Region name: `"main"` (constant).

```text
TAG_Compound
├── "Position":    TAG_Compound { x, y, z: TAG_Int }   // region origin in world coords; we write (0,0,0)
├── "Size":        TAG_Compound { x, y, z: TAG_Int }   // signed; v1 always emits positive sizes
├── "BlockStatePalette": TAG_List of TAG_Compound      // see palette structure below
├── "BlockStates": TAG_LongArray                       // packed indices into palette
├── "Entities":              TAG_List = []
├── "TileEntities":          TAG_List = []
├── "PendingBlockTicks":     TAG_List = []
└── "PendingFluidTicks":     TAG_List = []
```

### `BlockStatePalette` entry

```text
TAG_Compound
├── "Name":       TAG_String       = "minecraft:redstone_wire" (etc.)
└── "Properties": TAG_Compound     = { "facing": TAG_String("north"), ... }   // omitted if empty
```

Palette ordering: index 0 = `minecraft:air` (always reserved). Subsequent
indices in first-encounter order (see `contracts/minecraft-blocks.md`).

### `BlockStates` packing

A `TAG_LongArray` of packed unsigned bit-fields, **little-endian within
each long**, where each field is a palette index.

```text
bits_per_index = max(2, ceil(log2(palette.len())))
total_indices  = Size.x * Size.y * Size.z
array_length   = ceil(total_indices * bits_per_index / 64)
```

Index iteration order — **the Litematica convention**:

```text
for y in 0..Size.y:
    for z in 0..Size.z:
        for x in 0..Size.x:
            yield index_at(x, y, z)
```

(I.e., X is the fastest-changing axis, then Z, then Y. This matches what
the Litematica mod expects when reading the schematic.)

## Writer pipeline

```rust
pub fn write_litematic(
    path: &Path,
    grid: &BlockGrid,
    meta: &Metadata,
) -> Result<(), NbtError> {
    let root: NbtCompound = build_root(grid, meta)?;
    let file = File::create(path)?;
    let mut gz = GzEncoder::new(BufWriter::new(file), Compression::default());
    fastnbt::to_writer(&mut gz, &root)?;
    gz.finish()?.flush()?;
    Ok(())
}
```

- `BlockGrid` (data-model.md, Stage 6) is the sparse input.
- `build_root` densifies the grid into the `BlockStates` packed long
  array and constructs the palette.
- Determinism: `BTreeMap` for properties, first-encounter palette ordering,
  zeroed timestamps, fixed `Compression::default()` level.

## Error type

```rust
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum NbtError {
    #[error("io error writing schematic: {0}")]
    Io(#[from] std::io::Error),

    #[error("nbt serialization error: {0}")]
    Nbt(#[from] fastnbt::error::Error),

    #[error("palette overflow: {count} unique block states (max supported in v1: {max})")]
    PaletteOverflow { count: usize, max: usize },
}
```

## Verification

- **Round-trip test**: write a schematic, parse it back with `fastnbt`,
  assert the `BlockGrid` reconstructs identically.
- **Golden-file test**: SHA-256 the gzipped output for a fixed reference
  design; assert it matches a committed hash (FR-017 / SC-007).
- **Litematica-load test** (manual / integration): import generated
  `.litematic` into a real Litematica install for MC 26.1 and confirm
  no warnings (FR-012).
