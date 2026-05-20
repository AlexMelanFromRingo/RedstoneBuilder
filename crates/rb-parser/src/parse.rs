//! Pest-driven parse of an HDL source file into [`Module`].

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use miette::{NamedSource, SourceSpan as MietteSpan};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use rb_core::{GateKind, SignalKind, SourceSpan};
use smol_str::SmolStr;

use crate::ast::{
    ClockEdge, CompareMode, Connection, Design, Edge, GateInst, Ident, Module, ModuleInst, Port,
    PortDir, WireDecl,
};
use crate::elaborate::elaborate;
use crate::error::ParseError;

#[derive(Parser)]
#[grammar = "hdl.pest"]
struct HdlParser;

/// Parse `source` (with `file_path` recorded for diagnostics) into a
/// single flattened [`Module`]. Multi-module files are elaborated:
/// sub-module instantiations are inlined into the top module.
pub fn parse(source: &str, file_path: &Path) -> Result<Module, ParseError> {
    let design = parse_design(source, file_path)?;
    elaborate(design, file_path, source)
}

/// Parse `source` into a [`Design`] — every module definition in the
/// file, plus the index of the elaboration top. Exposed for tooling
/// that wants the un-elaborated hierarchy; the pipeline uses [`parse`].
pub fn parse_design(source: &str, file_path: &Path) -> Result<Design, ParseError> {
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

    let modules: Vec<Module> = file_pair
        .into_inner()
        .filter(|p| matches!(p.as_rule(), Rule::module_decl))
        .map(|p| build_module(p, &file_arc, source))
        .collect();

    if modules.is_empty() {
        return Err(ParseError::Syntax {
            message: "expected at least one module".into(),
            at: MietteSpan::from((0, 0)),
            src: NamedSource::new(file_display(file_path), source.to_string()),
        });
    }

    let top = pick_top(&modules);
    Ok(Design { modules, top })
}

/// The elaboration top is the last module not instantiated by any
/// other module. If every module is instantiated (a cycle), falls
/// back to the last module — elaboration will then report the cycle.
fn pick_top(modules: &[Module]) -> usize {
    let instantiated: HashSet<&str> = modules
        .iter()
        .flat_map(|m| m.mod_instances.iter())
        .map(|mi| mi.module_name.as_str())
        .collect();
    modules
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| !instantiated.contains(m.name.as_str()))
        .map(|(i, _)| i)
        .unwrap_or(modules.len() - 1)
}

fn build_module(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Module {
    let span = make_span(&pair, file, source);

    let mut name: Option<Ident> = None;
    let mut ports: Vec<Port> = Vec::new();
    let mut wires: Vec<WireDecl> = Vec::new();
    let mut instances: Vec<GateInst> = Vec::new();
    let mut mod_instances: Vec<ModuleInst> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::kw_module | Rule::kw_endmodule => {}
            Rule::ident if name.is_none() => {
                name = Some(make_ident(inner, file, source));
            }
            Rule::port_list => {
                for port_pair in inner.into_inner() {
                    if matches!(port_pair.as_rule(), Rule::port_decl) {
                        ports.extend(build_ports(port_pair, file, source));
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
            Rule::module_inst => {
                mod_instances.push(build_module_inst(inner, file, source));
            }
            _ => {}
        }
    }

    Module {
        name: name.unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span)),
        ports,
        wires,
        instances,
        mod_instances,
        span,
    }
}

/// Build one `module_inst` — `<module> <inst>(.port(net), ...)`. The
/// rule yields two leading `ident`s (module name, instance name) then
/// a `conn_list`.
fn build_module_inst(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> ModuleInst {
    let span = make_span(&pair, file, source);
    let mut idents: Vec<Ident> = Vec::new();
    let mut connections: Vec<Connection> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => idents.push(make_ident(inner, file, source)),
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

    let module_name = idents
        .first()
        .cloned()
        .unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span));
    let inst_name = idents
        .get(1)
        .cloned()
        .unwrap_or_else(|| placeholder_ident("<missing>", file, source, &span));

    ModuleInst {
        module_name,
        inst_name,
        connections,
        span,
    }
}

/// Build the port(s) for one `port_decl`. A scalar declaration yields
/// exactly one [`Port`]; a bus declaration (`input [3:0] a`) desugars
/// into one [`Port`] per bit, named `a[0]..a[3]`.
fn build_ports(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Vec<Port> {
    let span = make_span(&pair, file, source);
    let mut dir = PortDir::Input;
    let mut kind = SignalKind::Boolean;
    let mut bus: Option<(u32, u32)> = None;
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
            Rule::bus_spec => {
                bus = Some(parse_bus_spec(&inner));
            }
            Rule::ident => {
                name = make_ident(inner, file, source);
            }
            _ => {}
        }
    }

    expand_bus_names(&name, bus)
        .into_iter()
        .map(|bit_name| Port {
            name: bit_name,
            dir,
            kind,
            span: span.clone(),
        })
        .collect()
}

fn build_wires(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Vec<WireDecl> {
    // Detect the wire_kind (`kw_analog` token) and an optional bus_spec.
    // `wire X;` → Boolean scalar; `analog wire X;` → AnalogStrength;
    // `wire [3:0] X;` → 4 scalar nets X[0]..X[3].
    let mut kind = SignalKind::Boolean;
    let mut bus: Option<(u32, u32)> = None;
    for inner in pair.clone().into_inner() {
        match inner.as_rule() {
            Rule::wire_kind => {
                for k in inner.into_inner() {
                    if matches!(k.as_rule(), Rule::kw_analog) {
                        kind = SignalKind::AnalogStrength;
                    }
                }
            }
            Rule::bus_spec => {
                bus = Some(parse_bus_spec(&inner));
            }
            _ => {}
        }
    }

    let mut out: Vec<WireDecl> = Vec::new();
    for p in pair
        .into_inner()
        .filter(|p| matches!(p.as_rule(), Rule::ident))
    {
        let id = make_ident(p, file, source);
        for bit_name in expand_bus_names(&id, bus) {
            let span = bit_name.span.clone();
            out.push(WireDecl {
                name: bit_name,
                kind,
                span,
            });
        }
    }
    out
}

/// Parse a `bus_spec` rule (`[msb:lsb]`) into an inclusive `(lo, hi)`
/// pair. Either endianness is accepted — the smaller endpoint is `lo`.
fn parse_bus_spec(pair: &Pair<'_, Rule>) -> (u32, u32) {
    let mut nums = pair
        .clone()
        .into_inner()
        .filter(|p| matches!(p.as_rule(), Rule::integer_lit))
        .filter_map(|p| p.as_str().parse::<u32>().ok());
    let a = nums.next().unwrap_or(0);
    let b = nums.next().unwrap_or(0);
    (a.min(b), a.max(b))
}

/// Expand a (possibly bus) base name into the concrete per-bit net
/// identifiers. A scalar (`bus == None`) yields just `[base]`; a bus
/// yields `base[lo]..base[hi]`. Each synthesised identifier keeps the
/// base identifier's span for diagnostics.
fn expand_bus_names(base: &Ident, bus: Option<(u32, u32)>) -> Vec<Ident> {
    match bus {
        None => vec![base.clone()],
        Some((lo, hi)) => (lo..=hi)
            .map(|i| Ident {
                text: SmolStr::new(format!("{}[{i}]", base.as_str())),
                span: base.span.clone(),
            })
            .collect(),
    }
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
                // conn_arg wraps either an integer_lit or a net_ref.
                // integer_lit → an Ident whose text is the literal
                // (downstream parses it, e.g. repeater .DELAY(N)).
                // net_ref → a scalar net name, with a bit index folded
                // into the name as `bus[i]`.
                if let Some(arg) = inner.into_inner().next() {
                    match arg.as_rule() {
                        Rule::integer_lit => {
                            let span = make_span(&arg, file, source);
                            net = Some(Ident {
                                text: SmolStr::new(arg.as_str()),
                                span,
                            });
                        }
                        Rule::net_ref => {
                            net = Some(build_net_ref(arg, file, source));
                        }
                        _ => {}
                    }
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

/// Build a net reference (`net_ref` rule): a bare ident or a
/// bit-indexed bus element. A bit index is folded into the net name so
/// the rest of the compiler sees a flat scalar net `bus[i]` — matching
/// the per-bit names [`expand_bus_names`] produces for declarations.
fn build_net_ref(pair: Pair<'_, Rule>, file: &Arc<PathBuf>, source: &str) -> Ident {
    let span = make_span(&pair, file, source);
    let mut base: Option<SmolStr> = None;
    let mut bit: Option<u32> = None;
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => base = Some(SmolStr::new(inner.as_str())),
            Rule::bit_index => {
                for bi in inner.into_inner() {
                    if matches!(bi.as_rule(), Rule::integer_lit) {
                        bit = bi.as_str().parse::<u32>().ok();
                    }
                }
            }
            _ => {}
        }
    }
    let base = base.unwrap_or_else(|| SmolStr::new("<missing>"));
    let text = match bit {
        Some(b) => SmolStr::new(format!("{base}[{b}]")),
        None => base,
    };
    Ident { text, span }
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
