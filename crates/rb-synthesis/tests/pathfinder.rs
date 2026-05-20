#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::{Bbox3, Pos3};
use rb_synthesis::{route_pathfinder, CostMap, NetTag, PathFinderConfig, RouteError};

#[test]
fn two_independent_nets_route_in_one_iteration() {
    let mut cost = CostMap::new(Bbox3::from_corners(
        Pos3::new(-5, 0, -5),
        Pos3::new(15, 0, 15),
    ));
    let routes = vec![
        (NetTag(1), Pos3::new(0, 0, 0), Pos3::new(10, 0, 0)),
        (NetTag(2), Pos3::new(0, 0, 5), Pos3::new(10, 0, 5)),
    ];
    let wires = route_pathfinder(&mut cost, &routes, &PathFinderConfig::DEFAULT)
        .expect("two parallel nets should route");
    assert_eq!(wires.len(), 2);
}

#[test]
fn pathfinder_aborts_with_convergence_exhausted_on_dense_input() {
    // Route many nets through a tiny corridor — they must overuse,
    // and PathFinder won't converge within a low iteration budget.
    let mut cost = CostMap::new(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 0, 1)));

    // 4 nets all forced through the same single-cell corridor.
    let routes: Vec<(NetTag, Pos3, Pos3)> = (0..4u32)
        .map(|i| {
            (
                NetTag(i),
                Pos3::new(0, 0, i as i32 % 2),
                Pos3::new(3, 0, i as i32 % 2),
            )
        })
        .collect();

    let cfg = PathFinderConfig {
        max_iterations: 1,
        ..PathFinderConfig::DEFAULT
    };
    let err = route_pathfinder(&mut cost, &routes, &cfg)
        .expect_err("must hit ConvergenceExhausted with iter cap 1 + overuse");
    match err {
        RouteError::ConvergenceExhausted {
            iterations,
            unrouted,
            ..
        } => {
            assert_eq!(iterations, 1);
            assert!(!unrouted.is_empty(), "expected ≥ 1 unrouted/overused net");
        }
        other => panic!("expected ConvergenceExhausted, got {other:?}"),
    }
}

#[test]
fn adjacency_isolation_forces_second_net_to_detour() {
    // 5×3×5 box. net#1 hard-anchored on z=0 row;
    // net#2 wants to go on z=1 right next to it — with adj_iso, must
    // detour vertically through z=2 or jump up.
    let mut cost = CostMap::new(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(4, 2, 2)));
    let routes = vec![
        (NetTag(1), Pos3::new(0, 0, 0), Pos3::new(4, 0, 0)),
        (NetTag(2), Pos3::new(0, 0, 1), Pos3::new(4, 0, 1)),
    ];

    // With adj_iso ON, second net can't share z=1 with neighbours
    // of z=0 dust — it has to take a higher Y route.
    let cfg = PathFinderConfig {
        max_iterations: 8,
        adjacency_isolation: true,
        ..PathFinderConfig::DEFAULT
    };
    let wires =
        route_pathfinder(&mut cost, &routes, &cfg).expect("both nets should route through detour");
    assert_eq!(wires.len(), 2);

    // net#2 must detour off the y=0,z=1 axis — at least one segment
    // must sit at y>=1 OR z>=2 (anything except the straight line
    // brushing net#1's shadow).
    let net2 = wires.iter().find(|w| w.net == NetTag(2)).unwrap();
    let detoured = net2.segments.iter().any(|s| {
        let p = match s {
            rb_synthesis::RouteSegment::Dust { pos } => *pos,
            rb_synthesis::RouteSegment::Repeater { pos, .. } => *pos,
        };
        p.y >= 1 || p.z >= 2
    });
    assert!(
        detoured,
        "net#2 should detour off the shadowed straight line; got {:?}",
        net2.segments
    );
}

#[test]
fn adjacency_isolation_off_lets_parallel_nets_share_plane() {
    // Same setup as above, but adj_iso=false. Both nets stay on y=0.
    let mut cost = CostMap::new(Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(4, 2, 2)));
    let routes = vec![
        (NetTag(1), Pos3::new(0, 0, 0), Pos3::new(4, 0, 0)),
        (NetTag(2), Pos3::new(0, 0, 1), Pos3::new(4, 0, 1)),
    ];
    let cfg = PathFinderConfig {
        max_iterations: 8,
        adjacency_isolation: false,
        ..PathFinderConfig::DEFAULT
    };
    let wires =
        route_pathfinder(&mut cost, &routes, &cfg).expect("cosmetic mode should route both flat");
    assert_eq!(wires.len(), 2);
    for w in &wires {
        for s in &w.segments {
            let y = match s {
                rb_synthesis::RouteSegment::Dust { pos } => pos.y,
                rb_synthesis::RouteSegment::Repeater { pos, .. } => pos.y,
            };
            assert_eq!(y, 0, "cosmetic mode keeps both nets on y=0");
        }
    }
}
