//! `redstonebuilder` — the compiler CLI binary.
//!
//! See `specs/001-hdl-compiler-cli/contracts/cli.md`.

mod cli;
mod pipeline;
mod report;

use clap::Parser;

use crate::cli::Cli;
use crate::pipeline::{run, ExitCategory, PipelineError};

fn main() {
    report::install();
    let cli = Cli::parse();

    match run(&cli) {
        Ok(summary) => {
            let (w, h, d) = summary.footprint;
            match summary.output {
                Some(path) => println!(
                    "wrote {} (gates={}, blocks={}, footprint={w}x{h}x{d}, time={:.3}s)",
                    path.display(),
                    summary.gate_count,
                    summary.block_count,
                    summary.elapsed.as_secs_f64()
                ),
                None => println!(
                    "ok (gates={}, blocks={}, footprint={w}x{h}x{d}, time={:.3}s)",
                    summary.gate_count,
                    summary.block_count,
                    summary.elapsed.as_secs_f64()
                ),
            }
            std::process::exit(ExitCategory::Success as i32);
        }
        Err(err) => {
            let code = ExitCategory::for_error(&err);
            print_error(err);
            std::process::exit(code as i32);
        }
    }
}

fn print_error(err: PipelineError) {
    let report = miette::Report::new(err);
    eprintln!("{report:?}");
}
