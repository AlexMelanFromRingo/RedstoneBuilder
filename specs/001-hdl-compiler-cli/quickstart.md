# Quickstart — RedstoneBuilder

Compile your first HDL design into a Minecraft schematic in under 5
minutes (success criterion SC-001).

## Prerequisites

- Rust toolchain: stable, edition 2021. Tested on 1.86+.
- Minecraft Java Edition 26.1 ("Tiny Takeover") with the
  [Litematica](https://www.curseforge.com/minecraft/mc-mods/litematica)
  mod installed (build for 26.1).

## Build

```bash
git clone <repo>
cd RedstoneBuilder
cargo build --release
# binary: target/release/redstonebuilder
```

## Hello, half-adder

Save the following as `half_adder.hdl`:

```verilog
module half_adder(input a, input b, output sum, output carry);
    xor x1(.A(a), .B(b), .Y(sum));
    and a1(.A(a), .B(b), .Y(carry));
endmodule
```

Compile:

```bash
target/release/redstonebuilder half_adder.hdl -o half_adder.litematic
```

Expected output:

```text
wrote half_adder.litematic (gates=2, blocks=37, footprint=11x3x7, time=0.04s)
```

Import into Minecraft:

1. Launch Minecraft 26.1 with Litematica.
2. Place `half_adder.litematic` into `<world>/schematics/`.
3. In-game: open Litematica's schematic browser (`M` by default), load
   `half_adder`, place it in build mode, and use the printer (or build
   it manually).
4. Right-click levers `a` and `b`; observe `sum` and `carry` lamps.
   Truth table: `(a=0,b=0)→sum=0,carry=0`, `(0,1)→1,0`, `(1,0)→1,0`,
   `(1,1)→0,1`.

## Debugging a design

Dump intermediate stages without producing a schematic:

```bash
redstonebuilder cpu.hdl --dump-ast ast.json --dump-netlist netlist.json --dump-placement placement.json --dump-only
```

Each file is human-readable JSON. Useful when a routing failure reports
unrouted nets — `placement.json` shows where they were headed.

## Reproducible builds

Same input + same flags + same `redstonebuilder` version → byte-identical
`.litematic` (FR-017). Verify:

```bash
redstonebuilder half_adder.hdl -o run1.litematic
redstonebuilder half_adder.hdl -o run2.litematic
sha256sum run1.litematic run2.litematic
# both hashes equal
```

If you intentionally want to explore a different P&R, vary the seed:

```bash
redstonebuilder cpu.hdl --seed 42 -o cpu.litematic
```

## When a build fails

| Exit | Meaning | First thing to check |
|------|---------|----------------------|
| 2 | Parse / semantic error | The diagnostic prints the file, line, column, and the offending span. Most often: typo in a port name. |
| 3 | Combinational cycle | Insert a `dtrigger` or `memcell` on the feedback path. |
| 4 | Design too large | Raise `--max-footprint`, or simplify the design. |
| 5 | Router exhausted | Same as 4 — give it more room. Use `--dump-placement` to see what's competing. |
| 6 | NBT/output error | Check disk space and write permissions on the output path. |

## What's next

- Read `specs/001-hdl-compiler-cli/contracts/hdl-grammar.md` for the
  full HDL surface syntax.
- Read `contracts/cli.md` for all available flags.
- Read `data-model.md` for the pipeline architecture.
