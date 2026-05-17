#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::Pos3;
use rb_synthesis::{route_single, Grid3D, NetTag, RouteSegment, SingleRouteOutcome};

fn dust_positions(out: &SingleRouteOutcome) -> Vec<Pos3> {
    if let SingleRouteOutcome::Routed(w) = out {
        w.segments
            .iter()
            .filter_map(|s| match *s {
                RouteSegment::Dust { pos } => Some(pos),
                _ => None,
            })
            .collect()
    } else {
        panic!("route was not Routed: {out:?}");
    }
}

#[test]
fn two_parallel_nets_do_not_share_adjacent_cells() {
    // Two nets routed through an open corridor 10 wide, 1 tall (y=0).
    // Net A: (0,0,0) → (10,0,0).
    // Net B: (0,0,3) → (10,0,3).
    // After routing A first, A's obstruction footprint at z=±1 of its
    // path must keep B's dust at least 2 cells away in Z.
    let mut grid = Grid3D::new();
    let a = NetTag(1);
    let b = NetTag(2);

    let a_out = route_single(&mut grid, a, Pos3::new(0, 0, 0), Pos3::new(10, 0, 0));
    let b_out = route_single(&mut grid, b, Pos3::new(0, 0, 3), Pos3::new(10, 0, 3));

    let a_dust = dust_positions(&a_out);
    let b_dust = dust_positions(&b_out);

    assert!(!a_dust.is_empty());
    assert!(!b_dust.is_empty());

    for pa in &a_dust {
        for pb in &b_dust {
            let same_or_adj_xz =
                (pa.x - pb.x).abs() <= 1 && (pa.z - pb.z).abs() <= 1 && pa.y == pb.y;
            assert!(
                !same_or_adj_xz,
                "net A at {pa:?} and net B at {pb:?} are too close; \
                 obstruction propagation should have separated them"
            );
        }
    }
}

#[test]
fn obstruction_state_blocks_foreign_dust_but_not_own_dust() {
    // After routing A, its dust cells should be Obstructed for B's
    // perspective at all 4-laterals.
    let mut grid = Grid3D::new();
    let a = NetTag(1);
    let b = NetTag(2);

    let _ = route_single(&mut grid, a, Pos3::new(0, 0, 0), Pos3::new(5, 0, 0));

    // A 4-lateral neighbor of A's dust must NOT be passable for B.
    assert!(!grid.is_passable_for(Pos3::new(2, 0, 1), b));
    // But A itself may still route through its own dust.
    assert!(grid.is_passable_for(Pos3::new(2, 0, 0), a));
}
