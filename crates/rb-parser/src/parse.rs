//! Pest-driven parse of an HDL source file into [`Module`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use miette::{NamedSource, SourceSpan as MietteSpan};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use rb_core::{GateKind, SignalKind, SourceSpan};
use smol_str::SmolStr;

use crate::ast::{
    ClockEdge, CompareMode, Connection, Edge, GateInst, Ident, Module, Port, PortDir, WireDecl,
};
use crate::error::ParseError;

#[derive(Parser)]
#[grammar = "hdl.pest"]
struct HdlParser;

/// Parse `source` (with `file_path` recorded for diagnostics) into an AST.
pub fn parse(source: &str, file_path: &Path) -> Result<Module, ParseError> {
    let file_arc: Arc<PathBuf> = Arc::new(file_path.to_path_buf());

    let mut pairs = HdlParser::parse(Rule::file, source).map_err(|err| {
        let (msg, at) = pest_error_to_span(&err, source);
        ParseError::Syntax {
            message: msg,
            at,
            src: NamedSource::new(file_display(file_path), source.to_string()),
        }
    })?;

    let file_pair = pairs.next().ok_or_else(|| ParseError::Syntax {
        message: "empty input — no top-level module found".into(),
        at: MietteSpan::from((0, 0)),
        src: NamedSource::new(file_display(file_path), source.to_string()),
    })?;

    let module_pair = file_pair
        .into_inner()
        .find(|p| matches!(p.as_rule(), Rule::module_decl))
        .ok_or_else(|| ParseError::Syntax {
            message: "expected a top-level module".into(),
            at: MietteSpan::from((0, 0)),
            src: NamedSource::new(file_display(file_path), source.to_string()),
        })?;

    Ok(build_module(module_pair, &file_arc, source))
}

fn build_module(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Module {
    let span = make_span(&pair, file, source);

    let mut name: Option<Ident> = None;
    let mut ports: Vec<Port> = Vec::new();
    let mut wires: Vec<WireDecl> = Vec::new();
    let mut instances: Vec<GateInst> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::kw_module | Rule::kw_endmodule => {}
            Rule::ident if name.is_none() => {
                name = Some(make_ident(inner, file, source));
            }
            Rule::port_list => {
                for port_pair in inner.into_inner() {
                    if matches!(port_pair.as_rule(), Rule::port_decl) {
                        ports.push(build_port(port_pair, file, source));
                    }
                }
            }
            Rule::wire_decl => {
                wires.extend(build_wires(inner, file, source));
            }
            Rule::gate_inst => {
                instances.push(build_gate_inst(inner, file, source));
            }
            Rule::comparator_inst => {
                instances.push(build_comparator_inst(inner, file, source));
            }
            Rule::observer_inst => {
                instances.push(build_observer_inst(inner, file, source));
            }
            Rule::always_block => {
                instances.extend(build_always_block(inner, file, source));
            }
            _ => {}
        }
    }

    Module {
        name: name.unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span)),
        ports,
        wires,
        instances,
        span,
    }
}

fn build_port(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Port {
    let span = make_span(&pair, file, source);
    let mut dir = PortDir::Input;
    let mut kind = SignalKind::Boolean;
    let mut name = placeholder_ident("<missing>", file, source, &span);

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::port_dir => {
                for kw in inner.into_inner() {
                    dir = match kw.as_rule() {
                        Rule::kw_input => PortDir::Input,
                        Rule::kw_output => PortDir::Output,
                        _ => dir,
                    };
                }
            }
            Rule::wire_kind => {
                for k in inner.into_inner() {
                    if matches!(k.as_rule(), Rule::kw_analog) {
                        kind = SignalKind::AnalogStrength;
                    }
                }
            }
            Rule::ident => {
                name = make_ident(inner, file, source);
            }
            _ => {}
        }
    }

    Port {
        name,
        dir,
        kind,
        span,
    }
}

fn build_wires(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Vec<WireDecl> {
    // First detect the wire_kind (looks for `kw_analog` token inside the
    // wire_kind rule). `wire X;` → Boolean (default); `analog wire X;`
    // → AnalogStrength.
    let mut kind = SignalKind::Boolean;
    for inner in pair.clone().into_inner() {
        if matches!(inner.as_rule(), Rule::wire_kind) {
            for k in inner.into_inner() {
                if matches!(k.as_rule(), Rule::kw_analog) {
                    kind = SignalKind::AnalogStrength;
                }
            }
        }
    }

    pair.into_inner()
        .filter(|p| matches!(p.as_rule(), Rule::ident))
        .map(|p| {
            let span = make_span(&p, file, source);
            let id = make_ident(p, file, source);
            WireDecl {
                name: id,
                kind,
                span,
            }
        })
        .collect()
}

fn build_gate_inst(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> GateInst {
    let span = make_span(&pair, file, source);
    let mut kind = GateKind::And;
    let mut inst_name = placeholder_ident("<missing>", file, source, &span);
    let mut connections: Vec<Connection> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::gate_kind => {
                if let Some(kw) = inner.into_inner().next() {
                    kind = match kw.as_rule() {
                        Rule::kw_and => GateKind::And,
                        Rule::kw_or => GateKind::Or,
                        Rule::kw_not => GateKind::Not,
                        Rule::kw_xor => GateKind::Xor,
                        Rule::kw_repeater => GateKind::Repeater,
                        Rule::kw_target_block => GateKind::TargetBlock,
                        _ => kind,
                    };
                }
            }
            Rule::ident => {
                inst_name = make_ident(inner, file, source);
            }
            Rule::conn_list => {
                for conn_pair in inner.into_inner() {
                    if matches!(conn_pair.as_rule(), Rule::conn) {
                        connections.push(build_conn(conn_pair, file, source));
                    }
                }
            }
            _ => {}
        }
    }

    // For repeater instances, extract optional `.DELAY(N)` from the
    // connection list — the validator enforces N ∈ 1..=4 (T014).
    let repeater_delay = if matches!(kind, GateKind::Repeater) {
        connections
            .iter()
            .find(|c| c.port.as_str().eq_ignore_ascii_case("DELAY"))
            .and_then(|c| c.net.as_str().parse::<u8>().ok())
            .or(Some(1))
    } else {
        None
    };

    GateInst {
        inst_name,
        kind,
        connections,
        clock: None,
        compare_mode: None,
        repeater_delay,
        span,
    }
}

fn build_comparator_inst(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> GateInst {
    let span = make_span(&pair, file, source);
    let mut inst_name = placeholder_ident("<missing>", file, source, &span);
    let mut connections: Vec<Connection> = Vec::new();
    let mut mode = CompareMode::Compare;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => {
                inst_name = make_ident(inner, file, source);
            }
            Rule::conn_list => {
                for conn_pair in inner.into_inner() {
                    if matches!(conn_pair.as_rule(), Rule::conn) {
                        connections.push(build_conn(conn_pair, file, source));
                    }
                }
            }
            Rule::mode_attr => {
                for kw in inner.into_inner() {
                    mode = match kw.as_rule() {
                        Rule::kw_subtract => CompareMode::Subtract,
                        Rule::kw_compare => CompareMode::Compare,
                        _ => mode,
                    };
                }
            }
            _ => {}
        }
    }

    GateInst {
        inst_name,
        kind: GateKind::Comparator,
        connections,
        clock: None,
        compare_mode: Some(mode),
        repeater_delay: None,
        span,
    }
}

fn build_observer_inst(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> GateInst {
    let span = make_span(&pair, file, source);
    let mut inst_name = placeholder_ident("<missing>", file, source, &span);
    let mut connections: Vec<Connection> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => {
                inst_name = make_ident(inner, file, source);
            }
            Rule::conn => {
                connections.push(build_conn(inner, file, source));
            }
            _ => {}
        }
    }

    GateInst {
        inst_name,
        kind: GateKind::Observer,
        connections,
        clock: None,
        compare_mode: None,
        repeater_delay: None,
        span,
    }
}

fn build_always_block(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Vec<GateInst> {
    let span = make_span(&pair, file, source);
    let mut clock: Option<ClockEdge> = None;
    let mut out: Vec<GateInst> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::edge_spec => {
                clock = Some(build_edge_spec(inner, file, source, &span));
            }
            Rule::stateful_inst => {
                out.push(build_stateful_inst(inner, file, source, clock.clone()));
            }
            _ => {}
        }
    }
    out
}

fn build_edge_spec(
    pair: Pair<'_, Rule>,
    file: &Arc<PathBuf>,
    source: &str,
    block_span: &SourceSpan,
) -> ClockEdge {
    let mut edge = Edge::Posedge;
    let mut clock_net = placeholder_ident("<missing>", file, source, block_span);
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::kw_posedge => edge = Edge::Posedge,
            Rule::kw_negedge => edge = Edge::Negedge,
            Rule::ident => clock_net = make_ident(inner, file, source),
            _ => {}
        }
    }
    ClockEdge {
        edge,
        clock_net,
        span: block_span.clone(),
    }
}

fn build_stateful_inst(
    pair: Pair<'_, Rule>,
    file: &Arc<PathBuf>,
    source: &str,
    clock: Option<ClockEdge>,
) -> GateInst {
    let span = make_span(&pair, file, source);
    let mut kind = GateKind::DTrigger;
    let mut inst_name = placeholder_ident("<missing>", file, source, &span);
    let mut connections: Vec<Connection> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::stateful_kind => {
                if let Some(kw) = inner.into_inner().next() {
                    kind = match kw.as_rule() {
                        Rule::kw_dtrigger => GateKind::DTrigger,
                        Rule::kw_memcell => GateKind::MemoryCell,
                        _ => kind,
                    };
                }
            }
            Rule::ident => {
                inst_name = make_ident(inner, file, source);
            }
            Rule::conn_list => {
                for conn_pair in inner.into_inner() {
                    if matches!(conn_pair.as_rule(), Rule::conn) {
                        connections.push(build_conn(conn_pair, file, source));
                    }
                }
            }
            _ => {}
        }
    }

    GateInst {
        inst_name,
        kind,
        connections,
        clock,
        compare_mode: None,
        repeater_delay: None,
        span,
    }
}

fn build_conn(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Connection {
    let span = make_span(&pair, file, source);
    let mut port: Option<Ident> = None;
    let mut net: Option<Ident> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => {
                let id = make_ident(inner, file, source);
                if port.is_none() {
                    port = Some(id);
                } else {
                    net = Some(id);
                }
            }
            Rule::conn_arg => {
                // The conn_arg wraps either an ident or an integer_lit.
                // For integer_lit, we synthesise an Ident whose text is
                // the literal — downstream code (e.g. repeater_delay
                // extraction) parses it as a u8.
                if let Some(arg) = inner.into_inner().next() {
                    let span = make_span(&arg, file, source);
                    let text = arg.as_str();
                    net = Some(Ident {
                        text: SmolStr::new(text),
                        span,
                    });
                }
            }
            _ => {}
        }
    }

    Connection {
        port: port.unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span)),
        net: net.unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span)),
        span,
    }
}

fn make_ident(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Ident {
    let span = make_span(&pair, file, source);
    Ident {
        text: SmolStr::new(pair.as_str()),
        span,
    }
}

fn placeholder_ident(text: &str, _file: &Arc<PathBuf>, _source: &str, span: &SourceSpan) -> Ident {
    Ident {
        text: SmolStr::new(text),
        span: span.clone(),
    }
}

fn make_span(pair: &Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> SourceSpan {
    let span = pair.as_span();
    let byte_start = to_u32_clamped(span.start());
    let byte_end = to_u32_clamped(span.end());
    let (line, column) = line_column_at(source, span.start());
    SourceSpan::new(file.clone(), byte_start, byte_end, line, column)
}

fn to_u32_clamped(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn line_column_at(source: &str, byte_offset: usize) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            col = 1;
        } else {
            col = col.saturating_add(1);
        }
    }
    (line, col)
}

fn pest_error_to_span(err: &pest::error::Error<Rule>, source: &str) -> (String, MietteSpan) {
    let msg = err.variant.message().to_string();
    let (start, end) = match err.location {
        pest::error::InputLocation::Pos(p) => (p, p.saturating_add(1).min(source.len())),
        pest::error::InputLocation::Span((s, e)) => (s, e),
    };
    let length = end.saturating_sub(start).max(1);
    (msg, MietteSpan::from((start, length)))
}

fn file_display(p: &Path) -> String {
    p.display().to_string()
}
