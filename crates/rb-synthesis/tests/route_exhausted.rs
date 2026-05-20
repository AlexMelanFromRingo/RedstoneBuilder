#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::Pos3;
use rb_synthesis::{route_with_retry, Grid3D, NetTag, RouteConfig, RouteError};

#[test]
fn distance_cap_exhaustion_returns_exhausted_with_unrouted_list() {
    // A single 20-block route. With initial_max_distance=2 and
    // bbox_grow_step=1 and max_bbox_retries=1, the cap reaches 3 — way
    // less than 20 — so the router must give up.
    let template = Grid3D::new();
    let cfg = RouteConfig {
        initial_max_distance: 2,
        bbox_grow_step: 1,
        max_bbox_retries: 1,
        seed: 0,
    };
    let routes = vec![(NetTag(42), Pos3::new(0, 0, 0), Pos3::new(20, 0, 0))];
    let err = route_with_retry(&template, &routes, &cfg).expect_err("must exhaust");
    match err {
        RouteError::Exhausted {
            unrouted,
            retries,
            final_bbox,
        } => {
            assert_eq!(unrouted, vec!["net#42".to_string()]);
            assert_eq!(retries, 1);
            assert!(final_bbox.contains("max_distance"));
        }
        other => panic!("expected Exhausted, got {other:?}"),
    }
}

#[test]
fn retry_eventually_succeeds_when_cap_grows_enough() {
    // Initial cap too small but retry growth covers the distance.
    let template = Grid3D::new();
    let cfg = RouteConfig {
        initial_max_distance: 2,
        bbox_grow_step: 30,
        max_bbox_retries: 4,
        seed: 0,
    };
    let routes = vec![(NetTag(1), Pos3::new(0, 0, 0), Pos3::new(20, 0, 0))];
    let (_, wires) = route_with_retry(&template, &routes, &cfg).expect("retry must succeed");
    assert_eq!(wires.len(), 1);
}
