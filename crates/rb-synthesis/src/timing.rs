//! Static timing analysis (v2 / FR-V12).
//!
//! Forward-pass topological walk over the combinational projection of
//! the netlist, accumulating per-cell tick delays. Detects data races
//! on data signals; clock-domain signals are excluded.

use std::collections::BTreeMap;

use miette::Diagnostic;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex as ProjNodeIndex;
use petgraph::stable_graph::NodeIndex;
use rb_core::{GateKind, Tick};
use thiserror::Error;

use crate::cell_library::macrocell_for;
use crate::cycle::build_projection;
use crate::netlist::{Netlist, NetlistNode};

/// Per-stage timing map.
#[derive(Debug, Clone, Default)]
pub struct TimingMap {
    /// Earliest tick at which a node's output is stable.
    pub arrival: BTreeMap<NodeIndex, Tick>,
}

/// Configuration knobs.
#[derive(Debug, Clone, Copy)]
pub struct TimingConfig {
    /// Per-data-net arrival-tick mismatch tolerance. Default 0.
    pub data_race_tolerance: u32,
}

impl TimingConfig {
    /// Conservative default: 0-tick tolerance — any mismatch is a race.
    pub const DEFAULT: Self = Self {
        data_race_tolerance: 0,
    };
}

/// Diagnostics produced by static timing.
#[derive(Debug, Error, Diagnostic)]
pub enum TimingError {
    /// Two or more paths converge on the same sink with mismatched
    /// arrival ticks — a data race per FR-V12.
    #[error("data race at sink (node {sink_index:?}): arrivals {arrivals:?} differ by more than {tolerance} tick(s)")]
    #[diagnostic(code(rb_synthesis::timing::data_race))]
    DataRace {
        /// Sink node index.
        sink_index: u32,
        /// Arrival ticks from each predecessor (sorted asc).
        arrivals: Vec<u32>,
        /// Active tolerance.
        tolerance: u32,
    },
}

/// Run forward-pass timing analysis on the combinational projection of
/// `netlist`. Returns a [`TimingMap`] on success, or a
/// [`TimingError::DataRace`] on first detected mismatch.
pub fn analyse_timing(netlist: &Netlist, cfg: &TimingConfig) -> Result<TimingMap, TimingError> {
    let (proj, back) = build_projection(netlist);
    let order = toposort(&proj, None).unwrap_or_default();

    let mut arrival: BTreeMap<NodeIndex, Tick> = BTreeMap::new();

    for p in order {
        let Some(&n) = back.get(&p) else { continue };
        let preds: Vec<ProjNodeIndex> = proj
            .neighbors_directed(p, petgraph::Direction::Incoming)
            .collect();

        if preds.is_empty() {
            arrival.insert(n, Tick::ZERO);
            continue;
        }

        let mut arrivals: Vec<u32> = Vec::with_capacity(preds.len());
        for pred_proj in &preds {
            let Some(&pred) = back.get(pred_proj) else {
                continue;
            };
            let pred_arrival = arrival.get(&pred).copied().unwrap_or(Tick::ZERO);
            let delay = node_tick_delay(&netlist.graph[pred]);
            arrivals.push(pred_arrival.saturating_add(delay).0);
        }

        // Data-race check on converging inputs (skip if this sink is a
        // module-output or a stateful node — those are allowed to skew).
        if arrivals.len() > 1 && is_data_sink(&netlist.graph[n]) {
            let min = arrivals.iter().min().copied().unwrap_or(0);
            let max = arrivals.iter().max().copied().unwrap_or(0);
            if max - min > cfg.data_race_tolerance {
                let mut sorted = arrivals.clone();
                sorted.sort();
                return Err(TimingError::DataRace {
                    sink_index: n.index() as u32,
                    arrivals: sorted,
                    tolerance: cfg.data_race_tolerance,
                });
            }
        }

        let worst = arrivals.iter().max().copied().unwrap_or(0);
        arrival.insert(n, Tick(worst));
    }

    Ok(TimingMap { arrival })
}

fn node_tick_delay(node: &NetlistNode) -> Tick {
    match node {
        NetlistNode::Gate { kind, .. } => match macrocell_for(*kind) {
            Ok(mc) => Tick(u32::from(mc.tick_delay)),
            Err(_) => Tick::ZERO,
        },
        // Module ports introduce no delay.
        _ => Tick::ZERO,
    }
}

fn is_data_sink(node: &NetlistNode) -> bool {
    matches!(
        node,
        NetlistNode::Gate {
            kind: GateKind::And
                | GateKind::Or
                | GateKind::Xor
                | GateKind::Not
                | GateKind::Comparator,
            ..
        }
    )
}
