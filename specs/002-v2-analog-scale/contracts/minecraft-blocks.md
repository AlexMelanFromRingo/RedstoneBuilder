# Contract — Minecraft Block-State Catalogue (v2 deltas)

**Crate**: `rb-nbt::palette`
**Extends**: `specs/001-hdl-compiler-cli/contracts/minecraft-blocks.md`
**Target**: Minecraft Java Edition 26.1 (unchanged from v1).

This document records only the **v2 additions**. All v1 block-state
entries remain unchanged.

## New block-state entries

Each entry below specifies the `Name` and all `Properties` the v2
`block_state_for(BlockId, Option<Direction>)` writes.

### `Repeater` (now user-instantiable; v1 had it as router-only)

```text
Name:       "minecraft:repeater"
Properties:
  facing:   north | south | east | west       (default north)
  delay:    "1" | "2" | "3" | "4"             (REQUIRED — pulled from HDL `.DELAY()` or default 1)
  locked:   "true" | "false"                   (REQUIRED — driven by HDL `.LOCK()` or "false")
  powered:  "false"                            (always — runtime-computed by MC)
```

`facing` is the **output direction** of the repeater.

### `Observer` (NEW)

```text
Name:       "minecraft:observer"
Properties:
  facing:   north | south | east | west | up | down    (REQUIRED — points toward the watched cell)
  powered:  "false"                                   (always)
```

The observer's *back* (opposite of `facing`) is the side that
listens; the *face* emits the 1-tick pulse.

### `TargetBlock` (NEW)

```text
Name:       "minecraft:target"
Properties:
  power:    "0"                                       (always — runtime-computed)
```

### `Slab` (NEW, router-only)

```text
Name:       "minecraft:stone_slab"
Properties:
  type:     "bottom"                                  (always — only bottom slabs allow
                                                       upward-only dust transmission)
  waterlogged: "false"
```

### `Glass` (NEW, router-only)

```text
Name:       "minecraft:glass"
Properties: (none)
```

## Palette emission rule (unchanged from v1)

- Index 0 is reserved for `minecraft:air`.
- Subsequent indices are assigned in first-encounter order during a
  `(y, z, x)` lex walk of the placed block grid.
- Property maps stored as `BTreeMap<SmolStr, SmolStr>` to keep
  serialisation order deterministic.

## Bit-width invariant (post-v2)

The v2 palette adds ≤ 5 new distinct block-states with their property
combinations:

- `minecraft:repeater` has 4 facings × 4 delays × 2 lock states = 32
  variants in theory; in practice, the router emits only the
  combinations actually used per design.
- `minecraft:observer` has 6 facings = 6 variants.
- `minecraft:target`, `minecraft:stone_slab`, `minecraft:glass` have
  exactly 1 variant each.

For a 10 000-gate design upper bound on palette size: pessimistic
estimate ≈ 100 distinct block-states. `bits_per_index = ceil(log2(palette.len()))`
caps at 7 bits — well within the spanning packed-long encoder's
capacity (the v1 packer handles up to 32 bits).

## Backward compatibility

All v1 example files use only v1 catalogue entries. v2 examples that
use the new blocks add palette entries; v2's packer is the same v1
packer with no format change. Byte-identical output for v1 inputs is
preserved.
