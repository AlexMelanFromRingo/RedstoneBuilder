//! Diagnostic types emitted by [`crate::parse`] and [`crate::validate`].

use miette::{Diagnostic, NamedSource, SourceSpan as MietteSpan};
use thiserror::Error;

/// Errors produced by the parser stage (pest-level + initial AST build).
#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    /// A pest parse failure (unexpected token / EOF / etc.).
    #[error("syntax error: {message}")]
    #[diagnostic(code(rb_parser::syntax))]
    Syntax {
        /// Human-readable description of the expected/found token.
        message: String,
        /// Span of the offending source range.
        #[label("here")]
        at: MietteSpan,
        /// Source code, for the rendered snippet.
        #[source_code]
        src: NamedSource<String>,
    },

    /// A construct the v1 grammar does not (yet) support.
    #[error("unsupported construct: {what}")]
    #[diagnostic(
        code(rb_parser::unsupported),
        help("see specs/001-hdl-compiler-cli/contracts/hdl-grammar.md for the v1 Verilog subset")
    )]
    Unsupported {
        /// What was found (keyword or construct name).
        what: String,
        /// Span pointing at the offending construct.
        #[label("not in v1 subset")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// Filesystem error reading the source file.
    #[error("could not read source file: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors produced by semantic validation (post-parse, pre-synthesis).
///
/// Returned as a `Vec` from [`crate::validate`]: validation collects
/// every problem in one pass rather than aborting at the first.
#[derive(Debug, Error, Diagnostic)]
pub enum SemanticError {
    /// A connection references a wire/port that was never declared.
    #[error("net '{name}' is referenced but not declared")]
    #[diagnostic(code(rb_parser::undeclared_net))]
    UndeclaredNet {
        /// Net name that was referenced.
        name: String,
        /// Span of the reference site.
        #[label("undeclared")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// An output port has no source driving it.
    #[error("output '{name}' is not driven by any gate")]
    #[diagnostic(code(rb_parser::undriven_output))]
    UndrivenOutput {
        /// Port name.
        name: String,
        /// Span of the port declaration.
        #[label("declared here")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// An output is driven by more than one gate.
    #[error("output '{name}' is driven by {count} sources")]
    #[diagnostic(code(rb_parser::multiply_driven_output))]
    MultiplyDrivenOutput {
        /// Port name.
        name: String,
        /// Number of distinct drivers.
        count: usize,
        /// Span of the port declaration.
        #[label("multiply driven")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// An identifier is declared more than once in the same scope.
    #[error("identifier '{name}' is declared more than once")]
    #[diagnostic(code(rb_parser::duplicate_ident))]
    DuplicateIdent {
        /// Identifier text.
        name: String,
        /// Span of the duplicate declaration site.
        #[label("duplicate declaration")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// A gate instance is missing a required port or has an unknown port name.
    #[error("gate '{inst}' has invalid port set: {detail}")]
    #[diagnostic(code(rb_parser::bad_port))]
    BadPort {
        /// Instance name.
        inst: String,
        /// What's wrong (missing port, unknown port, wrong arity).
        detail: String,
        /// Span of the instance declaration.
        #[label("invalid port set")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// A wire's declared `SignalKind` does not match how a gate
    /// instance is consuming it (v2 — e.g., feeding an `analog wire`
    /// into a boolean `and` gate, or vice versa).
    #[error("net '{name}' has kind {found:?} but gate '{inst}' expects {expected:?}")]
    #[diagnostic(code(rb_parser::signal_kind_mismatch))]
    SignalKindMismatch {
        /// Net name.
        name: String,
        /// Gate instance referencing the net.
        inst: String,
        /// Kind the wire was declared as.
        found: String,
        /// Kind the gate's port expects.
        expected: String,
        /// Reference span.
        #[label("kind mismatch")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// A v2 `repeater` instance declared a `.DELAY(N)` outside the
    /// allowed `1..=4` range.
    #[error("repeater '{inst}' has invalid delay {actual}; allowed range is 1..=4")]
    #[diagnostic(code(rb_parser::bad_delay))]
    BadDelay {
        /// Repeater instance name.
        inst: String,
        /// The offending delay value.
        actual: u32,
        /// Span of the instance.
        #[label("invalid delay")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },

    /// A v2 `comparator` instance declared a `.MODE(...)` outside the
    /// allowed `{compare, subtract}` set. (Currently caught at parse
    /// time, kept here so future custom modes have a slot.)
    #[error("comparator '{inst}' has invalid mode '{actual}'; allowed: compare, subtract")]
    #[diagnostic(code(rb_parser::bad_mode))]
    BadMode {
        /// Comparator instance name.
        inst: String,
        /// The offending mode keyword.
        actual: String,
        /// Span of the instance.
        #[label("invalid mode")]
        at: MietteSpan,
        /// Source code.
        #[source_code]
        src: NamedSource<String>,
    },
}
