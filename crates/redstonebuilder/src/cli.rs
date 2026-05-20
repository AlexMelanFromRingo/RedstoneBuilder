//! `clap` schema for the CLI front-end.
//!
//! See `specs/001-hdl-compiler-cli/contracts/cli.md` for the full
//! command contract, including exit codes and stdout/stderr policy.

use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;

/// Compile an HDL source file into a Minecraft `.litematic` schematic.
#[derive(Parser, Debug)]
#[command(
    name = "redstonebuilder",
    version,
    about = "Compile a Verilog-subset HDL to a Minecraft Litematica .litematic schematic.",
    long_about = None
)]
pub struct Cli {
    /// Input HDL source file.
    pub input: PathBuf,

    /// Output schematic path. Required unless `--dump-only` is set.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Maximum schematic footprint `W×H×D` in blocks. Default:
    /// `256x384x256`.
    #[arg(long, value_name = "WxHxD")]
    pub max_footprint: Option<Footprint>,

    /// Seed for any randomized P&R stage (FR-017). v1 is deterministic
    /// by default; this flag is accepted for forward-compat.
    #[arg(long, value_name = "SEED")]
    pub seed: Option<u64>,

    /// Dump the AST to PATH (or stdout if PATH is `-`).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_ast: Option<PathBuf>,

    /// Dump the post-synthesis netlist to PATH (or stdout if `-`).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_netlist: Option<PathBuf>,

    /// Dump the placement to PATH (or stdout if `-`).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "-")]
    pub dump_placement: Option<PathBuf>,

    /// Stop after dumps; do not write a schematic.
    #[arg(long)]
    pub dump_only: bool,

    /// Increase verbosity: `-v` info, `-vv` debug, `-vvv` trace.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Placement algorithm. `sa` (default, v2) = simulated annealing.
    /// `greedy` (v1) = row-based — kept for backward-compat and tiny
    /// inputs.
    #[arg(long, value_enum, default_value_t = Placer::Sa)]
    pub placer: Placer,

    /// Routing algorithm. `lee` (default, v1) = bounded Lee BFS, fast on
    /// small designs but can leave nets unrouted at scale. `pathfinder`
    /// (v2) = A*-based negotiated congestion with rip-up & reroute and
    /// adjacency isolation; handles wide buses but is slower.
    #[arg(long, value_enum, default_value_t = Router::Lee)]
    pub router: Router,

    /// Cap on the iterative router's outer-loop iterations
    /// (FR-V16). On overrun, exit with code 7.
    #[arg(long, value_name = "N", default_value_t = 64)]
    pub max_routing_iterations: u32,

    /// Per-net Manhattan-distance cap for the bounded BFS router.
    /// Default `64` is safe for ≤ ~10-gate designs; bump to `256`+ for
    /// wide schematics (e.g. 8-bit adder ≈ 96×5×107 footprint needs
    /// `--max-route-distance 320`).
    #[arg(long, value_name = "N", default_value_t = 64)]
    pub max_route_distance: u32,

    /// Treat static-timing data-race diagnostics (FR-V12) as warnings
    /// instead of hard errors. **Default: true** — purely combinational
    /// designs (e.g., v1 full-adder) naturally converge with skewed
    /// gate-settle times but produce correct stable outputs, so a
    /// race is informational. Opt into strict mode with `--strict-timing`.
    #[arg(long, default_value_t = true)]
    pub allow_timing_races: bool,

    /// Treat static-timing data-race diagnostics as hard errors (exit
    /// code 9). Overrides `--allow-timing-races`.
    #[arg(long)]
    pub strict_timing: bool,

    /// Hard cap on the compiler's peak resident memory, in MiB
    /// (FR-V09 / FR-V10). On overrun, exit with code 8.
    #[arg(long, value_name = "MB", default_value_t = 4096)]
    pub max_ram: u32,

    /// Print per-stage wall-clock + peak-RAM + gate/net/block counts
    /// to stderr at end of compile (FR-V14).
    #[arg(long)]
    pub stats: bool,
}

/// Placer choice for the `--placer` CLI flag.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placer {
    /// v2 simulated annealing placer.
    Sa,
    /// v1 row-based greedy placer (deterministic, fast).
    Greedy,
}

/// Router choice for the `--router` CLI flag.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Router {
    /// v1 bounded Lee BFS (default).
    Lee,
    /// v2 PathFinder A* + negotiated congestion with adjacency isolation.
    Pathfinder,
}

/// Parsed value of the `--max-footprint` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// Width along +X.
    pub width: u32,
    /// Height along +Y.
    pub height: u32,
    /// Depth along +Z.
    pub depth: u32,
}

impl FromStr for Footprint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(['x', 'X', '×']).collect();
        if parts.len() != 3 {
            return Err(format!(
                "expected WxHxD (three components separated by 'x' or '×'), got '{s}'"
            ));
        }
        let parse = |label: &str, p: &str| -> Result<u32, String> {
            let v: u32 = p
                .trim()
                .parse()
                .map_err(|e| format!("invalid {label} '{p}': {e}"))?;
            if v == 0 {
                return Err(format!("{label} must be > 0, got 0"));
            }
            if v > 4096 {
                return Err(format!("{label} too large: {v} > 4096 (sanity cap)"));
            }
            Ok(v)
        };
        Ok(Footprint {
            width: parse("width", parts[0])?,
            height: parse("height", parts[1])?,
            depth: parse("depth", parts[2])?,
        })
    }
}
