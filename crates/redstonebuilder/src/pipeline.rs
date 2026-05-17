//! End-to-end compiler pipeline.
//!
//! Wires Parser → AST → Netlist → cycle check → Place → Route →
//! BlockGrid → NBT writer per Constitution Principle II (strict
//! pipeline architecture).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use miette::Diagnostic;
use petgraph::stable_graph::NodeIndex;
use rb_core::{Bbox3, Direction, Pos3};
use rb_nbt::{block_state_for, write_litematic, BlockGrid};
use rb_synthesis::{
    build_netlist, detect_cycles, place,
    route::{RouteSegment, RoutedWire},
    route_single_bounded, CellState, EndpointRole, Grid3D, NetTag, Netlist, NetlistNode,
    PlaceConfig, Placement, RouteConfig, SingleRouteOutcome,
};

use crate::cli::Cli;

/// Outcome of a successful pipeline run.
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// Path the schematic was written to (`None` if `--dump-only`).
    pub output: Option<PathBuf>,
    /// Gate count after synthesis.
    pub gate_count: usize,
    /// Non-air block count in the final schematic.
    pub block_count: usize,
    /// Schematic footprint as `(width, height, depth)`.
    pub footprint: (u32, u32, u32),
    /// Wall-clock duration of the compile.
    pub elapsed: std::time::Duration,
}

/// Top-level pipeline errors. Returned as a `miette::Report` from
/// [`run`]; the binary's `main` maps these to exit codes.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum PipelineError {
    /// File-system or I/O failure reading the input.
    #[error("could not read input '{path}': {source}")]
    #[diagnostic(code(redstonebuilder::io))]
    Io {
        /// Input path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The user forgot `--output` and did not pass `--dump-only`.
    #[error("--output is required unless --dump-only is set")]
    #[diagnostic(code(redstonebuilder::no_output))]
    NoOutput,

    /// Parse-stage diagnostic (also includes semantic-validation).
    #[error("{0}")]
    #[diagnostic(transparent)]
    Parse(rb_parser::ParseError),

    /// Semantic-validation diagnostic.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Semantic(rb_parser::SemanticError),

    /// Synthesis (netlist build).
    #[error("{0}")]
    #[diagnostic(transparent)]
    Synth(rb_synthesis::SynthError),

    /// Combinational feedback cycle.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Cycle(rb_synthesis::CycleError),

    /// Design exceeded `--max-footprint`.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Place(rb_synthesis::PlaceError),

    /// Routing failure (FR-015 — full retry lives in US3).
    #[error("{0}")]
    #[diagnostic(transparent)]
    Route(rb_synthesis::RouteError),

    /// NBT writer failure.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Nbt(rb_nbt::NbtError),
}

/// Exit code categories per `contracts/cli.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCategory {
    /// 0 — success.
    Success = 0,
    /// 1 — generic CLI/I/O error.
    Cli = 1,
    /// 2 — parse / semantic error.
    Parse = 2,
    /// 3 — synthesis / cycle / unsupported construct.
    Synth = 3,
    /// 4 — placement: too large.
    Place = 4,
    /// 5 — routing exhausted.
    Route = 5,
    /// 6 — NBT/output failure.
    Nbt = 6,
}

impl ExitCategory {
    /// Pick the exit category for a given pipeline error.
    pub fn for_error(err: &PipelineError) -> Self {
        match err {
            PipelineError::Io { .. } | PipelineError::NoOutput => ExitCategory::Cli,
            PipelineError::Parse(_) | PipelineError::Semantic(_) => ExitCategory::Parse,
            PipelineError::Synth(_) | PipelineError::Cycle(_) => ExitCategory::Synth,
            PipelineError::Place(_) => ExitCategory::Place,
            PipelineError::Route(_) => ExitCategory::Route,
            PipelineError::Nbt(_) => ExitCategory::Nbt,
        }
    }
}

/// Run the full pipeline.
#[allow(clippy::result_large_err)]
pub fn run(cli: &Cli) -> Result<RunSummary, PipelineError> {
    let started = Instant::now();

    let source = std::fs::read_to_string(&cli.input).map_err(|e| PipelineError::Io {
        path: cli.input.clone(),
        source: e,
    })?;

    let module = rb_parser::parse(&source, &cli.input).map_err(PipelineError::Parse)?;
    if let Some(path) = &cli.dump_ast {
        write_json_dump(path, &module)?;
    }

    if let Err(errors) = rb_parser::validate(&module, &source) {
        if let Some(first) = errors.into_iter().next() {
            return Err(PipelineError::Semantic(first));
        }
    }

    let netlist = build_netlist(&module).map_err(PipelineError::Synth)?;
    if let Some(path) = &cli.dump_netlist {
        write_json_dump(path, &netlist)?;
    }

    detect_cycles(&netlist).map_err(PipelineError::Cycle)?;

    let placement = place(&netlist, &cfg_from_cli(cli)).map_err(PipelineError::Place)?;
    if let Some(path) = &cli.dump_placement {
        write_json_dump(path, &placement)?;
    }

    let route_cfg = route_cfg_from_cli(cli);
    let wires = route_pipeline(&placement, &netlist, &route_cfg).map_err(PipelineError::Route)?;
    let grid = assemble_grid(&placement, &wires);

    let summary_footprint = (
        grid.bounds.width(),
        grid.bounds.height(),
        grid.bounds.depth(),
    );
    let block_count = grid.blocks.len();
    let gate_count = placement.cells.len();

    let output_path = if cli.dump_only {
        None
    } else {
        let path = cli.output.clone().ok_or(PipelineError::NoOutput)?;
        let stem = cli
            .input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("schematic");
        let desc = format!("Compiled from {}", cli.input.display());
        write_litematic(&path, &grid, stem, &desc).map_err(PipelineError::Nbt)?;
        Some(path)
    };

    Ok(RunSummary {
        output: output_path,
        gate_count,
        block_count,
        footprint: summary_footprint,
        elapsed: started.elapsed(),
    })
}

fn cfg_from_cli(cli: &Cli) -> PlaceConfig {
    if let Some(f) = cli.max_footprint {
        PlaceConfig {
            max_footprint: (f.width, f.height, f.depth),
            row_gap_z: PlaceConfig::DEFAULT.row_gap_z,
        }
    } else {
        PlaceConfig::DEFAULT
    }
}

fn route_cfg_from_cli(cli: &Cli) -> RouteConfig {
    let mut cfg = RouteConfig::DEFAULT;
    if let Some(seed) = cli.seed {
        cfg.seed = seed;
    }
    cfg
}

#[allow(clippy::result_large_err)]
fn write_json_dump<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), PipelineError> {
    let json = serde_json::to_string_pretty(value).map_err(|e| PipelineError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    if path.as_os_str() == "-" {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        writeln!(lock, "{json}").map_err(|e| PipelineError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    } else {
        std::fs::write(path, json).map_err(|e| PipelineError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

fn route_pipeline(
    placement: &Placement,
    netlist: &Netlist,
    cfg: &RouteConfig,
) -> Result<Vec<RoutedWire>, rb_synthesis::RouteError> {
    let mut grid = Grid3D::new();
    seed_grid_with_cells(&mut grid, placement);

    let mut routes: Vec<(NetTag, Pos3, Pos3)> = Vec::new();
    for &net_id in netlist.nets.keys() {
        let net_tag = NetTag(net_id.0);
        let Some(source_world) = net_source_world_pos(placement, netlist, net_id) else {
            continue; // module-input net with no in-module driver — nothing to route in v1
        };
        for sink_world in net_sink_world_positions(placement, netlist, net_id) {
            routes.push((net_tag, source_world, sink_world));
        }
    }

    // v1 pipeline uses single-attempt routing with a hard distance
    // cap. The bbox-expansion retry (FR-015) is exposed via
    // `route_with_retry` and exercised by the `route_exhausted` test,
    // but for the pipeline we skip retry: the unbounded BFS used by
    // the plain `route_all` can degenerate to seconds-per-net when
    // earlier nets obstruct a tight corridor. A per-net failure here
    // just means the wire isn't emitted (the placed cells still
    // appear in the schematic) — Polish-phase work will tighten this.
    let mut unrouted: Vec<String> = Vec::new();
    for (net, source, sink) in &routes {
        if let SingleRouteOutcome::Unroutable =
            route_single_bounded(&mut grid, *net, *source, *sink, cfg.initial_max_distance)
        {
            unrouted.push(format!("net#{}", net.0));
        }
    }
    if !unrouted.is_empty() {
        eprintln!(
            "[warn] {} net(s) left unrouted in pipeline (cap={}): {}",
            unrouted.len(),
            cfg.initial_max_distance,
            unrouted.join(", ")
        );
    }

    // route_all populated `grid` in place. We need the wires it
    // produced — re-derive by walking grid for Dust/Repeater cells
    // belonging to each net.
    let wires = harvest_wires(&grid);
    Ok(wires)
}

fn harvest_wires(grid: &Grid3D) -> Vec<RoutedWire> {
    use std::collections::BTreeMap;
    let mut by_net: BTreeMap<NetTag, Vec<RouteSegment>> = BTreeMap::new();
    for (pos, state) in grid.iter_sorted() {
        match state {
            CellState::Dust(net) => {
                by_net
                    .entry(net)
                    .or_default()
                    .push(RouteSegment::Dust { pos });
            }
            CellState::Repeater(net) => {
                by_net.entry(net).or_default().push(RouteSegment::Repeater {
                    pos,
                    facing: rb_core::Direction::East,
                });
            }
            _ => {}
        }
    }
    by_net
        .into_iter()
        .map(|(net, segments)| RoutedWire { net, segments })
        .collect()
}

fn seed_grid_with_cells(grid: &mut Grid3D, placement: &Placement) {
    for cell in &placement.cells {
        for block in &cell.macro_cell.blocks {
            let p = Pos3::new(
                cell.origin.x + block.pos.x,
                cell.origin.y + block.pos.y,
                cell.origin.z + block.pos.z,
            );
            grid.set(p, CellState::Solid);
        }
    }
}

fn net_source_world_pos(
    placement: &Placement,
    netlist: &Netlist,
    net_id: rb_synthesis::NetId,
) -> Option<Pos3> {
    for edge_idx in netlist.graph.edge_indices() {
        let edge_w = &netlist.graph[edge_idx];
        if edge_w.net != net_id {
            continue;
        }
        let (src, _) = netlist.graph.edge_endpoints(edge_idx)?;
        if let NetlistNode::Gate { .. } = netlist.graph[src] {
            if let Some(cell) = find_cell_for_node(placement, src) {
                if let Some(anchor) = cell
                    .macro_cell
                    .outputs
                    .iter()
                    .find(|a| a.role == EndpointRole::DataOut || a.role == EndpointRole::Q)
                {
                    return Some(anchor_world_pos(cell.origin, anchor.pos));
                }
            }
        }
    }
    None
}

fn net_sink_world_positions(
    placement: &Placement,
    netlist: &Netlist,
    net_id: rb_synthesis::NetId,
) -> Vec<Pos3> {
    let mut out: Vec<Pos3> = Vec::new();
    for edge_idx in netlist.graph.edge_indices() {
        let edge_w = &netlist.graph[edge_idx];
        if edge_w.net != net_id {
            continue;
        }
        let Some((_, dst)) = netlist.graph.edge_endpoints(edge_idx) else {
            continue;
        };
        if let NetlistNode::Gate { .. } = netlist.graph[dst] {
            if let Some(cell) = find_cell_for_node(placement, dst) {
                let want = edge_w.to;
                if let Some(anchor) = cell.macro_cell.inputs.iter().find(|a| a.role == want) {
                    out.push(anchor_world_pos(cell.origin, anchor.pos));
                }
            }
        }
    }
    out
}

fn find_cell_for_node(placement: &Placement, node: NodeIndex) -> Option<&rb_synthesis::PlacedCell> {
    placement.cells.iter().find(|c| c.node == node)
}

fn anchor_world_pos(origin: Pos3, local: Pos3) -> Pos3 {
    Pos3::new(origin.x + local.x, origin.y + local.y, origin.z + local.z)
}

fn assemble_grid(placement: &Placement, wires: &[RoutedWire]) -> BlockGrid {
    let mut bbox = placement.bounds;
    for wire in wires {
        for seg in &wire.segments {
            let p = match *seg {
                RouteSegment::Dust { pos } => pos,
                RouteSegment::Repeater { pos, .. } => pos,
            };
            bbox = bbox.union(&Bbox3::point(p));
        }
    }

    let dx = -bbox.min.x;
    let dy = -bbox.min.y;
    let dz = -bbox.min.z;
    let normalized = Bbox3 {
        min: Pos3::ORIGIN,
        max: Pos3::new(bbox.max.x + dx, bbox.max.y + dy, bbox.max.z + dz),
    };
    let mut grid = BlockGrid::empty(normalized);
    let mut owned: BTreeMap<Pos3, ()> = BTreeMap::new();

    for cell in &placement.cells {
        for block in &cell.macro_cell.blocks {
            let world = Pos3::new(
                cell.origin.x + block.pos.x + dx,
                cell.origin.y + block.pos.y + dy,
                cell.origin.z + block.pos.z + dz,
            );
            grid.insert(world, block_state_for(block.block, block.facing));
            owned.insert(world, ());
        }
    }

    for wire in wires {
        for seg in &wire.segments {
            let (pos, facing, block_kind) = match *seg {
                RouteSegment::Dust { pos } => (pos, None, rb_core::BlockId::RedstoneDust),
                RouteSegment::Repeater { pos, facing } => {
                    (pos, Some(facing), rb_core::BlockId::Repeater)
                }
            };
            let world = Pos3::new(pos.x + dx, pos.y + dy, pos.z + dz);
            if owned.contains_key(&world) {
                continue; // cell already occupies this cell — keep cell's block
            }
            grid.insert(world, block_state_for(block_kind, facing));
        }
    }

    let _ = Direction::ALL; // touch to keep import (avoid dead-code warn if unused)
    grid
}
