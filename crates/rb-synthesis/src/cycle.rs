//! Combinational-cycle detection (FR-005).
//!
//! Algorithm: build a *combinational projection* of the netlist that
//! removes incoming edges to stateful primitives' data inputs
//! (`DataInStateful`, `WriteEnable`). On that projection, an SCC of
//! size > 1 (or a self-loop) is a purely-combinational cycle and
//! illegal. Sequential feedback through `D-Trigger` / `Memory Cell` is
//! cut by the projection and so does not appear as a cycle.

use std::collections::BTreeMap;

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex as ProjNodeIndex};
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rb_core::GateKind;

use crate::error::CycleError;
use crate::netlist::{EndpointRole, NetId, Netlist, NetlistNode};

/// Run cycle detection on `netlist`. Returns `Ok(())` if the
/// combinational projection is a DAG, else a
/// [`CycleError::Combinational`] naming the wires that form the
/// offending cycle.
pub fn detect_cycles(netlist: &Netlist) -> Result<(), CycleError> {
    let (proj, _back) = build_projection(netlist);

    for scc in tarjan_scc(&proj) {
        let has_real_cycle = scc.len() > 1
            || (scc.len() == 1 && {
                let n = scc[0];
                proj.edges(n).any(|e| e.target() == n)
            });
        if !has_real_cycle {
            continue;
        }

        let mut wires: Vec<String> = scc
            .iter()
            .flat_map(|&node| {
                proj.edges(node).filter_map(|e| {
                    if scc.contains(&e.target()) {
                        netlist.nets.get(e.weight()).map(|info| info.name.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        wires.sort();
        wires.dedup();
        if wires.is_empty() {
            wires.push("<unknown>".to_string());
        }
        return Err(CycleError::Combinational { wires });
    }
    Ok(())
}

/// Build the combinational projection of a netlist.
///
/// Exposed for reuse by the placer, which needs a DAG for topological
/// ordering even when the source netlist has sequential feedback loops.
pub(crate) fn build_projection(
    netlist: &Netlist,
) -> (
    DiGraph<NodeIndex, NetId>,
    BTreeMap<ProjNodeIndex, NodeIndex>,
) {
    let mut proj: DiGraph<NodeIndex, NetId> = DiGraph::new();
    let mut fwd: BTreeMap<NodeIndex, ProjNodeIndex> = BTreeMap::new();
    let mut back: BTreeMap<ProjNodeIndex, NodeIndex> = BTreeMap::new();

    for n in netlist.graph.node_indices() {
        let p = proj.add_node(n);
        fwd.insert(n, p);
        back.insert(p, n);
    }

    for edge_idx in netlist.graph.edge_indices() {
        let w = &netlist.graph[edge_idx];
        let Some((src, dst)) = netlist.graph.edge_endpoints(edge_idx) else {
            continue;
        };
        if is_cycle_cutting(&netlist.graph[dst], w.to) {
            continue;
        }
        if let (Some(&ps), Some(&pd)) = (fwd.get(&src), fwd.get(&dst)) {
            proj.add_edge(ps, pd, w.net);
        }
    }

    (proj, back)
}

fn is_cycle_cutting(dst: &NetlistNode, to_role: EndpointRole) -> bool {
    let NetlistNode::Gate { kind, .. } = dst else {
        return false;
    };
    if !kind_is_stateful(*kind) {
        // v2: RepeaterLock is also a stateful cycle-cut even though
        // the host gate (Repeater) is not itself "stateful" — the
        // lock semantics make a Q←D feedback through lock legal.
        return matches!(to_role, EndpointRole::RepeaterLock);
    }
    matches!(
        to_role,
        EndpointRole::DataInStateful
            | EndpointRole::WriteEnable
            | EndpointRole::ObserverWatch
            | EndpointRole::RepeaterLock
    )
}

fn kind_is_stateful(k: GateKind) -> bool {
    k.is_stateful()
}
