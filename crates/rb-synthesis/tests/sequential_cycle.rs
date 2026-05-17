#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::{build_netlist, detect_cycles, CycleError};

#[test]
fn feedback_through_dtrigger_is_legal() {
    // counter-style: ff.Q feeds back into ff.D through a wire.
    // The cycle is closed via the stateful primitive; cycle detector
    // must accept this.
    let src = r#"
        module counter(input clk, output q);
            wire w;
            not inv(.A(w), .Y(q));
            always @(posedge clk) begin
                dtrigger ff(.D(w), .CLK(clk), .Q(w));
            end
        endmodule
    "#;
    let path = PathBuf::from("counter.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    detect_cycles(&netlist).expect("sequential feedback through DTrigger must be legal");
}

#[test]
fn feedback_through_memcell_is_legal() {
    let src = r#"
        module hold(input data, input we, input clk, output q);
            wire w;
            not inv(.A(w), .Y(q));
            always @(posedge clk) begin
                memcell m(.DATA(w), .WRITE(we), .Q(w));
            end
        endmodule
    "#;
    let path = PathBuf::from("hold.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    detect_cycles(&netlist).expect("self-feedback through MemoryCell must be legal");
}

#[test]
fn pure_combinational_cycle_still_rejected_when_stateful_gates_present() {
    // The XOR feedback loop on w is purely combinational — must fail
    // even though there's an unrelated D-trigger elsewhere in the module.
    let src = r#"
        module bad(input a, input clk, output q);
            wire w1, w2, x;
            xor g1(.A(a), .B(w2), .Y(w1));
            xor g2(.A(w1), .B(a), .Y(w2));
            always @(posedge clk) begin
                dtrigger ff(.D(w1), .CLK(clk), .Q(x));
            end
            not inv(.A(x), .Y(q));
        endmodule
    "#;
    let path = PathBuf::from("bad.hdl");
    let module = parse(src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");

    let err = detect_cycles(&netlist).expect_err("combinational w1↔w2 cycle must be rejected");
    let CycleError::Combinational { wires } = err;
    assert!(
        wires.iter().any(|w| w == "w1") && wires.iter().any(|w| w == "w2"),
        "expected w1 and w2 in cycle, got: {wires:?}"
    );
}
