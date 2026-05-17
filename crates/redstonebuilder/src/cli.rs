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
