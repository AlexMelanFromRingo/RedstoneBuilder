//! Netlist graph built from a parsed [`Module`].
//!
//! Nodes are gate instances + module-boundary ports. Edges are wires,
//! each carrying a [`NetId`] assigned in **source order** — the v1
//! determinism anchor (FR-017).

use std::collections::BTreeMap;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use rb_core::{GateKind, SourceSpan};
use rb_parser::ast::{Connection, GateInst, Ident, Module, PortDir};
use serde::Serialize;

use crate::error::SynthError;

/// A net ID, assigned in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct NetId(pub u32);

/// Role that an endpoint plays on a particular [`NetlistNode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum EndpointRole {
    /// External module input — drives downstream nodes.
    ModuleInput,
    /// External module output — driven by exactly one source.
    ModuleOutput,
    /// Numbered data input on a combinational gate.
    DataIn(u8),
    /// Unique output of a combinational gate.
    DataOut,
    /// Clock input of a stateful gate (US2+).
    ClockIn,
    /// `D` or `DATA` input of a stateful gate (US2+).
    DataInStateful,
    /// `WRITE` enable of a memory cell (US2+).
    WriteEnable,
    /// `Q` output of a stateful gate (US2+).
    Q,
}

/// One node in the netlist graph.
#[derive(Debug, Clone, Serialize)]
pub enum NetlistNode {
    /// Module-boundary input port.
    Input {
        /// Port name.
        port: Ident,
        /// Span of the port declaration.
        span: SourceSpan,
    },
    /// Module-boundary output port.
    Output {
        /// Port name.
        port: Ident,
        /// Span of the port declaration.
        span: SourceSpan,
    },
    /// A gate instance.
    Gate {
        /// Instance name.
        inst: Ident,
        /// Primitive kind.
        kind: GateKind,
        /// Span of the instance declaration.
        span: SourceSpan,
    },
}

impl NetlistNode {
    /// Short label for diagnostics.
    pub fn label(&self) -> String {
        match self {
            NetlistNode::Input { port, .. } => format!("in:{}", port.as_str()),
            NetlistNode::Output { port, .. } => format!("out:{}", port.as_str()),
            NetlistNode::Gate { inst, kind, .. } => format!("{}:{}", kind.keyword(), inst.as_str()),
        }
    }
}

/// One edge in the netlist graph.
#[derive(Debug, Clone, Serialize)]
pub struct NetlistEdge {
    /// Which net this edge belongs to.
    pub net: NetId,
    /// Role on the source node.
    pub from: EndpointRole,
    /// Role on the sink node.
    pub to: EndpointRole,
}

/// Convenient type alias.
pub type NetlistGraph = StableDiGraph<NetlistNode, NetlistEdge>;

/// The full netlist: graph + per-net metadata.
#[derive(Debug, Clone, Serialize)]
pub struct Netlist {
    /// Underlying directed graph.
    pub graph: NetlistGraph,
    /// Per-net metadata, keyed by `NetId`. Ordered for determinism.
    pub nets: BTreeMap<NetId, NetInfo>,
}

/// Metadata about one net (wire/port-as-net).
#[derive(Debug, Clone, Serialize)]
pub struct NetInfo {
    /// Net name (the wire or port identifier text).
    pub name: String,
    /// Span of the declaration.
    pub span: SourceSpan,
}

/// Build a netlist from a parsed module.
///
/// `NetId`s are assigned in declaration order: input ports first (in
/// declaration order), then output ports (in declaration order), then
/// wires (in declaration order). This is the determinism anchor for
/// downstream stages.
pub fn build_netlist(module: &Module) -> Result<Netlist, SynthError> {
    let mut graph: NetlistGraph = StableDiGraph::new();
    let mut nets: BTreeMap<NetId, NetInfo> = BTreeMap::new();
    let mut net_id_by_name: BTreeMap<String, NetId> = BTreeMap::new();
    let mut node_for_input: BTreeMap<String, NodeIndex> = BTreeMap::new();
    let mut node_for_output: BTreeMap<String, NodeIndex> = BTreeMap::new();

    let mut next_id: u32 = 0;
    let mut assign_net = |name: &str,
                          span: &SourceSpan,
                          nets: &mut BTreeMap<NetId, NetInfo>,
                          by_name: &mut BTreeMap<String, NetId>| {
        if !by_name.contains_key(name) {
            let id = NetId(next_id);
            next_id = next_id.saturating_add(1);
            by_name.insert(name.to_string(), id);
            nets.insert(
                id,
                NetInfo {
                    name: name.to_string(),
                    span: span.clone(),
                },
            );
        }
    };

    for port in &module.ports {
        if port.dir == PortDir::Input {
            assign_net(
                port.name.as_str(),
                &port.span,
                &mut nets,
                &mut net_id_by_name,
            );
            let idx = graph.add_node(NetlistNode::Input {
                port: port.name.clone(),
                span: port.span.clone(),
            });
            node_for_input.insert(port.name.as_str().to_string(), idx);
        }
    }
    for port in &module.ports {
        if port.dir == PortDir::Output {
            assign_net(
                port.name.as_str(),
                &port.span,
                &mut nets,
                &mut net_id_by_name,
            );
            let idx = graph.add_node(NetlistNode::Output {
                port: port.name.clone(),
                span: port.span.clone(),
            });
            node_for_output.insert(port.name.as_str().to_string(), idx);
        }
    }
    for wire in &module.wires {
        assign_net(
            wire.name.as_str(),
            &wire.span,
            &mut nets,
            &mut net_id_by_name,
        );
    }

    let mut gate_nodes: Vec<(NodeIndex, &GateInst)> = Vec::with_capacity(module.instances.len());
    for inst in &module.instances {
        let idx = graph.add_node(NetlistNode::Gate {
            inst: inst.inst_name.clone(),
            kind: inst.kind,
            span: inst.span.clone(),
        });
        gate_nodes.push((idx, inst));
    }

    for (gate_idx, inst) in &gate_nodes {
        for conn in &inst.connections {
            let role = port_role(inst.kind, conn);
            let net_id = match net_id_by_name.get(conn.net.as_str()) {
                Some(id) => *id,
                None => continue,
            };

            if role.is_output() {
                let driver = *gate_idx;
                // Note: we do NOT skip self-edges here. A combinational
                // self-loop (e.g. `not g(.A(w), .Y(w));`) is a real
                // cycle that detect_cycles must catch; a stateful
                // self-loop (e.g. `dtrigger ff(.D(w), .CLK(clk), .Q(w));`)
                // is legal and is cut by the cycle-detection projection.
                for (sink_idx, sink_inst) in &gate_nodes {
                    for sink_conn in &sink_inst.connections {
                        if sink_conn.net.as_str() == conn.net.as_str() {
                            let sink_role = port_role(sink_inst.kind, sink_conn);
                            if !sink_role.is_output() {
                                graph.add_edge(
                                    driver,
                                    *sink_idx,
                                    NetlistEdge {
                                        net: net_id,
                                        from: EndpointRole::DataOut,
                                        to: sink_role,
                                    },
                                );
                            }
                        }
                    }
                }
                if let Some(out_idx) = node_for_output.get(conn.net.as_str()) {
                    graph.add_edge(
                        driver,
                        *out_idx,
                        NetlistEdge {
                            net: net_id,
                            from: EndpointRole::DataOut,
                            to: EndpointRole::ModuleOutput,
                        },
                    );
                }
            } else if let Some(in_idx) = node_for_input.get(conn.net.as_str()) {
                graph.add_edge(
                    *in_idx,
                    *gate_idx,
                    NetlistEdge {
                        net: net_id,
                        from: EndpointRole::ModuleInput,
                        to: role,
                    },
                );
            }
        }
    }

    Ok(Netlist { graph, nets })
}

impl EndpointRole {
    /// True iff this role is on the *output* side of its node.
    pub fn is_output(self) -> bool {
        matches!(
            self,
            EndpointRole::DataOut | EndpointRole::Q | EndpointRole::ModuleInput
        )
    }
}

fn port_role(kind: GateKind, conn: &Connection) -> EndpointRole {
    let p = conn.port.as_str();
    match kind {
        GateKind::Not => {
            if p.eq_ignore_ascii_case("Y") {
                EndpointRole::DataOut
            } else {
                EndpointRole::DataIn(0)
            }
        }
        GateKind::And | GateKind::Or | GateKind::Xor => {
            if p.eq_ignore_ascii_case("Y") {
                EndpointRole::DataOut
            } else if p.eq_ignore_ascii_case("A") {
                EndpointRole::DataIn(0)
            } else if p.eq_ignore_ascii_case("B") {
                EndpointRole::DataIn(1)
            } else {
                EndpointRole::DataIn(0)
            }
        }
        GateKind::DTrigger => {
            if p.eq_ignore_ascii_case("Q") {
                EndpointRole::Q
            } else if p.eq_ignore_ascii_case("CLK") {
                EndpointRole::ClockIn
            } else {
                EndpointRole::DataInStateful
            }
        }
        GateKind::MemoryCell => {
            if p.eq_ignore_ascii_case("Q") {
                EndpointRole::Q
            } else if p.eq_ignore_ascii_case("WRITE") {
                EndpointRole::WriteEnable
            } else {
                EndpointRole::DataInStateful
            }
        }
        _ => EndpointRole::DataIn(0),
    }
}
