//! Logic synthesis, placement, and routing for RedstoneBuilder.
//!
//! See `specs/001-hdl-compiler-cli/contracts/netlist-ir.md` and
//! `contracts/minecraft-blocks.md` for the contracts this crate
//! implements.

#![deny(missing_docs)]

pub mod cell_library;
pub mod cycle;
pub mod error;
pub mod grid3d;
pub mod netlist;
pub mod place;
pub mod route;

pub use cell_library::{macrocell_for, Anchor, Macrocell, PlacedBlock};
pub use cycle::detect_cycles;
pub use error::{CycleError, PlaceError, RouteError, SynthError};
pub use grid3d::{CellState, Grid3D, NetTag};
pub use netlist::{
    build_netlist, EndpointRole, NetId, Netlist, NetlistEdge, NetlistGraph, NetlistNode,
};
pub use place::{place, PlaceConfig, PlacedCell, Placement};
pub use route::{
    route_all, route_single, route_single_bounded, route_with_retry, RouteConfig, RouteSegment,
    RoutedWire, SingleRouteOutcome, MAX_SIGNAL,
};

/// Compile-time default RNG seed (FR-017). Override via the CLI `--seed`
/// flag at the binary layer.
pub const DEFAULT_SEED: u64 = 0xCAFE_F00D_DEAD_BEEF;
