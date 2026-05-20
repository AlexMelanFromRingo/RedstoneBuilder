#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{analyse_timing, build_netlist, TimingConfig, TimingError};

#[test]
fn linear_chain_accumulates_arrival_ticks() {
    // a → not g1 → w1 → not g2 → w2 → not g3 → y.
    // Each not_cell has tick_delay = 1, so y arrives at tick 3.
    let src = r#"
        module chain(input a, output y);
            wire w1, w2;
            not g1(.A(a), .Y(w1));
            not g2(.A(w1), .Y(w2));
            not g3(.A(w2), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("chain.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");
    let timing = analyse_timing(&netlist, &TimingConfig::DEFAULT).expect("analyse");

    let max_arrival = timing.arrival.values().map(|t| t.0).max().unwrap_or(0);
    assert!(
        max_arrival >= 3,
        "expected at least tick 3 (3 NOTs), got {max_arrival}"
    );
}

#[test]
fn matched_converging_paths_have_no_data_race() {
    // a → not g1 → w1 ──┐
    // a → not g2 → w2 ──┴── and out(w1, w2) ⇒ y    (balanced, both 1 tick)
    let src = r#"
        module balanced(input a, output y);
            wire w1, w2;
            not g1(.A(a), .Y(w1));
            not g2(.A(w1), .Y(w2));
            and out(.A(w1), .B(w2), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("balanced.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");
    // Tolerance 5 so the (intentionally) small skew passes.
    let cfg = TimingConfig {
        data_race_tolerance: 5,
    };
    let _ = analyse_timing(&netlist, &cfg).expect("no race expected with tolerance=5");
}

#[test]
fn skewed_converging_paths_report_data_race() {
    // a → not g1 → w1 ──┐                          (1 tick)
    // a → not g2 → not g3 → not g4 → w2 ──┴── and out  (3 ticks)
    let src = r#"
        module skewed(input a, output y);
            wire w1, w2, mid1, mid2;
            not g1(.A(a), .Y(w1));
            not g2(.A(a), .Y(mid1));
            not g3(.A(mid1), .Y(mid2));
            not g4(.A(mid2), .Y(w2));
            and out(.A(w1), .B(w2), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("skewed.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let err = analyse_timing(&netlist, &TimingConfig::DEFAULT).expect_err("race expected");
    let TimingError::DataRace {
        arrivals,
        tolerance,
        ..
    } = err;
    assert_eq!(tolerance, 0);
    assert!(arrivals.len() >= 2, "expected >= 2 converging arrivals");
    let spread = arrivals
        .iter()
        .max()
        .unwrap_or(&0)
        .saturating_sub(*arrivals.iter().min().unwrap_or(&0));
    assert!(
        spread > 0,
        "expected positive skew, got arrivals {arrivals:?}"
    );
}
