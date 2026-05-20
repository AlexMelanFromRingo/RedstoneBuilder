#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{build_netlist, place_simulated_annealing, PlaceConfig, SaConfig};

fn synth_chain(n: usize) -> String {
    let mut src = String::from("module chain(input a, output q);\n    wire ");
    for i in 0..n {
        if i > 0 {
            src.push_str(", ");
        }
        src.push_str(&format!("w{i}"));
    }
    src.push_str(";\n");
    for i in 0..n {
        let from = if i == 0 {
            "a".to_string()
        } else {
            format!("w{}", i - 1)
        };
        src.push_str(&format!("    not g{i}(.A({from}), .Y(w{i}));\n"));
    }
    src.push_str(&format!("    not gq(.A(w{}), .Y(q));\n", n - 1));
    src.push_str("endmodule\n");
    src
}

#[test]
fn sa_placer_is_deterministic_with_fixed_seed() {
    let src = synth_chain(50);
    let path = PathBuf::from("chain50.hdl");
    let module = parse(&src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let cfg = SaConfig {
        seed: 0xDEAD_BEEF,
        ..SaConfig::DEFAULT
    };
    let bounds = PlaceConfig::MAX_PERMISSIVE;

    let p1 = place_simulated_annealing(&netlist, &cfg, &bounds).expect("p1");
    let p2 = place_simulated_annealing(&netlist, &cfg, &bounds).expect("p2");

    assert_eq!(p1.cells.len(), p2.cells.len());
    for (a, b) in p1.cells.iter().zip(p2.cells.iter()) {
        assert_eq!(a.node, b.node, "node ordering differs across runs");
        assert_eq!(a.origin, b.origin, "origin differs for node {:?}", a.node);
        assert_eq!(a.footprint, b.footprint, "footprint differs");
    }
    assert_eq!(p1.bounds, p2.bounds, "bounds differ");
}

#[test]
fn sa_placer_handles_small_inputs_without_crashing() {
    // For a 2-gate input there are too few cells for SA to do
    // meaningful work — and on a perfectly-greedy-placed chain there
    // are no improving swaps so the algorithm rejects everything. The
    // contract is "still returns a valid placement" — no panic.
    let src = r#"
        module tiny(input a, input b, output y);
            and g(.A(a), .B(b), .Y(y));
        endmodule
    "#;
    let path = PathBuf::from("tiny.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let p = place_simulated_annealing(&netlist, &SaConfig::DEFAULT, &PlaceConfig::DEFAULT)
        .expect("sa on tiny input");
    assert_eq!(p.cells.len(), 1, "exactly one gate placed");
}
