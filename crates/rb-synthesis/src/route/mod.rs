//! Routing algorithms.
//!
//! - `lee` — v1 3D Lee's maze router (kept for the `route_exhausted`
//!   test and as a `--placer greedy` fallback).
//! - `astar` + `pathfinder` — v2 production router (lands in Phase 4).
//! - `cost` — v2 cost-map (lands in Phase 4).

pub mod astar;
pub mod cost;
pub mod lee;
pub mod pathfinder;

pub use astar::{astar_route, astar_route_multisrc};
pub use cost::{CellAttrs, CostMap, EdgeCostConfig, VerticalCost};
pub use lee::{
    route_all, route_single, route_single_bounded, route_with_retry, RouteConfig, RouteSegment,
    RoutedWire, SingleRouteOutcome, MAX_SIGNAL,
};
pub use pathfinder::{route_pathfinder, PathFinderConfig};
