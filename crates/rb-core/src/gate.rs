//! The closed set of gate primitives supported by the HDL.

use serde::{Deserialize, Serialize};

/// All gate primitives the parser, synthesizer, and cell library
/// understand.
///
/// `#[non_exhaustive]` so future Post-MVP additions (`Comparator`,
/// `Observer` — see Post-MVP Epic in `spec.md`) can be added without a
/// SemVer-major bump of downstream crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GateKind {
    /// Combinational AND of two or more inputs.
    And,
    /// Combinational OR of two or more inputs.
    Or,
    /// Combinational logical NOT (single input).
    Not,
    /// Combinational XOR of two or more inputs.
    Xor,
    /// Edge-triggered D flip-flop. Stateful — see `data-model.md` §Cycle handling.
    DTrigger,
    /// Write-gated 1-bit memory cell (SR latch with WRITE enable). Stateful.
    MemoryCell,
}

impl GateKind {
    /// True for primitives whose data inputs cut combinational cycles
    /// (see FR-005 and `contracts/netlist-ir.md` §Cycle detection algorithm).
    pub const fn is_stateful(self) -> bool {
        matches!(self, GateKind::DTrigger | GateKind::MemoryCell)
    }

    /// Lower-case keyword as it appears in the HDL source.
    pub const fn keyword(self) -> &'static str {
        match self {
            GateKind::And => "and",
            GateKind::Or => "or",
            GateKind::Not => "not",
            GateKind::Xor => "xor",
            GateKind::DTrigger => "dtrigger",
            GateKind::MemoryCell => "memcell",
        }
    }
}
