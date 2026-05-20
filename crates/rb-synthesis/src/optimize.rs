//! Synthesis optimisation passes that run on the AST before netlist
//! construction (V3.P3).
//!
//! Currently one pass: dead-gate elimination. The synthesis-time
//! lowering ([`crate::lower`]) and gate→macrocell tech-mapping
//! ([`crate::cell_library::macrocell_for`]) are the other halves of
//! the "tech-mapper" — this module adds the optimiser that prunes
//! logic which cannot influence any module output.

use std::collections::HashSet;

use rb_core::GateKind;
use rb_parser::ast::{GateInst, Module, PortDir};

/// Combinational dead-gate elimination.
///
/// A gate is *live* iff its output net feeds — directly or transitively
/// — a module output port. Everything else can never be observed, so
/// it is removed (shrinking placement + routing work). Computed as a
/// backward fixpoint from the module's output ports.
///
/// Gates whose output port cannot be identified are conservatively
/// kept. Returns the number of gates removed.
pub fn prune_dead_gates(module: &mut Module) -> usize {
    // Seed: every module output port net is live.
    let mut live: HashSet<String> = module
        .ports
        .iter()
        .filter(|p| p.dir == PortDir::Output)
        .map(|p| p.name.as_str().to_string())
        .collect();

    // Fixpoint: a gate driving a live net makes all its *input* nets
    // live too.
    loop {
        let mut changed = false;
        for inst in &module.instances {
            let Some(out_port) = gate_output_port(inst.kind) else {
                continue;
            };
            let drives_live = inst.connections.iter().any(|c| {
                c.port.as_str().eq_ignore_ascii_case(out_port) && live.contains(c.net.as_str())
            });
            if !drives_live {
                continue;
            }
            for c in &inst.connections {
                if c.port.as_str().eq_ignore_ascii_case(out_port) {
                    continue; // the output net itself
                }
                if live.insert(c.net.as_str().to_string()) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Drop gates whose output net is not live. Gates with an
    // unrecognised output port are kept (conservative).
    let before = module.instances.len();
    module
        .instances
        .retain(|inst| match gate_output_port(inst.kind) {
            None => true,
            Some(out_port) => inst.connections.iter().any(|c| {
                c.port.as_str().eq_ignore_ascii_case(out_port) && live.contains(c.net.as_str())
            }),
        });
    before - module.instances.len()
}

/// Canonical output-port name for a gate kind, or `None` if unknown.
fn gate_output_port(kind: GateKind) -> Option<&'static str> {
    Some(match kind {
        GateKind::And | GateKind::Or | GateKind::Not | GateKind::Xor | GateKind::Comparator => "Y",
        GateKind::DTrigger | GateKind::MemoryCell => "Q",
        GateKind::Observer | GateKind::Repeater | GateKind::TargetBlock => "OUT",
        _ => return None,
    })
}

/// True iff `inst` drives `net` on its output port — handy for tests.
#[doc(hidden)]
pub fn gate_drives(inst: &GateInst, net: &str) -> bool {
    match gate_output_port(inst.kind) {
        None => false,
        Some(out_port) => inst
            .connections
            .iter()
            .any(|c| c.port.as_str().eq_ignore_ascii_case(out_port) && c.net.as_str() == net),
    }
}
