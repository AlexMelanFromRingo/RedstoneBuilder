//! Hierarchical elaboration — flatten a multi-module [`Design`] into a
//! single [`Module`] (V3.P5).
//!
//! Each sub-module instantiation `sub inst(.p(n), ...)` is inlined:
//! the sub-module's internal nets are renamed `inst.<net>`, its ports
//! are bound to the nets the instantiation connects them to, and its
//! gates are copied (with `inst.`-prefixed instance names) into the
//! parent. Recursion is depth-first; an instantiation cycle is a hard
//! error.
//!
//! A single-module file with no instantiations short-circuits to the
//! module unchanged, so flat v1/v2 designs are unaffected.

use std::collections::HashMap;
use std::path::Path;

use miette::{NamedSource, SourceSpan as MietteSpan};
use smol_str::SmolStr;

use crate::ast::{Connection, Design, GateInst, Ident, Module, ModuleInst, WireDecl};
use crate::error::ParseError;

/// Flatten `design` into its elaboration top as a single [`Module`].
pub fn elaborate(design: Design, file_path: &Path, source: &str) -> Result<Module, ParseError> {
    let Design { modules, top } = design;

    // Fast path: a lone flat module — nothing to inline.
    if let [only] = modules.as_slice() {
        if only.mod_instances.is_empty() {
            return Ok(only.clone());
        }
    }

    let by_name: HashMap<&str, &Module> = modules.iter().map(|m| (m.name.as_str(), m)).collect();

    let top_mod = &modules[top];
    let mut visiting: Vec<SmolStr> = Vec::new();
    let flat = flatten(top_mod, &by_name, &mut visiting, file_path, source)?;

    Ok(Module {
        name: top_mod.name.clone(),
        ports: flat.ports,
        wires: flat.wires,
        instances: flat.gates,
        mod_instances: Vec::new(),
        span: top_mod.span.clone(),
    })
}

/// Result of flattening one module: boundary ports plus the fully
/// inlined internal nets and gates.
struct Flat {
    ports: Vec<crate::ast::Port>,
    wires: Vec<WireDecl>,
    gates: Vec<GateInst>,
}

fn flatten(
    module: &Module,
    by_name: &HashMap<&str, &Module>,
    visiting: &mut Vec<SmolStr>,
    file_path: &Path,
    source: &str,
) -> Result<Flat, ParseError> {
    if visiting.iter().any(|n| n.as_str() == module.name.as_str()) {
        return Err(elab_err(
            format!(
                "recursive module instantiation: '{}' eventually instantiates itself",
                module.name.as_str()
            ),
            &module.name,
            file_path,
            source,
        ));
    }
    visiting.push(SmolStr::new(module.name.as_str()));

    let mut flat = Flat {
        ports: module.ports.clone(),
        wires: module.wires.clone(),
        gates: module.instances.clone(),
    };

    for mi in &module.mod_instances {
        let sub = by_name.get(mi.module_name.as_str()).ok_or_else(|| {
            elab_err(
                format!("unknown module '{}'", mi.module_name.as_str()),
                &mi.module_name,
                file_path,
                source,
            )
        })?;

        // Recurse first: get the sub-module fully flattened, then
        // rename its nets for this particular instance.
        let sub_flat = flatten(sub, by_name, visiting, file_path, source)?;

        let rename = build_rename(mi, &sub_flat, file_path, source)?;

        // Renamed internal wires (ports are the boundary — not copied;
        // they become whatever net the parent bound them to).
        for w in &sub_flat.wires {
            flat.wires.push(WireDecl {
                name: rename_ident(&w.name, &rename),
                kind: w.kind,
                span: w.span.clone(),
            });
        }
        // Renamed gates.
        for g in &sub_flat.gates {
            flat.gates.push(rename_gate(g, mi, &rename));
        }
    }

    visiting.pop();
    Ok(flat)
}

/// Build the net-rename map for one instantiation: each sub-module
/// port maps to the net the parent bound it to; each internal wire
/// maps to an `inst.`-prefixed name.
fn build_rename(
    mi: &ModuleInst,
    sub_flat: &Flat,
    file_path: &Path,
    source: &str,
) -> Result<HashMap<SmolStr, SmolStr>, ParseError> {
    let mut rename: HashMap<SmolStr, SmolStr> = HashMap::new();
    let inst = mi.inst_name.as_str();

    // Ports → bound external nets.
    for port in &sub_flat.ports {
        let bound = mi
            .connections
            .iter()
            .find(|c| c.port.as_str() == port.name.as_str())
            .map(|c| SmolStr::new(c.net.as_str()))
            // Unconnected port: keep it as a private prefixed net so
            // the design still elaborates (validate flags undriven).
            .unwrap_or_else(|| SmolStr::new(format!("{inst}.{}", port.name.as_str())));
        rename.insert(SmolStr::new(port.name.as_str()), bound);
    }
    // Internal wires → prefixed private nets.
    for w in &sub_flat.wires {
        rename.insert(
            SmolStr::new(w.name.as_str()),
            SmolStr::new(format!("{inst}.{}", w.name.as_str())),
        );
    }

    // Reject connections that name a port the sub-module doesn't have.
    for c in &mi.connections {
        let known = sub_flat
            .ports
            .iter()
            .any(|p| p.name.as_str() == c.port.as_str());
        if !known {
            return Err(elab_err(
                format!(
                    "instance '{}' connects unknown port '.{}'",
                    inst,
                    c.port.as_str()
                ),
                &c.port,
                file_path,
                source,
            ));
        }
    }

    Ok(rename)
}

/// Apply a rename map to an identifier's text; unknown names (integer
/// literals, parent-scope nets) pass through unchanged.
fn rename_ident(id: &Ident, rename: &HashMap<SmolStr, SmolStr>) -> Ident {
    match rename.get(id.text.as_str()) {
        Some(new) => Ident {
            text: new.clone(),
            span: id.span.clone(),
        },
        None => id.clone(),
    }
}

/// Clone a gate with renamed connection nets and an `inst.`-prefixed
/// instance name.
fn rename_gate(g: &GateInst, mi: &ModuleInst, rename: &HashMap<SmolStr, SmolStr>) -> GateInst {
    let inst = mi.inst_name.as_str();
    let connections: Vec<Connection> = g
        .connections
        .iter()
        .map(|c| Connection {
            port: c.port.clone(),
            net: rename_ident(&c.net, rename),
            span: c.span.clone(),
        })
        .collect();
    GateInst {
        inst_name: Ident {
            text: SmolStr::new(format!("{inst}.{}", g.inst_name.as_str())),
            span: g.inst_name.span.clone(),
        },
        kind: g.kind,
        connections,
        clock: g.clock.clone(),
        compare_mode: g.compare_mode,
        repeater_delay: g.repeater_delay,
        span: g.span.clone(),
    }
}

fn elab_err(message: String, at: &Ident, file_path: &Path, source: &str) -> ParseError {
    let len = (at.span.byte_end.saturating_sub(at.span.byte_start) as usize).max(1);
    ParseError::Elaboration {
        message,
        at: MietteSpan::from((at.span.byte_start as usize, len)),
        src: NamedSource::new(file_path.display().to_string(), source.to_string()),
    }
}
