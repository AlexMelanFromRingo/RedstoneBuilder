#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::{Bbox3, Pos3};
use rb_synthesis::{astar_route, CostMap, EdgeCostConfig, NetTag};

#[test]
fn trivial_straight_route_returns_path() {
    let cost = CostMap::new(Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(10, 0, 0)));
    let path = astar_route(
        &cost,
        NetTag(1),
        Pos3::ORIGIN,
        Pos3::new(5, 0, 0),
        &EdgeCostConfig::DEFAULT,
        10_000,
    )
    .expect("path exists");
    assert_eq!(path.len(), 6, "5-block straight = 6 cells (incl. source)");
}

#[test]
fn obstacle_forces_detour() {
    let mut cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-10, 0, -10),
        Pos3::new(10, 0, 10),
    ));
    // Wall of blocked cells at x=2 forces routes around.
    for z in -5..=5 {
        cost.mark_blocked(Pos3::new(2, 0, z));
    }
    // Leave a gap at z=0 — actually, block it too so the router must
    // go all the way around at z=6.
    cost.mark_blocked(Pos3::new(2, 0, 6));

    let path = astar_route(
        &cost,
        NetTag(1),
        Pos3::new(0, 0, 0),
        Pos3::new(5, 0, 0),
        &EdgeCostConfig::DEFAULT,
        20_000,
    )
    .expect("path exists via detour");
    // Manhattan distance from source to sink is 5; with the wall, the
    // path must be ≥ 5 + 2 × |z-detour|.
    assert!(
        path.len() > 6,
        "expected a detour, got direct path of length {}",
        path.len()
    );
}

#[test]
fn astar_is_deterministic() {
    let cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-5, -5, -5),
        Pos3::new(5, 5, 5),
    ));
    let p1 = astar_route(
        &cost,
        NetTag(1),
        Pos3::ORIGIN,
        Pos3::new(3, 0, 3),
        &EdgeCostConfig::DEFAULT,
        5_000,
    )
    .expect("p1");
    let p2 = astar_route(
        &cost,
        NetTag(1),
        Pos3::ORIGIN,
        Pos3::new(3, 0, 3),
        &EdgeCostConfig::DEFAULT,
        5_000,
    )
    .expect("p2");
    assert_eq!(p1, p2, "A* must be deterministic across runs");
}
