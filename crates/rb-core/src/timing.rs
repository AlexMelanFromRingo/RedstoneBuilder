//! Redstone-tick newtype for static timing analysis.

use serde::{Deserialize, Serialize};

/// One redstone tick (≈ 100 ms wall time at default game speed).
///
/// Used by `rb_synthesis::timing` for static timing analysis and by
/// the cell library for per-primitive tick-delay annotations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct Tick(pub u32);

impl Tick {
    /// Zero ticks (no delay).
    pub const ZERO: Self = Self(0);

    /// Saturating add — never wraps. Use this in all timing-accumulation
    /// loops so a pathological input can't silently overflow.
    pub fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}
