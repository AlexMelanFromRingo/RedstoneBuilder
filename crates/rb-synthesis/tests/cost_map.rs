#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::{Bbox3, Direction, Pos3};
use rb_synthesis::{CostMap, EdgeCostConfig, NetTag};

#[test]
fn slab_blocks_downward_traversal_but_allows_upward() {
    let mut map = CostMap::new(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 2, 2)));
    map.mark_slab(Pos3::new(1, 1, 1));

    let net = NetTag(1);
    let cfg = EdgeCostConfig::DEFAULT;

    // Stepping DOWN from the slab → forbidden.
    let down_cost = map.cost_for_edge(
        Pos3::new(1, 1, 1),
        None,
        Direction::Down,
        Pos3::new(1, 0, 1),
        net,
        &cfg,
    );
    assert_eq!(down_cost, u32::MAX, "slab top must block downward");

    // Stepping UP from the slab → expensive but finite.
    let up_cost = map.cost_for_edge(
        Pos3::new(1, 1, 1),
        None,
        Direction::Up,
        Pos3::new(1, 2, 1),
        net,
        &cfg,
    );
    assert!(up_cost < u32::MAX, "slab top must permit upward step");
}

#[test]
fn blocked_cell_rejects_foreign_net_traversal() {
    let mut map = CostMap::new(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 0, 2)));
    map.mark_blocked(Pos3::new(1, 0, 1));

    let cfg = EdgeCostConfig::DEFAULT;
    let cost = map.cost_for_edge(
        Pos3::new(0, 0, 1),
        None,
        Direction::East,
        Pos3::new(1, 0, 1),
        NetTag(1),
        &cfg,
    );
    assert_eq!(cost, u32::MAX, "blocked cell rejects foreign net");
}

#[test]
fn turn_penalty_applied_on_direction_change() {
    let map = CostMap::new(Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(5, 0, 5)));
    let cfg = EdgeCostConfig::DEFAULT;
    let net = NetTag(1);

    let straight = map.cost_for_edge(
        Pos3::ORIGIN,
        Some(Direction::East),
        Direction::East,
        Pos3::new(1, 0, 0),
        net,
        &cfg,
    );
    let turn = map.cost_for_edge(
        Pos3::ORIGIN,
        Some(Direction::East),
        Direction::South,
        Pos3::new(0, 0, 1),
        net,
        &cfg,
    );

    assert!(
        turn > straight,
        "turn must cost more than straight: turn={turn}, straight={straight}"
    );
}

#[test]
fn shadow_blocks_foreign_net_but_not_own() {
    let mut map = CostMap::new(Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(4, 0, 4)));
    let net_a = NetTag(1);
    let net_b = NetTag(2);

    // Simulate net A having committed cell (2,0,2).
    map.assigned.insert(Pos3::new(2, 0, 2), net_a);
    // Now mark its 6 orthogonal neighbours as shadows.
    for d in rb_core::Direction::ALL {
        let (dx, dy, dz) = d.offset();
        map.mark_shadow(Pos3::new(2 + dx, dy, 2 + dz), net_a);
    }

    let cfg = EdgeCostConfig::DEFAULT;

    // Foreign net B tries to step onto a shadow cell → forbidden.
    let cost_foreign = map.cost_for_edge(
        Pos3::new(0, 0, 2),
        None,
        Direction::East,
        Pos3::new(1, 0, 2),
        net_b,
        &cfg,
    );
    assert_eq!(
        cost_foreign,
        u32::MAX,
        "foreign net must not traverse another net's shadow"
    );

    // Same step for net A itself (whose shadow it is) → allowed.
    let cost_own = map.cost_for_edge(
        Pos3::new(0, 0, 2),
        None,
        Direction::East,
        Pos3::new(1, 0, 2),
        net_a,
        &cfg,
    );
    assert!(
        cost_own < u32::MAX,
        "net must be allowed into its own shadow ({cost_own})"
    );
}

#[test]
fn mark_shadow_does_not_overwrite_existing_owner() {
    let mut map = CostMap::new(Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(2, 0, 2)));
    let p = Pos3::new(1, 0, 1);
    map.mark_shadow(p, NetTag(1));
    map.mark_shadow(p, NetTag(2));
    assert_eq!(map.shadows.get(&p), Some(&NetTag(1)), "first-in wins");
}

#[test]
fn clear_iteration_state_clears_shadows_too() {
    let mut map = CostMap::new(Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(2, 0, 2)));
    map.mark_shadow(Pos3::new(1, 0, 1), NetTag(1));
    map.assigned.insert(Pos3::new(0, 0, 0), NetTag(1));
    map.clear_iteration_state();
    assert!(map.shadows.is_empty(), "shadows cleared between iterations");
    assert!(
        map.assigned.is_empty(),
        "assigned cleared between iterations"
    );
}
