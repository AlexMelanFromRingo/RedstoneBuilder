#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{build_netlist, place, place_simulated_annealing, PlaceConfig, SaConfig};

#[test]
fn sa_placer_does_not_catastrophically_regress_vs_greedy() {
    let src = r#"
        module test(input a, input b, output sum, output carry);
            xor x1(.A(a), .B(b), .Y(sum));
            and a1(.A(a), .B(b), .Y(carry));
        endmodule
    "#;
    let path = PathBuf::from("ha.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let bounds = PlaceConfig::DEFAULT;
    let greedy_p = place(&netlist, &bounds).expect("greedy");
    let sa_p = place_simulated_annealing(&netlist, &SaConfig::DEFAULT, &bounds).expect("sa");

    // SA starts from the greedy placement and only accepts moves that
    // either improve cost or stochastically explore — bbox volume can
    // grow on a temperature spike but should not blow up by orders of
    // magnitude.
    let greedy_vol = greedy_p.bounds.volume();
    let sa_vol = sa_p.bounds.volume();
    assert!(
        sa_vol <= greedy_vol.saturating_mul(4),
        "SA bbox volume {sa_vol} catastrophically larger than greedy {greedy_vol}"
    );
    assert_eq!(sa_p.cells.len(), greedy_p.cells.len());
}
