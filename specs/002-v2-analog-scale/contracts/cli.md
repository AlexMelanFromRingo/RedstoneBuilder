# Contract — CLI Command Schema (v2 deltas)

**Crate**: `redstonebuilder` (binary)
**Extends**: `specs/001-hdl-compiler-cli/contracts/cli.md`

This document records **only the v2 additions / changes**. The v1 CLI
surface (`input` arg, `--output`, `--max-footprint`, `--seed`,
`--dump-ast`/`--dump-netlist`/`--dump-placement`, `--dump-only`,
`--verbose`) carries over unchanged.

## New flags

```rust
#[derive(Parser, Debug)]
pub struct Cli {
    // ... v1 fields above ...

    /// Hard cap on the compiler's peak resident memory, in MiB
    /// (FR-V09 / FR-V10). On overrun, exit with code 8.
    #[arg(long, value_name = "MB", default_value_t = 4096)]
    pub max_ram: u32,

    /// Cap on the iterative router's iteration count (FR-V16). On
    /// overrun, exit with code 7.
    #[arg(long, value_name = "N", default_value_t = 64)]
    pub max_routing_iterations: u32,

    /// Print per-stage wall-clock + peak-RAM + gate/net/block counts
    /// to stderr at end of compile (FR-V14).
    #[arg(long)]
    pub stats: bool,

    /// Placement algorithm. `sa` (default) = simulated annealing —
    /// scales to 10 000 gates. `greedy` = v1 row-based — kept for
    /// backward-compatibility and tiny inputs.
    #[arg(long, value_enum, default_value_t = Placer::Sa)]
    pub placer: Placer,

    /// Allow timing-analysis data-race diagnostics to be treated as
    /// warnings instead of hard errors (FR-V12 escape hatch).
    #[arg(long)]
    pub allow_timing_races: bool,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placer {
    Sa,
    Greedy,
}
```

## Extended exit-code table

| Code | v1/v2 | Meaning                                                                 |
|------|-------|-------------------------------------------------------------------------|
| 0    | v1    | Success.                                                                |
| 1    | v1    | Generic CLI / I/O error.                                                |
| 2    | v1    | Parse / semantic error.                                                 |
| 3    | v1    | Synthesis / cycle / unsupported construct.                              |
| 4    | v1    | Placement: footprint exceeds `--max-footprint`.                         |
| 5    | v1    | Routing: single-attempt exhaustion (legacy Lee's; `--placer greedy`).    |
| 6    | v1    | NBT / output error.                                                     |
| **7**  | **v2** | **Routing: PathFinder iteration cap exceeded** (FR-V16).               |
| **8**  | **v2** | **Memory cap exceeded** (FR-V10).                                      |
| **9**  | **v2** | **Static-timing data race** (FR-V12).                                  |

All non-zero exits print a `miette::Report` to stderr (v1 convention
preserved).

## stdout / stderr contract (extended)

- **stdout** on success: same one-line summary as v1, **plus** a v2
  `+stats=...` suffix if `--stats` is set:

  ```text
  wrote ha.litematic (gates=2, blocks=31, footprint=11x3x3, time=0.001s)
    stats: parse=0.0001s synth=0.0002s timing=0.0001s place=0.0003s route=0.0005s nbt=0.0002s peak_ram=12MB
  ```

- **stderr**: all diagnostics (v1 convention).

## `--stats` semantics

When `--stats` is set, the binary writes a structured stats line to
**stderr** (kept separate from stdout's terse machine-parseable
output). Format:

```text
stats: parse=Ts synth=Ts timing=Ts place=Ts route=Ts(iters=N) nbt=Ts peak_ram=MB
```

Per-stage wall-clock is measured with `Instant::now()`. `peak_ram` is
the maximum value sampled by `BudgetGuard` across all stage
boundaries.

## Backward-compat invariants

- All v1 flags continue to behave identically. New flags have safe
  defaults — a user who only knows the v1 CLI gets the v2 behaviour
  with no surprises beyond the new (better) router.
- v1 examples (`examples/half_adder.hdl` etc.) MUST continue to
  compile with exit code 0 under both `--placer sa` and
  `--placer greedy`.
