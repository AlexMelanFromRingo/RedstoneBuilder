# Contract — CLI Command Schema

**Crate**: `redstonebuilder` (binary)
**Library**: `clap` v4 with `derive`.

## Synopsis

```text
redstonebuilder [OPTIONS] <INPUT> [--output <PATH>]
redstonebuilder --help | --version
```

## Arguments and flags

```rust
#[derive(Parser, Debug)]
#[command(version, about = "Compile HDL to Minecraft .litematic")]
pub struct Cli {
    /// Input HDL source file.
    pub input: PathBuf,

    /// Output schematic path. Required unless --dump-only is set.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Maximum schematic footprint W×H×D in blocks.
    /// Default: 256×<MC build height>×256 (FR-016).
    #[arg(long, value_parser = parse_footprint)]
    pub max_footprint: Option<Footprint>,

    /// Seed for any randomized P&R stage (FR-017).
    /// Default: a fixed compile-time constant for reproducibility.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Dump the AST. With no value, prints to stdout; with PATH, writes there.
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_ast: Option<PathBuf>,

    /// Dump the post-synthesis netlist (same conventions as --dump-ast).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_netlist: Option<PathBuf>,

    /// Dump the placement (same conventions as --dump-ast).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_placement: Option<PathBuf>,

    /// Stop after dumps; do not write a schematic. Implies any --dump-* set.
    #[arg(long)]
    pub dump_only: bool,

    /// Verbosity: -v info, -vv debug, -vvv trace.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

pub struct Footprint { pub width: u32, pub height: u32, pub depth: u32 }
```

## Exit codes

| Code | Meaning                                                                 |
|------|-------------------------------------------------------------------------|
| 0    | Success — schematic written (or dumps emitted with `--dump-only`).      |
| 1    | Generic error (CLI usage, I/O).                                         |
| 2    | Parse error or semantic-validation error (FR-003).                      |
| 3    | Synthesis error: combinational cycle, unsupported construct (FR-005).   |
| 4    | Placement error: design exceeds `--max-footprint` (FR-016).             |
| 5    | Routing error: bbox-expansion budget exhausted (FR-015).                |
| 6    | NBT/output error (write failure, palette overflow).                     |

All non-zero exits print a `miette::Report` diagnostic to **stderr**.

## stdout / stderr contract (FR-013)

- **stdout**: on success, one line: `wrote <path> (gates=N, blocks=M, footprint=WxHxD, time=Ts)`.
  Dumps go to stdout when the user passes `--dump-X` with no PATH.
- **stderr**: all diagnostics (errors + `--verbose` info/debug/trace).

## Argument value parsers

- `parse_footprint("256x320x256") -> Footprint { width: 256, height: 320, depth: 256 }`.
  Reject if any component is 0 or > some sanity cap (e.g., 4096).
- `--seed`: parsed as `u64`; default is `rb_synthesis::DEFAULT_SEED`.

## Examples

```bash
# MVP path: half-adder demo.
redstonebuilder examples/half_adder.hdl -o half_adder.litematic

# Debug a hard P&R failure.
redstonebuilder cpu.hdl --dump-netlist netlist.json --dump-placement placement.json -o cpu.litematic

# Inspect AST without producing output.
redstonebuilder cpu.hdl --dump-ast --dump-only

# Try a wider footprint.
redstonebuilder cpu.hdl --max-footprint 512x320x512 -o cpu.litematic
```
