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
use rb_core::BlockId;
use rb_core::{Bbox3, Direction, Pos3};
use rb_nbt::{block_state_for, override_property, write_litematic, BlockGrid};
use rb_synthesis::{
    analyse_timing, astar_route_multisrc, build_netlist, detect_cycles, place_with,
    route::{RouteSegment, RoutedWire},
    route_pathfinder, route_single_bounded, BudgetGuard, CellState, CostMap, EndpointRole, Grid3D,
    MemoryError, NetTag, Netlist, NetlistNode, PathFinderConfig, PlaceConfig, Placement,
    PlacerKind, RouteConfig, RouteError, SaConfig, SingleRouteOutcome, TimingConfig, TimingError,
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
    /// v2: peak resident-set-size sampled during the compile, bytes.
    pub peak_ram_bytes: u64,
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

    /// v2: PathFinder router did not converge (FR-V16).
    /// Wrapped into the Route variant when emitted from the v2 router.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Memory(MemoryError),

    /// v2: Static-timing analysis detected a data race (FR-V12).
    #[error("{0}")]
    #[diagnostic(transparent)]
    Timing(TimingError),
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
    /// 7 — v2 PathFinder router convergence-exhausted (FR-V16).
    PathFinderExhausted = 7,
    /// 8 — v2 memory cap exceeded (FR-V10).
    MemoryCap = 8,
    /// 9 — v2 static-timing data race (FR-V12).
    TimingRace = 9,
}

impl ExitCategory {
    /// Pick the exit category for a given pipeline error.
    pub fn for_error(err: &PipelineError) -> Self {
        match err {
            PipelineError::Io { .. } | PipelineError::NoOutput => ExitCategory::Cli,
            PipelineError::Parse(_) | PipelineError::Semantic(_) => ExitCategory::Parse,
            PipelineError::Synth(_) | PipelineError::Cycle(_) => ExitCategory::Synth,
            PipelineError::Place(_) => ExitCategory::Place,
            PipelineError::Route(err) => match err {
                rb_synthesis::RouteError::ConvergenceExhausted { .. } => {
                    ExitCategory::PathFinderExhausted
                }
                _ => ExitCategory::Route,
            },
            PipelineError::Nbt(_) => ExitCategory::Nbt,
            PipelineError::Memory(_) => ExitCategory::MemoryCap,
            PipelineError::Timing(_) => ExitCategory::TimingRace,
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

    // Synthesis-time lowering: rewrite composite primitives that don't
    // have a working physical macrocell as compositions of primitives
    // that do. Currently only xor → (a&!b)|(!a&b). The post-lowering
    // form is what the netlist will see; `--dump-ast` above shows the
    // original parsed AST before lowering.
    let mut module = module;
    rb_synthesis::lower_xor_gates(&mut module);

    let netlist = build_netlist(&module).map_err(PipelineError::Synth)?;
    if let Some(path) = &cli.dump_netlist {
        write_json_dump(path, &netlist)?;
    }

    detect_cycles(&netlist).map_err(PipelineError::Cycle)?;

    // v2: static timing analysis (FR-V12). Errors map to exit code 9.
    // `--allow-timing-races` downgrades the race to a stderr warning.
    match analyse_timing(&netlist, &TimingConfig::DEFAULT) {
        Ok(_) => {}
        Err(err) => {
            let strict = cli.strict_timing || !cli.allow_timing_races;
            if strict {
                return Err(PipelineError::Timing(err));
            }
            eprintln!("[warn] timing race (use --strict-timing to fail): {err}");
        }
    }

    // v2: stage-boundary RAM-cap check (FR-V10). Maps to exit code 8.
    BudgetGuard::new(cli.max_ram, "after_timing")
        .check()
        .map_err(PipelineError::Memory)?;

    let placer_kind = match cli.placer {
        crate::cli::Placer::Sa => PlacerKind::Sa,
        crate::cli::Placer::Greedy => PlacerKind::Greedy,
    };
    let mut sa_cfg = SaConfig::DEFAULT;
    if let Some(seed) = cli.seed {
        sa_cfg.seed = seed;
    }
    let placement = place_with(placer_kind, &netlist, &cfg_from_cli(cli), &sa_cfg)
        .map_err(PipelineError::Place)?;
    if let Some(path) = &cli.dump_placement {
        write_json_dump(path, &placement)?;
    }

    BudgetGuard::new(cli.max_ram, "after_place")
        .check()
        .map_err(PipelineError::Memory)?;

    let wires = match cli.router {
        crate::cli::Router::Lee => {
            let route_cfg = route_cfg_from_cli(cli);
            route_pipeline_lee(&placement, &netlist, &route_cfg).map_err(PipelineError::Route)?
        }
        crate::cli::Router::Pathfinder => {
            // Pipeline mode: keep adjacency-isolation OFF by default
            // (paritetic with v1 Lee router) so dense macrocell
            // placements don't fail to converge. Stub-aware composition
            // (compose_two_stubs) flips this on explicitly.
            let pf_cfg = PathFinderConfig {
                max_iterations: cli.max_routing_iterations,
                max_explored_cells: 256_000,
                adjacency_isolation: false,
                ..PathFinderConfig::DEFAULT
            };
            // PathFinder is best-effort in pipeline mode: on convergence
            // failure we degrade to whatever single-pass routing
            // succeeds (matches Lee's "warn-and-emit" behaviour above).
            // A future strict-router flag could re-raise the error to
            // hit exit code 7 for batch correctness checks.
            match route_pipeline_pathfinder(&placement, &netlist, &pf_cfg) {
                Ok(w) => w,
                Err(RouteError::ConvergenceExhausted {
                    unrouted,
                    iterations,
                    peak_congestion,
                }) => {
                    eprintln!(
                        "[warn] pathfinder did not converge in {iterations} iters \
                         (peak_congestion={peak_congestion}); falling back to single-pass: \
                         {} unrouted/overused cells",
                        unrouted.len()
                    );
                    route_pipeline_pathfinder_singlepass(&placement, &netlist, &pf_cfg)
                        .map_err(PipelineError::Route)?
                }
                Err(e) => return Err(PipelineError::Route(e)),
            }
        }
    };
    let grid = assemble_grid(&placement, &wires);

    BudgetGuard::new(cli.max_ram, "after_route")
        .check()
        .map_err(PipelineError::Memory)?;

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
        peak_ram_bytes: BudgetGuard::current_rss_bytes(),
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
    cfg.initial_max_distance = cli.max_route_distance;
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

fn route_pipeline_lee(
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

/// Same source/sink discovery as the Lee path, but routes through the
/// v2 PathFinder with adjacency isolation. Used when `--router pathfinder`
/// is selected. Returns Err on `RouteError::ConvergenceExhausted` so the
/// CLI maps it to exit code 7 (FR-V16).
fn route_pipeline_pathfinder(
    placement: &Placement,
    netlist: &Netlist,
    cfg: &PathFinderConfig,
) -> Result<Vec<RoutedWire>, RouteError> {
    // Seed CostMap: every cell occupied by a placed macro-cell is a
    // hard obstacle (foreign nets must route around).
    let bounds = pad_bbox(placement.bounds, 4);
    let mut cost = CostMap::new(bounds);
    for cell in &placement.cells {
        for block in &cell.macro_cell.blocks {
            let p = Pos3::new(
                cell.origin.x + block.pos.x,
                cell.origin.y + block.pos.y,
                cell.origin.z + block.pos.z,
            );
            cost.mark_blocked(p);
        }
    }

    let mut routes: Vec<(NetTag, Pos3, Pos3)> = Vec::new();
    for &net_id in netlist.nets.keys() {
        let net_tag = NetTag(net_id.0);
        let Some(source_world) = net_source_world_pos(placement, netlist, net_id) else {
            continue;
        };
        for sink_world in net_sink_world_positions(placement, netlist, net_id) {
            routes.push((net_tag, source_world, sink_world));
        }
    }

    // Mark every source/sink as a pin so adjacency-isolation doesn't
    // lock the router out of its own anchors.
    for (_, src, sink) in &routes {
        cost.mark_pin(*src);
        cost.mark_pin(*sink);
    }

    route_pathfinder(&mut cost, &routes, cfg)
}

/// Single-pass A* fallback for PathFinder. Groups routes by net so each
/// net's fan-out is built as a Steiner tree: first sink → A* from source;
/// every subsequent sink → multi-source A* whose source set is the trunk
/// already laid (every cell of every previously routed branch of this
/// net). Mirrors Lee's `is_passable_for(own_net)` semantics — own-net
/// cells are free to reuse.
fn route_pipeline_pathfinder_singlepass(
    placement: &Placement,
    netlist: &Netlist,
    cfg: &PathFinderConfig,
) -> Result<Vec<RoutedWire>, RouteError> {
    use std::collections::BTreeSet;

    let bounds = pad_bbox(placement.bounds, 4);
    let mut cost = CostMap::new(bounds);
    for cell in &placement.cells {
        for block in &cell.macro_cell.blocks {
            let p = Pos3::new(
                cell.origin.x + block.pos.x,
                cell.origin.y + block.pos.y,
                cell.origin.z + block.pos.z,
            );
            cost.mark_blocked(p);
        }
    }

    // Group: net_tag → (source, vec_of_sinks).
    let mut groups: BTreeMap<NetTag, (Pos3, Vec<Pos3>)> = BTreeMap::new();
    for &net_id in netlist.nets.keys() {
        let net_tag = NetTag(net_id.0);
        let Some(src) = net_source_world_pos(placement, netlist, net_id) else {
            continue;
        };
        let sinks = net_sink_world_positions(placement, netlist, net_id);
        if sinks.is_empty() {
            continue;
        }
        groups.insert(net_tag, (src, sinks));
    }

    // Register every endpoint as a pin so future stub-aware adj_iso
    // doesn't lock A* out of its own anchors.
    for (src, sinks) in groups.values() {
        cost.mark_pin(*src);
        for &s in sinks {
            cost.mark_pin(s);
        }
    }

    let mut wires_by_net: BTreeMap<NetTag, Vec<RouteSegment>> = BTreeMap::new();
    let mut unrouted: Vec<String> = Vec::new();
    for (net, group) in &groups {
        let (src, sinks) = group;
        // Trunk grows as we route each sink. Start with the source only.
        let mut trunk: BTreeSet<Pos3> = BTreeSet::new();
        trunk.insert(*src);
        let mut net_segments: Vec<RouteSegment> = Vec::new();

        for &sink in sinks {
            let seeds: Vec<Pos3> = trunk.iter().copied().collect();
            let path =
                astar_route_multisrc(&cost, *net, &seeds, sink, &cfg.edge, cfg.max_explored_cells);
            match path {
                Some(p) if !p.is_empty() => {
                    for &cell in &p {
                        cost.assigned.insert(cell, *net);
                        if trunk.insert(cell) {
                            // Newly-grown trunk cell — emit it once.
                            net_segments.push(RouteSegment::Dust { pos: cell });
                        }
                    }
                }
                _ => unrouted.push(format!("net#{}→{:?}", net.0, sink)),
            }
        }
        if !net_segments.is_empty() {
            wires_by_net.insert(*net, net_segments);
        }
    }

    if !unrouted.is_empty() {
        eprintln!(
            "[warn] {} sink(s) left unrouted by pathfinder single-pass: {}",
            unrouted.len(),
            unrouted.join(", ")
        );
    }

    Ok(wires_by_net
        .into_iter()
        .map(|(net, segments)| RoutedWire { net, segments })
        .collect())
}

fn pad_bbox(b: Bbox3, n: i32) -> Bbox3 {
    Bbox3 {
        min: Pos3::new(b.min.x - n, b.min.y - n, b.min.z - n),
        max: Pos3::new(b.max.x + n, b.max.y + n, b.max.z + n),
    }
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
            // Phase A: dust/repeater require a solid block at y-1 for
            // support — expand bbox so the support cell fits.
            bbox = bbox.union(&Bbox3::point(Pos3::new(p.x, p.y - 1, p.z)));
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
            let mut state = block_state_for(block.block, block.facing);
            // v2: patch per-instance properties for user-instantiable
            // repeater (.DELAY/.LOCK) and comparator (.MODE).
            if matches!(block.block, BlockId::Repeater) {
                if let Some(delay) = cell.macro_cell.repeater_delay {
                    override_property(&mut state, "delay", &delay.to_string());
                }
            }
            if matches!(block.block, BlockId::Comparator) {
                if let Some(mode) = cell.macro_cell.comparator_mode {
                    let mode_str = match mode {
                        rb_parser::ast::CompareMode::Compare => "compare",
                        rb_parser::ast::CompareMode::Subtract => "subtract",
                    };
                    override_property(&mut state, "mode", mode_str);
                }
            }
            grid.insert(world, state);
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
            owned.insert(world, ());

            // Phase A: emit a Stone support block directly under the
            // wire/repeater so it doesn't float (real Minecraft physics).
            // Skip if a cell already owns that position — its block (a
            // solid planks/stone of the macrocell, or possibly another
            // wire of an earlier net that owns this cell) is good enough.
            let support = Pos3::new(world.x, world.y - 1, world.z);
            if let std::collections::btree_map::Entry::Vacant(e) = owned.entry(support) {
                grid.insert(support, block_state_for(BlockId::Stone, None));
                e.insert(());
            }
        }
    }

    let _ = Direction::ALL; // touch to keep import (avoid dead-code warn if unused)
    grid
}
