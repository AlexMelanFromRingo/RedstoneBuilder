//! Error types for the synthesis crate.

use miette::Diagnostic;
use thiserror::Error;

/// Errors raised while building the netlist from a parsed module.
#[derive(Debug, Error, Diagnostic)]
pub enum SynthError {
    /// A connection's port name was not recognised for the gate kind.
    /// (This is also caught earlier by `rb_parser::validate`; we keep
    /// this for the case where the synthesizer is called directly.)
    #[error("gate '{inst}' has unknown port '{port}'")]
    UnknownPort {
        /// Gate instance name.
        inst: String,
        /// Offending port name.
        port: String,
    },

    /// The HDL contains a gate primitive the synthesizer has no cell for.
    #[error("no cell library entry for primitive {kind:?}")]
    NoCellFor {
        /// Gate kind that is missing.
        kind: rb_core::GateKind,
    },
}

/// Errors raised by cycle detection (FR-005).
#[derive(Debug, Error, Diagnostic)]
pub enum CycleError {
    /// One or more purely-combinational feedback cycles found.
    #[error("combinational feedback cycle through {} wire(s)", wires.len())]
    #[diagnostic(
        code(rb_synthesis::cycle),
        help("insert a `dtrigger` or `memcell` on the feedback path to legalize it")
    )]
    Combinational {
        /// Wire names that form the cycle, in the order Tarjan returns
        /// them.
        wires: Vec<String>,
    },
}

/// Errors raised by placement (FR-016).
#[derive(Debug, Error, Diagnostic)]
pub enum PlaceError {
    /// The placement would exceed the active `--max-footprint` bound.
    #[error("design too large: footprint {actual} exceeds --max-footprint {bound}")]
    #[diagnostic(code(rb_synthesis::footprint))]
    TooLarge {
        /// Actual footprint as `W×H×D`.
        actual: String,
        /// Active bound as `W×H×D`.
        bound: String,
    },
}

/// Errors raised by routing (FR-008, FR-009, FR-015).
#[derive(Debug, Error, Diagnostic)]
pub enum RouteError {
    /// The router exhausted its bbox-expansion budget without finding a
    /// valid layout for all nets.
    #[error(
        "routing failed: {} unrouted net(s) after {retries} bbox expansion(s)",
        unrouted.len()
    )]
    #[diagnostic(code(rb_synthesis::unroutable))]
    Exhausted {
        /// Names of the nets that could not be routed.
        unrouted: Vec<String>,
        /// Number of retries the router attempted.
        retries: u8,
        /// Final bounding box, as `W×H×D`.
        final_bbox: String,
    },
}
