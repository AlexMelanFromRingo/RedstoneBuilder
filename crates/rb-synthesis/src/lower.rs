//! Synthesis-time lowering passes that rewrite the AST before
//! [`build_netlist`] sees it. Specifically: complex gate primitives
//! that don't have a working physical macrocell get decomposed into
//! sequences of primitives that do.
//!
//! Currently:
//! - **XOR** → `(A AND !B) OR (!A AND B)`. Five sub-instances + four
//!   intermediate wires per XOR. Reuses our verified NOT/AND/OR cells.

use rb_core::{GateKind, SignalKind, SourceSpan};
use rb_parser::ast::{Connection, GateInst, Ident, Module, WireDecl};
use smol_str::SmolStr;

// (Note: rb_core::SourceSpan currently has no Default impl. We always
// pass a real span from the originating XOR instance, so synthetic
// nodes inherit useful spans for diagnostics.)

/// Rewrite every `xor` gate instance in `module` as
/// `(A AND NOT B) OR (NOT A AND B)`. Adds 5 sub-instances and 4 wires
/// per XOR. Idempotent — running twice produces no further changes
/// (since no XOR instances remain after the first pass).
pub fn lower_xor_gates(module: &mut Module) {
    let mut new_instances: Vec<GateInst> = Vec::with_capacity(module.instances.len());
    let mut new_wires: Vec<WireDecl> = Vec::new();

    for inst in module.instances.drain(..) {
        if inst.kind != GateKind::Xor {
            new_instances.push(inst);
            continue;
        }

        let span = inst.span.clone();
        let inst_name = inst.inst_name.text.clone();

        let a_net = port_net(&inst, "A").unwrap_or_else(|| missing(&inst_name, "A", &span));
        let b_net = port_net(&inst, "B").unwrap_or_else(|| missing(&inst_name, "B", &span));
        let y_net = port_net(&inst, "Y").unwrap_or_else(|| missing(&inst_name, "Y", &span));

        let na = synth_ident(&inst_name, "na", &span);
        let nb = synth_ident(&inst_name, "nb", &span);
        let ab = synth_ident(&inst_name, "ab", &span); // A AND !B
        let nba = synth_ident(&inst_name, "nba", &span); // !A AND B

        for w in [&na, &nb, &ab, &nba] {
            new_wires.push(WireDecl {
                name: w.clone(),
                kind: SignalKind::Boolean,
                span: span.clone(),
            });
        }

        // not g__not_a(.A(a), .Y(na))
        new_instances.push(make_inst(
            synth_ident(&inst_name, "not_a", &span),
            GateKind::Not,
            vec![
                conn("A", a_net.clone(), &span),
                conn("Y", na.clone(), &span),
            ],
            &span,
        ));
        // not g__not_b(.A(b), .Y(nb))
        new_instances.push(make_inst(
            synth_ident(&inst_name, "not_b", &span),
            GateKind::Not,
            vec![
                conn("A", b_net.clone(), &span),
                conn("Y", nb.clone(), &span),
            ],
            &span,
        ));
        // and g__and_ab(.A(a), .B(nb), .Y(ab))
        new_instances.push(make_inst(
            synth_ident(&inst_name, "and_ab", &span),
            GateKind::And,
            vec![
                conn("A", a_net.clone(), &span),
                conn("B", nb, &span),
                conn("Y", ab.clone(), &span),
            ],
            &span,
        ));
        // and g__and_ba(.A(na), .B(b), .Y(nba))
        new_instances.push(make_inst(
            synth_ident(&inst_name, "and_ba", &span),
            GateKind::And,
            vec![
                conn("A", na, &span),
                conn("B", b_net, &span),
                conn("Y", nba.clone(), &span),
            ],
            &span,
        ));
        // or g__or(.A(ab), .B(nba), .Y(y))
        new_instances.push(make_inst(
            synth_ident(&inst_name, "or", &span),
            GateKind::Or,
            vec![
                conn("A", ab, &span),
                conn("B", nba, &span),
                conn("Y", y_net, &span),
            ],
            &span,
        ));
    }

    module.instances = new_instances;
    module.wires.extend(new_wires);
}

fn port_net(inst: &GateInst, port: &str) -> Option<Ident> {
    inst.connections
        .iter()
        .find(|c| c.port.as_str().eq_ignore_ascii_case(port))
        .map(|c| c.net.clone())
}

fn missing(inst_name: &str, port: &str, span: &SourceSpan) -> Ident {
    // A malformed XOR (missing port) should already have been rejected
    // by the parser/validator. If we somehow get here we synthesise a
    // dummy name so the rest of the pipeline can still produce a
    // diagnostic instead of panicking.
    Ident {
        text: SmolStr::new(format!("__missing_{port}_in_{inst_name}__")),
        span: span.clone(),
    }
}

fn synth_ident(inst_name: &SmolStr, suffix: &str, span: &SourceSpan) -> Ident {
    Ident {
        text: SmolStr::new(format!("__xor_{inst_name}_{suffix}__")),
        span: span.clone(),
    }
}

fn make_inst(
    name: Ident,
    kind: GateKind,
    connections: Vec<Connection>,
    span: &SourceSpan,
) -> GateInst {
    GateInst {
        inst_name: name,
        kind,
        connections,
        clock: None,
        compare_mode: None,
        repeater_delay: None,
        span: span.clone(),
    }
}

fn conn(port: &str, net: Ident, span: &SourceSpan) -> Connection {
    Connection {
        port: Ident {
            text: SmolStr::new(port),
            span: span.clone(),
        },
        net,
        span: span.clone(),
    }
}
