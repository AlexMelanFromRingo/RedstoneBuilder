//! AST produced by [`crate::parse`].
//!
//! Sequential constructs (`ClockEdge`, `Edge`) are intentionally absent
//! in US1 — they land in US2.

use rb_core::{GateKind, SignalKind, SourceSpan};
use serde::Serialize;
use smol_str::SmolStr;

/// A whole source file: one or more module definitions plus the index
/// of the elaboration top (the last module not instantiated by any
/// other). [`crate::parse`] flattens this into a single [`Module`]
/// before returning, so most consumers never see a `Design`.
#[derive(Debug, Clone, Serialize)]
pub struct Design {
    /// Every module defined in the file, in source order.
    pub modules: Vec<Module>,
    /// Index into `modules` of the elaboration top.
    pub top: usize,
}

/// One module definition.
#[derive(Debug, Clone, Serialize)]
pub struct Module {
    /// Module name (the identifier after the `module` keyword).
    pub name: Ident,
    /// Port declarations in source order.
    pub ports: Vec<Port>,
    /// Wire declarations in source order. Each `wire a, b, c;`
    /// expands into one `WireDecl` per identifier.
    pub wires: Vec<WireDecl>,
    /// Gate instantiations in source order.
    pub instances: Vec<GateInst>,
    /// Sub-module instantiations in source order (v3). Empty for a
    /// flat module; resolved away by elaboration.
    #[serde(default)]
    pub mod_instances: Vec<ModuleInst>,
    /// Span covering the whole `module ... endmodule` block.
    pub span: SourceSpan,
}

/// A sub-module instantiation: `<module> <inst>(.port(net), ...)`.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleInst {
    /// Name of the module being instantiated.
    pub module_name: Ident,
    /// Instance label, unique within the enclosing module.
    pub inst_name: Ident,
    /// Named-port connections binding the sub-module's ports to nets
    /// in the enclosing module.
    pub connections: Vec<Connection>,
    /// Span covering the whole `<module> <inst>(...)` statement.
    pub span: SourceSpan,
}

/// A boundary port (input or output) on a module.
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    /// Port name.
    pub name: Ident,
    /// Direction (input or output).
    pub dir: PortDir,
    /// Signal kind. `Boolean` by default (v1); `AnalogStrength` if
    /// declared as `input analog wire …` / `output analog wire …`
    /// (v2).
    pub kind: SignalKind,
    /// Span covering the `input <name>` or `output <name>` declaration.
    pub span: SourceSpan,
}

/// Direction of a module-boundary port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PortDir {
    /// Driven from outside the module.
    Input,
    /// Driven from inside the module; visible outside.
    Output,
}

/// One named internal wire.
#[derive(Debug, Clone, Serialize)]
pub struct WireDecl {
    /// Wire name.
    pub name: Ident,
    /// What kind of signal the wire carries. Bare `wire X;` is
    /// [`SignalKind::Boolean`] (v1 default); `analog wire X;` is
    /// [`SignalKind::AnalogStrength`] (v2).
    pub kind: SignalKind,
    /// Span of this individual identifier within its declaration.
    pub span: SourceSpan,
}

/// One gate instantiation.
///
/// v1 carried only `kind: GateKind` + `clock`. v2 adds two optional
/// fields (`compare_mode`, `repeater_delay`) that are `None` for v1
/// primitives and `Some(_)` for the corresponding v2 primitives
/// (`comparator`, `repeater`). Keeping this additive lets all v1
/// pattern-matches on `gate.kind` continue to work unchanged.
#[derive(Debug, Clone, Serialize)]
pub struct GateInst {
    /// Instance name (the identifier between the gate keyword and the
    /// parameter list).
    pub inst_name: Ident,
    /// Which primitive this instantiates.
    pub kind: GateKind,
    /// Named-port connections.
    pub connections: Vec<Connection>,
    /// `Some(_)` iff this instance was declared inside an
    /// `always @(<edge> <clk>)` block (US2+). `None` for purely
    /// combinational primitives.
    pub clock: Option<ClockEdge>,
    /// `Some(_)` iff `kind == GateKind::Comparator` — selects compare
    /// vs subtract semantics for the comparator (v2).
    pub compare_mode: Option<CompareMode>,
    /// `Some(_)` iff `kind == GateKind::Repeater` — the repeater's
    /// delay setting in MC repeater-clicks (1..=4 redstone-tick
    /// pairs).
    pub repeater_delay: Option<u8>,
    /// Span covering the whole `kind name(...)` statement.
    pub span: SourceSpan,
}

/// Comparator operating mode (v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CompareMode {
    /// `compare` mode: output = A if A ≥ B else 0.
    Compare,
    /// `subtract` mode: output = max(0, A − B).
    Subtract,
}

/// `always @(posedge clk)` / `always @(negedge clk)` annotation attached
/// to a stateful gate instance.
#[derive(Debug, Clone, Serialize)]
pub struct ClockEdge {
    /// Which edge triggers the gate.
    pub edge: Edge,
    /// Name of the clock net.
    pub clock_net: Ident,
    /// Span of the `always @(<edge> <clk>)` header.
    pub span: SourceSpan,
}

/// Clock edge polarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Edge {
    /// Triggered on rising edge.
    Posedge,
    /// Triggered on falling edge.
    Negedge,
}

/// One named-port connection: `.<port>(<net>)`.
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    /// Formal port name on the gate (e.g. `"A"`, `"Y"`).
    pub port: Ident,
    /// Net name on the surrounding module.
    pub net: Ident,
    /// Span covering `.<port>(<net>)`.
    pub span: SourceSpan,
}

/// An identifier with its source span.
#[derive(Debug, Clone, Serialize)]
pub struct Ident {
    /// The identifier text.
    pub text: SmolStr,
    /// Span the identifier occupied in source.
    pub span: SourceSpan,
}

impl Ident {
    /// Cheap reference to the identifier text.
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Ident {}

impl std::hash::Hash for Ident {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}
