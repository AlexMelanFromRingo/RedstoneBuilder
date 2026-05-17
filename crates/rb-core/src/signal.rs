//! Signal classification used by the netlist and synthesizer.
//!
//! Designed for forward-compatibility with the Post-MVP analog/event
//! epic. v1 only carries `Boolean` signals; future variants will model
//! redstone signal strength (0–15) and edge-triggered events emitted by
//! observers. Marked `#[non_exhaustive]` so future variants do not
//! force a SemVer-major bump.

use serde::{Deserialize, Serialize};

/// What kind of value a net carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum SignalKind {
    /// Two-valued logical signal (`0` or `1`). The only kind in v1.
    #[default]
    Boolean,
}
