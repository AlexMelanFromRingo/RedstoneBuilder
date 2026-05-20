#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Dead-gate elimination (V3.P3 optimiser pass).

use std::path::PathBuf;

use rb_parser::parse;
use rb_synthesis::prune_dead_gates;

#[test]
fn dead_gate_is_removed() {
    // `g_live` drives output y; `g_dead` drives an internal wire that
    // nothing reads. After pruning only g_live remains.
    let src = r#"
        module m(input a, input b, output y);
            wire dead;
            and g_live(.A(a), .B(b), .Y(y));
            and g_dead(.A(a), .B(b), .Y(dead));
        endmodule
    "#;
    let path = PathBuf::from("dead.hdl");
    let mut module = parse(src, &path).expect("parse");
    assert_eq!(module.instances.len(), 2);

    let removed = prune_dead_gates(&mut module);
    assert_eq!(removed, 1, "exactly g_dead should be pruned");
    assert_eq!(module.instances.len(), 1);
    assert_eq!(module.instances[0].inst_name.as_str(), "g_live");
}

#[test]
fn transitive_dead_chain_is_fully_removed() {
    // d0 → d1 → d2 feeds a dead wire; the whole chain goes.
    let src = r#"
        module m(input a, output y);
            wire w0, w1, w2;
            not live(.A(a), .Y(y));
            not d0(.A(a), .Y(w0));
            not d1(.A(w0), .Y(w1));
            not d2(.A(w1), .Y(w2));
        endmodule
    "#;
    let path = PathBuf::from("chain.hdl");
    let mut module = parse(src, &path).expect("parse");
    let removed = prune_dead_gates(&mut module);
    assert_eq!(removed, 3, "all 3 dead gates pruned");
    assert_eq!(module.instances.len(), 1);
}

#[test]
fn live_design_is_untouched() {
    // Every gate in a half-adder reaches an output — prune is a no-op.
    let src = r#"
        module ha(input a, input b, output sum, output carry);
            wire ab;
            xor x1(.A(a), .B(b), .Y(ab));
            xor x2(.A(ab), .B(b), .Y(sum));
            and a1(.A(a), .B(b), .Y(carry));
        endmodule
    "#;
    let path = PathBuf::from("ha.hdl");
    let mut module = parse(src, &path).expect("parse");
    let before = module.instances.len();
    let removed = prune_dead_gates(&mut module);
    assert_eq!(removed, 0, "fully-live design must not shrink");
    assert_eq!(module.instances.len(), before);
}
