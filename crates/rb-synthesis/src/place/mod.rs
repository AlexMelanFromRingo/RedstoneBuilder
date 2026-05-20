//! Placement algorithms.
//!
//! - `greedy` — v1 row-based deterministic placer (`--placer greedy`).
//! - `sa` — v2 simulated-annealing placer (`--placer sa`, default).

pub mod greedy;
pub mod sa;

pub use crate::error::PlaceError;
pub use greedy::{place, PlaceConfig, PlacedCell, Placement};
pub use sa::{place_simulated_annealing, SaConfig};

use crate::netlist::Netlist;

/// Which placer to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlacerKind {
    /// Simulated annealing (v2 default; scales to 10 000 gates).
    Sa,
    /// v1 row-based greedy (deterministic, fast for tiny inputs).
    Greedy,
}

/// Dispatch to the chosen placer. Greedy ignores `sa_cfg`.
pub fn place_with(
    kind: PlacerKind,
    netlist: &Netlist,
    bounds: &PlaceConfig,
    sa_cfg: &SaConfig,
) -> Result<Placement, PlaceError> {
    match kind {
        PlacerKind::Sa => place_simulated_annealing(netlist, sa_cfg, bounds),
        PlacerKind::Greedy => greedy::place(netlist, bounds),
    }
}
