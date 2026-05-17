# Contract — AST Types

**Crate**: `rb-parser` (re-exported from `rb_parser::ast`)

The AST is the **first stable IR** in the pipeline (Constitution
Principle II). Everything downstream consumes these types; any breaking
change here requires a SemVer-major bump of `rb-parser`.

## Type definitions

```rust
use rb_core::{SourceSpan, GateKind};
use smol_str::SmolStr;

/// One source file = one top-level module in v1.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Module {
    pub name: Ident,
    pub ports: Vec<Port>,
    pub wires: Vec<WireDecl>,
    pub instances: Vec<GateInst>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Port {
    pub name: Ident,
    pub dir: PortDir,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PortDir { Input, Output }

#[derive(Debug, Clone, serde::Serialize)]
pub struct WireDecl {
    pub name: Ident,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GateInst {
    pub inst_name: Ident,
    pub kind: GateKind,
    pub connections: Vec<Connection>,
    /// Outer `always @(posedge clk)` if this is a sequential instance.
    /// `None` for purely combinational gates.
    pub clock: Option<ClockEdge>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClockEdge {
    pub edge: Edge,
    pub clock_net: Ident,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Edge { Posedge, Negedge }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Connection {
    pub port: Ident,      // e.g. "A", "Y", "D", "CLK", "Q"
    pub net: Ident,       // referenced wire or port name
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct Ident {
    pub text: SmolStr,
    pub span: SourceSpan,
}
```

## Constructor / validator API

```rust
pub fn parse(source: &str, file: &Path) -> Result<Module, ParseError>;

pub fn validate(module: &Module) -> Result<(), Vec<SemanticError>>;
```

- `parse` only catches syntactic errors (FR-003: location + offending span).
- `validate` enforces FR-004 (every output driven exactly once, every net
  reference resolves) and the gate-arity rules (see hdl-grammar.md).
- Cycle detection is **not** performed here — it requires netlist
  construction (see data-model.md, Stage 2). It lives in `rb-synthesis`.

## Error types

```rust
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum ParseError {
    #[error("syntax error: expected {expected}")]
    #[diagnostic(code(rb_parser::syntax))]
    Syntax {
        expected: String,
        #[label("here")]
        at: miette::SourceSpan,
        #[source_code]
        src: miette::NamedSource<String>,
    },

    #[error("unsupported construct: {what} is not supported in v1")]
    #[diagnostic(code(rb_parser::unsupported), help("see contracts/hdl-grammar.md for the v1 subset"))]
    Unsupported { what: String, at: miette::SourceSpan, #[source_code] src: miette::NamedSource<String> },
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum SemanticError {
    #[error("net '{name}' is referenced but not declared")]
    UndeclaredNet { name: String, at: miette::SourceSpan, /* ... */ },

    #[error("output '{name}' is not driven by any source")]
    UndrivenOutput { name: String, at: miette::SourceSpan, /* ... */ },

    #[error("output '{name}' is driven by multiple sources")]
    MultiplyDrivenOutput { name: String, sites: Vec<miette::SourceSpan>, /* ... */ },

    #[error("identifier '{name}' is declared more than once")]
    DuplicateIdent { name: String, sites: Vec<miette::SourceSpan>, /* ... */ },

    #[error("gate '{inst}' missing required port '{port}'")]
    BadPort { inst: String, port: String, at: miette::SourceSpan, /* ... */ },
}
```

## Dump format (`--dump-ast`)

JSON via `serde_json::to_writer_pretty`. The on-disk schema is the
`#[derive(serde::Serialize)]` projection above. Treat the JSON as a
**non-stable**, debug-only contract — useful for golden-file tests but
not part of the public API.
