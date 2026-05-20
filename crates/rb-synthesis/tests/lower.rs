#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Tests for the synthesis-time lowering passes
//! (`crates/rb-synthesis/src/lower.rs`).

use std::path::PathBuf;

use rb_core::GateKind;
use rb_parser::parse;
use rb_synthesis::lower_xor_gates;

fn parse_module(src: &str) -> rb_parser::ast::Module {
    parse(src, &PathBuf::from("test.hdl")).expect("parse")
}

#[test]
fn lowering_replaces_xor_with_two_nots_two_ands_one_or() {
    let src = "module m(input a, input b, output y);\n    xor g(.A(a), .B(b), .Y(y));\nendmodule\n";
    let mut module = parse_module(src);
    assert_eq!(module.instances.len(), 1);
    assert_eq!(module.instances[0].kind, GateKind::Xor);

    lower_xor_gates(&mut module);

    // 1 xor → 2 not + 2 and + 1 or = 5 instances; 0 xor remains.
    assert_eq!(module.instances.len(), 5);
    assert_eq!(
        module
            .instances
            .iter()
            .filter(|i| i.kind == GateKind::Xor)
            .count(),
        0
    );
    assert_eq!(
        module
            .instances
            .iter()
            .filter(|i| i.kind == GateKind::Not)
            .count(),
        2
    );
    assert_eq!(
        module
            .instances
            .iter()
            .filter(|i| i.kind == GateKind::And)
            .count(),
        2
    );
    assert_eq!(
        module
            .instances
            .iter()
            .filter(|i| i.kind == GateKind::Or)
            .count(),
        1
    );

    // 4 new internal wires added.
    let wire_names: Vec<_> = module
        .wires
        .iter()
        .map(|w| w.name.as_str().to_string())
        .collect();
    let internal_count = wire_names.iter().filter(|n| n.contains("__xor_g_")).count();
    assert_eq!(internal_count, 4, "got wires: {wire_names:?}");
}

#[test]
fn lowering_preserves_non_xor_instances() {
    let src = "module m(input a, input b, output y);\n    wire t;\n    and g1(.A(a), .B(b), .Y(t));\n    not g2(.A(t), .Y(y));\nendmodule\n";
    let mut module = parse_module(src);
    let before_kinds: Vec<_> = module.instances.iter().map(|i| i.kind).collect();
    lower_xor_gates(&mut module);
    let after_kinds: Vec<_> = module.instances.iter().map(|i| i.kind).collect();
    assert_eq!(before_kinds, after_kinds);
}

#[test]
fn lowering_handles_multiple_xors_with_unique_synthetic_names() {
    let src = "module m(input a, input b, input c, output y1, output y2);\n    xor g1(.A(a), .B(b), .Y(y1));\n    xor g2(.A(b), .B(c), .Y(y2));\nendmodule\n";
    let mut module = parse_module(src);
    lower_xor_gates(&mut module);
    // 2 xors → 10 instances.
    assert_eq!(module.instances.len(), 10);
    // All synthesized instance names must be unique.
    let mut names: Vec<_> = module
        .instances
        .iter()
        .map(|i| i.inst_name.as_str().to_string())
        .collect();
    names.sort();
    let n_before = names.len();
    names.dedup();
    assert_eq!(names.len(), n_before, "duplicate instance names");
}

#[test]
fn lowering_is_idempotent() {
    let src = "module m(input a, input b, output y);\n    xor g(.A(a), .B(b), .Y(y));\nendmodule\n";
    let mut module = parse_module(src);
    lower_xor_gates(&mut module);
    let after_first: usize = module.instances.len();
    lower_xor_gates(&mut module);
    assert_eq!(module.instances.len(), after_first);
}
