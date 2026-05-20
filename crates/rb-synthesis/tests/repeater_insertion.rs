#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::{Bbox3, Pos3};
use rb_synthesis::{route_pathfinder, CostMap, NetTag, PathFinderConfig, RouteSegment, MAX_SIGNAL};

fn count_repeaters(segs: &[RouteSegment]) -> usize {
    segs.iter()
        .filter(|s| matches!(s, RouteSegment::Repeater { .. }))
        .count()
}

#[test]
fn long_straight_run_via_pathfinder_inserts_at_least_one_repeater() {
    let mut cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-1, 0, -1),
        Pos3::new(25, 0, 1),
    ));
    let routes = vec![(NetTag(1), Pos3::new(0, 0, 0), Pos3::new(20, 0, 0))];
    let wires = route_pathfinder(&mut cost, &routes, &PathFinderConfig::DEFAULT).expect("routes");
    let w = &wires[0];

    let n_rep = count_repeaters(&w.segments);
    assert!(
        n_rep >= 1,
        "expected ≥ 1 repeater on a 20-block straight run, got {n_rep} ({:?})",
        w.segments
    );

    // Sanity: the repeater appears within MAX_SIGNAL=15 cells from source.
    let first_rep_idx = w
        .segments
        .iter()
        .position(|s| matches!(s, RouteSegment::Repeater { .. }))
        .expect("at least one repeater");
    assert!(
        first_rep_idx <= MAX_SIGNAL as usize,
        "first repeater at idx {first_rep_idx} exceeds 15-block strength budget"
    );
}

#[test]
fn very_long_run_inserts_multiple_repeaters() {
    let mut cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-1, 0, -1),
        Pos3::new(40, 0, 1),
    ));
    let routes = vec![(NetTag(1), Pos3::new(0, 0, 0), Pos3::new(35, 0, 0))];
    let wires = route_pathfinder(&mut cost, &routes, &PathFinderConfig::DEFAULT).expect("routes");
    let n_rep = count_repeaters(&wires[0].segments);
    assert!(
        n_rep >= 2,
        "expected ≥ 2 repeaters on a 35-block run, got {n_rep}"
    );
}

#[test]
fn short_run_has_no_repeaters() {
    let mut cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-1, 0, -1),
        Pos3::new(10, 0, 1),
    ));
    let routes = vec![(NetTag(1), Pos3::new(0, 0, 0), Pos3::new(5, 0, 0))];
    let wires = route_pathfinder(&mut cost, &routes, &PathFinderConfig::DEFAULT).expect("routes");
    assert_eq!(count_repeaters(&wires[0].segments), 0);
}
