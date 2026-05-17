//! Semantic validation of a parsed [`Module`].
//!
//! Runs the FR-004 checks: every referenced net is declared, every
//! output port is driven exactly once, no duplicate identifiers, every
//! gate instance has the right ports. Collects all errors in one pass.

use std::collections::HashMap;

use miette::{NamedSource, SourceSpan as MietteSpan};
use rb_core::{GateKind, SourceSpan};

use crate::ast::{GateInst, Ident, Module, PortDir};
use crate::error::SemanticError;

/// Validate a parsed [`Module`]. Returns `Ok(())` if clean, or every
/// found problem if not.
pub fn validate(module: &Module, source: &str) -> Result<(), Vec<SemanticError>> {
    let mut errors: Vec<SemanticError> = Vec::new();
    let file_name = module.name.span.file.display().to_string();

    let mut declared: HashMap<&str, &SourceSpan> = HashMap::new();
    for port in &module.ports {
        check_duplicate(&port.name, &mut declared, &mut errors, source, &file_name);
    }
    for wire in &module.wires {
        check_duplicate(&wire.name, &mut declared, &mut errors, source, &file_name);
    }

    for inst in &module.instances {
        for conn in &inst.connections {
            if !declared.contains_key(conn.net.as_str()) {
                errors.push(SemanticError::UndeclaredNet {
                    name: conn.net.as_str().to_string(),
                    at: span_to_miette(&conn.net.span),
                    src: NamedSource::new(&file_name, source.to_string()),
                });
            }
        }
        check_gate_ports(inst, &mut errors, source, &file_name);
    }

    let drivers = count_drivers(module);
    for port in &module.ports {
        if port.dir == PortDir::Output {
            let n = *drivers.get(port.name.as_str()).unwrap_or(&0);
            if n == 0 {
                errors.push(SemanticError::UndrivenOutput {
                    name: port.name.as_str().to_string(),
                    at: span_to_miette(&port.span),
                    src: NamedSource::new(&file_name, source.to_string()),
                });
            } else if n > 1 {
                errors.push(SemanticError::MultiplyDrivenOutput {
                    name: port.name.as_str().to_string(),
                    count: n,
                    at: span_to_miette(&port.span),
                    src: NamedSource::new(&file_name, source.to_string()),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_duplicate<'a>(
    ident: &'a Ident,
    declared: &mut HashMap<&'a str, &'a SourceSpan>,
    errors: &mut Vec<SemanticError>,
    source: &str,
    file_name: &str,
) {
    if declared.contains_key(ident.as_str()) {
        errors.push(SemanticError::DuplicateIdent {
            name: ident.as_str().to_string(),
            at: span_to_miette(&ident.span),
            src: NamedSource::new(file_name, source.to_string()),
        });
    } else {
        declared.insert(ident.as_str(), &ident.span);
    }
}

fn count_drivers(module: &Module) -> HashMap<String, usize> {
    let mut drivers: HashMap<String, usize> = HashMap::new();
    for inst in &module.instances {
        let output_port = output_port_name(inst.kind);
        for conn in &inst.connections {
            if conn.port.as_str().eq_ignore_ascii_case(output_port) {
                *drivers.entry(conn.net.as_str().to_string()).or_insert(0) += 1;
            }
        }
    }
    drivers
}

/// The canonical name of the *output* port for each combinational kind.
fn output_port_name(kind: GateKind) -> &'static str {
    match kind {
        GateKind::And | GateKind::Or | GateKind::Not | GateKind::Xor => "Y",
        GateKind::DTrigger | GateKind::MemoryCell => "Q",
        // Forward-compat: any future GateKind variant needs its own
        // output-port name; we treat unknowns as "Y" so validation
        // still runs (it just may misattribute drivers). The synthesizer
        // is the real gate.
        _ => "Y",
    }
}

fn check_gate_ports(
    inst: &GateInst,
    errors: &mut Vec<SemanticError>,
    source: &str,
    file_name: &str,
) {
    let (required_inputs, output) = match inst.kind {
        GateKind::Not => (vec!["A"], "Y"),
        GateKind::And | GateKind::Or | GateKind::Xor => (vec!["A", "B"], "Y"),
        GateKind::DTrigger => (vec!["D", "CLK"], "Q"),
        GateKind::MemoryCell => (vec!["DATA", "WRITE"], "Q"),
        _ => return, // Post-MVP primitives (e.g., Comparator, Observer)
    };

    let mut seen_inputs: Vec<&str> = Vec::new();
    let mut seen_output = false;
    let mut unknown: Vec<&str> = Vec::new();

    for conn in &inst.connections {
        let p = conn.port.as_str();
        if p.eq_ignore_ascii_case(output) {
            seen_output = true;
        } else if required_inputs
            .iter()
            .any(|req| req.eq_ignore_ascii_case(p))
        {
            seen_inputs.push(p);
        } else {
            unknown.push(p);
        }
    }

    let mut problems: Vec<String> = Vec::new();
    for req in &required_inputs {
        if !seen_inputs.iter().any(|p| p.eq_ignore_ascii_case(req)) {
            problems.push(format!("missing input port '{req}'"));
        }
    }
    if !seen_output {
        problems.push(format!("missing output port '{output}'"));
    }
    for u in unknown {
        problems.push(format!("unknown port '{u}'"));
    }

    if !problems.is_empty() {
        errors.push(SemanticError::BadPort {
            inst: inst.inst_name.as_str().to_string(),
            detail: problems.join("; "),
            at: span_to_miette(&inst.span),
            src: NamedSource::new(file_name, source.to_string()),
        });
    }
}

fn span_to_miette(span: &SourceSpan) -> MietteSpan {
    let len = span.len() as usize;
    MietteSpan::from((span.byte_start as usize, len.max(1)))
}
