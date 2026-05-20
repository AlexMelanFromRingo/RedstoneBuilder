//! Semantic validation of a parsed [`Module`].
//!
//! Runs the FR-004 checks: every referenced net is declared, every
//! output port is driven exactly once, no duplicate identifiers, every
//! gate instance has the right ports. Collects all errors in one pass.

use std::collections::HashMap;

use miette::{NamedSource, SourceSpan as MietteSpan};
use rb_core::{GateKind, SignalKind, SourceSpan};

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

    // Build a SignalKind lookup keyed by net name so we can check
    // analog/boolean mismatches without per-connection re-traversal.
    let mut net_kind: HashMap<&str, SignalKind> = HashMap::new();
    for port in &module.ports {
        net_kind.insert(port.name.as_str(), port.kind);
    }
    for wire in &module.wires {
        net_kind.insert(wire.name.as_str(), wire.kind);
    }

    for inst in &module.instances {
        for conn in &inst.connections {
            // Integer-literal targets (e.g. `.DELAY(3)` on a repeater)
            // are not nets and must not trigger UndeclaredNet.
            if is_integer_literal(conn.net.as_str()) {
                continue;
            }
            if !declared.contains_key(conn.net.as_str()) {
                errors.push(SemanticError::UndeclaredNet {
                    name: conn.net.as_str().to_string(),
                    at: span_to_miette(&conn.net.span),
                    src: NamedSource::new(&file_name, source.to_string()),
                });
            }
        }
        check_gate_ports(inst, &mut errors, source, &file_name);
        check_signal_kind(inst, &net_kind, &mut errors, source, &file_name);
        check_repeater_delay(inst, &mut errors, source, &file_name);
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

/// The canonical name of the *output* port for each gate kind.
fn output_port_name(kind: GateKind) -> &'static str {
    match kind {
        GateKind::And | GateKind::Or | GateKind::Not | GateKind::Xor | GateKind::Comparator => "Y",
        GateKind::DTrigger | GateKind::MemoryCell => "Q",
        GateKind::Observer | GateKind::Repeater | GateKind::TargetBlock => "OUT",
        // Forward-compat: any future GateKind variant gets "Y" by
        // default; validate stays running (may misattribute drivers).
        _ => "Y",
    }
}

/// v2: enforce that a wire's declared SignalKind matches the kind the
/// instantiating gate expects on each port.
fn check_signal_kind(
    inst: &GateInst,
    net_kind: &HashMap<&str, SignalKind>,
    errors: &mut Vec<SemanticError>,
    source: &str,
    file_name: &str,
) {
    for conn in &inst.connections {
        // Skip integer-literal targets (e.g. .DELAY(3) on a repeater).
        if is_integer_literal(conn.net.as_str()) {
            continue;
        }
        let Some(&found) = net_kind.get(conn.net.as_str()) else {
            continue; // UndeclaredNet already reported by the caller.
        };
        let expected = expected_signal_kind(inst.kind, conn.port.as_str());
        if !signal_kinds_compatible(expected, found) {
            errors.push(SemanticError::SignalKindMismatch {
                name: conn.net.as_str().to_string(),
                inst: inst.inst_name.as_str().to_string(),
                found: format!("{found:?}"),
                expected: format!("{expected:?}"),
                at: span_to_miette(&conn.net.span),
                src: NamedSource::new(file_name, source.to_string()),
            });
        }
    }
}

fn is_integer_literal(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn expected_signal_kind(kind: GateKind, _port: &str) -> SignalKind {
    // For v2 the only kind-check we enforce is: analog wires must not
    // be silently fed into strictly-boolean combinational gates. So
    // boolean-only gates expect Boolean; comparator (the analog
    // primitive) expects AnalogStrength. Everything else (observer,
    // repeater, target_block, stateful primitives) is treated as
    // kind-agnostic for v2 — they accept any wire kind.
    match kind {
        GateKind::Comparator => SignalKind::AnalogStrength,
        GateKind::And | GateKind::Or | GateKind::Not | GateKind::Xor => SignalKind::Boolean,
        _ => SignalKind::Boolean, // permissive: kind-mismatch never fires for non-bool gates
    }
}

/// True iff a wire carrying `found` is allowed at a port expecting
/// `expected`. Only one strict rejection in v2: an analog wire feeding
/// a strictly-boolean gate input — the gate would silently lose the
/// analog information. All other cross-kind connections (boolean →
/// analog, edge → boolean, etc.) are accepted.
fn signal_kinds_compatible(expected: SignalKind, found: SignalKind) -> bool {
    !matches!(
        (expected, found),
        (SignalKind::Boolean, SignalKind::AnalogStrength)
    )
}

/// v2: enforce repeater `.DELAY(N)` is in `1..=4`.
fn check_repeater_delay(
    inst: &GateInst,
    errors: &mut Vec<SemanticError>,
    source: &str,
    file_name: &str,
) {
    if inst.kind != GateKind::Repeater {
        return;
    }
    let Some(delay) = inst.repeater_delay else {
        return;
    };
    if !(1..=4).contains(&delay) {
        errors.push(SemanticError::BadDelay {
            inst: inst.inst_name.as_str().to_string(),
            actual: u32::from(delay),
            at: span_to_miette(&inst.span),
            src: NamedSource::new(file_name, source.to_string()),
        });
    }
}

fn check_gate_ports(
    inst: &GateInst,
    errors: &mut Vec<SemanticError>,
    source: &str,
    file_name: &str,
) {
    let (required_inputs, output, optional_inputs): (Vec<&str>, &str, &[&str]) = match inst.kind {
        GateKind::Not => (vec!["A"], "Y", &[]),
        GateKind::And | GateKind::Or | GateKind::Xor => (vec!["A", "B"], "Y", &[]),
        GateKind::DTrigger => (vec!["D", "CLK"], "Q", &[]),
        GateKind::MemoryCell => (vec!["DATA", "WRITE"], "Q", &[]),
        GateKind::Comparator => (vec!["A", "B"], "Y", &[]),
        GateKind::Observer => (vec!["WATCH"], "OUT", &[]),
        GateKind::Repeater => (vec!["IN"], "OUT", &["DELAY", "LOCK"]),
        GateKind::TargetBlock => (vec!["IN"], "OUT", &[]),
        _ => return,
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
        } else if optional_inputs
            .iter()
            .any(|opt| opt.eq_ignore_ascii_case(p))
        {
            // Optional port — present but not required. No error.
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
