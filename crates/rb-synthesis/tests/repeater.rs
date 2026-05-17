#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::Pos3;
use rb_synthesis::{route_single, Grid3D, NetTag, RouteSegment, SingleRouteOutcome};

#[test]
fn long_straight_run_inserts_at_least_one_repeater() {
    // Force a 20-block straight run (15 blocks would already need one).
    let mut grid = Grid3D::new();
    let net = NetTag(7);

    let out = route_single(&mut grid, net, Pos3::new(0, 0, 0), Pos3::new(20, 0, 0));
    let SingleRouteOutcome::Routed(w) = out else {
        panic!("expected route to succeed");
    };

    let n_repeaters = w
        .segments
        .iter()
        .filter(|s| matches!(s, RouteSegment::Repeater { .. }))
        .count();
    let n_dust = w
        .segments
        .iter()
        .filter(|s| matches!(s, RouteSegment::Dust { .. }))
        .count();

    assert!(
        n_repeaters >= 1,
        "expected at least one repeater on a 20-block run, got {n_repeaters} \
         (segments={}, dust={n_dust})",
        w.segments.len()
    );
    assert_eq!(
        w.segments.len(),
        21,
        "expected 21 segments (start through end inclusive), got {}",
        w.segments.len()
    );
}

#[test]
fn short_run_has_no_repeaters() {
    let mut grid = Grid3D::new();
    let net = NetTag(8);

    let out = route_single(&mut grid, net, Pos3::new(0, 0, 0), Pos3::new(5, 0, 0));
    let SingleRouteOutcome::Routed(w) = out else {
        panic!("expected route to succeed");
    };

    assert!(
        w.segments
            .iter()
            .all(|s| matches!(s, RouteSegment::Dust { .. })),
        "5-block run should be all dust, no repeaters; got {:?}",
        w.segments
    );
}
