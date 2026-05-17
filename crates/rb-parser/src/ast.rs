//! AST produced by [`crate::parse`].
//!
//! Sequential constructs (`ClockEdge`, `Edge`) are intentionally absent
//! in US1 — they land in US2.

use rb_core::{GateKind, SourceSpan};
use serde::Serialize;
use smol_str::SmolStr;

/// One source file's top-level module.
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
    /// Span covering the whole `module ... endmodule` block.
    pub span: SourceSpan,
}

/// A boundary port (input or output) on a module.
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    /// Port name.
    pub name: Ident,
    /// Direction (input or output).
    pub dir: PortDir,
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
    /// Span of this individual identifier within its declaration.
    pub span: SourceSpan,
}

/// One gate instantiation.
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
    /// Span covering the whole `kind name(...)` statement.
    pub span: SourceSpan,
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
