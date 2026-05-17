#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{build_netlist, detect_cycles, CycleError};

#[test]
fn three_gate_combinational_cycle_is_rejected() {
    // a → not g1 → w1 → not g2 → w2 → not g3 → w3 (feeds back into g1.A)
    let src = r#"
        module loop3(input start, output y);
            wire w1, w2, w3;
            not g1(.A(w3), .Y(w1));
            not g2(.A(w1), .Y(w2));
            not g3(.A(w2), .Y(w3));
            not gy(.A(w1), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("loop3.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let err = detect_cycles(&netlist).expect_err("cycle must be detected");
    let CycleError::Combinational { wires } = err;
    assert!(wires.iter().any(|w| w == "w1"), "got wires: {wires:?}");
    assert!(wires.iter().any(|w| w == "w2"), "got wires: {wires:?}");
    assert!(wires.iter().any(|w| w == "w3"), "got wires: {wires:?}");
}

#[test]
fn linear_chain_has_no_cycle() {
    let src = r#"
        module chain(input a, output y);
            wire w;
            not g1(.A(a), .Y(w));
            not g2(.A(w), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("chain.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    detect_cycles(&netlist).expect("DAG: no cycle expected");
}
