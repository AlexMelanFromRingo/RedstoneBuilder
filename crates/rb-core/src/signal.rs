//! Signal classification and value.
//!
//! v1 had only `SignalKind { Boolean }` as a marker enum. v2 promotes
//! signals to a `Signal { value: u8, kind }` value type so the netlist
//! can model redstone signal strength (0..=15) and edge-triggered
//! pulses end-to-end.

use serde::{Deserialize, Serialize};

/// What kind of value a net carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum SignalKind {
    /// Two-valued logical signal. Value is conventionally 0 or 15.
    #[default]
    Boolean,
    /// Analog signal strength in 0..=15 (v2 — `analog wire …`).
    AnalogStrength,
    /// 1-tick pulse output of an observer (v2). Value is 15 for one
    /// tick after a watched-cell change, then 0.
    EdgeTrigger,
}

/// A concrete signal value: strength + classification.
///
/// v2-only — v1 code that only spoke in [`SignalKind`] keeps working
/// because the kind field is still present and remains `Boolean` by
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signal {
    /// Signal strength, 0..=15.
    pub value: u8,
    /// What kind of signal this is.
    pub kind: SignalKind,
}

impl Signal {
    /// The off state for any kind.
    pub const OFF: Self = Self {
        value: 0,
        kind: SignalKind::Boolean,
    };

    /// A boolean-on signal (strength 15, kind Boolean).
    pub const ON_BOOL: Self = Self {
        value: 15,
        kind: SignalKind::Boolean,
    };

    /// Construct an analog signal at the given strength.
    pub const fn analog(value: u8) -> Self {
        Self {
            value,
            kind: SignalKind::AnalogStrength,
        }
    }

    /// Construct a 1-tick edge pulse at full strength.
    pub const fn pulse() -> Self {
        Self {
            value: 15,
            kind: SignalKind::EdgeTrigger,
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::OFF
    }
}
