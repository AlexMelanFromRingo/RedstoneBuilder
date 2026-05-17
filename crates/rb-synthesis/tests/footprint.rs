#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{build_netlist, place, PlaceConfig, PlaceError};

#[test]
fn exceeding_max_footprint_reports_too_large() {
    // Half-adder fits in ~11×3×3. Cap to 3×3×3 to force the violation.
    let src = r#"
        module ha(input a, input b, output sum, output carry);
            xor x1(.A(a), .B(b), .Y(sum));
            and a1(.A(a), .B(b), .Y(carry));
        endmodule
    "#;
    let path = PathBuf::from("ha.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let cfg = PlaceConfig {
        max_footprint: (3, 3, 3),
        row_gap_z: 1,
    };
    let err = place(&netlist, &cfg).expect_err("must exceed footprint");
    match err {
        PlaceError::TooLarge { actual, bound } => {
            assert!(
                bound.contains("3×3×3"),
                "bound formatted unexpectedly: {bound}"
            );
            assert!(
                !actual.is_empty(),
                "actual footprint must be reported, got: {actual}"
            );
        }
    }
}

#[test]
fn comfortably_sized_design_passes() {
    let src = r#"
        module ha(input a, input b, output sum, output carry);
            xor x1(.A(a), .B(b), .Y(sum));
            and a1(.A(a), .B(b), .Y(carry));
        endmodule
    "#;
    let path = PathBuf::from("ha.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    place(&netlist, &PlaceConfig::DEFAULT).expect("half-adder fits in defaults");
}
